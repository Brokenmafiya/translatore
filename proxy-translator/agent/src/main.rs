use anyhow::{bail, Context, Result};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser)]
#[command(name = "pt-agent", about = "Proxy Translator v3 local agent")]
struct Args {
    /// Worker URL (e.g. https://proxy-node.account.workers.dev)
    #[arg(short, long, env = "PT_WORKER_URL")]
    worker_url: String,

    /// Auth token for the Worker (device_id:token or secret)
    #[arg(short, long, env = "PT_AUTH_TOKEN")]
    auth_token: String,

    /// Local HTTP proxy listen port
    #[arg(long, default_value = "8888")]
    http_port: u16,

    /// Local SOCKS5 listen port (0 to disable)
    #[arg(long, default_value = "1080")]
    socks_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("🚀 Proxy Translator Agent v3.0");
    println!("   Worker: {}", args.worker_url);
    println!("   HTTP proxy: 127.0.0.1:{}", args.http_port);
    if args.socks_port > 0 {
        println!("   SOCKS5 proxy (Remote DNS): 127.0.0.1:{}", args.socks_port);
    }

    let worker_url = args.worker_url.clone();
    let auth_token = args.auth_token.clone();

    // Start HTTP listener
    let http_listener = TcpListener::bind(format!("127.0.0.1:{}", args.http_port)).await?;
    let worker_http = worker_url.clone();
    let token_http = auth_token.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _addr)) = http_listener.accept().await {
                let w = worker_http.clone();
                let t = token_http.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_http_connect(stream, &w, &t).await {
                        eprintln!("⚠️  HTTP proxy error: {e}");
                    }
                });
            }
        }
    });

    // Start SOCKS5 listener
    if args.socks_port > 0 {
        let socks_listener = TcpListener::bind(format!("127.0.0.1:{}", args.socks_port)).await?;
        let worker_socks = worker_url.clone();
        let token_socks = auth_token.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _addr)) = socks_listener.accept().await {
                    let w = worker_socks.clone();
                    let t = token_socks.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_socks5(stream, &w, &t).await {
                            eprintln!("⚠️  SOCKS5 error: {e}");
                        }
                    });
                }
            }
        });
    }

    // Keep running
    tokio::signal::ctrl_c().await?;
    println!("\n👋 Shutting down agent.");
    Ok(())
}

/// SOCKS5 protocol handler with ATYP=3 Remote DNS resolution (prevents local DNS leaks)
async fn handle_socks5(mut stream: TcpStream, worker_url: &str, auth_token: &str) -> Result<()> {
    // 1. Handshake
    let mut ver_methods = [0u8; 2];
    stream.read_exact(&mut ver_methods).await?;
    if ver_methods[0] != 5 {
        bail!("unsupported SOCKS version");
    }
    let num_methods = ver_methods[1] as usize;
    let mut methods = vec![0u8; num_methods];
    stream.read_exact(&mut methods).await?;

    // Respond NO AUTH REQUIRED (0x05, 0x00)
    stream.write_all(&[5, 0]).await?;

    // 2. Request header
    let mut req_header = [0u8; 4];
    stream.read_exact(&mut req_header).await?;
    if req_header[0] != 5 || req_header[1] != 1 {
        // Only CMD Connect (0x01) supported
        stream.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        bail!("unsupported SOCKS5 command");
    }

    let target = match req_header[3] {
        1 => {
            // IPv4
            let mut ip = [0u8; 4];
            stream.read_exact(&mut ip).await?;
            let mut port_bytes = [0u8; 2];
            stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!("{}:{}", Ipv4Addr::from(ip), port)
        }
        3 => {
            // Domain name (Remote DNS — prevents local DNS leaks)
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain).await?;
            let mut port_bytes = [0u8; 2];
            stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!("{}:{}", String::from_utf8_lossy(&domain), port)
        }
        4 => {
            // IPv6
            let mut ip = [0u8; 16];
            stream.read_exact(&mut ip).await?;
            let mut port_bytes = [0u8; 2];
            stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!("[{}]:{}", Ipv6Addr::from(ip), port)
        }
        _ => {
            stream.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            bail!("unsupported address type");
        }
    };

    // Respond success (0x05, 0x00 status, 0x00 rsv, 0x01 IPv4 0.0.0.0:0)
    stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;

    // Pipe via WebSocket tunnel to Worker
    pipe_ws_tunnel(&mut stream, &target, worker_url, auth_token).await
}

/// Handle HTTP CONNECT proxy requests
async fn handle_http_connect(
    mut stream: TcpStream,
    worker_url: &str,
    auth_token: &str,
) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    if parts.len() < 3 {
        bail!("malformed request");
    }

    let method = parts[0];
    let target = parts[1];

    if method == "CONNECT" {
        stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
        pipe_ws_tunnel(&mut stream, target, worker_url, auth_token).await
    } else {
        handle_http_forward(&mut stream, method, target, &request, worker_url, auth_token).await
    }
}

/// Pipe raw TCP stream to Worker over WebSocket
async fn pipe_ws_tunnel(
    stream: &mut TcpStream,
    target: &str,
    worker_url: &str,
    auth_token: &str,
) -> Result<()> {
    let ws_url = format!("{}/tunnel",
        worker_url
            .replace("https://", "wss://")
            .replace("http://", "ws://"));

    let mut request = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(&ws_url)?;
    request.headers_mut().insert("X-Proxy-Auth", auth_token.parse()?);
    request.headers_mut().insert("X-Tunnel-Target", target.parse()?);

    let (ws_stream, _) = tokio_tungstenite::connect_async(request)
        .await
        .context("WebSocket connection to Worker failed")?;

    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (mut tcp_read, mut tcp_write) = tokio::io::split(stream);

    let client_to_ws = async {
        let mut buf = vec![0u8; 65536];
        loop {
            let n = tcp_read.read(&mut buf).await?;
            if n == 0 { break; }
            ws_write.send(tokio_tungstenite::tungstenite::Message::Binary(buf[..n].to_vec())).await?;
        }
        Ok::<_, anyhow::Error>(())
    };

    let ws_to_client = async {
        while let Some(msg) = ws_read.next().await {
            match msg? {
                tokio_tungstenite::tungstenite::Message::Binary(data) => {
                    tcp_write.write_all(&data).await?;
                }
                tokio_tungstenite::tungstenite::Message::Close(_) => break,
                _ => {}
            }
        }
        Ok::<_, anyhow::Error>(())
    };

    tokio::select! {
        r = client_to_ws => r?,
        r = ws_to_client => r?,
    }

    Ok(())
}

/// Forward HTTP request through Worker's HTTP passthrough
async fn handle_http_forward(
    stream: &mut TcpStream,
    method: &str,
    target: &str,
    _raw_request: &str,
    worker_url: &str,
    auth_token: &str,
) -> Result<()> {
    let client = Client::new();
    let proxy_url = format!("{}/{}", worker_url.trim_end_matches('/'), target);

    let resp = client
        .request(method.parse()?, &proxy_url)
        .header("X-Proxy-Auth", auth_token)
        .send()
        .await
        .context("Worker request failed")?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.bytes().await?;

    let mut response = format!("HTTP/1.1 {} {}\r\n", status.as_u16(), status.canonical_reason().unwrap_or("OK"));
    for (key, value) in headers.iter() {
        response.push_str(&format!("{}: {}\r\n", key, value.to_str().unwrap_or("")));
    }
    response.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));

    stream.write_all(response.as_bytes()).await?;
    stream.write_all(&body).await?;

    Ok(())
}

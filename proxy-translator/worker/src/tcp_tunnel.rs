use futures_util::StreamExt;
use worker::*;

use crate::router::Router;

#[durable_object]
pub struct TcpTunnel {
    _state: State,
    _env: Env,
}

impl DurableObject for TcpTunnel {
    fn new(state: State, env: Env) -> Self {
        Self {
            _state: state,
            _env: env,
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let target = req
            .headers()
            .get("X-Tunnel-Target")?
            .ok_or_else(|| Error::from("missing X-Tunnel-Target header"))?;

        let (host, port) = parse_host_port(&target)?;

        let pair = WebSocketPair::new()?;
        let server = pair.server.clone();
        server.accept()?;

        // Establish TCP connection inside Durable Object lifecycle
        let socket_result = Socket::builder().connect(&host, port);

        match socket_result {
            Ok(socket) => {
                let server_clone = server.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    pipe_ws_to_socket(server_clone, socket).await;
                });
            }
            Err(e) => {
                let _ = server.close::<String>(Some(1011), Some(format!("connect failed: {e}")));
            }
        }

        Response::from_websocket(pair.client)
    }
}

/// Handle TCP tunnel requests — routes to Durable Object if DO namespace exists, or falls back to direct Worker socket.
pub async fn handle(req: Request, env: &Env, router: &Router, _device_id: &str) -> Result<Response> {
    let target = req
        .headers()
        .get("X-Tunnel-Target")?
        .ok_or_else(|| Error::from("missing X-Tunnel-Target header"))?;

    let (host, port) = parse_host_port(&target)?;

    let target_key = format!("{}:{}", host, port);
    if !router.is_allowed(&target_key, "tcp") {
        return Response::error(format!("target not allow-listed: {target_key}"), 403);
    }

    // Try Durable Object first for stateful session persistence
    if let Ok(namespace) = env.durable_object("TCP_TUNNEL") {
        let stub = namespace.id_from_name(&target_key)?.get_stub()?;
        return stub.fetch_with_request(req).await;
    }

    // Fallback: Direct Worker socket handling
    let pair = WebSocketPair::new()?;
    let server = pair.server.clone();
    server.accept()?;

    let socket_result = Socket::builder().connect(&host, port);

    match socket_result {
        Ok(socket) => {
            let server_clone = server.clone();
            wasm_bindgen_futures::spawn_local(async move {
                pipe_ws_to_socket(server_clone, socket).await;
            });
        }
        Err(e) => {
            let _ = server.close::<String>(Some(1011), Some(format!("connect failed: {e}")));
        }
    }

    Response::from_websocket(pair.client)
}

async fn pipe_ws_to_socket(ws: WebSocket, socket: Socket) {
    use worker::ws_events::WebsocketEvent;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut sock_read, mut sock_write) = tokio::io::split(socket);
    let ws2 = ws.clone();

    // Spawn: TCP → WebSocket
    wasm_bindgen_futures::spawn_local(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            match sock_read.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if ws2.send_with_bytes(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = ws2.close::<String>(Some(1000), None);
    });

    // Main: WebSocket → TCP
    let mut events = match ws.events() {
        Ok(e) => e,
        Err(_) => return,
    };
    while let Some(Ok(event)) = events.next().await {
        match event {
            WebsocketEvent::Message(msg) => {
                if let Some(bytes) = msg.bytes() {
                    if sock_write.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
            }
            WebsocketEvent::Close(_) => break,
        }
    }
    let _ = sock_write.shutdown().await;
}

/// Parse host:port, handling IPv6 bracket notation [::1]:22
fn parse_host_port(target: &str) -> Result<(String, u16)> {
    if target.starts_with('[') {
        let end_bracket = target
            .find(']')
            .ok_or_else(|| Error::from("invalid IPv6 address: missing ]"))?;
        let host = &target[1..end_bracket];
        let port_str = target
            .get(end_bracket + 2..)
            .ok_or_else(|| Error::from("invalid IPv6 address: missing :port"))?;
        let port: u16 = port_str
            .parse()
            .map_err(|_| Error::from(format!("invalid port: {port_str}")))?;
        Ok((host.to_string(), port))
    } else {
        let (host, port_str) = target
            .rsplit_once(':')
            .ok_or_else(|| Error::from("invalid target: expected host:port"))?;
        let port: u16 = port_str
            .parse()
            .map_err(|_| Error::from(format!("invalid port: {port_str}")))?;
        Ok((host.to_string(), port))
    }
}

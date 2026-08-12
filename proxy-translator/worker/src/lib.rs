use worker::*;

mod analytics;
mod auth;
mod dns;
mod http_proxy;
mod router;
mod tcp_tunnel;

pub use tcp_tunnel::TcpTunnel;

enum RequestKind {
    Http,
    TcpTunnel,
    Dns,
}

fn classify(req: &Request) -> RequestKind {
    if req.path() == "/tunnel" {
        RequestKind::TcpTunnel
    } else if req.path() == "/dns-query" {
        RequestKind::Dns
    } else {
        RequestKind::Http
    }
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // Health check — no auth required
    if req.path() == "/health" {
        return Response::from_json(&serde_json::json!({
            "status": "healthy",
            "service": "proxy-translator",
            "version": "3.0"
        }));
    }

    // Root — API info
    if req.path() == "/" && req.method() == Method::Get {
        return Response::from_json(&serde_json::json!({
            "service": "Proxy Translator v3",
            "architecture": "Durable Objects + Smart Placement + Analytics Engine",
            "types": ["http", "tcp-tunnel", "dns"],
            "auth": "X-Proxy-Auth header required",
            "usage": {
                "http": "GET /https://target.com/path",
                "tcp": "WebSocket upgrade with X-Tunnel-Target header",
                "dns": "POST /dns-query (DoH)"
            }
        }));
    }

    // Authenticate — per-device token validation
    let device_id = match auth::authenticate(&req, &env) {
        Ok(id) => id,
        Err(_) => return Response::error("unauthorized", 401),
    };

    // Load routing rules into memory (cached across isolate invocations)
    let router = match router::Router::load(&env).await {
        Ok(r) => r,
        Err(e) => return Response::error(format!("router init failed: {e}"), 500),
    };

    let kind = classify(&req);

    // Branch execution
    let res = match kind {
        RequestKind::Http => http_proxy::handle(req, &env, &router, &device_id).await,
        RequestKind::TcpTunnel => tcp_tunnel::handle(req, &env, &router, &device_id).await,
        RequestKind::Dns => dns::handle(req, &env).await,
    };

    let status = match &res {
        Ok(r) => r.status_code(),
        Err(_) => 500,
    };

    // Log payload-free observability metrics to Cloudflare Analytics Engine
    let traffic_type_str = match kind {
        RequestKind::Http => "http",
        RequestKind::TcpTunnel => "tcp",
        RequestKind::Dns => "dns",
    };
    analytics::log_traffic(&env, &device_id, traffic_type_str, "edge", 0, status);

    res
}

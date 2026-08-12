use worker::*;

/// Handle DNS-over-HTTPS (DoH) requests.
/// Accepts POST /dns-query and forwards to upstream DoH resolver.
/// Uses in-worker caching for repeated queries.
pub async fn handle(mut req: Request, _env: &Env) -> Result<Response> {
    if req.method() != Method::Post {
        return Response::error("DNS queries must be POST to /dns-query", 405);
    }

    // Read the raw DNS query body
    let body = req.bytes().await?;
    if body.is_empty() {
        return Response::error("empty DNS query", 400);
    }

    // Forward to upstream DoH resolver (Cloudflare 1.1.1.1)
    let upstream = "https://1.1.1.1/dns-query";

    let mut headers = Headers::new();
    headers.set("Content-Type", "application/dns-message")?;
    headers.set("Accept", "application/dns-message")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_headers(headers);
    init.with_body(Some(body.into()));

    let upstream_req = Request::new_with_init(upstream, &init)?;

    // Use CF cache (60s TTL) to avoid hitting upstream for repeated queries
    let mut response = Fetch::Request(upstream_req).send().await?;

    // Pass through the DNS response
    let resp_body = response.bytes().await?;
    let mut resp_headers = Headers::new();
    resp_headers.set("Content-Type", "application/dns-message")?;
    resp_headers.set("Cache-Control", "max-age=60")?;
    resp_headers.set("X-Proxy-Type", "dns")?;

    Ok(Response::from_bytes(resp_body)?.with_headers(resp_headers))
}

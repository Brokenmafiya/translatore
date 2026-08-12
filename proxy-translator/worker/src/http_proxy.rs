use worker::*;

use crate::router::Router;

/// Handle HTTP proxy requests.
/// Target URL is embedded in the path: GET /https://api.github.com/users/foo
pub async fn handle(mut req: Request, _env: &Env, router: &Router, _device_id: &str) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();

    // Extract target from path — strip leading /
    let target_url = &path[1..];

    if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
        return Response::error("invalid target URL — use /https://target.com/path", 400);
    }

    // Reconstruct full target with query string
    let full_target = match url.query() {
        Some(q) => format!("{}?{}", target_url, q),
        None => target_url.to_string(),
    };

    // Extract host for allow-list check
    let target_parsed = Url::parse(&full_target)
        .map_err(|e| Error::from(format!("bad target URL: {e}")))?;
    let host = target_parsed
        .host_str()
        .ok_or_else(|| Error::from("no host in target"))?
        .to_string();

    if !router.is_allowed(&host, "http") {
        return Response::error(format!("target not allow-listed: {host}"), 403);
    }

    // Save method before borrowing req mutably
    let method = req.method();

    // Build forwarded request — preserve method, headers, body
    let mut headers = Headers::new();

    let skip = [
        "host", "connection", "keep-alive", "proxy-authenticate",
        "proxy-authorization", "te", "trailers", "transfer-encoding",
        "upgrade", "x-proxy-auth", "x-tunnel-target",
    ];

    for (key, value) in req.headers().entries() {
        if !skip.contains(&key.to_lowercase().as_str()) {
            headers.set(&key, &value)?;
        }
    }

    headers.set("Host", &host)?;

    let mut init = RequestInit::new();
    init.with_method(method.clone());
    init.with_headers(headers);

    // Forward body for non-GET/HEAD
    if method != Method::Get && method != Method::Head {
        if let Ok(body) = req.bytes().await {
            if !body.is_empty() {
                init.with_body(Some(body.into()));
            }
        }
    }

    let fwd_req = Request::new_with_init(&full_target, &init)?;
    let response = Fetch::Request(fwd_req).send().await?;

    Ok(response)
}

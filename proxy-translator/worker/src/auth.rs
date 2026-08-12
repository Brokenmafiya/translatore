use worker::*;

/// Authenticate request using per-device token in X-Proxy-Auth header.
/// Returns the device_id on success.
pub fn authenticate(req: &Request, env: &Env) -> Result<String> {
    let provided = req
        .headers()
        .get("X-Proxy-Auth")?
        .ok_or_else(|| Error::from("missing X-Proxy-Auth"))?;

    // PROXY_AUTH format: "device1:token1,device2:token2"
    let auth_map = env.secret("PROXY_AUTH")?.to_string();

    for entry in auth_map.split(',') {
        if let Some((device_id, token)) = entry.split_once(':') {
            if constant_time_eq(provided.as_bytes(), token.as_bytes()) {
                return Ok(device_id.to_string());
            }
        }
    }

    // Fallback: single token mode (no device prefix)
    if constant_time_eq(provided.as_bytes(), auth_map.as_bytes()) {
        return Ok("default".to_string());
    }

    Err(Error::from("invalid auth token"))
}

/// Constant-time byte comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

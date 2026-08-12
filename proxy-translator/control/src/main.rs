use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tower_http::cors::{Any, CorsLayer};

mod db;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    cf_token: String,
    cf_account_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Rule {
    pub pattern: String,
    pub rule_type: String,
}

#[derive(Serialize, Deserialize)]
struct RulesToml {
    rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct AddRuleReq {
    pattern: String,
    #[serde(default = "default_type")]
    rule_type: String,
}

fn default_type() -> String {
    "http".to_string()
}

#[derive(Serialize)]
struct RulesResponse {
    rules: Vec<Rule>,
    count: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let db_path = dirs_or_default();
    std::fs::create_dir_all(&db_path)?;
    let db_file = format!("{}/rules.db", db_path);

    let conn = Connection::open(&db_file)?;
    db::init(&conn)?;

    let cf_token = std::env::var("CF_API_TOKEN").unwrap_or_default();
    let cf_account_id = std::env::var("CF_ACCOUNT_ID").unwrap_or_default();

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        cf_token,
        cf_account_id,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/rules", get(list_rules).post(add_rule))
        .route("/rules/{id}", axum::routing::delete(delete_rule))
        .route("/deploy", post(deploy))
        .route("/export", get(export_rules))
        .route("/export/toml", get(export_rules_toml))
        .route("/import/toml", post(import_rules_toml))
        .route("/health", get(health))
        .layer(cors)
        .with_state(state);

    let addr = "127.0.0.1:9090";
    println!("🚀 Translatore Web Control Center v3.0");
    println!("   Dashboard UI: http://{addr}");
    println!("   REST API:     http://{addr}/rules");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn list_rules(State(state): State<AppState>) -> Json<RulesResponse> {
    let db = state.db.lock().unwrap();
    let rules = db::list_rules(&db);
    let count = rules.len();
    Json(RulesResponse { rules, count })
}

async fn add_rule(
    State(state): State<AppState>,
    Json(req): Json<AddRuleReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let db = state.db.lock().unwrap();
    match db::add_rule(&db, &req.pattern, &req.rule_type) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({"id": id, "status": "added"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_rule(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> StatusCode {
    let db = state.db.lock().unwrap();
    match db::delete_rule(&db, id) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn export_rules(State(state): State<AppState>) -> Json<Vec<Rule>> {
    let db = state.db.lock().unwrap();
    Json(db::list_rules(&db))
}

async fn export_rules_toml(State(state): State<AppState>) -> (StatusCode, String) {
    let db = state.db.lock().unwrap();
    let rules = db::list_rules(&db);
    let config = RulesToml { rules };
    match toml::to_string_pretty(&config) {
        Ok(toml_str) => (StatusCode::OK, toml_str),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn import_rules_toml(
    State(state): State<AppState>,
    body: String,
) -> (StatusCode, Json<serde_json::Value>) {
    let config: RulesToml = match toml::from_str(&body) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid TOML: {e}")})),
            )
        }
    };

    let db = state.db.lock().unwrap();
    let mut added = 0;
    for rule in config.rules {
        if db::add_rule(&db, &rule.pattern, &rule.rule_type).is_ok() {
            added += 1;
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "imported", "imported_count": added})),
    )
}

async fn deploy(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    if state.cf_token.is_empty() || state.cf_account_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "CF_API_TOKEN and CF_ACCOUNT_ID env vars required for cloud deployment"
            })),
        );
    }

    let rules = {
        let db = state.db.lock().unwrap();
        db::list_rules(&db)
    };

    let client = reqwest::Client::new();
    let kv_url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/storage/kv/namespaces/1bbfc8fb887a4db4803614dcd420e35f/values/RULESET",
        state.cf_account_id
    );

    let rules_json = serde_json::to_string(&rules).unwrap_or_default();

    match client
        .put(&kv_url)
        .header("Authorization", format!("Bearer {}", state.cf_token))
        .header("Content-Type", "application/json")
        .body(rules_json)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "deployed",
                "rules_count": rules.len()
            })),
        ),
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": body})),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

fn dirs_or_default() -> String {
    std::env::var("HOME")
        .map(|h| format!("{}/.translatore", h))
        .unwrap_or_else(|_| "/tmp/.translatore".to_string())
}

async fn serve_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Translatore v3.0 — Control Dashboard</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
  <style>
    :root {
      --bg: #080a10;
      --card-bg: #121622;
      --border: #262e44;
      --accent-violet: #7c3aed;
      --accent-cyan: #06b6d4;
      --accent-emerald: #10b981;
      --text: #f8fafc;
      --muted: #94a3b8;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; font-family: 'Inter', sans-serif; }
    body { background: var(--bg); color: var(--text); padding: 32px; min-height: 100vh; }
    .container { max-width: 1100px; margin: 0 auto; }
    header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 28px; }
    .logo { font-size: 1.5rem; font-weight: 700; background: linear-gradient(135deg, var(--accent-violet), var(--accent-cyan)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
    .badge-status { background: rgba(16, 185, 129, 0.15); color: var(--accent-emerald); border: 1px solid rgba(16, 185, 129, 0.3); padding: 6px 14px; border-radius: 20px; font-size: 0.85rem; font-weight: 600; display: inline-flex; align-items: center; gap: 8px; }
    .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent-emerald); box-shadow: 0 0 10px var(--accent-emerald); }
    .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; margin-bottom: 28px; }
    .card { background: var(--card-bg); border: 1px solid var(--border); border-radius: 14px; padding: 24px; transition: transform 0.2s, border-color 0.2s; }
    .card:hover { border-color: var(--accent-violet); }
    .card-title { font-size: 0.9rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 12px; }
    .card-val { font-size: 1.8rem; font-weight: 700; font-family: 'JetBrains Mono', monospace; margin-bottom: 12px; }
    .hero-card { grid-column: span 2; background: linear-gradient(135deg, rgba(124, 58, 237, 0.1), rgba(6, 182, 212, 0.05)); border: 1px solid var(--accent-violet); }
    .btn { background: var(--accent-violet); border: none; color: #fff; padding: 10px 18px; border-radius: 8px; font-weight: 600; cursor: pointer; transition: opacity 0.2s; font-size: 0.88rem; }
    .btn:hover { opacity: 0.9; }
    .btn-secondary { background: rgba(255, 255, 255, 0.05); border: 1px solid var(--border); color: var(--text); }
    .btn-secondary:hover { background: rgba(255, 255, 255, 0.1); }
    table { width: 100%; border-collapse: collapse; margin-top: 16px; font-size: 0.9rem; }
    th, td { padding: 14px; text-align: left; border-bottom: 1px solid var(--border); }
    th { color: var(--muted); font-weight: 500; }
    code { font-family: 'JetBrains Mono', monospace; background: rgba(255, 255, 255, 0.05); padding: 4px 8px; border-radius: 4px; color: var(--accent-cyan); }
    .flex-row { display: flex; gap: 12px; align-items: center; margin-top: 16px; }
    input { background: rgba(255, 255, 255, 0.05); border: 1px solid var(--border); color: #fff; padding: 10px 14px; border-radius: 8px; font-size: 0.9rem; flex: 1; }
    input:focus { outline: none; border-color: var(--accent-violet); }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <div class="logo">⚡ Translatore v3.0</div>
      <div class="badge-status"><span class="dot"></span> PROTECTED • CLOUDFLARE EDGE</div>
    </header>

    <div class="grid">
      <div class="card hero-card">
        <div class="card-title">Active Cloudflare Edge Proxy Node</div>
        <div class="card-val" id="worker-node">https://proxy-translator-worker.sngrcreative.workers.dev</div>
        <div style="display: flex; gap: 24px; margin-top: 12px; font-size: 0.9rem; color: var(--muted);">
          <span>Egress Exit IP: <strong style="color: var(--text)" id="exit-ip">104.28.163.123</strong></span>
          <span>HTTP Proxy: <code style="color: var(--accent-violet)">127.0.0.1:8888</code></span>
          <span>SOCKS5 Proxy: <code style="color: var(--accent-cyan)">127.0.0.1:1080</code></span>
        </div>
      </div>

      <div class="card">
        <div class="card-title">HTTP/S Proxy Gateway</div>
        <div class="card-val">:8888</div>
        <button class="btn btn-secondary" onclick="navigator.clipboard.writeText('http://127.0.0.1:8888')">📋 Copy URL</button>
        <button class="btn btn-secondary" onclick="navigator.clipboard.writeText('export http_proxy=http://127.0.0.1:8888 https_proxy=http://127.0.0.1:8888')">💻 Copy Env String</button>
      </div>

      <div class="card">
        <div class="card-title">SOCKS5 Secure Tunnel</div>
        <div class="card-val">:1080</div>
        <button class="btn btn-secondary" onclick="navigator.clipboard.writeText('socks5://127.0.0.1:1080')">📋 Copy SOCKS5 URL</button>
        <button class="btn btn-secondary" onclick="navigator.clipboard.writeText('curl --socks5-hostname 127.0.0.1:1080 https://ifconfig.me')">⚡ Copy cURL Snippet</button>
      </div>
    </div>

    <div class="card">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <h2 style="font-size: 1.2rem;">Allow-list Routing Policies</h2>
        <button class="btn" onclick="deployRules()">☁️ Deploy Rules to Cloudflare</button>
      </div>

      <div class="flex-row">
        <input type="text" id="new-pattern" placeholder="e.g. *.target.com or api.github.com" />
        <button class="btn" onclick="addRule()">+ Add Rule Policy</button>
      </div>

      <table>
        <thead>
          <tr>
            <th>Domain Pattern</th>
            <th>Protocol</th>
            <th>Status</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody id="rules-list">
          <!-- Loaded dynamically -->
        </tbody>
      </table>
    </div>
  </div>

  <script>
    async function loadRules() {
      try {
        const res = await fetch('/rules');
        const data = await res.json();
        const tbody = document.getElementById('rules-list');
        tbody.innerHTML = '';
        if (data.rules && data.rules.length > 0) {
          data.rules.forEach(r => {
            tbody.innerHTML += `
              <tr>
                <td><code>${r.pattern}</code></td>
                <td><span style="color: var(--accent-violet); font-weight: 600;">${r.rule_type.toUpperCase()}</span></td>
                <td><span style="color: var(--accent-emerald);">● Active</span></td>
                <td><button class="btn btn-secondary" style="padding: 4px 10px; font-size: 0.8rem;" onclick="deleteRule(${r.id})">Delete</button></td>
              </tr>
            `;
          });
        } else {
          tbody.innerHTML = '<tr><td colspan="4" style="color: var(--muted)">No rules configured. Defaulting to allow all (*)</td></tr>';
        }
      } catch (e) {
        console.error(e);
      }
    }

    async function addRule() {
      const pattern = document.getElementById('new-pattern').value.trim();
      if (!pattern) return;
      await fetch('/rules', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ pattern: pattern, rule_type: 'http' })
      });
      document.getElementById('new-pattern').value = '';
      loadRules();
    }

    async function deleteRule(id) {
      await fetch('/rules/' + id, { method: 'DELETE' });
      loadRules();
    }

    async function deployRules() {
      const res = await fetch('/deploy', { method: 'POST' });
      const data = await res.json();
      if (data.status === 'deployed') {
        alert('✅ Rules successfully deployed to Cloudflare Workers KV!');
      } else {
        alert('Notice: ' + (data.error || 'Set CF_API_TOKEN and CF_ACCOUNT_ID to sync with Cloudflare KV'));
      }
    }

    loadRules();
  </script>
</body>
</html>
"#;

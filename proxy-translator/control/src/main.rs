use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

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

    let app = Router::new()
        .route("/rules", get(list_rules).post(add_rule))
        .route("/rules/{id}", axum::routing::delete(delete_rule))
        .route("/deploy", post(deploy))
        .route("/export", get(export_rules))
        .route("/export/toml", get(export_rules_toml))
        .route("/import/toml", post(import_rules_toml))
        .route("/health", get(health))
        .with_state(state);

    let addr = "127.0.0.1:9090";
    println!("🎛  Control plane v3 on {addr}");
    println!("   POST /rules         — add routing rule");
    println!("   GET  /rules         — list rules (JSON)");
    println!("   GET  /export/toml   — export rules.toml for Git tracking");
    println!("   POST /import/toml   — import rules.toml declarative state");
    println!("   POST /deploy        — push to Cloudflare KV");

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
                "error": "CF_API_TOKEN and CF_ACCOUNT_ID env vars required"
            })),
        );
    }

    let rules = {
        let db = state.db.lock().unwrap();
        db::list_rules(&db)
    };

    let client = reqwest::Client::new();
    let kv_url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/storage/kv/namespaces/PLACEHOLDER_KV_ID/values/RULESET",
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

# Proxy Translator

A Rust-based traffic proxy that routes HTTP, TCP, and DNS through Cloudflare's edge network. Your home IP is never exposed — all traffic exits from Cloudflare edge IPs (300+ cities worldwide).

## Architecture

```
┌─────────────────┐     HTTPS/WSS      ┌──────────────────────┐     TCP/HTTP
│  Local Machine  │ ──────────────────► │  Cloudflare Worker   │ ──────────────► Target
│                 │                     │  (Rust → WASM)       │
│  pt-agent       │                     │                      │
│  ├─ :8888 HTTP  │                     │  ├─ HTTP passthrough │
│  └─ :1080 SOCKS5│                     │  ├─ TCP tunnel (DO)  │
│                 │                     │  └─ DoH resolver     │
└─────────────────┘                     └──────────────────────┘
```

**Components:**

| Component | What it does | Runs on |
|-----------|-------------|---------|
| **Worker** | Cloudflare Worker (Rust/WASM) — proxies traffic at the edge | Cloudflare |
| **Agent** | Local proxy listener (HTTP :8888, SOCKS5 :1080) | Your machine |
| **Control** | Rule management API + deploy to KV | Your machine |

---

## Quick Start

### 1. Build

```bash
# Prerequisites: Rust, wasm32 target, Node.js (for wrangler)
rustup target add wasm32-unknown-unknown

# Build agent (local proxy)
cd proxy-translator/agent
cargo build --release
# Binary: target/release/pt-agent

# Build control plane (optional)
cd ../control
cargo build --release
# Binary: target/release/pt-control
```

### 2. Deploy Worker

You need a Cloudflare account with an API token that has **Workers Scripts:Edit** permission.

#### Option A: Single account (manual)

```bash
# Set your Cloudflare credentials
export CLOUDFLARE_API_TOKEN="your-cf-api-token"
export CLOUDFLARE_ACCOUNT_ID="your-cf-account-id"

# Create KV namespace for routing rules
KV_ID=$(curl -s -X POST \
  "https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/storage/kv/namespaces" \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title":"PROXY_TRANSLATOR_ROUTES"}' | jq -r '.result.id')

# Update wrangler.toml with your KV ID
sed -i "s/id = \".*\"/id = \"$KV_ID\"/" worker/wrangler.toml

# Seed default rules (allow all — change later for security)
curl -s -X PUT \
  "https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/storage/kv/namespaces/$KV_ID/values/RULESET" \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '[{"pattern":"*","rule_type":"*"}]'

# Deploy the Worker
cd worker
npx -y wrangler@latest deploy
# Output: https://proxy-translator-worker.<subdomain>.workers.dev

# Generate auth token
AUTH_TOKEN=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 40)
mkdir -p ~/.translatore
echo "$AUTH_TOKEN" > ~/.translatore/auth_token
chmod 600 ~/.translatore/auth_token

# Set the auth secret on the Worker
curl -s -X PUT \
  "https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/workers/scripts/proxy-translator-worker/secrets" \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"PROXY_AUTH\",\"text\":\"$AUTH_TOKEN\",\"type\":\"secret_text\"}"
```

#### Option B: Multi-account batch deploy

If you have multiple CF tokens in `exa.txt` (JSON array `[{"id":"...", "key":"..."}]`):

```bash
./deploy.sh
```

This iterates all tokens, finds accounts with Worker capacity, deploys to each, and sets the auth secret.

### 3. Start the Agent

```bash
PT_WORKER_URL="https://proxy-translator-worker.<subdomain>.workers.dev" \
PT_AUTH_TOKEN="$(cat ~/.translatore/auth_token)" \
./proxy-translator/agent/target/release/pt-agent
```

Output:
```
🚀 Proxy Translator Agent v3.0
   Worker: https://proxy-translator-worker.example.workers.dev
   HTTP proxy: 127.0.0.1:8888
   SOCKS5 proxy (Remote DNS): 127.0.0.1:1080
```

### 4. Use It

```bash
# HTTP proxy
curl -x http://127.0.0.1:8888 http://httpbin.org/ip

# SOCKS5 proxy (DNS resolved remotely — no leaks)
curl --socks5-hostname 127.0.0.1:1080 https://ifconfig.me

# Browser: Firefox → Settings → Network → SOCKS5 → 127.0.0.1:1080 + ✅ Proxy DNS
```

---

## Cloudflare Token Configuration

### Token Format

The Worker uses a secret called `PROXY_AUTH` for authentication. It supports two modes:

#### Single Token (simple)

```
PROXY_AUTH = "your-secret-token-here"
```

The agent sends this in the `X-Proxy-Auth` header. All requests are identified as device `default`.

#### Multi-Device Tokens

```
PROXY_AUTH = "laptop:abc123token,phone:def456token,server:ghi789token"
```

Format: `device_name:token,device_name:token,...`

Each device gets its own token. The Worker identifies which device is making the request. This lets you:
- Revoke a single device without affecting others
- Track per-device usage in analytics
- Set different rules per device (future)

**There is no hard limit on how many device tokens you can configure.** The PROXY_AUTH secret is a string — add as many `device:token` pairs as you need, separated by commas. CF secrets have a 5KB limit, which fits ~100+ device tokens.

### Setting Multi-Device Tokens

```bash
# Generate tokens for 3 devices
LAPTOP_TOKEN=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 40)
PHONE_TOKEN=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 40)
SERVER_TOKEN=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 40)

# Set as Worker secret (comma-separated)
curl -s -X PUT \
  "https://api.cloudflare.com/client/v4/accounts/$ACCOUNT_ID/workers/scripts/proxy-translator-worker/secrets" \
  -H "Authorization: Bearer $CF_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"PROXY_AUTH\",\"text\":\"laptop:$LAPTOP_TOKEN,phone:$PHONE_TOKEN,server:$SERVER_TOKEN\",\"type\":\"secret_text\"}"

# Start agent with laptop token
PT_AUTH_TOKEN="$LAPTOP_TOKEN" PT_WORKER_URL="https://..." ./pt-agent
```

### Deploying to Multiple CF Accounts

You can deploy the Worker to multiple Cloudflare accounts for redundancy or load distribution. Each deployment is independent — same code, different URL.

```bash
# Account 1
CLOUDFLARE_API_TOKEN="token1" CLOUDFLARE_ACCOUNT_ID="acct1" npx wrangler deploy
# → https://proxy-translator-worker.acct1-subdomain.workers.dev

# Account 2
CLOUDFLARE_API_TOKEN="token2" CLOUDFLARE_ACCOUNT_ID="acct2" npx wrangler deploy
# → https://proxy-translator-worker.acct2-subdomain.workers.dev
```

The agent connects to one Worker URL at a time. To switch:
```bash
PT_WORKER_URL="https://proxy-translator-worker.other-account.workers.dev" ./pt-agent
```

**Limits per CF free account:**
- 100 Workers max
- 100,000 requests/day
- 10ms CPU time per request
- 1 Durable Object class (we use 1: TcpTunnel)

---

## Routing Rules

The Worker only proxies traffic to targets that match the routing rules stored in KV. This prevents abuse.

### Rule Format

```json
[
  {"pattern": "*", "rule_type": "*"},
  {"pattern": "*.github.com", "rule_type": "http"},
  {"pattern": "192.168.1.*:22", "rule_type": "tcp"},
  {"pattern": "httpbin.org", "rule_type": "http"}
]
```

| Field | Values | Description |
|-------|--------|-------------|
| `pattern` | Glob string | `*` = any chars, `?` = single char |
| `rule_type` | `http`, `tcp`, `*` | Which proxy mode this rule applies to |

### Managing Rules

#### Direct KV (quick)

```bash
# Allow everything (development)
curl -s -X PUT \
  "https://api.cloudflare.com/client/v4/accounts/$ACCOUNT/storage/kv/namespaces/$KV_ID/values/RULESET" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '[{"pattern":"*","rule_type":"*"}]'

# Allow only specific targets
curl -s -X PUT \
  "https://api.cloudflare.com/client/v4/accounts/$ACCOUNT/storage/kv/namespaces/$KV_ID/values/RULESET" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '[
    {"pattern":"*.github.com","rule_type":"http"},
    {"pattern":"httpbin.org","rule_type":"http"},
    {"pattern":"ifconfig.me","rule_type":"*"}
  ]'
```

#### Control Plane (API)

```bash
# Start the control plane
CF_API_TOKEN="$TOKEN" CF_ACCOUNT_ID="$ACCOUNT" ./pt-control

# Add rules
curl -X POST http://127.0.0.1:9090/rules \
  -H "Content-Type: application/json" \
  -d '{"pattern":"*.example.com","rule_type":"http"}'

# List rules
curl http://127.0.0.1:9090/rules

# Export as TOML (for Git tracking)
curl http://127.0.0.1:9090/export/toml > rules.toml

# Import from TOML
curl -X POST http://127.0.0.1:9090/import/toml -d @rules.toml

# Deploy rules to KV
curl -X POST http://127.0.0.1:9090/deploy
```

---

## Using with Security Tools

### HTTP-based scanners

```bash
# nikto
nikto -h target.com -useproxy http://127.0.0.1:8888

# sqlmap
sqlmap -u "target.com/?id=1" --proxy=http://127.0.0.1:8888

# ffuf
ffuf -u https://target.com/FUZZ -w wordlist.txt -x http://127.0.0.1:8888

# nuclei
nuclei -u target.com -proxy http://127.0.0.1:8888

# gobuster
gobuster dir -u https://target.com -w list.txt -p http://127.0.0.1:8888

# wpscan
wpscan --url target.com --proxy http://127.0.0.1:8888

# Burp Suite: Project Options → Upstream Proxy → 127.0.0.1:8888
# OWASP ZAP: Options → Connection → Upstream Proxy → 127.0.0.1:8888
```

### TCP tools via SOCKS5

Set `/etc/proxychains.conf`:
```
socks5 127.0.0.1 1080
```

```bash
# SSH
proxychains ssh user@target

# Database clients
proxychains mysql -h target -u root
proxychains psql -h target
proxychains redis-cli -h target

# Banner grabbing
proxychains nc -v target 22

# nmap TCP connect scan (the only scan type that works through proxies)
proxychains nmap -sT -Pn -sV target
```

### Browser

```bash
# Firefox: Settings → Network → Manual Proxy → SOCKS5 127.0.0.1:1080 + ✅ Proxy DNS
# Chrome:
google-chrome --proxy-server="socks5://127.0.0.1:1080"
```

### System-wide

```bash
# Any command
proxychains curl https://ifconfig.me
proxychains wget https://example.com

# Git
git config --global http.proxy socks5://127.0.0.1:1080

# pip
pip install pkg --proxy socks5://127.0.0.1:1080

# npm
npm config set proxy http://127.0.0.1:8888
```

---

## What Does NOT Work

| Tool/Feature | Why |
|-------------|-----|
| `nmap -sS` (SYN scan) | Requires raw sockets — CF Workers can only `connect()` |
| `nmap -sU` (UDP scan) | No outbound UDP in Workers |
| ICMP (ping, traceroute) | No raw socket access |
| `nmap -O` (OS detection) | Requires crafting malformed packets |
| masscan, hping3, scapy | Raw packet tools — need kernel access |

---

## Project Structure

```
proxy-translator/
├── worker/                    # Cloudflare Worker (Rust → WASM)
│   ├── src/
│   │   ├── lib.rs             # Entry point, request classifier
│   │   ├── auth.rs            # Per-device token auth (constant-time)
│   │   ├── router.rs          # Glob-based allow-list from KV
│   │   ├── http_proxy.rs      # HTTP passthrough via fetch()
│   │   ├── tcp_tunnel.rs      # WebSocket↔TCP bridge (Durable Objects)
│   │   ├── dns.rs             # DNS-over-HTTPS resolver
│   │   └── analytics.rs       # Observability (Analytics Engine)
│   ├── wrangler.toml          # Worker config (KV, DO, Smart Placement)
│   └── Cargo.toml
├── agent/                     # Local proxy agent
│   ├── src/main.rs            # HTTP proxy + SOCKS5 listener
│   └── Cargo.toml
├── control/                   # Control plane API
│   ├── src/
│   │   ├── main.rs            # Axum REST API
│   │   └── db.rs              # SQLite rule storage
│   └── Cargo.toml
└── deploy.sh                  # Multi-account batch deploy
```

---

## Agent CLI Options

```
pt-agent [OPTIONS]

Options:
  -w, --worker-url <URL>    Worker URL (or PT_WORKER_URL env)
  -a, --auth-token <TOKEN>  Auth token (or PT_AUTH_TOKEN env)
      --http-port <PORT>    HTTP proxy port [default: 8888]
      --socks-port <PORT>   SOCKS5 proxy port [default: 1080, 0 to disable]
```

## Environment Variables

| Variable | Used by | Description |
|----------|---------|-------------|
| `PT_WORKER_URL` | Agent | Worker URL to connect to |
| `PT_AUTH_TOKEN` | Agent | Auth token for the Worker |
| `CF_API_TOKEN` | Control | CF API token for KV operations |
| `CF_ACCOUNT_ID` | Control | CF account ID |

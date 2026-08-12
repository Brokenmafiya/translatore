#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKER_DIR="$SCRIPT_DIR/worker"
TOKENS_FILE="${TOKENS_FILE:-./tokens.json}"

echo "================================================================="
echo "PROXY TRANSLATOR v2 — DEPLOYMENT"
echo "================================================================="

# Check deps
command -v cargo >/dev/null || { echo "❌ cargo not found"; exit 1; }
command -v jq >/dev/null || { echo "❌ jq not found"; exit 1; }
rustup target list --installed | grep -q wasm32-unknown-unknown || {
    echo "Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
}

# Generate auth token if needed
AUTH_TOKEN_FILE="$HOME/.translatore/auth_token"
if [ ! -f "$AUTH_TOKEN_FILE" ]; then
    mkdir -p "$HOME/.translatore"
    AUTH_TOKEN=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 40)
    echo "$AUTH_TOKEN" > "$AUTH_TOKEN_FILE"
    chmod 600 "$AUTH_TOKEN_FILE"
    echo "🔐 Generated auth token: $AUTH_TOKEN_FILE"
else
    AUTH_TOKEN=$(cat "$AUTH_TOKEN_FILE")
    echo "🔐 Using existing auth token"
fi

# Build Worker
echo ""
echo "📦 Building Worker (Rust → WASM)..."
cd "$WORKER_DIR"

# Install worker-build if needed
cargo install worker-build 2>/dev/null || true

worker-build --release
echo "✅ Worker built"

# Deploy to Cloudflare accounts
echo ""
echo "🚀 Deploying to Cloudflare Workers..."

COMPUTE_TOKENS=("631" "638" "717" "718" "733" "782" "1303" "1308" "1352" "1396" "1618")
DEPLOYED=()

for token_id in "${COMPUTE_TOKENS[@]}"; do
    token=$(jq -r ".[] | select(.id == \"$token_id\") | .key" "$TOKENS_FILE" 2>/dev/null)
    [ -z "$token" ] && continue

    accounts=$(curl -s "https://api.cloudflare.com/client/v4/accounts" \
        -H "Authorization: Bearer $token" | jq -r '.result[]?.id // empty')

    for account_id in $accounts; do
        [ -z "$account_id" ] && continue

        echo "  Deploying to account $account_id (token $token_id)..."

        # Set the auth secret
        curl -s -X PUT \
            "https://api.cloudflare.com/client/v4/accounts/$account_id/workers/scripts/proxy-translator/secrets" \
            -H "Authorization: Bearer $token" \
            -H "Content-Type: application/json" \
            -d "{\"name\":\"PROXY_AUTH\",\"text\":\"$AUTH_TOKEN\",\"type\":\"secret_text\"}" \
            >/dev/null 2>&1

        # Deploy via wrangler
        CLOUDFLARE_API_TOKEN="$token" CLOUDFLARE_ACCOUNT_ID="$account_id" \
            npx -y wrangler deploy --name proxy-translator 2>&1 | tail -3

        DEPLOYED+=("https://proxy-translator.$account_id.workers.dev")
        sleep 1
    done
done

echo ""
echo "================================================================="
echo "DEPLOYMENT COMPLETE"
echo "================================================================="
echo "Auth token: $AUTH_TOKEN"
echo "Deployed to ${#DEPLOYED[@]} workers:"
for url in "${DEPLOYED[@]}"; do
    echo "  → $url"
done
echo ""
echo "Start the local agent:"
echo "  PT_WORKER_URL=${DEPLOYED[0]:-'https://your-worker.workers.dev'} \\"
echo "  PT_AUTH_TOKEN=$AUTH_TOKEN \\"
echo "  cargo run --manifest-path $SCRIPT_DIR/agent/Cargo.toml"
echo ""
echo "Start the control plane:"
echo "  CF_API_TOKEN=<token> CF_ACCOUNT_ID=<id> \\"
echo "  cargo run --manifest-path $SCRIPT_DIR/control/Cargo.toml"

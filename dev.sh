#!/usr/bin/env bash
set -euo pipefail

CONTAINER_NAME="spawn-dev"
DEV_IMAGE="spawn-dev:latest"
SANDBOX_DIR="$HOME/.claude_sandbox"

# --- Stop flow ---
if [[ "${1:-}" == "stop" ]]; then
    echo "Stopping $CONTAINER_NAME..."
    container rm -f "$CONTAINER_NAME" 2>/dev/null || true
    echo "Done."
    exit 0
fi

# --- Preflight ---
if ! command -v container &>/dev/null; then
    echo "Error: Apple Container CLI not found."
    echo "Install with: brew install container"
    exit 1
fi

# Ensure sandbox directory exists
mkdir -p "$SANDBOX_DIR"

# --- Check for existing container ---
container_status() {
    container inspect "$CONTAINER_NAME" 2>/dev/null \
        | python3 -c "import sys,json; print(json.load(sys.stdin)[0].get('status',''))" 2>/dev/null || true
}

get_ip() {
    container inspect "$CONTAINER_NAME" 2>/dev/null \
        | python3 -c "
import sys, json
data = json.load(sys.stdin)
addr = data[0].get('networks', [{}])[0].get('address', '')
print(addr.split('/')[0])
" 2>/dev/null || true
}

STATUS=$(container_status)

if [[ "$STATUS" == "running" ]]; then
    echo "Reusing running $CONTAINER_NAME container."
    IP=$(get_ip)
    [[ -n "$IP" ]] && echo "Container IP: $IP"
    exec container exec -it -w /work "$CONTAINER_NAME" su -l claude -c "cd /work && exec bash -li"
fi

if [[ -n "$STATUS" ]]; then
    echo "Removing stopped $CONTAINER_NAME container..."
    container rm -f "$CONTAINER_NAME" 2>/dev/null || true
fi

# --- Build image if missing ---
if ! container image list 2>/dev/null | grep -q "spawn-dev"; then
    echo "Building $DEV_IMAGE..."
    container build -t "$DEV_IMAGE" -f Dockerfile.dev .
fi

# --- Run container ---
echo "Starting $CONTAINER_NAME..."
container run -d \
    --name "$CONTAINER_NAME" \
    --volume "$(pwd):/work" \
    --volume "$SANDBOX_DIR:/home/claude/.claude" \
    -w /work \
    "$DEV_IMAGE" \
    sleep infinity

IP=$(get_ip)
echo ""
echo "Apple Container started."
[[ -n "$IP" ]] && echo "Container IP: $IP"
echo "Shell ready at /work"
echo ""

exec container exec -it -w /work "$CONTAINER_NAME" su -l claude -c "cd /work && exec bash -li"

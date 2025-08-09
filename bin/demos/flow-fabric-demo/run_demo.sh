#!/usr/bin/env bash
set -euo pipefail

CLEANED_UP=0
cleanup() {
  if [ $CLEANED_UP -eq 1 ]; then return; fi
  CLEANED_UP=1
  if [ -n "${FABRIC_PID:-}" ] && kill -0 "$FABRIC_PID" 2>/dev/null; then
    kill "$FABRIC_PID" 2>/dev/null || true
    wait "$FABRIC_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

# Flow + Fabric demo runner.
# Usage:
#   ./run_demo.sh <endpoint> <goal> [node_count] [--direct] [--nifs]
# Examples:
#   ./run_demo.sh "https://dummyjson.com/products/1" "fetch product" 4
#   ./run_demo.sh "https://api.data.gov.sg/v1/transport/taxi-availability" "analyse taxi availability" --direct
#   ./run_demo.sh "https://..." "goal" 5 --nifs
#   ./run_demo.sh "https://..." "goal" 5 --nifs --direct

if [ $# -lt 2 ]; then
  echo "Usage: $0 <endpoint> <goal> [node_count] [--direct] [--nifs]" >&2
  exit 1
fi

ENDPOINT="$1"; shift
goal_raw="$1"; shift
GOAL="$goal_raw"
NODE_COUNT=3
DIRECT=0
ENABLE_NIFS_FLAG=0

# Parse remaining args
for arg in "$@"; do
  case "$arg" in
    --direct) DIRECT=1 ;;
    --nifs|--enable-nifs) ENABLE_NIFS_FLAG=1 ;;
    ''|*[!0-9]*) # non-numeric, ignore (already handled flags)
      ;;
    *) NODE_COUNT="$arg" ;;
  esac
done

FABRIC_DIR="$(dirname "$0")/../../gtr-fabric"
FABRIC_DIR="$(cd "$FABRIC_DIR" && pwd)"

export DEMO_NODE_COUNT="$NODE_COUNT"
export ENABLE_NIFS="$ENABLE_NIFS_FLAG"

DEFAULT_PORT=4000
MAX_PORT=4010
FABRIC_PORT=${FABRIC_PORT:-$DEFAULT_PORT}

start_fabric() {
  local port=$1
  ( cd "$FABRIC_DIR" && PORT=$port MIX_ENV=dev elixir -S mix run --no-halt >/tmp/gtr_fabric_demo.log 2>&1 & echo $! > /tmp/gtr_fabric_pid )
  FABRIC_PID=$(cat /tmp/gtr_fabric_pid)
  sleep 0.8
  if ! kill -0 "$FABRIC_PID" 2>/dev/null; then
    return 1
  fi
  if grep -q 'eaddrinuse' /tmp/gtr_fabric_demo.log; then
    kill "$FABRIC_PID" 2>/dev/null || true
    wait "$FABRIC_PID" 2>/dev/null || true
    return 2
  fi
  if grep -q 'Cannot execute "mix run"' /tmp/gtr_fabric_demo.log; then
    kill "$FABRIC_PID" 2>/dev/null || true
    wait "$FABRIC_PID" 2>/dev/null || true
    return 3
  fi
  echo "Launched fabric on port $port (pid=$FABRIC_PID)"
  return 0
}

FOUND=0
for p in $(seq $FABRIC_PORT $MAX_PORT); do
  if start_fabric $p; then
    FABRIC_PORT=$p
    FOUND=1
    break
  fi
  sleep 0.3
done

if [ $FOUND -ne 1 ]; then
  echo "Failed to start fabric on any port in range $FABRIC_PORT-$MAX_PORT" >&2
  exit 1
fi

export FABRIC_PROVIDER_URL="http://localhost:${FABRIC_PORT}/providers"

printf "Waiting for fabric"
for i in {1..30}; do
  if curl -fsS http://localhost:${FABRIC_PORT}/health >/dev/null 2>&1; then
    echo " ready (port ${FABRIC_PORT})"
    break
  fi
  printf "."; sleep 1
  if [ $i -eq 30 ]; then
    echo "\nFabric did not become healthy (port ${FABRIC_PORT})" >&2
    exit 1
  fi
done

echo "Providers:"; curl -s http://localhost:${FABRIC_PORT}/providers; echo

echo "Running flow-fabric demo (nodes=$NODE_COUNT, direct=$DIRECT, nifs=$ENABLE_NIFS_FLAG)..."
pushd "$(dirname "$0")" >/dev/null
ARGS=(--endpoint "$ENDPOINT" --goal "$GOAL")
if [ $DIRECT -eq 1 ]; then ARGS+=(--direct); fi
cargo run --quiet -- "${ARGS[@]}"
STATUS=$?
popd >/dev/null

exit $STATUS

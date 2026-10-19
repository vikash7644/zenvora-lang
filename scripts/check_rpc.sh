#!/usr/bin/env sh
set -eu

: "\${RPC_URL:?Set RPC_URL, for example http://127.0.0.1:8545/}"
case "$RPC_URL" in
  http://*) ;;
  *) echo "FAIL: current node uses plain HTTP on loopback; do not send tokens over an untrusted network." >&2; exit 1 ;;
esac

rpc_call() {
  method="$1"
  params="$2"
  id="$3"
  if [ -n "\${RPC_TOKEN:-}" ]; then
    curl --fail --silent --show-error --max-time 5 \
      -H 'Content-Type: application/json' \
      -H "Authorization: Bearer $RPC_TOKEN" \
      --data "{\"jsonrpc\":\"2.0\",\"id\":$id,\"method\":\"$method\",\"params\":$params}" \
      "$RPC_URL"
  else
    curl --fail --silent --show-error --max-time 5 \
      -H 'Content-Type: application/json' \
      --data "{\"jsonrpc\":\"2.0\",\"id\":$id,\"method\":\"$method\",\"params\":$params}" \
      "$RPC_URL"
  fi
  printf '\n'
}

echo "Checking chain_getInfo..."
INFO="$(rpc_call chain_getInfo '[]' 1)"
printf '%s\n' "$INFO" | grep -q '"result"' || { echo "FAIL: chain_getInfo has no result" >&2; exit 1; }
printf '%s\n' "$INFO" | grep -q '"chainId"' || { echo "FAIL: chainId missing" >&2; exit 1; }
printf '%s\n' "$INFO" | grep -q '"height"' || { echo "FAIL: height missing" >&2; exit 1; }
echo "PASS: chain_getInfo returned chainId and height"

if [ -n "\${RPC_TEST_ADDRESS:-}" ]; then
  echo "Checking chain_getBalance..."
  BALANCE="$(rpc_call chain_getBalance "{\"address\":\"$RPC_TEST_ADDRESS\"}" 2)"
  printf '%s\n' "$BALANCE" | grep -q '"balance"' || { echo "FAIL: balance missing" >&2; exit 1; }
  printf '%s\n' "$BALANCE" | grep -q '"nonce"' || { echo "FAIL: nonce missing" >&2; exit 1; }
  echo "PASS: chain_getBalance returned balance and nonce"
else
  echo "SKIP: set RPC_TEST_ADDRESS to check a known address"
fi

if [ -n "\${RPC_TEST_HEIGHT:-}" ]; then
  HEIGHT="$RPC_TEST_HEIGHT"
else
  HEIGHT="$(printf '%s\n' "$INFO" | sed -n 's/.*"height":[[:space:]]*\([0-9][0-9]*\).*/\1/p')"
fi
case "$HEIGHT" in ''|*[!0-9]*) echo "FAIL: could not determine a numeric block height" >&2; exit 1 ;; esac
echo "Checking chain_getBlock at height $HEIGHT..."
BLOCK="$(rpc_call chain_getBlock "{\"height\":$HEIGHT}" 3)"
printf '%s\n' "$BLOCK" | grep -q '"height"' || { echo "FAIL: block height missing" >&2; exit 1; }
echo "PASS: chain_getBlock returned a block response"
echo "NOTE: read-only smoke checks only; not a consensus or production-security audit."

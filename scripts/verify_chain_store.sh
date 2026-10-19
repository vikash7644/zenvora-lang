#!/usr/bin/env sh
# Verify a local store by asking the real Zenvora CLI to load and replay it.
set -eu

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "Usage: $0 <zv-binary> <store.zvb> <chain-id> [expected-height]" >&2
  exit 2
fi

ZV_BIN="$1"
STORE="$2"
CHAIN_ID="$3"
EXPECTED_HEIGHT="${4:-}"

case "$CHAIN_ID" in ''|*[!0-9]*|0) echo "FAIL: chain ID must be a positive integer" >&2; exit 2 ;; esac
if [ -n "$EXPECTED_HEIGHT" ]; then
  case "$EXPECTED_HEIGHT" in *[!0-9]*|'') echo "FAIL: expected height must be a non-negative integer" >&2; exit 2 ;; esac
fi
[ -x "$ZV_BIN" ] || { echo "FAIL: Zenvora CLI is not executable: $ZV_BIN" >&2; exit 2; }
[ -f "$STORE" ] || { echo "FAIL: block store not found: $STORE" >&2; exit 2; }

# --chain-store-info calls BlockStore::load_from_file, which replays the full store and
# rejects invalid signatures, linkage, transaction roots, receipt roots, and state roots.
INFO="$("$ZV_BIN" --chain-store-info "$STORE" "$CHAIN_ID")" || {
  echo "FAIL: Zenvora could not replay-verify the block store" >&2
  exit 1
}
printf '%s\n' "$INFO"

printf '%s\n' "$INFO" | awk -v expected_chain="$CHAIN_ID" -v expected_height="$EXPECTED_HEIGHT" '
{
  for (i = 1; i <= NF; i++) {
    split($i, part, "=")
    value[part[1]] = part[2]
  }
}
END {
  if (value["chain_id"] != expected_chain) {
    print "FAIL: chain ID mismatch" > "/dev/stderr"; exit 1
  }
  if (value["blocks"] !~ /^[0-9]+$/ || value["height"] !~ /^[0-9]+$/) {
    print "FAIL: missing numeric block count or height" > "/dev/stderr"; exit 1
  }
  if (value["blocks"] != value["height"] + 1) {
    print "FAIL: block count and zero-based height are inconsistent" > "/dev/stderr"; exit 1
  }
  if (expected_height != "" && value["height"] != expected_height) {
    print "FAIL: unexpected tip height" > "/dev/stderr"; exit 1
  }
  if (value["state_root"] !~ /^[0-9a-f]+$/ || length(value["state_root"]) != 64) {
    print "FAIL: state root missing or malformed" > "/dev/stderr"; exit 1
  }
  if (value["tip_hash"] !~ /^[0-9a-f]+$/ || length(value["tip_hash"]) != 64) {
    print "FAIL: tip hash missing or malformed" > "/dev/stderr"; exit 1
  }
  print "PASS: block store replay verification succeeded"
  print "PASS: chain ID, height, state root, and tip hash are consistent"
}'

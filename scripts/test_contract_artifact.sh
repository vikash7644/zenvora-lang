#!/usr/bin/env bash
set -euo pipefail

bin="./target/release/zv"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/counter.zv" <<'ZV'
contract Counter {
state count := 0
action increment(amount) {
count := count + amount
}
view current() {
return count
}
}
ZV

"$bin" --contract-artifact "$tmp/counter.zv" "$tmp/counter.artifact.json"
python3 - "$tmp/counter.artifact.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    artifact = json.load(stream)
assert artifact["format"] == "ZENVORA_CONTRACT_ARTIFACT_V1"
assert artifact["contract"] == "Counter"
assert len(artifact["source_sha256"]) == 64
assert artifact["abi"]["states"] == [{"name": "count", "type": "integer"}]
methods = {item["name"]: item["kind"] for item in artifact["abi"]["methods"]}
assert methods == {"increment": "action", "current": "view"}, methods
assert artifact["bytecode_v2"].startswith("ZENVORA_BYTECODE 2\n")
print("Contract artifact integration passed: ABI, source hash, and verified Bytecode v2.")
PY

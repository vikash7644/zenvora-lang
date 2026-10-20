#!/usr/bin/env bash
set -euo pipefail

bin="./target/release/zv"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/counter.zv" <<'ZV'
contract Counter {
state count := 0
action add(amount) {
if amount > 0 {
count := count + amount
} otherwise {
count := count - amount
}
}
view current() {
return count
}
}
ZV

"$bin" --contract-run "$tmp/counter.zv" counter "$tmp/counter.snapshot" action 'add(3)' 100000 > "$tmp/action.out"
test -s "$tmp/counter.snapshot"
grep -q '^gas_used=[1-9][0-9]*$' "$tmp/action.out"
"$bin" --contract-run "$tmp/counter.zv" counter "$tmp/counter.snapshot" action 'add(-2)' 100000 > "$tmp/action2.out"
"$bin" --contract-run "$tmp/counter.zv" counter "$tmp/counter.snapshot" view 'current()' 100000 > "$tmp/view.out"
grep -qx '1' "$tmp/view.out"

before="$(sha256sum "$tmp/counter.snapshot" | cut -d ' ' -f 1)"
if "$bin" --contract-run "$tmp/counter.zv" counter "$tmp/counter.snapshot" action 'add(9)' 21001 > "$tmp/out-of-gas.out" 2>&1; then
  echo "out-of-gas action unexpectedly succeeded" >&2
  exit 1
fi
after="$(sha256sum "$tmp/counter.snapshot" | cut -d ' ' -f 1)"
if [[ "$before" != "$after" ]]; then
  echo "out-of-gas action changed persisted state" >&2
  exit 1
fi

echo "Contract CLI integration passed: persistent actions, read-only view, deterministic gas, and rollback."

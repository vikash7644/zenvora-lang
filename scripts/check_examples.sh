#!/usr/bin/env bash
set -euo pipefail

# The release CLI is built earlier in the CI job. Execute every checked-in
# .qv example, including module-only files, so stale or broken examples fail CI.
shopt -s nullglob
examples=(examples/*.qv examples/modules/*.qv)
if (("${#examples[@]}" == 0)); then
  echo "No .qv examples found" >&2
  exit 1
fi

for example in "${examples[@]}"; do
  echo "Checking runnable example: ${example}"
  ./target/release/qorvynt "${example}" >/dev/null
done

echo "All ${#examples[@]} QORVYNT example files executed successfully."

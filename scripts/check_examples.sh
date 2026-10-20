#!/usr/bin/env bash
set -euo pipefail

# The release CLI is built earlier in the CI job. Execute every checked-in
# .zv example, including module-only files, so stale or broken examples fail CI.
shopt -s nullglob
examples=(examples/*.zv examples/modules/*.zv)
if (("${#examples[@]}" == 0)); then
  echo "No .zv examples found" >&2
  exit 1
fi

for example in "${examples[@]}"; do
  echo "Checking runnable example: ${example}"
  ./target/release/zv "${example}" >/dev/null
done

echo "All ${#examples[@]} Zenvora Lang example files executed successfully."

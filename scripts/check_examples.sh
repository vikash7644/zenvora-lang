#!/usr/bin/env bash
set -euo pipefail

# The release CLI is built earlier in the CI job. Execute every checked-in
# .qv example, including module-only files, so stale or broken examples fail CI.
shopt -s nullglob
examples=(examples/*.qv examples/modules/*.qv examples/*.zv examples/modules/*.zv)
if (("${#examples[@]}" == 0)); then
  echo "No QORVYNT source examples found" >&2
  exit 1
fi

for example in "${examples[@]}"; do
  echo "Checking runnable example: ${example}"
  ./target/release/zv "${example}" >/dev/null
done

echo "All ${#examples[@]} QORVYNT source example files executed successfully."

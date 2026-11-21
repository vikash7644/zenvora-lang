#!/usr/bin/env bash
set -euo pipefail

legacy_sources="$(git ls-files '*.zv')"
if [[ -n "$legacy_sources" ]]; then
  printf 'Legacy .zv source files are not allowed in QORVYNT:\n%s\n' "$legacy_sources" >&2
  exit 1
fi

if ! git ls-files '*.qv' | grep -q .; then
  echo "No tracked .qv QORVYNT source files were found." >&2
  exit 1
fi

echo "QORVYNT source extension check passed: .qv is canonical and no .zv sources are tracked."

#!/usr/bin/env python3
"""Static and JavaScript syntax checks for the standalone browser game page."""
from html.parser import HTMLParser
from pathlib import Path
import re
import subprocess
import tempfile

PAGE = Path(__file__).resolve().parents[1] / "website" / "game-2d.html"
source = PAGE.read_text(encoding="utf-8")

required = [
    'id="game"', 'id="reset"', 'id="status"', 'id="moves"',
    'keydown', 'data-dir', 'LEVEL COMPLETE', 'getContext("2d")',
    'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight',
]
missing = [item for item in required if item not in source]
if missing:
    raise SystemExit("Missing game page requirements: " + ", ".join(missing))

scripts = re.findall(r"<script>(.*?)</script>", source, flags=re.DOTALL)
if len(scripts) != 1:
    raise SystemExit(f"Expected exactly one inline game script, found {len(scripts)}")

with tempfile.NamedTemporaryFile("w", suffix=".js", encoding="utf-8", delete=False) as handle:
    handle.write(scripts[0])
    script_path = handle.name
try:
    subprocess.run(["node", "--check", script_path], check=True)
finally:
    Path(script_path).unlink(missing_ok=True)

print("Browser game page checks passed: required controls/rendering hooks present and JavaScript syntax valid.")

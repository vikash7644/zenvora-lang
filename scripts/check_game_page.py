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
    'id="zv-source"', 'id="build-level"', 'id="scene-file"', 'function parseLevel', 'ZENVORA_GAME_SCENE_V1',
    'LEVEL BUILT', 'Duplicate declaration', 'overlaps spawn or goal',
    'id="play-vm-script"', 'VM SCRIPT PASSED', 'incomingScript', 'incomingTrace', 'VM STATE TRACE MISMATCH',
    'declarations.join("\\n")',
    'Scene contains duplicate walls', 'Scene wall overlaps spawn or goal',
    'Scene walls must be valid integer coordinates', 'Scene spawn and goal must be integer coordinates',
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

level_source = Path(__file__).resolve().parents[1] / "examples" / "game-2d-level.zv"
level_text = level_source.read_text(encoding="utf-8")
for declaration in ("define game_width := 10", "define game_height := 8",
                    "define player_start_x := 1", "define goal_x := 8",
                    'define wall_1 := "3,1"'):
    if declaration not in level_text:
        raise SystemExit(f"Missing Zenvora game-level declaration: {declaration}")

exporter = Path(__file__).resolve().parent / "export_game_level.py"
if not exporter.is_file():
    raise SystemExit("Missing Zenvora VM scene exporter script")

scripted_source = Path(__file__).resolve().parents[1] / "examples" / "game-2d-scripted.zv"
if not scripted_source.is_file():
    raise SystemExit("Missing Zenvora VM scripted-game fixture")
for declaration in ('grid_step(', 'show "script"', 'show "state"', 'show "down"', 'show "right"', 'repeat step < 3'):
    if declaration not in scripted_source.read_text(encoding="utf-8"):
        raise SystemExit(f"Missing Zenvora scripted-game command: {declaration}")

print("Browser game checks passed: Canvas, Zenvora level/script bridge, VM scene import, source fixtures, and JavaScript syntax valid.")

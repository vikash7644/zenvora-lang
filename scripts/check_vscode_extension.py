#!/usr/bin/env python3
"""Validate the checked-in VS Code language extension metadata and TextMate regexes."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXT = ROOT / "editors" / "vscode"
PACKAGE = EXT / "package.json"
GRAMMAR = EXT / "syntaxes" / "zenvora.tmLanguage.json"
CONFIG = EXT / "language-configuration.json"


def fail(message: str) -> None:
    print(f"QORVYNT editor validation failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"{path.relative_to(ROOT)}: {exc}")


package = load_json(PACKAGE)
grammar = load_json(GRAMMAR)
config = load_json(CONFIG)

languages = package.get("contributes", {}).get("languages", [])
zenvora = next((item for item in languages if item.get("id") == "zenvora"), None)
if not zenvora:
    fail("package.json does not register language id 'zenvora'")
if ".zv" not in zenvora.get("extensions", []):
    fail("language id 'zenvora' must register the .zv extension")
if zenvora.get("configuration") != "./language-configuration.json":
    fail("language configuration path does not match the checked-in file")

grammars = package.get("contributes", {}).get("grammars", [])
grammar_entry = next((item for item in grammars if item.get("language") == "zenvora"), None)
if not grammar_entry:
    fail("package.json does not register a grammar for 'zenvora'")
if grammar_entry.get("scopeName") != grammar.get("scopeName"):
    fail("package.json and TextMate grammar scopeName differ")
if grammar_entry.get("path") != "./syntaxes/zenvora.tmLanguage.json":
    fail("package.json grammar path does not match the checked-in file")

checked = 0


def check_patterns(value, location="grammar"):
    global checked
    if isinstance(value, dict):
        for key, child in value.items():
            if key in {"match", "begin", "end", "while"} and isinstance(child, str):
                try:
                    re.compile(child)
                except re.error as exc:
                    fail(f"invalid regex at {location}.{key}: {child!r}: {exc}")
                checked += 1
            else:
                check_patterns(child, f"{location}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            check_patterns(child, f"{location}[{index}]")


check_patterns(grammar)
if not isinstance(config.get("comments"), dict) or config["comments"].get("lineComment") != "#":
    fail("language configuration must define # line comments")
print(f"QORVYNT VS Code extension validated: JSON metadata valid; {checked} TextMate regexes compile; .zv language registration and scope match.")

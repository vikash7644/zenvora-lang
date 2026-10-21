#!/usr/bin/env python3
"""Run a Zenvora level program in the real CLI/VM and export its scene JSON."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="Zenvora .zv level source")
    parser.add_argument("output", type=Path, help="destination JSON scene descriptor")
    parser.add_argument("--zv", default="target/release/zv", help="Zenvora CLI executable")
    args = parser.parse_args()

    result = subprocess.run(
        [args.zv, str(args.source)],
        check=True,
        capture_output=True,
        text=True,
    )
    lines = result.stdout.splitlines()
    if not lines or lines[0] != "ZENVORA_GAME_LEVEL_V1":
        raise SystemExit("Source did not emit ZENVORA_GAME_LEVEL_V1 from the Zenvora VM")

    try:
        cursor = 1

        def take(expected):
            nonlocal cursor
            if cursor >= len(lines) or lines[cursor] != expected:
                actual = lines[cursor] if cursor < len(lines) else "<end of output>"
                raise ValueError(f"expected {expected!r}, got {actual!r}")
            cursor += 1

        def integer(label):
            nonlocal cursor
            take(label)
            value = int(lines[cursor])
            cursor += 1
            return value

        take("size")
        width, height = int(lines[cursor]), int(lines[cursor + 1])
        cursor += 2
        take("spawn")
        spawn = {"x": int(lines[cursor]), "y": int(lines[cursor + 1])}
        cursor += 2
        take("goal")
        goal = {"x": int(lines[cursor]), "y": int(lines[cursor + 1])}
        cursor += 2
        walls = []
        while cursor < len(lines):
            take("wall")
            x, y = lines[cursor].split(",", 1)
            walls.append({"x": int(x), "y": int(y)})
            cursor += 1
        if not (4 <= width <= 20 and 4 <= height <= 16):
            raise ValueError("world size must be 4–20 columns and 4–16 rows")
        def inside(point):
            return 0 <= point["x"] < width and 0 <= point["y"] < height
        if not inside(spawn) or not inside(goal):
            raise ValueError("spawn and goal must be inside the world")
        if any(not inside(wall) for wall in walls):
            raise ValueError("all walls must be inside the world")
        if spawn in walls or goal in walls:
            raise ValueError("a wall overlaps spawn or goal")
        if len({(wall["x"], wall["y"]) for wall in walls}) != len(walls):
            raise ValueError("duplicate wall coordinate")
        if cursor != len(lines):
            raise ValueError("unexpected trailing VM output")
    except (ValueError, IndexError) as error:
        raise SystemExit(f"Invalid Zenvora game level output: {error}") from error

    scene = {
        "format": "ZENVORA_GAME_SCENE_V1",
        "source": str(args.source),
        "world": {"width": width, "height": height},
        "spawn": spawn,
        "goal": goal,
        "walls": walls,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(scene, indent=2) + "\n", encoding="utf-8")
    print(f"Exported {len(walls)} walls to {args.output}")


if __name__ == "__main__":
    main()

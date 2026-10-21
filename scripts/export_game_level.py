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
        script = []
        trace = []
        while cursor < len(lines):
            if lines[cursor] == "script":
                cursor += 1
                while cursor < len(lines):
                    take("move")
                    if cursor >= len(lines):
                        raise ValueError("missing direction after move")
                    direction = lines[cursor]
                    cursor += 1
                    if direction not in {"up", "down", "left", "right"}:
                        raise ValueError(f"unsupported move direction: {direction!r}")
                    take("state")
                    if cursor + 1 >= len(lines):
                        raise ValueError("missing x/y state after movement")
                    trace.append({"x": int(lines[cursor]), "y": int(lines[cursor + 1])})
                    cursor += 2
                    script.append(direction)
                break
            take("wall")
            if cursor >= len(lines):
                raise ValueError("missing wall coordinate")
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
        if len(script) > 10000:
            raise ValueError("script exceeds 10000 moves")
        if len(trace) != len(script):
            raise ValueError("script and VM state trace lengths differ")
        occupied = {(wall["x"], wall["y"]) for wall in walls}
        position = dict(spawn)
        deltas = {"up": (0, -1), "down": (0, 1), "left": (-1, 0), "right": (1, 0)}
        for index, (direction, vm_state) in enumerate(zip(script, trace), start=1):
            dx, dy = deltas[direction]
            candidate = {"x": position["x"] + dx, "y": position["y"] + dy}
            if not inside(candidate) or (candidate["x"], candidate["y"]) in occupied:
                raise ValueError(f"script move {index} collides with a wall or world boundary")
            if vm_state != candidate:
                raise ValueError(f"VM grid_step state mismatch at move {index}: expected {candidate}, got {vm_state}")
            position = vm_state
        if script and position != goal:
            raise ValueError("script must finish at the goal")
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
        "script": script,
        "trace": trace,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(scene, indent=2) + "\n", encoding="utf-8")
    print(f"Exported {len(walls)} walls to {args.output}")


if __name__ == "__main__":
    main()

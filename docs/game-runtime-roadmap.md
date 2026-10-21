# Zenvora Game Runtime Roadmap

Status: experimental source-to-scene prototype. A Zenvora program can execute in the real CLI/VM, emit a versioned scene descriptor, and have that descriptor loaded by the Canvas game. Arbitrary Zenvora game functions are not yet executed by the graphical runtime.

## Verified baseline

- `src/game_math.rs` provides deterministic integer vectors, rectangles, input snapshots, and a headless `GridWorld` with unit tests.
- The console-grid and ASCII wireframe examples run through the Zenvora VM; the tests compare their source and experimental Bytecode v2 output.
- `examples/game-2d-level.zv` defines a level and emits a `ZENVORA_GAME_LEVEL_V1` protocol using normal Zenvora `define` and `show` statements.
- `scripts/export_game_level.py` executes source with the real `zv` CLI, validates the VM output, and writes a `ZENVORA_GAME_SCENE_V1` JSON descriptor.
- `examples/game-2d-scripted.zv` emits a 12-step movement script using Zenvora `repeat` loops, in addition to the level and obstacles.
- The exporter validates every emitted move against bounds and walls and requires a non-empty script to finish at the goal.
- `website/game-2d.html` imports the VM-exported scene JSON and can replay the VM-emitted commands in its Canvas viewport; keyboard and touch play remain available.
- CI runs the source through the release CLI, checks the exported script and its final goal position, and validates browser JavaScript syntax and bridge controls.

This is a working source → real VM → validated scene/script JSON → graphical replay prototype, not a production engine. The browser still owns input, collision enforcement, timing, and drawing; the sample Zenvora program emits a deterministic command sequence rather than an arbitrary per-frame game function executed inside the graphical VM.

## Reproduce the compiler-backed level path

Build the CLI, export a scene through the Zenvora VM, then load the JSON file in the browser game:

```sh
cargo build --release --bin zv
python scripts/export_game_level.py examples/game-2d-level.zv game-scene.json --zv ./target/release/zv
python scripts/export_game_level.py examples/game-2d-scripted.zv game-script.json --zv ./target/release/zv
```

Open `website/game-2d.html` and choose `game-script.json` in “Load VM-exported scene JSON”. Then press **Play Zenvora VM script** to replay the moves emitted by the Zenvora program. The non-scripted `game-2d-level.zv` example remains supported.

## Next implementation gates

1. Replace the custom `show`-output protocol with a stable compiler-owned scene artifact schema and validation/versioning.
2. Expose deterministic game input/state/collision primitives through the Zenvora VM and maintain source/Bytecode v2 execution parity.
3. Execute a Zenvora-defined per-frame update function in the runtime, then pass the resulting state to the renderer; test deterministic replays.
4. Add sprites/assets, a proper frame loop, audio, and documented browser/native runtime paths.
5. Add end-to-end tests proving `.zv` source → actual VM execution → scene/state artifact → rendered game behavior.

## Phase 3 — 3D

- Add camera and transform math, mesh/material representation, depth-tested rendering, asset loading, and input.
- Keep the ASCII wireframe example as a language regression fixture only.
- Require an actual rendered scene and documented hardware/software requirements before claiming 3D support.

## Separate Web3 / Layer 3 work

Game rendering does not itself provide Web3 capability. A Web3 toolchain needs a defined target chain, RPC/client integration, wallet/signing flow, ABI/artifact compatibility, deployment tooling, and testnet integration. Layer 3 additionally needs a specific rollup/app-chain design, settlement/data-availability assumptions, bridge/messaging design, and security review. The current contract CLI/artifact is experimental and is not a production deployment pipeline.

## Release gate

Do not describe Zenvora as a production graphical game engine, production smart-contract compiler, or Layer 3 deployment platform until end-to-end tests prove those capabilities on a clean environment.

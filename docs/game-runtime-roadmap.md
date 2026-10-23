# Zenvora Game Runtime Roadmap

Status: experimental source-to-scene prototype. A Zenvora program can execute in the real CLI/VM, emit a versioned scene descriptor, and have that descriptor loaded by the Canvas game. Arbitrary Zenvora game functions are not yet executed by the graphical runtime.

## Verified baseline

- `src/game_math.rs` provides deterministic integer vectors, rectangles, input snapshots, and a headless `GridWorld` with unit tests.
- The VM has a checked `grid_step(x, y, direction, width, height, walls)` builtin. It computes a deterministic next grid position, blocks walls/world edges, validates dimensions and argument types, and returns a map with `x`, `y`, and `moved`. Rust unit tests cover movement, wall/boundary collisions, invalid directions, and unsafe world sizes.
- `examples/game-2d-scripted.zv` calls `grid_step` from Zenvora `repeat` loops, so movement state is computed by the VM, not merely described as browser-side commands.
- `scripts/export_game_level.py` runs source through the real `zv` CLI, validates each VM-reported position against the expected move, and emits the versioned scene, movement script, and per-step state trace.
- `website/game-2d.html` imports the exported scene and replays commands while checking every rendered player position against the VM state trace; keyboard/touch controls remain available for manual play.
- CI checks the VM-produced trace, goal position, builtin behavior, JavaScript syntax, and the source-to-Canvas bridge.

This is a stronger working source → real VM game-step logic → validated scene/state trace → graphical replay prototype, not a production engine. The renderer still uses browser JavaScript for Canvas drawing, timing, and manual input, and the current source format describes a scripted grid game rather than arbitrary per-frame game functions.

## Reproduce the compiler-backed level path

Build the CLI, export a scene through the Zenvora VM, then load the JSON file in the browser game:

```sh
cargo build --release --bin zv
python scripts/export_game_level.py examples/game-2d-level.zv game-scene.json --zv ./target/release/zv
python scripts/export_game_level.py examples/game-2d-scripted.zv game-script.json --zv ./target/release/zv
```

Open `website/game-2d.html` and choose `game-script.json` in “Load VM-exported scene JSON”. Then press **Play Zenvora VM script** to replay the moves emitted by the Zenvora program. The non-scripted `game-2d-level.zv` example remains supported.

## Next implementation gates

1. Add a compiler-owned scene artifact command instead of relying on the current `show` protocol.
2. Extend the VM game API beyond one grid-step primitive: persistent world/entity state, input snapshots, collision queries, and deterministic frame updates; add source/Bytecode v2 parity coverage.
3. Execute a Zenvora-defined per-frame update function in the graphical runtime and pass its resulting state to the renderer.
4. Add sprites/assets, a proper frame loop, audio, and documented browser/native runtime paths.
5. Keep the end-to-end VM trace test as a gate before claiming broader 2D game-engine support.

## Phase 3 — 3D

- Add camera and transform math, mesh/material representation, depth-tested rendering, asset loading, and input.
- Keep the ASCII wireframe example as a language regression fixture only.
- Require an actual rendered scene and documented hardware/software requirements before claiming 3D support.

## Separate Web3 / Layer 3 work

Game rendering does not itself provide Web3 capability. A Web3 toolchain needs a defined target chain, RPC/client integration, wallet/signing flow, ABI/artifact compatibility, deployment tooling, and testnet integration. Layer 3 additionally needs a specific rollup/app-chain design, settlement/data-availability assumptions, bridge/messaging design, and security review. The current contract CLI/artifact is experimental and is not a production deployment pipeline.

## Release gate

Do not describe Zenvora as a production graphical game engine, production smart-contract compiler, or Layer 3 deployment platform until end-to-end tests prove those capabilities on a clean environment.

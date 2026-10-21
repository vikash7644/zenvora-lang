# Zenvora Game Runtime Roadmap

Status: experimental prototype. The repository has a browser Canvas renderer and a limited Zenvora-authored level-definition bridge. Arbitrary Zenvora game logic is not yet executed by the graphical runtime.

## Verified baseline

The compiler/VM executes the console-grid example, ASCII wireframe example, and headless simulation; CI checks source/experimental Bytecode v2 parity. `src/game_math.rs` supplies tested deterministic integer primitives and a headless `GridWorld`. `website/game-2d.html` renders a playable Canvas grid game with keyboard/touch input. `examples/game-2d-level.zv` supplies literal level declarations (dimensions, spawn, goal, and walls) consumed by the browser level bridge. The browser bridge is deliberately limited to literal `define` declarations; it does not run arbitrary Zenvora statements/functions inside the browser game. These are prototypes, not a production engine.

## Phase 1 — deterministic 2D simulation core

1. Define a stable game-facing API for vectors, rectangles, input snapshots, frame timing, and scene state.
2. Implement deterministic math/collision primitives in the runtime with unit tests (overlap edge cases, negative coordinates, zero-size rectangles, and fixed-point or explicitly bounded numeric semantics).
3. Expose only tested primitives as Zenvora built-ins; keep compiler parsing, source VM execution, and Bytecode v2 verification/execution in parity.
4. Add a headless simulation example with reproducible input frames and expected state/output.
5. Only after headless tests pass, add a real window/input/render backend behind a feature flag so default CLI builds remain portable.

## Phase 2 — graphical 2D

- Choose and document a cross-platform backend and its licensing/build requirements before adding dependencies.
- Add window creation, event/input handling, frame lifecycle, sprite/texture drawing, and asset loading.
- Test graceful behavior in headless CI; keep graphical smoke tests optional where display/GPU access is unavailable.
- Provide a minimal playable example and a documented install/build path.

## Phase 3 — 3D

- Add camera and transform math, mesh/material representation, depth-tested rendering, asset loading, and input.
- Keep the current ASCII wireframe example as a language regression fixture only.
- Require an actual rendered scene and documented hardware/software requirements before claiming 3D support.

## Separate Web3 / Layer 3 work

Game rendering does not itself provide Web3 capability. A Web3 toolchain needs a defined target chain, RPC/client integration, wallet/signing flow, ABI/artifact compatibility, deployment tooling, and testnet integration. Layer 3 additionally needs a specific rollup/app-chain design, settlement/data availability assumptions, bridge/messaging design, and security review. The current contract CLI/artifact is experimental and is not a production deployment pipeline.

## Release gate

Do not describe Zenvora as a graphical game engine, production smart-contract compiler, or Layer 3 deployment platform until end-to-end tests prove those capabilities on a clean environment.

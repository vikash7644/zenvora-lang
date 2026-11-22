# QORVYNT Developer Platform

## Goal

Create a language-first developer platform inspired by the information architecture of mature developer hubs: learn the core model, build from runnable examples, use developer tools, follow use-case guides, track releases, and get community support.

The reference Solana hub groups content around core primitives, working code, migration, use cases, developer products, courses, updates, and community. QORVYNT should use that organizational pattern without copying Solana's architecture or claiming feature parity.

## Non-negotiable language policy

- QORVYNT is the primary and canonical developer language.
- All official language examples use `.qv` and syntax accepted by the current QORVYNT compiler.
- Do not make Rust, JavaScript, Solidity, or TypeScript the required language for QORVYNT application developers.
- Host interfaces may temporarily use web-standard HTML/CSS/JavaScript until QORVYNT has a browser/web target. This is a delivery limitation, not the target developer experience.
- Do not claim the entire platform is implemented in QORVYNT while the compiler cannot compile browser UI, websites, or server processes from `.qv`.
- Mark every item as Implemented, Experimental, Partial, or Planned. Never describe a roadmap item as shipped.

## Current verified boundary from README and PROJECT_STATUS

- QORVYNT compiler and VM are currently hosted by a Rust implementation.
- The QVM CLI path uses the existing VM implementation; standalone VM extraction and clean-environment self-hosted bootstrap remain pending.
- The browser playground is a JavaScript demo runner, not the production compiler/QVM, and does not have full language parity.
- The contract/state/action/view syntax has local execution support but is not a decentralized chain program or production deployment artifact.
- Chain, wallet, token, DAO, DEX, game engine, graphics runtime, and AI-agent capabilities must remain roadmap items until independently implemented and tested.
- Current supported examples and language feature claims must be derived from executable `.qv` examples and compiler regression tests.

## Developer hub information architecture

1. **Learn** — language overview, installation, syntax, values, functions, modules, standard library, errors, and resource limits.
2. **Build** — runnable examples, project templates, CLI workflows, compiler/QVM pipeline, bytecode, and testing.
3. **Tools** — browser playground, CLI, QVM, VS Code extension, API references, and diagnostics.
4. **Use cases** — web services, games/graphics, AI agents, contracts, token tooling, DAO, and wallet integrations; each page starts as Planned until an actual supported runtime exists.
5. **Reference** — grammar, standard library, bytecode specification, ABI, compiler API, compatibility policy, and security model.
6. **Updates & community** — release notes, upgrade notices, contribution guide, discussions, issue templates, and examples gallery.

## Delivery milestones

### DHP-1: Public documentation hub
- Responsive developer landing page.
- Links to real source docs, executable examples, CLI guide, QVM notes, playground, and contribution guide.
- Clear status labels and prototype safety warnings.
- GitHub Pages validation and deployment.

### DHP-2: Compiler-backed browser playground
- Compile and run using the actual QORVYNT compiler/QVM, not a separately reimplemented JavaScript interpreter.
- Prefer a browser-compatible compiler build (for example, a WASM target) or a secure isolated hosted compiler service.
- Differential-test playground results against the canonical CLI.
- Add resource limits, cancellation, output bounds, and no secret/network access by default.

### DHP-3: QORVYNT-first project workflows
- Project manifest and package manager written in QORVYNT.
- Generated templates whose application logic is `.qv`.
- Standard library APIs for files, networking, serialization, async/concurrency, and structured errors with explicit capability permissions.
- Editor language server, diagnostics, completion, formatting, and debugger.

### DHP-4: Application runtimes
- Web target and server runtime.
- 2D graphics/game runtime before 3D; deterministic simulation and asset pipeline.
- AI agent SDK and tool interface with permission boundaries.
- Each runtime must expose QORVYNT APIs and pass end-to-end tests.

### DHP-5: Web3 and chain developer stack
- Typed contract language semantics and stable ABI.
- Deterministic execution, storage model, transaction envelope, signer authorization, fee settlement, replay, and state commitments.
- A separate chain repository and testnet; do not mix chain implementation into the QORVYNT language repository.
- Wallet SDK, token standards, DAO primitives, indexer, explorer, and deployment verification only after protocol design and security review.

## Acceptance gates

- Every code sample compiles/runs with the canonical compiler unless explicitly labeled pseudocode or roadmap syntax.
- Playground behavior is tested against CLI behavior.
- No claim of “100% QORVYNT” until a QORVYNT-authored app can compile and execute through the relevant runtime without a handwritten JavaScript/Rust implementation of the application logic.
- No production contract, wallet, token, or chain claim without tests, security review, and an independently reproducible release.
- GitHub Actions must pass on the exact commit being proposed. A queued workflow is not a passing check.

## Initial page

The first public page is `website/developers.html`. It intentionally distinguishes current compiler/QVM features from experimental and planned capabilities.

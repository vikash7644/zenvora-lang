# QORVYNT Omni-Domain Dual-Runtime Architecture

**Status:** Proposal and implementation roadmap; not a claim of shipped functionality.  
**Scope:** QORVYNT language/tooling. Blockchain node and consensus development remain separate.

QORVYNT's long-term goal is one language, compiler toolchain, project model, and IDE for games/graphics, AI agents, web services, deterministic Web3/DAO programs, and controlled hardware I/O.

## 1. Two execution profiles

### Chain / Sandbox
For deterministic contracts and governance rules. It permits pure deterministic computation, bounded state operations through a specified host interface, and explicitly metered cryptographic operations. It forbids wall-clock time, OS randomness, filesystem, USB, GPU, sockets, unrestricted HTTP, threads, and arbitrary native FFI. External/oracle data must arrive as authenticated transaction input, not a direct contract network request.

### Native / Agent
For desktop apps, games, graphics/VFX, AI agents, web services, and device integration. Capabilities must be explicitly requested and granted by the launcher/deployment manifest. Proposed capabilities include `fs.read`, `fs.write`, `net.client`, `net.listen`, `gpu.render`, `gpu.compute`, `usb.enumerate`, `usb.storage.read`, `usb.storage.write`, and high-risk `native.ffi`. Access is scoped and least-privilege; USB storage writes require stronger authorization than enumeration/read access.

Native mode is not automatically trusted. Sandboxing, consent, path/device scoping, resource quotas, and clear permission prompts are required. Prefer OS-supported storage APIs; raw block-device writes need additional safeguards.

## 2. Proposed project declaration

The following is illustrative syntax and is not currently accepted by the compiler:

```qorvynt
project "starfield" {
    runtime native
    capabilities ["gpu.render", "fs.read:assets/**", "net.client"]
    modules [std.gfx, std.ai]
}
```

A contract project would select `runtime chain` and receive no native capabilities. The compiler must reject incompatible imports at build time; the runtime must enforce permissions again at execution time. An import must never grant authority by itself.

## 3. Compiler/runtime architecture

```text
.qv source
   |
Lexer -> Parser -> Semantic analysis -> Validated IR
                                      |
                         +------------+------------+
                         |                         |
                    Chain profile             Native profile
                    deterministic IR          capability-checked IR
                         |                         |
                    metered bytecode           native/runtime backend
                         |                         |
                    Chain VM + host             Native runtime + host
                         |                         |
                    state / events              OS / GPU / network / devices
```

Share the language front end, source maps, diagnostics, module metadata, common IR validation, and conformance tests. Separate effect checking, code generation, host interfaces, and runtime policy.

## 4. Modular standard-library plan

| Module | Responsibility | Target |
|---|---|---|
| `std.core` | collections, strings, math, errors | Both; deterministic subset |
| `std.web3` | addresses, hashes, signatures, contract types/events | Chain + native client subset |
| `std.dao` | proposals, voting rules, policy evaluation | Chain-safe core; native indexing separate |
| `std.gfx` | 2D/3D scenes, materials, cameras, render loop | Native |
| `std.vfx` | particles, shaders, post-processing | Native |
| `std.ai` | model interfaces, inference, agent orchestration | Native |
| `std.web` | HTTP server/client, REST, WebSocket; GraphQL adapters | Native |
| `std.usb` | device discovery and authorized storage operations | Native |
| `std.io` | scoped files, streams, serialization | Native; no chain filesystem |

Large backends are optional adapters, not mandatory compiler dependencies: WebGPU/Vulkan for graphics, ONNX Runtime for inference, and platform USB/storage APIs. CUDA, drivers, and vendor runtimes remain optional external dependencies.

## 5. Security invariants

1. Chain execution is deterministic across supported hosts: integer widths, overflow, encoding, map ordering, cryptography, serialization, and host-call semantics are fixed.
2. Floating-point, GPU, OS, time, entropy, network, and filesystem results cannot affect chain execution unless a future specification defines deterministic representations and consensus rules.
3. Every chain instruction and host call is metered; limits are consensus parameters.
4. Native capabilities are explicit, least-privilege, scoped, and checked by the host.
5. FFI is native-only and opt-in. ABI, ownership, errors, thread safety, memory lifetime, and unsafe behavior must be documented.
6. Native artifacts are never accepted as chain contracts. Runtime and bytecode versions are explicit; incompatible versions fail closed.
7. Native crashes or memory corruption must not compromise the chain VM.

## 6. Illustrative domain sketches (not runnable yet)

### 2D/3D game
```qorvynt
import std.gfx
fn update(delta) {
    player.move(input.axis("horizontal") * delta)
    scene.camera.follow(player)
}
fn render() { scene.draw() }
```

### AI agent
```qorvynt
import std.ai
agent "support_assistant" {
    memory bounded(1000)
    model local("support-model")
    on_message(message) {
        let context = memory.search(message)
        let answer = model.respond(context, message)
        memory.remember(message, answer)
        return answer
    }
}
```

### Web API
```qorvynt
import std.web
server.get("/health", fn(request) {
    return response.json(200, map("ok", true))
})
server.listen("127.0.0.1:8080")
```

### USB storage
```qorvynt
import std.usb
# Requires explicit usb.enumerate and usb.storage.read permissions.
for device in usb.storage.devices() {
    show device.label()
}
```

### Deterministic contract
```qorvynt
contract Counter {
    state value: Int = 0
    action increment() { value = value + 1 }
    view read() -> Int { return value }
}
```

These sketches describe intended ergonomics only. Current experimental contract syntax does not yet provide production persistent state, authorization, gas accounting, or chain deployment.

## 7. Implementation sequence and exit gates

### A. Runtime policy foundation
- [ ] Specify profile metadata and reject unknown profiles.
- [ ] Add effect/capability analysis to semantic validation.
- [ ] Add negative tests proving chain code cannot import/call native-only APIs.
- [ ] Fix deterministic value, integer, serialization, and error semantics.
- [ ] Keep the Rust-hosted compiler labeled as a reference host until Rust-independent bootstrap is proven.

### B. Host interfaces and sandboxing
- [ ] Specify a versioned host-call ABI and resource accounting.
- [ ] Add chain host mocks and deterministic replay tests.
- [ ] Add native capability manifests and runtime permission checks.
- [ ] Test denied permissions, path traversal, device removal, malformed host data, and resource exhaustion.

### C. Native FFI and modules
- [ ] Specify ABI versioning, ownership, error propagation, and safety contracts.
- [ ] Prototype one small native adapter before broad FFI.
- [ ] Add optional package/module resolution and lockfile behavior.
- [ ] Verify unused domain modules are not linked/loaded unnecessarily.

### D. Domain libraries
- [ ] Minimal 2D graphics, then 3D and VFX.
- [ ] Web server/client primitives and secure deployment examples.
- [ ] Model inference adapter, then bounded agent orchestration and memory.
- [ ] USB enumeration and read-only storage first; writes only after permission/recovery tests.
- [ ] Keep deterministic Web3/DAO core separate from native indexers, wallets, and network clients.

### E. Release gates
- [ ] Every advertised example compiles/runs in CI on its declared target.
- [ ] Chain conformance passes across independent hosts/platforms.
- [ ] Native permission and sandbox tests pass.
- [ ] FFI adapters receive focused security review.
- [ ] Docs accurately label implemented, experimental, and planned features.

## 8. Honest current baseline

- QORVYNT has a Rust-hosted compiler/reference VM, a QORVYNT-written source frontend, experimental bytecode v2 work, a CLI, a browser demo playground, and an early Windows Studio shell.
- The source frontend is not yet a proven Rust-independent self-hosted bootstrap.
- The current contract DSL is experimental, not a production chain runtime.
- The dual-runtime split and profile enforcement described here are design targets; they are not implemented yet.
- `std.gfx`, `std.vfx`, `std.ai`, `std.web`, and `std.usb` are planned modules, not shipped standard-library modules.
- No production FFI, GPU renderer, AI runtime, web server, or USB API is claimed here.

## 9. Decision

Build the safety boundary before broad domain features: profile metadata -> capability/effect checking -> deterministic chain host contract -> native permission host -> optional FFI -> domain modules. This avoids retrofitting security after powerful APIs have already shipped.

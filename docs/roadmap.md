# Zenvora Master Roadmap

**Status date:** 2026-10-18  
**Current milestone:** Zenvora Lang v0.6.0 prototype  
**Principle:** Build, test, and document each layer before depending on it. This roadmap is a plan, not a claim that future milestones are already complete.

## The long-term vision

Zenvora aims to grow into an independent technology stack: a programming language, a bytecode runtime, developer tools, a native-code path, and eventually an experimental Zenvora instruction set and hardware prototype. Blockchain, wallets, smart contracts, and the wider ecosystem are related long-term projects, but they should not be treated as finished or mixed into the language before its execution foundation is reliable.

## Where we are now

The repository has a Rust-based compiler/VM prototype, a versioned text bytecode format, basic control flow and functions, a CLI, documentation, examples, and automated checks. The current VM is an early stack-based runtime. The bytecode format is readable text, not a compact raw-binary encoding. The browser playground is a limited JavaScript demo and is not the Rust compiler.

## Roadmap phases

### Phase 1 — Stabilize the language and VM (next)

- [ ] Make break, continue, return, recursion, and nested control flow behave consistently; add regression tests for every discovered bug.
- [ ] Improve parser diagnostics with source file, line, column, and a helpful error message.
- [ ] Define language behavior for values, operators, function arguments, return values, and errors.
- [ ] Add tests for malformed programs, huge inputs, invalid identifiers, and boundary cases.
- [ ] Review and document VM limits; make runtime errors safe and understandable.
- [ ] Keep CI running cargo check, cargo test, and all examples.

**Exit criteria:** CI passes, documented language behavior matches tests, and invalid programs fail cleanly rather than crashing or hanging.

### Phase 2 — Secure bytecode verification

- [ ] Define the bytecode instruction set and operand formats precisely.
- [ ] Add a verifier that validates every instruction, constant, function entry, jump target, and control-flow path before execution.
- [ ] Validate stack behavior and reject malformed or hostile bytecode before it runs.
- [ ] Add fuzz/property-style tests for the parser, bytecode loader, verifier, and VM.
- [ ] Document security boundaries and remaining limitations.

**Exit criteria:** The VM refuses invalid bytecode before execution, with automated tests covering invalid instructions, jumps, function entries, and stack errors.

### Phase 3 — Compact binary bytecode and runtime packaging

- [ ] Design a versioned binary container with magic bytes, format version, section lengths, and explicit integer/string encodings.
- [ ] Specify UTF-8 handling, limits, and compatibility rules.
- [ ] Implement binary encode/decode and round-trip tests.
- [ ] Keep readable disassembly/debug output for developers.
- [ ] Provide clear CLI commands to compile, inspect, verify, and run bytecode.
- [ ] Package the VM as a reusable runtime/library where practical.

**Exit criteria:** Source compiles to a binary bytecode file, the verifier accepts valid files and rejects invalid files, and documented examples run through the CLI. This is still VM bytecode, not CPU machine code.

### Phase 4 — Improve the language and developer experience

- [ ] Add a stable grammar and language reference.
- [ ] Add source locations to compiled instructions for useful runtime traces.
- [ ] Design a proper module/import system and visibility rules.
- [ ] Add a standard library gradually, with documented APIs and tests.
- [ ] Decide on a type system deliberately; prototype static checks before promising them.
- [ ] Add formatter/linter or editor support only after syntax stabilizes.

**Exit criteria:** A small multi-file program can be written, diagnosed, documented, and tested reproducibly.

### Phase 5 — Native machine-code backend

- [ ] Choose a realistic backend strategy: first use a mature backend/toolchain (for example LLVM or an existing target assembler) rather than trying to invent every CPU encoding at once.
- [ ] Define a target and ABI; an initial target may be AArch64/ARM64, with x86-64 later if feasible.
- [ ] Lower a small, well-defined subset of Zenvora to native code.
- [ ] Compare native results against VM results with the same test suite.
- [ ] Add debugging, linking, and platform-specific build documentation.

**Exit criteria:** Selected Zenvora programs produce executable native machine code for a declared target and pass differential tests. This does not require manufacturing a semiconductor.

### Phase 6 — Experimental Zenvora ISA and emulator

- [ ] Write an architecture proposal: registers, instruction formats, memory model, privilege assumptions, exceptions, and calling convention.
- [ ] Build an instruction-set simulator/emulator in software.
- [ ] Create assembler/disassembler and test vectors.
- [ ] Build a tiny compiler backend targeting the experimental ISA.
- [ ] Compare emulator behavior with the architecture specification.

**Exit criteria:** Documented sample programs execute deterministically in the emulator. A custom ISA is an architecture design, not yet a physical processor.

### Phase 7 — FPGA prototype (long-term hardware research)

- [ ] Learn digital logic, HDL (such as SystemVerilog or VHDL), simulation, and timing basics.
- [ ] Translate a minimal subset of the ISA into a simple CPU core.
- [ ] Test the design in simulation first, then consider an FPGA board if budget and access allow.
- [ ] Add memory and I/O carefully; publish reproducible tests and limitations.

**Exit criteria:** A small design runs known test programs in simulation and, if hardware is available, on an FPGA. This is not the same as manufacturing a silicon chip.

### Phase 8 — Separate ecosystem integration

Only after the language/runtime has a stable, reviewed execution model:

- [ ] Define whether and how Zenvora contracts should be sandboxed and metered.
- [ ] Design deterministic execution, resource/gas accounting, cryptographic boundaries, and a threat model before connecting to a chain.
- [ ] Keep wallet keys, node networking, consensus, and contract execution as separately reviewed components.
- [ ] Prototype locally before considering public networks or real assets.

**Exit criteria:** A documented local prototype with deterministic tests and an explicit security review. Never treat a prototype as safe for real funds without independent review.

## Recommended immediate work

1. Audit the current compiler/VM and turn every known edge case into a regression test.
2. Implement bytecode control-flow/stack verification.
3. Design the binary bytecode container and its versioning rules.
4. Improve diagnostics and runtime packaging.
5. Only then begin a small native-code backend experiment.

## Resource and safety rules

- Keep the project free-first and cloud/GitHub-friendly where possible; avoid requiring a hot Android phone for every task.
- Prefer small, reviewable commits and CI verification.
- Do not promise production readiness, a finished blockchain, a native compiler, or a physical chip until each is implemented and independently tested.
- Security, compatibility, and reproducibility take priority over adding many features quickly.

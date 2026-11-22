# QORVYNT Virtual Machine (QVM)

## Current execution path

QVM is the name for the verified QORVYNT bytecode execution target. The current implementation is a milestone toward a separately packaged VM, not a claim that the runtime is already Rust-independent.

```text
source (.qv)
   |
   v
QORVYNT compiler frontend
   |
   v
instruction stream + function layout
   |
   v
Bytecode v2 serializer
   |
   v
Bytecode v2 verifier
   |
   v
QVM execution (`src/qvm/runtime.rs` instruction loop)
```

The canonical emitted v1/v2 headers are `QORVYNT_BYTECODE 1` and `QORVYNT_BYTECODE 2`. The loader keeps read-only compatibility for old `ZENVORA_BYTECODE` artifacts during migration; new artifacts are never emitted with the old prefix. The Bytecode v2 format is experimental. The decoder validates framing, instruction records, function regions, control-flow targets, and stack behavior before the instruction stream reaches the shared VM engine. The existing v1 path remains available for compatibility; QVM commands intentionally accept v2 only.

## CLI

Compile a QORVYNT source file into a verified QVM artifact:

```sh
qorvynt --qvm-compile examples/hello.qv build/hello.qvmb
```

Run the artifact through the QVM v2 loader and execution engine:

```sh
qorvynt --qvm-run build/hello.qvmb
```

The `.qvmb` suffix identifies a QVM Bytecode v2 artifact. The current serialized representation is the repository's textual Bytecode v2 framing; it is not yet a compact binary encoding.

The generic `--run-bytecode` command continues to support legacy v1 and v2 artifacts. The QVM-specific `--qvm-run` command rejects v1 so that callers cannot accidentally bypass the v2 verifier contract.

## Current boundaries

- The compiler, serializer, verifier, and runtime are still hosted in the Rust `qorvynt` executable.
- The self-hosted `compiler/parser.qv` is an evolving frontend foundation; it is not yet the complete compiler or a self-hosting bootstrap.
- The QVM instruction set (`src/qvm/bytecode.rs`), runtime values (`src/qvm/value.rs`), and instruction dispatch loop (`src/qvm/runtime.rs`) now live under the dedicated QVM module. The module still imports host-side builtin, verifier, and safety helpers, so it is not independently buildable yet.
- Bytecode v2 remains experimental and must not be described as stable or production-safe before format review and independent security testing.

## Next engineering gates

1. Keep compile → serialize → verify → execute parity covered by tests for functions, collections, control flow, builtins, and runtime errors.
2. Move the remaining host-side builtin and runtime safety helpers behind the QVM module API, then make the QVM module independently buildable.
3. Define a versioned, deterministic binary artifact encoding only after the v2 instruction and metadata contract is frozen.
4. Add a standalone QVM library/runner and prove it can load artifacts without invoking the source compiler.
5. Port the compiler backend incrementally into QORVYNT and prove clean-environment bootstrap parity before claiming Rust independence.


The VM instruction enum and runtime value type have since moved to `src/qvm/bytecode.rs` and `src/qvm/value.rs`; execution and variable-environment handling remain in the Rust host pending the next extraction.

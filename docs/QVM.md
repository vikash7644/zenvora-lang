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
QVM execution (dedicated runtime module in the Rust host)
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
- The instruction loop now lives in `src/qvm/runtime.rs`; the shared value, collection, and resource-accounting helpers live in `src/qvm/runtime_support.rs`. The CLI retains compatibility wrappers and re-exports the helpers used by the existing compiler/tests. The runtime imports these helpers through the QVM module, but it still calls the host `builtin` implementation, uses host contract-gas types, and depends on crate-level limits. This is a stronger internal boundary, not yet an independently buildable QVM package.
- Bytecode v2 remains experimental and must not be described as stable or production-safe before format review and independent security testing.

## Next engineering gates

1. Keep compile → serialize → verify → execute parity covered by tests for functions, collections, control flow, builtins, and runtime errors.
2. Move built-in dispatch and gas-meter interfaces behind a runtime-owned API; remove the remaining compiler-host coupling while keeping execution parity green.
3. Define a versioned, deterministic binary artifact encoding only after the v2 instruction and metadata contract is frozen.
4. Add a standalone QVM library/runner and prove it can load artifacts without invoking the source compiler.
5. Port the compiler backend incrementally into QORVYNT and prove clean-environment bootstrap parity before claiming Rust independence.


The VM instruction enum, runtime value type, and instruction execution loop now live in `src/qvm/bytecode.rs`, `src/qvm/value.rs`, and `src/qvm/runtime.rs`. The CLI retains a thin compatibility wrapper. The extracted runner is crate-visible only so the host wrapper can pass the optional contract gas meter; it is not a public library API yet. The extracted runtime still depends on host-crate helpers and is not yet standalone.

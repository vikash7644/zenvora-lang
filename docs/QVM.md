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
QVM execution (currently hosted in the Rust CLI)
```

The Bytecode v2 format is experimental. The decoder validates framing, instruction records, function regions, control-flow targets, and stack behavior before the instruction stream reaches the shared VM engine. The existing v1 path remains available for compatibility; QVM commands intentionally accept v2 only.

## CLI

Compile a QORVYNT source file into a verified QVM artifact:

```sh
zv --qvm-compile examples/hello.qv build/hello.qvmb
```

Run the artifact through the QVM v2 loader and execution engine:

```sh
zv --qvm-run build/hello.qvmb
```

The `.qvmb` suffix identifies a QVM Bytecode v2 artifact. The current serialized representation is the repository's textual Bytecode v2 framing; it is not yet a compact binary encoding.

The generic `--run-bytecode` command continues to support legacy v1 and v2 artifacts. The QVM-specific `--qvm-run` command rejects v1 so that callers cannot accidentally bypass the v2 verifier contract.

## Current boundaries

- The compiler, serializer, verifier, and runtime are still hosted in the Rust `zv` executable.
- The self-hosted `compiler/parser.qv` is an evolving frontend foundation; it is not yet the complete compiler or a self-hosting bootstrap.
- QVM currently reuses the existing VM execution engine; the interpreter has not yet been extracted into an independently buildable QVM package.
- Bytecode v2 remains experimental and must not be described as stable or production-safe before format review and independent security testing.

## Next engineering gates

1. Keep compile → serialize → verify → execute parity covered by tests for functions, collections, control flow, builtins, and runtime errors.
2. Extract the runtime value model, opcode definitions, and execution loop from `src/main.rs` into a dedicated `src/qvm/` module without changing behavior.
3. Define a versioned, deterministic binary artifact encoding only after the v2 instruction and metadata contract is frozen.
4. Add a standalone QVM library/runner and prove it can load artifacts without invoking the source compiler.
5. Port the compiler backend incrementally into QORVYNT and prove clean-environment bootstrap parity before claiming Rust independence.

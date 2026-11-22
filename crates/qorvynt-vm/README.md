# qorvynt-vm

Standalone execution runtime for QORVYNT Bytecode v2 programs.

## Current package boundary

This package owns:

- VM instruction and runtime value types
- execution loop and output-sink API
- built-in dispatch and collection/value helpers
- resource limits and the host-neutral gas-meter interface

The host compiler still owns QORVYNT source parsing and source-to-bytecode compilation. This crate now owns Bytecode v2 framing and structural/control-flow verification through `bytecode_v2::{parse_v2_file, verify_v2_file}`. The crate also exposes `decode_verified_v2_file`, which converts only verified instruction records into VM instructions. Do not execute unverified instructions from untrusted files.


## Verified Bytecode v2 API

```rust
use qorvynt_vm::bytecode_v2::{parse_and_verify_v2_file, decode_verified_v2_file};

let parsed = parse_and_verify_v2_file(bytecode_text)?;
let instructions = decode_verified_v2_file(&parsed)?;
# Ok::<(), String>(())
```

`parse_and_verify_v2_file` is the preferred untrusted-input boundary: it parses the file and runs structural, control-flow, jump-target, call-target, and operand-stack verification before decoding. `decode_verified_v2_file` rechecks verification before materializing executable instructions.

## API sketch

```rust
use qorvynt_vm::qvm::{runtime::run_with_output_metered, Op};

let program = [Op::Push(40), Op::Push(2), Op::Add, Op::Print];
let mut output = Vec::new();
run_with_output_metered(&program, &mut |line| output.push(line), None)?;
assert_eq!(output, vec!["42"]);
# Ok::<(), String>(())
```

Run package tests from the repository root with `cargo test -p qorvynt-vm`.

This is an extraction milestone, not yet a stable published crate. Bytecode v2 parsing, verification, and opcode materialization are in this crate; QORVYNT source parsing and source-to-bytecode compilation remain in the host compiler.

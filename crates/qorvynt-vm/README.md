# qorvynt-vm

Standalone execution runtime for QORVYNT Bytecode v2 programs.

## Current package boundary

This package owns:

- VM instruction and runtime value types
- execution loop and output-sink API
- built-in dispatch and collection/value helpers
- resource limits and the host-neutral gas-meter interface

The host compiler currently owns source parsing, bytecode serialization, and Bytecode v2 verification. The root repository adapter validates artifacts before handing instructions to this runtime. Do not execute unverified instructions from untrusted files.

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

This is the first extraction milestone, not yet a stable published crate or an independent bytecode loader.

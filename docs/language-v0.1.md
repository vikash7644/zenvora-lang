# Zenvora Lang v0.5 — current implementation

Zenvora Lang is an independent language project for the Zenvora ecosystem. This milestone includes `repeat` loops with `break` and `continue`, plus functions with value parameters, return values, recursion, and isolated local variable environments in the Rust compiler and stack-based bytecode virtual machine. It remains an early prototype, not a production smart-contract language.

## Build and run

Requires Rust and Cargo (including in Termux where the Rust toolchain is installed).

```sh
cargo check
cargo test
cargo run -- examples/hello.zv
cargo run -- examples/functions.zv
cargo run -- examples/booleans.zv
```

## Automated tests

The in-source Rust test suite uses output capture to verify actual VM results, not just successful compilation. It covers arithmetic and assignment, booleans and nested conditions, loop control (including nested loops), function parameters and return values, recursion, invalid loop control, undefined functions, argument-count errors, division by zero, integer overflow, undefined variables, and unmatched braces. Run the suite with `cargo test`; GitHub Actions runs it automatically for pushes to `main` and pull requests.

## Supported syntax

```zv
define age := 20

if age >= 18 {
    show 1
} otherwise {
    show 0
}
```

The program prints `1`. Set `age := 16` and it prints `0`.

- `define name := expression` declares a variable whose value can be an integer or boolean.
- `name := expression` updates or creates an integer variable.
- `show expression` prints an integer or boolean; `print(expression)` is also accepted.
- Integer arithmetic: unary minus, `+`, `-`, `*`, `/`, and parentheses.
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`. Comparisons produce native boolean values `true` or `false`.
- Logical operators: `not`, `and`, and `or`; they require boolean operands and currently evaluate both sides eagerly.
- Conditional blocks use `if condition {`, followed by statements and a closing `}`; an optional else branch uses the exact line ` } otherwise {` without the leading space (normally written as `} otherwise {`).
- `#` starts a comment.
- Nested if/otherwise blocks are supported.
- `repeat condition { ... }` loops reevaluate their comparison condition before each iteration.
- `break` exits the nearest enclosing loop; `continue` jumps back to reevaluate its condition.
- `break` and `continue` outside a loop are rejected during compilation.
- `fn name(a, b) { ... }` defines a function with parameters; `call name(2 + 3, 4)` invokes it with expressions as arguments.
- `return` exits the current function. Calls may occur before definitions and functions may call other functions.
- Each function call has an isolated local variable environment; caller variables are restored on return. Function calls can appear inside expressions, and `return expression` returns an integer value to the caller. Access to caller/global variables from functions is not implemented yet.
- Division by zero, undefined variables, invalid syntax, integer overflow, and unmatched block braces are reported as errors.

## Bytecode files

Compile a `.zv` program into a versioned bytecode file and run it later:

```sh
cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
```

Version 1 is a readable text format with the `ZENVORA_BYTECODE 1` header. The loader rejects unknown instructions, unsupported versions, malformed operands, invalid jump targets, and invalid function-entry targets. The format is an early public-facing prototype; compatibility guarantees and a formal verifier are future work.

## How execution works

1. Source text is parsed into stack-machine instructions.
2. The compiler emits operations such as PUSH, LOAD, STORE, ADD, comparison operations and conditional jumps.
3. The VM executes the bytecode and stores values in the active runtime environment.

## Not implemented yet

Strings, short-circuit logical evaluation, explicit global/module scope rules, custom types, modules, a formally frozen bytecode specification, native machine-code generation, smart contracts and blockchain integration are future milestones. Arithmetic operators require integers; ordered comparisons require integers; equality supports values of the same type. Conditions accept booleans (integer zero/nonzero is also accepted for compatibility). The VM stops after 1,000,000 executed instructions to guard against accidental infinite loops; more resource-aware limits and runtime safety checks remain future work. Do not use this prototype for funds or production contracts.

## Roadmap

1. Expand regression coverage and stabilize syntax and diagnostics.
2. Add explicit global/module scope rules and useful types.
3. Define a stable bytecode format.
4. Add serializable bytecode and a stable VM specification.
5. Explore WebAssembly output and a contract ABI only after the core language is reliable.

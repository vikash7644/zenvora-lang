# Zenvora Lang v0.3 — current implementation

Zenvora Lang is an independent language project for the Zenvora ecosystem. This milestone includes `repeat` loops with `break` and `continue`, plus functions with integer parameters and isolated local variable environments in the Rust compiler and stack-based bytecode virtual machine. It remains an early prototype, not a production smart-contract language.

## Build and run

Requires Rust and Cargo (including in Termux where the Rust toolchain is installed).

```sh
cargo check
cargo test
cargo run -- examples/hello.zv
cargo run -- examples/functions.zv
```

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

- `define name := expression` declares an integer variable.
- `name := expression` updates or creates an integer variable.
- `show expression` prints an integer; `print(expression)` is also accepted.
- Integer arithmetic: unary minus, `+`, `-`, `*`, `/`, and parentheses.
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`. Comparisons evaluate to `1` (true) or `0` (false).
- Conditional blocks use `if condition {`, followed by statements and a closing `}`; an optional else branch uses the exact line ` } otherwise {` without the leading space (normally written as `} otherwise {`).
- `#` starts a comment.
- Nested if/otherwise blocks are supported.
- `repeat condition { ... }` loops reevaluate their comparison condition before each iteration.
- `break` exits the nearest enclosing loop; `continue` jumps back to reevaluate its condition.
- `break` and `continue` outside a loop are rejected during compilation.
- `fn name(a, b) { ... }` defines a function with integer parameters; `call name(2 + 3, 4)` invokes it with expressions as arguments.
- `return` exits the current function. Calls may occur before definitions and functions may call other functions.
- Each function call has an isolated local variable environment; caller variables are restored on return. Function return values and access to caller/global variables from functions are not implemented yet.
- Division by zero, undefined variables, invalid syntax, integer overflow, and unmatched block braces are reported as errors.

## How execution works

1. Source text is parsed into stack-machine instructions.
2. The compiler emits operations such as PUSH, LOAD, STORE, ADD, comparison operations and conditional jumps.
3. The VM executes the bytecode and stores integer variables in its runtime environment.

## Not implemented yet

Strings, native boolean values, function return values, explicit global/module scope rules, custom types, modules, bytecode file serialization, native machine-code generation, smart contracts and blockchain integration are future milestones. Comparison results are represented as integer `1` or `0` for now. The VM stops after 1,000,000 executed instructions to guard against accidental infinite loops; more resource-aware limits and runtime safety checks remain future work. Do not use this prototype for funds or production contracts.

## Roadmap

1. Stabilize syntax, diagnostics and automated tests.
2. Add function return values and explicit global/module scope rules.
3. Add useful types and a stable bytecode format.
4. Add serializable bytecode and a stable VM specification.
5. Explore WebAssembly output and a contract ABI only after the core language is reliable.

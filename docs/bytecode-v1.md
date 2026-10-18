# Zenvora Lang Bytecode Format v1

**Status:** early, versioned prototype. The format is readable text, not a frozen long-term compatibility promise. Do not treat bytecode from an untrusted source as safe for production.

## File framing

A file begins with this exact first line:

```text
ZENVORA_BYTECODE 1
```

Each following non-empty line encodes exactly one instruction. Instructions use ASCII opcode names and whitespace-separated operands. Identifiers use letters, digits, and underscores, and may not begin with a digit. Function parameter names in `CALL_TARGET` are comma-separated. A function with no parameters uses `-`.

Create and execute a bytecode file:

```sh
cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
```

## Instruction set

| Instruction | Stack effect | Meaning |
| --- | --- | --- |
| `PUSH n` | `→ Int` | Push signed 64-bit integer |
| `PUSH_STRING_HEX hex` | `→ String` | Push UTF-8 string bytes encoded as hex; `-` represents an empty string |
| `PUSH_BOOL true/false` | `→ Bool` | Push boolean |
| `LOAD name` | `→ Value` | Push a variable's current value; undefined names fail at runtime |
| `STORE name` | `Value →` | Update an existing local or global binding; undefined names fail |\n| `DECLARE name` | `Value →` | Declare a global at top level or a local inside a function |\n| `INDEX` | `Collection Index → Value` | Read an array by integer index or a map by string key; invalid indexes fail |\n| `SET_INDEX name` | `Index Value →` | Update the named array element or map key; array bounds and key types are checked |
| `ADD`, `SUB`, `MUL`, `DIV` | `a b → Value` | Integer arithmetic with overflow and divide-by-zero checks; `ADD` also concatenates two strings |
| `NEG` | `Int → Int` | Negate an integer |
| `NOT` | `Bool → Bool` | Boolean negation |
| `AND`, `OR` | `Bool Bool → Bool` | Combine two booleans after operand evaluation; the compiler uses the keep-jump instructions for short-circuit semantics |
| `PRINT` | `Value →` | Print an integer, string, or boolean |
| `EQ`, `NE` | `a b → Bool` | Compare values for equality or inequality |
| `LT`, `LE`, `GT`, `GE` | `Int Int → Bool` | Ordered integer comparisons |
| `JUMP_IF_FALSE target` | `Condition →` | Pop a condition; jump when false. Booleans are preferred; integer zero/non-zero is also accepted |
| `JUMP_IF_FALSE_KEEP target` | `Bool → Bool` | Jump when the top value is false without popping it; used to implement short-circuit `and` |
| `JUMP_IF_TRUE_KEEP target` | `Bool → Bool` | Jump when the top value is true without popping it; used to implement short-circuit `or` |
| `JUMP target` | no change | Continue at the absolute instruction index |
| `CALL_TARGET target argc params` | `argc args →` | Pop arguments, create a local frame, and enter the function; global bindings remain accessible |
| `RETURN` | no change | Return from the current function without a value |
| `RETURN_VALUE` | `Value →` | Return a value to the caller's operand stack |

String bytes are encoded as lowercase hexadecimal so strings may contain spaces, newlines, commas, Unicode, or quote characters without changing instruction framing. Empty strings use `-`.

All jump and function targets are zero-based instruction indices, not source line numbers. The compiler emits a jump past the function bodies so normal program execution does not fall through into function definitions.

## Loader validation and control-flow verification

The loader checks the header/version, opcode names, operand counts and types, identifiers, function parameter counts, jump bounds, and function-entry bounds. It then runs a control-flow verifier before returning the decoded program:

- Rejects unresolved calls, out-of-region jumps, invalid function-entry locations, and returns reached from the main-program region.
- Performs a work-list traversal from each region entry and also seeds disconnected instruction islands at an empty stack to inspect code hidden behind unconditional jumps.
- Rejects stack underflow in checked paths, stack growth beyond the configured limit, control-flow joins where incoming stack depths disagree, and function returns with leftover or missing operand values.
- Checks that the main program exits with an empty operand stack and that reachable function paths do not fall through into another region.

The v1 format does not encode a function table or return types. Function regions are inferred from call targets, and call stack effects infer value returns from the target's instructions. Consequently, the verifier is an important hardening layer, not a proof that arbitrary bytecode is safe: function regions and return-value effects are inferred rather than declared in metadata, type safety is not proven, and static analysis of disconnected code starts with an assumed empty stack. Runtime checks remain necessary.

The loader enforces a 16 MiB bytecode-file limit, a 1,000,000-instruction limit, a 1 MiB decoded-string limit, and a 1,024-argument limit for calls and built-ins. The VM also enforces instruction-count, function-call-depth, and operand-stack limits.

This prototype is not formally verified or production-audited. Next steps include explicit function metadata in a future bytecode version, return-kind/type validation, validation of all instruction regions including unreachable code, fuzzing, and an independent security review.

# Zenvora Bytecode v2 — Function Metadata and Verification Design

**Status:** experimental Bytecode v2 text loading, metadata validation, opcode/control-flow/stack verification, execution through the existing VM, and opt-in compiler emission are implemented and CI-tested. Deterministic adversarial tests, targeted fuzz-corpus seeds, a parse/serialize/parse round-trip property, and layout instruction-count limits are covered by CI. Four 300-second main-branch libFuzzer campaigns completed successfully with 14,343,900, 15,084,335, 17,765,371, and 15,579,903 executions, respectively, without a reported crash. Fuzzing found and drove a fix for embedded carriage returns in instruction records. Independent security review is still pending. The format/opcode set remains provisional. This is not a production-security or audit claim. Bytecode v1 remains the default and is supported unchanged.

## Why v2 is needed

Bytecode v1 infers function regions from `CALL_TARGET` instructions. This leaves an important verification gap: an unreferenced function body has no declared entry, parameter list, or return kind, so the verifier cannot reliably partition and verify every instruction region. V2 should make code regions explicit instead of trying to infer them from call sites.

## Design goals

1. Make the bytecode format version explicit and reject unknown versions.
2. Describe every function, including functions that are never called by the current program.
3. Verify every instruction in every declared region, including unreachable instructions.
4. Make function-call stack effects deterministic from metadata.
5. Keep v1 decoding behavior unchanged; never silently reinterpret v1 as v2.
6. Preserve runtime checks. Verification is not a substitute for resource limits or type checks.

## Proposed file framing

The current **provisional canonical grammar** used by `parse_v2_file` is:

```text
ZENVORA_BYTECODE 2
PROGRAM 0 2
FUNCTION 2 4 1 value amount
CODE
PUSH 1
JUMP 2
PUSH 7
RETURN_VALUE
```

The records mean:

- `PROGRAM <start> <end>`: the main instruction region, using a half-open interval `[start, end)`.
- `FUNCTION <entry> <end> <parameter-count> <return-kind> <parameter-names...>`: a function region, also half-open. Return kind is one of `void` or `value`.
- Function entries and region boundaries are absolute instruction indices, not source line numbers.
- A zero-parameter function uses the single `-` placeholder.
- `CODE` terminates metadata; every following non-empty line is one instruction record.
- Calls use `CALL <entry> <argument-count>`; parameter names and return kind are taken from the target's metadata.
- Main must exit through a final `JUMP <main-end>` with an empty operand stack. Function bodies must return with the declared kind.
- This grammar and opcode set are provisional; v2 is opt-in and is not yet a stable production format.

## Required structural checks

The decoder/verifier must reject:

- Missing, duplicate, overlapping, empty, out-of-bounds, or ambiguously ordered regions.
- A main-program region that does not begin at instruction zero.
- Function entries that overlap the main program or another function.
- Invalid identifiers, duplicate parameter names, and parameter counts that do not match metadata.
- Unknown opcodes, malformed operands, invalid jump targets, and jumps that cross region boundaries.
- Calls to undeclared function entries or calls whose argument count disagrees with the declared parameter count.
- Return instructions in the main region; value returns in void functions; void returns in value-returning functions.
- Any instruction in a function region that can fall through past its declared end.
- A main-program exit that leaves values on the operand stack.

## Required control-flow and stack checks

For each region independently:

1. Start a work-list traversal at the region entry with operand-stack depth zero.
2. Validate the stack requirement and stack effect of every reachable instruction.
3. Reject stack underflow, integer overflow while calculating stack depth, and depth above the configured limit.
4. At every control-flow join, require exactly the same incoming stack depth.
5. Require each reachable return to have exactly zero stack values for `void`, or exactly one for `value`.
6. Validate every instruction's structure even if unreachable. For complete stack verification of unreachable blocks, build basic blocks and validate each disconnected component under a documented entry-depth policy; do not silently assume arbitrary initial stack values.
7. Validate that all branch targets are instruction boundaries inside the same declared region, except the one explicitly defined main-program exit.
8. Compute each call's stack effect from the callee's declared return kind, never from call-site guesses.

The verifier should use checked arithmetic for offsets, lengths, argument counts, and stack-depth calculations. Limits must be checked before allocating large buffers or vectors where practical.

## Runtime checks that remain mandatory

Even valid v2 bytecode must run under limits for:

- Total bytecode bytes and decoded string bytes.
- Instruction count and executed instruction count.
- Operand stack depth and call depth.
- Argument count, collection size, and string growth.
- Integer overflow, divide-by-zero, invalid indexing, missing bindings, and invalid value types.

The verifier must not be treated as a sandbox by itself. Any future host functions or chain actions need an explicit capability/authorization policy and bounded execution costs.

## Migration and compatibility

- Keep `deserialize_bytecode` strict: v1 headers invoke only the v1 decoder; v2 headers invoke only the v2 decoder.
- Do not change the v1 serializer's output or existing v1 test fixtures as part of the first v2 implementation.
- Keep v2 parsing and verification behind dedicated functions, with unit tests for metadata and region validation.
- Keep v2 emission opt-in until malformed-input/fuzz testing and independent review are complete.
- Document the version and compatibility policy in the README and bytecode format reference.
- Keep old v1 fixtures in regression tests so later refactors cannot accidentally weaken v1 validation.

## Acceptance checklist for implementation

- [x] Initial region-layout validator rejects empty programs, invalid main boundaries, gaps, overlaps/out-of-order regions, out-of-bounds regions, undeclared trailing instructions, invalid/duplicate parameter names, and excessive parameter counts.
- [x] Provisional v2 framing parser checks the version header, main start, metadata record shape, parameter count/name consistency, return-kind tags, CODE terminator, instruction-count/size bounds, and region layout.
- [x] Metadata framing round-trip tests cover a main region plus declared functions, including a zero-parameter function.
- [x] Initial v2 verifier validates opcode syntax, declared call targets/argument counts, region-local jump targets, reachable stack underflow and stack-depth joins, main exit stack cleanliness, and function return-kind metadata.
- [x] The loader dispatches the v2 header to the v2 parser/verifier and executes only after verification succeeds.
- [x] A valid v2 fixture round-trips through metadata serialization/parsing, loads through the CLI bytecode decoder, and executes a metadata-driven function with expected output.
- [x] A declared but uncalled function is still region-checked.
- [x] Every region has exactly one declared entry and a valid end boundary under the contiguous-region layout.
- [x] Overlapping and out-of-bounds regions are rejected.
- [x] Cross-function and cross-main jumps are rejected by the initial v2 verifier.
- [x] Calls to unknown functions and mismatched argument counts are rejected by the initial v2 verifier.
- [x] Void/value return mismatches are rejected by the initial v2 verifier.
- [x] Reachable stack underflow and inconsistent stack depth at joins are rejected by the initial v2 verifier.
- [x] Unreachable malformed opcodes and invalid jump targets are rejected structurally.
- [x] Disconnected unreachable control-flow components are independently analyzed from an empty operand stack; this conservative policy is documented in the verifier.
- [x] Compiler-generated output round-trips through v2 encode/decode and executes with the same observable output; the opt-in CLI command uses this same compiler/emitter path.
- [x] Existing v1 behavior and current regression suite continue to pass in GitHub Actions CI.
- [x] Deterministic malformed-input tests cover header/metadata parsing, integer boundaries, invalid operands, region tables, and jump targets.
- [x] A dependency-free bounded parser fuzz-smoke test exercises 4,096 generated ASCII inputs in CI.
- [x] Adversarial regression tests cover file/string size caps, exact region-end branch targets, extreme jump/call operands, and serializer newline injection.
- [x] A dedicated `cargo-fuzz` target passes arbitrary byte inputs through the bounded v2 parser, verifier, and serializer when parsing succeeds.
- [x] Seed the fuzz corpus with valid main-only bytecode and valid function-call metadata so mutation can explore beyond malformed headers.
- [x] First 60-second GitHub Actions libFuzzer campaign completed without a reported crash (9,921,402 executions); repeat and extend campaigns before production use.
- [x] Dedicated longer-running fuzz/property-based campaigns cover header parsing, metadata parsing, integer boundaries, region tables, jump targets, parse/serialize/parse invariants, and the embedded-carriage-return regression; four 300-second main-branch campaigns completed successfully.

## Current implementation boundary

The Rust module `src/bytecode_v2.rs` implements `ProgramLayout`, `FunctionRegion`, `ReturnKind`, `validate_layout`, `parse_v2_file`/`serialize_v2_file`, and `verify_v2_file`. `validate_layout` rejects instruction counts above `MAX_V2_INSTRUCTIONS`; the parser and verifier enforce the same bound. `compile_with_layout` preserves every declared function's metadata, including uncalled functions, and the opt-in `--emit-bytecode-v2 <source.zv> <output.zv2b>` path emits a verified v2 file. `deserialize_bytecode` dispatches v2 files through the parser/verifier, then maps verified instructions into the existing VM representation; the main-region exit is translated to skip function bodies. Each disconnected component starts with an empty stack. Adversarial tests cover file/string size caps, exact region-end branch targets, extreme jump/call operands, serializer newline injection, unsupported builtins, and embedded carriage returns in instruction records. The fuzz target checks arbitrary byte input, requires every structurally parsed file to serialize, reparses serialized output, asserts structural equality, and re-runs verification. The corpus contains valid main-only and function-call files plus seeds for unsupported header versions, metadata integer overflow, invalid region overlap, extreme jump targets, and embedded carriage returns. Pull-request campaigns are bounded to 60 seconds; pushes to `main`, manual dispatches, and scheduled campaigns are configured for 300 seconds. Four 300-second main-branch campaigns completed without a reported crash (14,343,900; 15,084,335; 17,765,371; and 15,579,903 executions). Four 60-second PR campaigns completed successfully without a reported crash (3,948,932; 3,188,115; 2,653,011; and 4,572,373 executions). Independent review, compatibility guarantees, and stabilizing the format remain before production use. The existing v1 serializer and loader remain unchanged; v1 remains the default format.

## Security status

This work is not formal verification, an audit, or a production-readiness statement. V2 should not be announced as a stable production format until dedicated fuzz/property-based testing, independent review, and compatibility guarantees are completed.

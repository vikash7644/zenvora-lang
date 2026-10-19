# Zenvora Bytecode v2 — Function Metadata and Verification Design

**Status:** design proposal; not implemented. Bytecode v1 remains the only executable format supported by the current loader. This document must not be read as a claim that v2 files can already be compiled or executed.

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

The exact syntax is provisional. A v2 file would start with:

```text
ZENVORA_BYTECODE 2
PROGRAM 0 5
FUNCTION 5 9 1 value value
```

The example above illustrates the *shape* only; it is not a complete executable program. The intended fields are:

- `PROGRAM <start> <end>`: the main instruction region, using a half-open interval `[start, end)`.
- `FUNCTION <entry> <end> <parameter-count> <return-kind> <parameter-names...>`: a function region, also half-open. Return kind is one of `void` or `value`.
- Function entries and region boundaries are absolute instruction indices, not source line numbers.
- A zero-parameter function has no parameter-name operands. The final wire grammar must define an unambiguous empty-parameter representation before implementation.

Metadata records should precede instruction records, or the format should use an explicit metadata section terminator, so parsing cannot confuse metadata with executable instructions. The implementation must choose and document one canonical grammar before shipping v2.

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
- Introduce v2 parsing and verification behind dedicated functions, with unit tests for metadata and region validation before adding v2 emission.
- Add compiler emission only after the v2 loader and verifier reject malformed fixtures reliably.
- Document the version and compatibility policy in the README and bytecode format reference.
- Keep old v1 fixtures in regression tests so later refactors cannot accidentally weaken v1 validation.

## Acceptance checklist for implementation

- [ ] A declared but uncalled function is still region-checked.
- [ ] Every region has exactly one declared entry and a valid end boundary.
- [ ] Overlapping and out-of-bounds regions are rejected.
- [ ] Cross-function and cross-main jumps are rejected.
- [ ] Calls to unknown functions and mismatched argument counts are rejected.
- [ ] Void/value return mismatches are rejected.
- [ ] Stack underflow and inconsistent stack depth at joins are rejected.
- [ ] Unreachable malformed instructions are rejected structurally.
- [ ] Disconnected unreachable control-flow components follow the documented stack-entry policy.
- [ ] Valid compiler output round-trips through v2 encode/decode and executes with the same observable output.
- [ ] Existing v1 fixtures and all current tests continue to pass.
- [ ] Fuzz tests cover header parsing, metadata parsing, integer boundaries, region tables, and jump targets.

## Security status

This is a design proposal, not an implementation, formal verification, audit, or production-readiness statement. V2 should not be announced as supported until its loader, verifier, compiler emission, negative tests, and CI checks are implemented and reviewed.

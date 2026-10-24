# Contributing to QORVYNT Lang

Thanks for helping improve QORVYNT Lang. The project is an early programming-language prototype; small, tested, clearly scoped changes are preferred over large unverified rewrites.

## Before opening a change

1. Check the current [README](README.md), [language implementation status](docs/language-v0.1.md), and [bytecode notes](docs/bytecode-v1.md) or [v2 design](docs/bytecode-v2-design.md), as relevant.
2. Search existing tests and examples before introducing a duplicate feature or changing behavior.
3. Keep the implementation, tests, executable examples, changelog, and docs consistent.
4. Do not describe a proposal or prototype as a production feature.

## Validation checklist

Run these commands with a current stable Rust toolchain:

```sh
cargo fmt -- --check
cargo check --all-targets
cargo clippy --all-targets
cargo test --all-targets
```

For a user-facing language change, also run at least one affected example, for example:

```sh
cargo run -- examples/zenvora-language-tour.zv
cargo run -- --check examples/functions.zv
```

If you cannot run Rust locally, you can still submit a focused change and clearly say which checks you could not run; GitHub Actions must then validate the exact commit. Do not report a check as passed based only on a planned command or an earlier commit's result.

## Compiler and VM changes

- Add regression tests for both valid and invalid inputs.
- Prefer deterministic tests for malformed source, bytecode, files, and network messages.
- Preserve checked arithmetic, bounded allocation/reads, and runtime limits.
- Treat bytecode input as untrusted. Do not weaken validation to make a fixture pass.
- Keep bytecode v2 opt-in until its format and compatibility policy are intentionally stabilized.
- Keep v1 compatibility tests when touching serialization or loading.
- For control-flow or stack changes, test branch joins, loops, function return kinds, invalid jumps, underflow, and resource-limit boundaries where relevant.

## Documentation and examples

- Use `.zv` for executable QORVYNT Lang programs.
- Keep examples minimal and runnable; avoid documenting syntax that the compiler does not accept.
- State clearly when behavior is experimental, local-only, or a design proposal.
- Update the README or `docs/language-v0.1.md` when the public feature inventory changes.
- Update `CHANGELOG.md` under `Unreleased` for notable user-facing or security-relevant changes.

## Security reports

Do not test against public networks or use real funds with this prototype. For suspected vulnerabilities, provide a minimal reproducer and the affected commit/version. Avoid publishing secrets, private keys, tokens, or live exploit details in a public issue before maintainers can assess the report.

## Pull-request description

Please include:
- What changed and why.
- The regression test(s) or executable example added.
- Commands/checks run and their actual results.
- Compatibility or security implications.
- Any known limitations or follow-up work.

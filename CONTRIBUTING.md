# Contributing to QORVYNT

Thanks for helping improve QORVYNT. QORVYNT is an early-stage programming-language project; contributors are welcome to participate voluntarily, in small reviewable tasks. The project is an early programming-language prototype; small, tested, clearly scoped changes are preferred over large unverified rewrites.

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
cargo run -- examples/qorvynt-language-tour.qv
cargo run -- --check examples/functions.qv
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

- Use `.qv` for executable Zenvora Lang programs.
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


## Join the volunteer contributor team

QORVYNT welcomes voluntary, community contributions. There is no salary or guaranteed paid work attached to these tasks. Contributors choose what they work on, can stop at any time, and should not share private credentials or personal information in issues.

### Current contribution tracks

| Track | Useful experience | Good first contribution |
|---|---|---|
| Compiler & language | Rust, parsing, interpreters, language design | Add a focused parser/runtime regression test |
| VM & runtime | Virtual machines, bytecode, memory safety | Document or test one verifier invariant |
| GPU & graphics | Vulkan, WebGPU, shaders, 2D/3D | Research a small, measurable integration prototype |
| Game tools | Rendering, animation, asset pipelines | Propose a minimal example and acceptance criteria |
| AI & speech | ML, inference, speech, evaluation | Compare open models and document device requirements |
| IDE & developer tools | TypeScript, web, desktop UI, debugging | Improve a playground or editor workflow |
| Web3 & governance | Solidity, EVM, cryptography, governance | Review an isolated design or add test fixtures |
| Security & QA | Fuzzing, threat modelling, testing | Reproduce a bug safely and add a regression test |
| Docs & localization | Technical writing, Hindi/English | Improve one tutorial with verified runnable examples |

Some tracks are research or roadmap areas, not implemented QORVYNT features. Confirm the current status in [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md) before choosing a task.

### How to start

1. Read the [project README](README.md) and [project status](docs/PROJECT_STATUS.md).
2. Browse [good first issues](https://github.com/vikash7644/zenvora-lang/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22).
3. If no issue fits, open a [contribution discussion/issue](https://github.com/vikash7644/zenvora-lang/issues/new/choose) describing your skills and the small task you want to take.
4. Wait for maintainer clarification before starting a large or breaking change.
5. Submit a focused pull request and report exactly which checks you ran.

### Contributor recognition and project governance

- Contributors will be credited through GitHub contribution history and, where appropriate, release notes or project documentation.
- Do not promise payment, tokens, equity, employment, or future rewards unless a separate written agreement is made by the project owner.
- Maintainers may request changes, decline work that does not fit the roadmap, or close inactive tasks.
- Never commit secrets, private keys, API tokens, generated binaries, or another person's code without permission.
- A contribution does not grant repository ownership, admin access, or control over project funds.
- Please be respectful, explain technical decisions, and review code for correctness and security rather than accepting changes only because they are large.

### Licensing

The repository currently includes an MIT license. By submitting a contribution, you agree that your contribution may be distributed under the repository's applicable license. If you are unsure about ownership, employer policies, or third-party dependencies, ask before submitting.

# Zenvora Lang Release Readiness Gates

This checklist separates implemented language/VM behavior from experimental contract and chain code. A green CI run is necessary, but it is not a production-readiness certification.

## Gate 1 — Language and VM

- [x] Run locked compiler, lint, and test checks in CI.
- [x] Build the release CLI and run a release-binary smoke test.
- [x] Keep bytecode v2 parser/verifier tests and bounded fuzzing in CI.
- [ ] Run the complete CI suite against the exact release tag/commit before publishing.
- [ ] Publish a reproducible-build/SBOM or dependency inventory and document supported toolchain/platform versions.
- [ ] Complete an independent security review of bytecode parsing, verification, and execution before treating bytecode v2 as stable.

## Gate 2 — Contract state and execution

- [x] Document that contract syntax is experimental and not a persistent smart-contract runtime.
- [ ] Define persistent contract storage semantics, state schema/version migration, and atomic commit/rollback behavior.
- [ ] Define caller identity, authorization rules, and permission failure behavior.
- [ ] Define deterministic gas/instruction accounting and hard resource limits for contract execution.
- [ ] Add adversarial tests for authorization bypass, failed-action rollback, replay, and state corruption.
- [ ] Prove deterministic execution/replay across independent clean processes and supported platforms.

**Do not describe the contract DSL as production-ready until every unchecked item in this section has evidence from code and tests.**

## Gate 3 — Repository boundary

- [ ] Keep this repository focused on Zenvora Lang/compiler/VM and language documentation.
- [ ] Create and verify the separate bhyvorix-chain repository before moving blockchain code.
- [ ] Inventory every chain-specific module, test, example, workflow, and document; preserve history where practical.
- [ ] Move chain-only code and CI into the chain repository, then update imports/dependencies to consume a versioned Zenvora Lang interface rather than copying implementation files.
- [ ] Run both repositories' full CI after migration; verify Zenvora Lang contains no chain runtime/store/protocol modules and BHYVORIX Chain builds from a clean checkout.
- [ ] Do not delete the existing local-chain implementation until the new repository has equivalent code, tests, and a verified migration commit.

## Gate 4 — Release decision

A release is a **developer preview** while any security, persistence, authorization, or repository-boundary gate above remains unchecked. Do not advertise a public testnet, production consensus, real-fund safety, or a stable smart-contract VM based only on local tests.

Before a stable release:
1. Record the exact release commit and successful CI run URLs.
2. Run tests and release-binary smoke tests on the exact tagged commit.
3. Publish limitations, upgrade/rollback behavior, and supported platforms.
4. Obtain an independent review for security-sensitive code.
5. Reassess every unchecked gate and link evidence for each item.

## Scope guard

The blockchain product is separate from Zenvora Lang. The planned blockchain branding is **BHYVORIX Chain** with native coin ticker **BVRX**; this does not rename Zenvora Lang. Product naming is not evidence of a launched chain, wallet, or publicly available testnet.

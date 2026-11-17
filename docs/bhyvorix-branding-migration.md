# BHYVORIX Ecosystem Branding Migration

## Approved product naming

The project branding direction is:

- **Ecosystem:** BHYVORIX
- **Blockchain:** BHYVORIX Chain
- **Native coin ticker:** BVRX
- **Wallet:** BHYVORIX Wallet
- **Smart-contract runtime / VM:** BHYVORIX VM
- **Explorer:** BHYVORIX Explorer
- **SDK:** BHYVORIX SDK
- **Target GitHub repository name:** `bhyvorix-chain`

## Migration rules

This document records the requested brand direction; it does not claim that all products already exist or that the technical migration is complete.

1. Keep **Zenvora Lang** as the language/compiler name for now. It is an existing compiler project and can target BHYVORIX Chain; changing its language syntax, crate/bin names, bytecode headers, or CLI is a separate compatibility decision.
2. Update public-facing chain documentation, website copy, examples, and release notes to say **BHYVORIX Chain** where they refer to the blockchain. Historical commit messages and immutable release artifacts need not be rewritten.
3. Use **BVRX** as the proposed native-coin ticker in documentation and test fixtures only. Do not imply a mainnet coin, market listing, token value, or real-fund support exists.
4. Do not silently change a chain ID, genesis encoding/hash, block/header encoding, signature domain, or persisted-store format during a branding-only change. Any such change requires an explicit protocol/genesis version and migration or fresh testnet genesis, with regression tests.
5. Describe BHYVORIX Wallet, BHYVORIX Explorer, and BHYVORIX SDK as planned unless their implementation and tests are present in the repository.
6. The GitHub repository target is `vikash7644/bhyvorix-chain`. Repository rename availability and GitHub settings must be checked separately; update badges, release URLs, workflow references, and documentation links only after the repository rename actually succeeds.
7. Verify branding changes with repository-wide searches, `cargo check`, `cargo test`, Clippy, examples, chain-store replay verification, and the relevant GitHub Actions workflows.

## Current implementation boundaries

The current repository contains a Rust Zenvora Lang compiler/VM and a local-development chain prototype. A local signed-chain prototype is not equivalent to a production decentralized network. Peer authentication, automated BFT consensus/finality, production wallet security, a public explorer, and a production SDK must not be claimed unless separately implemented and verified.

## Acceptance checklist

- [ ] Rename the repository to `bhyvorix-chain` in GitHub settings and verify redirects/clone URLs.
- [ ] Update README, website metadata, docs, release links, workflow badges, and repository references.
- [ ] Review all remaining `Zenvora Chain` / old chain branding occurrences; retain Zenvora Lang references intentionally.
- [ ] Decide and document the testnet chain ID/genesis migration before changing consensus-critical identifiers.
- [ ] Add tests for any protocol, genesis, ticker, or serialization changes.
- [ ] Run the full Rust and GitHub Actions verification suite.
- [ ] Keep all unimplemented ecosystem products clearly marked as planned.

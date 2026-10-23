# Fleur Network Roadmap and Acceptance Gates

This roadmap tracks evidence, not optimistic estimates. A milestone is complete only when its acceptance checks have actually passed and logs or CI results are linked.

## M0 — Establish a reproducible upstream baseline

- [ ] Review the current Agave repository, license, supported platforms, and contributor/build documentation.
- [ ] Select and record an exact upstream commit or release tag.
- [ ] Build the unmodified upstream source on a supported Linux runner.
- [ ] Run the upstream project's relevant tests; save the exact commands and results.
- [ ] Record hardware/storage requirements and whether the workflow is practical on a free CI runner.

**Exit gate:** pinned revision, successful build, and test results are recorded. No Fleur protocol changes yet.

## M1 — Local development network

- [ ] Follow upstream-supported local-validator instructions on the pinned revision.
- [ ] Start a local validator without relying on a public network.
- [ ] Query node health and slot/block information through the supported RPC interface.
- [ ] Submit a test transaction and verify its signature/status and resulting state.
- [ ] Capture reproducible logs and commands.

**Exit gate:** a fresh environment can reproduce the local run using documented commands.

## M2 — Fleur network identity and multi-node test

- [ ] Define the intended network identity and genesis/configuration changes.
- [ ] Keep the changes isolated and documented as a small patch set over upstream.
- [ ] Start multiple local nodes using only supported configuration paths.
- [ ] Verify nodes agree on progress and that transactions can be queried consistently.
- [ ] Add automated smoke tests and CI checks where practical.

**Exit gate:** multi-node tests pass repeatedly; no claim of independent production consensus is made without evidence.

## M3 — Public testnet readiness

- [ ] Document validator hardware, operations, monitoring, upgrades, and recovery.
- [ ] Publish RPC behavior and rate limits; do not expose development keys or secrets.
- [ ] Threat-model transaction handling, key management, validator operations, and upgrades.
- [ ] Run load, restart, failure-recovery, and network-partition tests.
- [ ] Invite external testers and publish known limitations.

**Exit gate:** a public testnet can be monitored and reset safely. Testnet assets have no promised monetary value.

## M4 — Wallet, explorer, and developer access

- [ ] Integrate only with a verified network/RPC endpoint.
- [ ] Make wallet signing explicit; never send seed phrases/private keys to a server.
- [ ] Show network identity clearly to prevent users signing for the wrong network.
- [ ] Verify transaction details and explorer data against RPC responses.
- [ ] Label prototypes and unsupported features honestly.

## M5 — Mainnet decision

Mainnet is **not scheduled by a date alone**. It requires a stable protocol, documented upgrade process, independent security review, tested recovery procedures, operational monitoring, and a clear legal/economic model. Do not accept real user funds before these gates are met.

## Progress log

- 2026-10-23: Initial roadmap added. No build or node test is claimed by this entry.
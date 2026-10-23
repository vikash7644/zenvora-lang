# ⚜️ Fleur Network

Fleur Network is the planned blockchain project name. **Zenvora Lang remains a separate, unchanged project.**

> **Status: bootstrap / not yet a running blockchain.** This directory is planning and migration scaffolding only. It does not contain a working Fleur validator, consensus implementation, wallet, RPC service, token, or mainnet.

## Engineering approach

Start by evaluating and pinning the upstream Agave validator client rather than pretending a new consensus engine already exists. Keep upstream changes minimal, document every divergence, and use a separate Fleur genesis/network configuration for local development. Do not use real funds or advertise a public mainnet until the network has passed independent security review.

- Upstream project: https://github.com/anza-xyz/agave
- Agave documentation: https://docs.anza.xyz/
- First milestone: reproduce the upstream build in CI or a supported Linux environment.
- Second milestone: launch a local validator and capture verifiable block/transaction output.
- Third milestone: run multiple local validators and verify RPC queries and transaction behavior.
- Only after those gates pass should a public testnet, wallet, explorer, or mainnet be considered.

## Project files

- [ROADMAP.md](ROADMAP.md) — milestone gates and acceptance criteria.
- [UPSTREAM.md](UPSTREAM.md) — upstream evaluation and reproducibility notes.

## Important

This is an early project scaffold, not evidence that Agave has already been forked or that a node builds. The first implementation step is to pin a specific upstream revision and verify its build/test instructions before changing protocol identity or genesis settings.
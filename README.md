# Zenvora Lang

Zenvora Lang is a lightweight smart-contract DSL / compiler prototype for the Zenvora Chain ecosystem.

Overview
- docs/technical_spec.md — draft technical architecture for the Zenvora Chain
- compiler/ — Rust compiler MVP prototype
- interpreter/ — toy Python interpreter
- examples/ — example .zvl contract source

Quickstart
- Rust compiler MVP:
  cd compiler
  cargo build
  cargo run --release -- ../examples/token.zvl -o ../generated_contract

Goal
- Build an independent blockchain stack: native chain, P2P, RPC, explorer, wallets, WASM-based smart contracts, ZVR token & dev ecosystem

License: MIT

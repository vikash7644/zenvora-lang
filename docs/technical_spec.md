# Zenvora Chain — Technical Architecture v1.0 (Draft)

Status: draft — add/adjust numbers, tokenomics, exact fields after review.

## 1. Goals & Scope
Zenvora is a wholly independent Layer‑1 blockchain stack providing:
- Native coin: ZVR
- Independent P2P network, consensus, validator economics and tooling
- WASM-based smart contract execution (primary)
- Production-grade infra: RPC, explorer, wallets, indexers, testnet → mainnet

Design priorities:
1. Security & auditability
2. Deterministic behavior & upgradeability
3. Developer ergonomics
4. Reasonable throughput & low fees
5. Interoperability (2nd stage)

## 2. High-level architecture
- Node: networking (libp2p), consensus, tx pool, executor (WASM runtime), state DB, RPC server
- Storage: RocksDB / paritydb for chain DB, snapshots for fast sync, archive mode option
- Execution: WASM runtime (wasmtime/wasmer) with host API for storage/gas/events
- Signatures: ED25519 / secp256k1 support configurable by chain
- Devtooling: compiler (zenvora-lang) → Rust/WASM, CLI, SDKs (TS/Python/Rust)

## 3. Protocol primitives (Block/Tx/State)
Block header (fields)
- version: u32
- chain_id: u32
- parent_hash: H256
- height: u64
- timestamp: u64 (ms)
- proposer: Address
- state_root: H256
- transactions_root: H256
- receipts_root: H256
- extra: bytes (future)
- signature(s)

Transaction basic format
- version: u8
- nonce: u64
- chain_id: u32
- from: Address
- to: Option<Address>
- gas_limit: u64
- gas_price: u128
- value: u128
- payload: bytes
- signature: bytes
- tx_type: u8

Receipt format
- status: success/failure
- gas_used: u64
- logs: [LogEntry]
- bloom: Bloom filter
- return_data: bytes

State model
- Accounts: nonce, balance, storage root (per-contract)
- Global state root: Merkle/Patricia or sparse Merkle (recommend sparse Merkle for light clients)

Merkle proofs & light clients
- Canonical proof format for accounts & storage slots
- Proof verification API in SDKs

## 4. Networking (P2P)
- Use libp2p for modular peer discovery, multiplexing & NAT
- Node roles: full, validator, archive, light
- Message types: handshake, peer_list, new_block, new_tx, request_block, response_block, ping/pong, snapshot
- Gossip for txs & blocks, with block priority
- Peer scoring, banning, bootstrap seed nodes, rate limiting

## 5. Consensus & Validators
- Recommended initial choice: Substrate (Aura + Grandpa) or Tendermint-style PoS
- Validator registration, epochs, proposer rotation, slashing, jailing, rewards, commission
- Finality: deterministic BFT or practical finality via Grandpa/Tendermint

## 6. Execution VM & Smart Contracts
- VM: WASM (primary)
- Host API examples:
  - storage_read(key) -> bytes
  - storage_write(key, value)
  - emit_event(topic, data)
  - get_caller() -> address
  - get_block_number() -> u64
  - get_block_timestamp() -> u64
  - charge_gas(units)
- Gas model: base tx cost + opcode + host costs (storage read/write expensive)
- Contract lifecycle: deploy -> store bytecode -> call -> receipts & logs
- ABI: zenvora-lang generates ABI for contracts

EVM compatibility
- Optional later: EVM pallet / compatibility layer

## 7. RPC & SDKs
JSON-RPC endpoints (subset)
- chain_id, getBlockByNumber, getTransaction, sendRawTransaction, getBalance, getValidators
WebSocket subscriptions: newHeads, logs, pendingTxs
SDKs: TypeScript, Rust, Python
CLI: zenvora init / run-node / accounts / send / deploy / call / faucet

## 8. Sync modes & nodes
- Full sync, Fast sync/snapshot, Light client, Archive mode
- Snapshot format: signed snapshot with chunking & integrity checks

## 9. Tokenomics (skeleton)
- Total supply: X ZVR (decide)
- Genesis allocation: treasury 20%, team 10% (vesting), ecosystem/grants 20%, community/testnet 30%, reserve 20% (example)
- Inflation model: define or fixed supply
- Fee model: burn vs distribution (split to proposer, treasury, stakers)

## 10. Governance & Upgrades
- On-chain proposals, voting, delegated voting, treasury governance
- Upgrade timelocks, emergency pause, runtime upgrades via governance

## 11. Security & testing
- Unit/integration/property tests, fuzzing, static analysis, audits, bug bounty, incident response

## 12. Explorer & Indexing
- Indexer service (Postgres), explorer APIs, event indexing, contract verification

## 13. Bridges & Interoperability
- Lock/mint initial bridge, move to decentralised relayers + light-client proofs later

## 14. Monitoring & Metrics
- Prometheus metrics from nodes: block production latency, mempool size, peer count, cpu/mem, rpc latency
- Grafana dashboards + alerting for validator operators

## 15. Dev process, repo layout & milestones
Suggested repo layout (single monorepo):
```
zenvora/
├─ node/                # node implementation (Substrate or custom)
├─ runtime/             # WASM runtime, host functions
├─ compiler/            # zenvora-lang compiler (Rust)
├─ interpreter/         # quick dev interpreter (toy)
├─ sdk/                 # ts/python/rust sdks
├─ explorer/            # indexer + frontend
├─ wallet/              # web & mobile wallets
├─ docs/                # spec & design docs
├─ scripts/             # deploy & dev scripts
├─ testnets/            # testnet config & genesis
└─ ci/                  # CI workflows & infra
```

Initial milestones (high-level)
- M0: Finalize spec v1.0 & repo scaffold (this task)
- M1: Node prototype (single-node local): genesis, block store, tx processing, db
- M2: P2P prototype & local multi-node sync
- M3: Consensus & validator staking prototype (private net)
- M4: RPC, CLI, wallet CLI, testnet scripts
- M5: Smart-contract VM integration (WASM) & zenvora-lang compiler MVP
- M6: Explorer + indexer + SDKs + faucet
- M7: Public testnet + audits
- M8: Mainnet launch (after audits, incentives & validators)

## 16. Versioning & release criteria
- Semantic versioning for node/runtime/tooling (major.minor.patch)
- Release criteria for each milestone: passing test-suite, stability for N hours under load tests, security review

## 17. Open questions (to finalize)
- final consensus choice (Substrate vs custom)
- cryptography scheme(s) (secp256k1, ed25519, bls options)
- final tokenomics numeric parameters
- EVM compatibility requirement & schedule

--- end of draft spec (adjust as we iterate) ---

# Zenvora Chain verification and RPC tools

These tools exercise the **current local-development prototype**. They do not establish production readiness or a public RPC endpoint.

## 1. Replay-verify a block store

The verifier invokes the real Zenvora CLI's `--chain-store-info` command. That command loads and replays the entire store, checking block linkage, signatures, transaction roots, receipt roots, and resulting state roots. The script also checks the expected chain ID, contiguous zero-based height/count, and 32-byte state/tip hashes.

```sh
cargo build --bin zv
sh scripts/verify_chain_store.sh ./target/debug/zv dev.zvb 71
# Optionally require an exact tip height:
sh scripts/verify_chain_store.sh ./target/debug/zv dev.zvb 71 3
```

The file must already exist and be readable by the CLI. A successful check is local replay verification, not a distributed consensus or finality proof.

## 2. RPC smoke checks

The current server accepts JSON-RPC 2.0 requests at `POST /` and implements:

- `chain_getInfo`
- `chain_getBalance` (optional check with `RPC_TEST_ADDRESS`)
- `chain_getBlock` (tip height by default, or `RPC_TEST_HEIGHT`)
- `chain_getTransaction`
- Pool-enabled mode additionally exposes `chain_submitTransaction` and `chain_getPendingTransactions`; these smoke checks do not submit transactions.

```sh
RPC_URL=http://127.0.0.1:8545/ sh scripts/check_rpc.sh
```

Optional known-address and explicit-height checks:

```sh
RPC_URL=http://127.0.0.1:8545/ \
RPC_TEST_ADDRESS=alice \
RPC_TEST_HEIGHT=0 \
sh scripts/check_rpc.sh
```

The Rust alternative is:

```sh
RPC_URL=http://127.0.0.1:8545/ cargo run --bin check_node_rpc
```

## 3. End-to-end CI

The `.github/workflows/chain-smoke.yml` workflow creates a disposable local key and chain, signs and creates a transfer block, replay-verifies the resulting store, starts a loopback RPC server, then checks chain info, balance, and block retrieval. This tests a real local workflow inside GitHub Actions; it does **not** create a publicly reachable node URL.

## Safety and limitations

- RPC intentionally binds to `127.0.0.1`; it is not a public service. Do not expose it to the internet.
- The current server uses plain HTTP and does not validate bearer tokens. `RPC_TOKEN` only adds a header for use with a trusted proxy/future auth layer; it does not enable node-side authentication.
- `/status`, `/compile`, `/deploy`, contract `call`, and `view` endpoints are not implemented. Contract compilation is local and the contract DSL is not yet blockchain deployment.
- Passing smoke checks does not prove decentralization, BFT consensus/finality, wallet safety, or production security. Do not use real funds.

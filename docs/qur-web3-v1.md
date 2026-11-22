# QORVYNT Universal Runtime (QUR) — Web3 Manifest v1

QUR is the proposed unified runtime layer for QORVYNT projects. This first implementation milestone adds a **manifest parser and validator** for chain, token, DAO, and wallet declarations.

## Validate a manifest

```qorvynt
chain MyChain {
    consensus = "proof_of_stake"
    native_token = "QOR"
}

token GOLD {
    standard = "fungible"
    supply = 100000000
}

dao Community {
    governance = "on_chain"
}

wallet MyWallet {
    network = MyChain
}
```

Run `qorvynt --qur-check examples/qur/manifest.qv` for a human-readable validation summary, or `qorvynt --qur-json examples/qur/manifest.qv` for a stable JSON manifest suitable for editor integrations and future cloud tooling.

The validator checks declaration names, duplicates, allowed and required fields, supported consensus/governance/token modes, positive u64 token supply, ticker shape, and wallet-to-chain references. It enforces a 1 MiB source limit and a 256-declaration limit. The CI suite also checks a deliberately invalid wallet-to-chain reference and requires `--qur-json` to return `valid: false` with a non-zero exit status.

## Local token-ledger runtime prototype

After validating the manifest, initialize a deterministic local token ledger:

```sh
qorvynt --qur-token-init examples/qur/manifest.qv token-ledger.json
qorvynt --qur-token-transfer token-ledger.json GOLD genesis alice 25
qorvynt --qur-token-balance token-ledger.json GOLD alice
```

Genesis allocates each declared token's full configured supply to the reserved `genesis` account. Transfers use checked integer arithmetic, reject zero amounts, unknown tokens, invalid account identifiers, insufficient funds, and sender/recipient aliasing. Every mutation preserves the declared total supply and writes a canonical state root into the JSON snapshot; the loader verifies that root before accepting a snapshot. Failed transfers leave the ledger unchanged.

This is a local single-process ledger prototype. It is not yet backed by QVM contract bytecode, chain transaction admission, consensus, validator signatures, or peer-to-peer replication. The next integration milestone is to expose these state transitions as deterministic, gas-metered QVM contract calls rather than treating this file ledger as a production token network.

### QVM-metered contract-storage transfer

The experimental CLI can also apply a transfer through the deterministic contract-storage layer:

```sh
qorvynt --qur-token-transfer-qvm token-ledger.json contract-state.bin qur_tokens GOLD genesis alice 25 30000
```

The command charges the versioned contract gas schedule, stages supply and account-balance keys in a write overlay, and commits the in-memory token ledger only after the contract-state overlay succeeds. It reports gas used and the contract storage root. This is a local integration prototype: it uses the repository's deterministic gas meter and contract storage module, but token transfer is not yet emitted as QVM bytecode/opcodes or admitted as a signed chain transaction. The two local snapshot files are not a cross-file atomic database transaction; production integration must move both records into one chain-state transaction.

## Execution boundary

This milestone **validates configuration only**. It does not create a blockchain node, start Proof of Stake, mint tokens, deploy contracts, create wallets/private keys, run a DAO, or launch an exchange. A consensus label is not a consensus implementation.

The repository separately contains experimental QVM-backed contract execution, deterministic gas primitives, isolated contract state, and local chain transaction/block-store prototypes. Those remain prototypes, not a production network or complete BFT consensus system. QUR will integrate with them incrementally and retain their security gates.

## Manifest v1 constraints

- One global namespace for declaration names.
- Consensus: `proof_of_stake`, `proof_of_authority`, or `proof_of_work`.
- Native ticker: 2–12 uppercase ASCII letters/digits.
- Token standard: `fungible` or `non_fungible`; supply is a positive 64-bit integer.
- DAO governance: `on_chain`, `off_chain`, or `hybrid`.
- Wallet network must reference a chain in the same manifest.
- Quoted-string escapes and semicolons are not supported in v1.


### Deterministic transfer events

Gas-metered token transfer receipts now include a `TokenTransfer` event with token, sender, recipient, amount, and a domain-separated SHA-256 event hash. The event payload is encoded with length-prefixed fields and charged through the versioned event gas schedule. The CLI prints the event and hash; CI checks the receipt fields. This is a local receipt/event prototype, not yet a canonical block receipt or a QVM bytecode event opcode.


### DAO proposal and voting runtime (local prototype)

The experimental DAO runtime persists proposal records and individual voter markers in the contract-state snapshot. It supports:

```sh
qorvynt --qur-dao-propose dao-state.bin community proposal1 alice "Fund public goods" 2 30000
qorvynt --qur-dao-vote dao-state.bin community proposal1 alice yes 30000
qorvynt --qur-dao-vote dao-state.bin community proposal1 bob yes 30000
qorvynt --qur-dao-finalize dao-state.bin community proposal1 30000
qorvynt --qur-dao-show dao-state.bin community proposal1
```

Each operation uses the deterministic local gas meter and an atomic contract-state overlay, and emits a state root plus a domain-separated event hash. A voter can vote only once per proposal. Finalization requires the configured vote-count quorum; if quorum is met, yes votes must strictly exceed no votes for approval. A tie is rejected. Proposal status is terminal after finalization.

Important limitations: voter identifiers are currently caller-supplied labels, not authenticated wallet identities; quorum counts votes, not token-weighted voting power; there is no voting deadline, delegation, treasury execution, timelock, chain transaction admission, or consensus replay. An `Approved` status records the result only and does not execute arbitrary actions. Treat this as a local governance-engine prototype, not a production DAO.


### Wallet-authenticated DAO voting

The signed-vote path uses Ed25519 keys from the existing QORVYNT key-file format. The wallet signs a domain-separated message containing the DAO ID, proposal ID, and vote choice. The runtime verifies the signature before deriving the voter address from the public key and recording the vote. The voter address is the replay-protection key, so one wallet can vote only once per proposal.

```sh
qorvynt --chain-keygen alice.qvk
qorvynt --qur-dao-vote-signed dao-state.bin community proposal1 alice.qvk yes 30000
```

The older `--qur-dao-vote` command remains only as a local test/development path and accepts an unauthenticated label; do not use it for real governance. Signed voting authenticates key ownership but does not yet establish token-weighted voting power, membership eligibility, chain-level transaction finality, hardware-wallet signing, or a consensus-enforced DAO.


### DAO treasury and block-height timelock (local accounting prototype)

DAO proposals may carry a single treasury-transfer action. The local contract-state ledger supports funding the DAO treasury, creating a transfer proposal with a positive quorum and block delay, finalizing it at an explicit block height, and executing it only after the delay has elapsed. Execution atomically debits the treasury, stores a transfer record, and marks the proposal executed. Re-execution and insufficient funds are rejected without partial state changes.

Example CLI flow:

```sh
qorvynt --qur-dao-treasury-fund dao-state.bin community 100 30000
qorvynt --qur-dao-propose-treasury dao-state.bin community payout1 founder "Contributor payout" contributor 40 1 5 30000
qorvynt --qur-dao-vote-signed dao-state.bin community payout1 alice.qvk yes 30000
qorvynt --qur-dao-finalize-height dao-state.bin community payout1 100 30000
qorvynt --qur-dao-treasury-execute dao-state.bin community payout1 105 30000
qorvynt --qur-dao-treasury-balance dao-state.bin community
```

This is not yet a transfer of native tokens or chain assets: treasury funding is local accounting state, the recipient is an identifier, and the supplied block height is not authenticated against a canonical chain header. Production use requires chain transaction admission, authenticated block context, token custody/escrow, consensus replay, and an on-chain execution adapter.


### DAO treasury payouts using the QUR token ledger

The DAO runtime now has a token-backed treasury execution path. Fund the DAO's token account by transferring tokens to the deterministic account `dao_<dao-id>`, then create and approve a treasury proposal with a signed wallet vote. At the authenticated integration boundary currently exposed by the local CLI, pass the block height explicitly; the transfer will be rejected before the timelock expires. On success, the QUR token ledger moves the proposal amount from `dao_<dao-id>` to the proposal recipient, preserves total token supply, stores the DAO execution marker and token balances in the contract-state overlay, and marks the proposal executed.

```sh
qorvynt --qur-token-init examples/qur/manifest.qv token-ledger.json
qorvynt --qur-token-transfer token-ledger.json GOLD genesis dao_community 100
qorvynt --qur-dao-propose-treasury dao-state.bin community payout1 founder "Contributor payout" contributor 40 1 5 30000
qorvynt --qur-dao-vote-signed dao-state.bin community payout1 alice.qvk yes 30000
qorvynt --qur-dao-finalize-height dao-state.bin community payout1 100 30000
qorvynt --qur-dao-treasury-token-execute token-ledger.json dao-state.bin community payout1 GOLD 105 50000
```

The execution API clones the token ledger, validates the transfer, stages DAO proposal and token-balance writes in one contract-state overlay, commits that overlay, and only then publishes the in-memory ledger candidate. The CLI stages both files before replacing them, but two separate filesystem renames are not a crash-atomic database transaction. This remains a local QUR runtime integration: the CLI-supplied height is not checked against a canonical block header, token balances are not yet part of consensus replay, and wallet signatures authenticate votes but not a network transaction envelope. Do not use it as production chain treasury custody yet.


### Token-balance-weighted wallet voting

The signed DAO voting API includes a token-weighted path. It verifies the Ed25519 domain-separated vote message, derives the wallet address from the verified public key, reads the selected token balance, and records that balance as the vote weight. Zero-balance wallets cannot vote; each derived wallet address can vote only once per proposal.

```sh
qorvynt --qur-dao-vote-weighted token-ledger.json dao-state.bin community proposal1 alice.qvk GOLD yes 30000
```

This is a balance-at-vote-time prototype, not yet secure token governance: balances are not snapshotted at proposal creation and tokens are not locked, so holders may transfer tokens between wallets and potentially reuse voting power. Production governance must use a consensus-authenticated snapshot height, immutable voting-power accounting or token locking/delegation, and chain-enforced replay protection.

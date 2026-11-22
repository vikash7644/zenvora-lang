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

Run `qorvynt --qur-check examples/qur_web3.qv`.

The validator checks declaration names, duplicates, allowed and required fields, supported consensus/governance/token modes, positive u64 token supply, ticker shape, and wallet-to-chain references. It enforces a 1 MiB source limit and a 256-declaration limit.

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

# QORVYNT Native Token State (Experimental)

This module adds a checked fungible-token ledger to the existing QVM 2.0 contract account state. It does not introduce another VM and it is not EVM bytecode compatibility.

## Implemented state and operations

- Token metadata: name, symbol, decimals (0–18), owner, total supply.
- Balances keyed by token ID and account.
- Allowances keyed by token ID, owner, and spender.
- Checked transfer, approve, transfer_from, owner-only mint, and holder burn.
- Deterministic Transfer and Approval event records.
- Bounded token registry and event history.
- Canonical snapshot encoding/decoding; token state is included in the contract executor snapshot and contract state root.
- Signed token actions reuse ContractTransactionV1, sender authentication, chain ID, nonce protection, gas limit, fee settlement, block execution and replay.

## Signed call interface

The token factory call target is qorvynt_token_factory.

Create a token with a signed contract call:
- target: qorvynt_token_factory
- method: create
- calldata: name,symbol,decimals,initial_supply

The token ID is deterministically derived from the signed transaction ID. The successful call receipt returns that token ID.

For a token ID returned by the factory, signed calls support:

| Method | Calldata | Authorization |
|---|---|---|
| transfer | recipient,amount | Sender spends own balance |
| approve | spender,amount | Sender sets allowance |
| transfer_from | owner,recipient,amount | Sender spends allowance |
| mint | recipient,amount | Token owner only |
| burn | amount | Sender burns own balance |

Amounts are unsigned integer base units; decimals controls display precision only. The current calldata encoding is a bounded UTF-8 comma-separated form, so names/accounts containing commas are not supported by this interface.

## Loopback JSON-RPC queries

When the contract pool is enabled, token state can be queried through:
- chain_getTokenInfo with tokenId
- chain_getTokenBalance with tokenId and account
- chain_getTokenAllowance with tokenId, owner, and spender
- chain_getTokenEvents with tokenId; optional limit (1–500, default 100) and offset (default 0) bound the response and support pagination. The response includes page count, total matching events, limit, and offset.

Large integer values (total supply, balance, allowance) are returned as decimal strings to avoid JSON number precision loss. The local RPC server binds to loopback only; this is not a public RPC endpoint.

## Important boundaries

- This is an experimental QORVYNT-native fungible-token interface, not a claim of complete ERC-20 conformance or an Ethereum deployment path.
- Token factory calls and token actions execute through the existing signed QVM contract transaction executor; they do not compile Solidity or create EVM bytecode.
- The token ledger has deterministic state and persistence, but a public wallet UX, token explorer/API surface, fee schedule activation, and production consensus are separate release gates.
- Do not use development keys or this local chain prototype for real funds.

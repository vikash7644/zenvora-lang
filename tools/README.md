# Zenvora Chain RPC smoke checks

These checks target the **current prototype**, not a production RPC API. The current server accepts JSON-RPC 2.0 requests at \`POST /\` and implements:

- \`chain_getInfo\` (required)
- \`chain_getBalance\` (optional check with \`RPC_TEST_ADDRESS\`)
- \`chain_getBlock\` (checks the current height by default, or \`RPC_TEST_HEIGHT\`)
- Transaction submission/pending-transaction methods exist only when the node is started with the pool-enabled RPC mode; this smoke harness intentionally does not submit transactions.

**Not implemented by this RPC:** \`/status\`, \`/compile\`, \`/deploy\`, contract \`call\`, or \`view\` endpoints. The harness must not pretend these are available. Contract compilation is currently a local compiler feature, and the contract DSL is not yet blockchain deployment.

## Safety

- The node deliberately binds to \`127.0.0.1\` and is not a public service. Keep it that way for local testing.
- The current server uses plain HTTP. Do not send credentials across an untrusted network. \`RPC_TOKEN\` is an optional header for a trusted proxy or future auth layer; the current node itself does **not** validate bearer tokens.
- The checks are read-only and do not create transactions, change balances, or produce blocks.
- A passing result proves only that the endpoint answered the checked methods. It does not prove decentralization, consensus/finality, wallet safety, or production readiness.

## Option 1: Rust CLI

Build and run from the repository root:

\`\`\`sh
RPC_URL=http://127.0.0.1:8545/ cargo run --bin check_node_rpc
\`\`\`

Optional checks:

\`\`\`sh
RPC_URL=http://127.0.0.1:8545/ \\
RPC_TEST_ADDRESS=alice \\
RPC_TEST_HEIGHT=0 \\
cargo run --bin check_node_rpc
\`\`\`

\`RPC_TOKEN\` can add an \`Authorization: Bearer ...\` header for a trusted reverse proxy. It does not enable authentication in the current node.

The Rust checker uses only existing dependencies and standard-library TCP, and supports plain HTTP only.

## Option 2: curl script

Requires \`curl\` and a running local node:

\`\`\`sh
RPC_URL=http://127.0.0.1:8545/ sh scripts/check_rpc.sh
\`\`\`

Optional balance and explicit block-height checks:

\`\`\`sh
RPC_URL=http://127.0.0.1:8545/ \\
RPC_TEST_ADDRESS=alice \\
RPC_TEST_HEIGHT=0 \\
sh scripts/check_rpc.sh
\`\`\`

Start a local read-only RPC in another terminal after initializing a local chain store:

\`\`\`sh
cargo run -- --chain-rpc dev.zvb 71 8545
\`\`\`

Use the actual store path and chain ID from your local test setup. Do not expose this prototype to the public internet.

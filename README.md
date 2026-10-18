# Zenvora Lang

Zenvora Lang is the planned custom programming language for the Zenvora ecosystem. The repository currently contains architecture documentation and a sample `.zvl` contract; it does **not** yet contain the compiler/interpreter folders described in earlier drafts of this README.

## Repository contents

- `docs/technical_spec.md` — draft architecture for the proposed Zenvora Chain; it is a design document, not a completed chain implementation.
- `docs/ECOSYSTEM_AUDIT_2026-10-18.md` — audit of the connected GitHub repositories, Vercel project/deployment metadata, and Supabase visibility.
- `examples/token.zvl` — illustrative smart-contract DSL example; it is not currently executable by a compiler in this repository.
- `website/` — static, responsive Zenvora ecosystem landing page. It needs no package installation or build step.

## Website preview

Open `website/index.html` directly in a browser, or serve the `website` directory with any static HTTP server.

Vercel deployment requires access to the intended Vercel project. The linked project/deployment scope must be verified before claiming that this source is deployed.

## Long-term goal

Build an independent Web3 stack in verifiable stages:

1. Stabilize the custom language and tests.
2. Define and implement a deterministic bytecode/VM or compiler runtime.
3. Build and test chain primitives, signatures, state validation, networking and consensus.
4. Add RPC, SDK, explorer and wallet against a testnet.
5. Consider DEX, governance and other dApps only after core protocol and security work.

## Development status

This is work in progress. Architecture notes and examples are not proof of a functioning compiler, blockchain, DEX, wallet, or mainnet. Use test data only until each component has been implemented, tested and reviewed.

## License

MIT

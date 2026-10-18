//! Append-only local block store with deterministic replay verification.
//!
//! The store persists genesis allocations, canonical headers, signed transaction envelopes, and
//! public keys. Loading replays every block through the atomic execution path and rejects any
//! state/receipt/transaction-root mismatch. This is local persistence, not distributed consensus.

use std::collections::BTreeMap;

use super::chain_block::{execute_block, SignedTransactionEnvelope};
use super::chain_protocol::{transaction_merkle_root, BlockHeaderV1, TransactionV1};
use super::chain_runtime::{ChainRuntime, TransferReceipt};

const STORE_VERSION: &str = "ZENVORA_BLOCK_STORE_V1";
const MAX_STORE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_STORE_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_GENESIS_ACCOUNTS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlock {
    pub header: BlockHeaderV1,
    pub transactions: Vec<SignedTransactionEnvelope>,
}

#[derive(Debug, Clone)]
pub struct BlockStore {
    chain_id: u64,
    genesis_balances: BTreeMap<String, i64>,
    blocks: Vec<StoredBlock>,
}

impl BlockStore {
    pub fn new(chain_id: u64, genesis_balances: BTreeMap<String, i64>) -> Result<Self, String> {
        if chain_id == 0 {
            return Err("chain ID must be nonzero".into());
        }
        if genesis_balances.is_empty() || genesis_balances.len() > MAX_GENESIS_ACCOUNTS {
            return Err("genesis account count is outside the permitted range".into());
        }
        let mut check = ChainRuntime::new();
        for (address, balance) in &genesis_balances {
            check.set_genesis_balance(address, *balance)?;
        }
        Ok(Self { chain_id, genesis_balances, blocks: Vec::new() })
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn blocks(&self) -> &[StoredBlock] {
        &self.blocks
    }

    /// Persist a block only after the caller has executed it successfully.
    /// The store independently checks linkage, signatures, and all supplied commitments.
    pub fn append_executed_block(
        &mut self,
        path: impl AsRef<std::path::Path>,
        runtime_after: &ChainRuntime,
        header: &BlockHeaderV1,
        transactions: &[SignedTransactionEnvelope],
        block_receipts: &[TransferReceipt],
    ) -> Result<(), String> {
        if header.chain_id != self.chain_id {
            return Err("block chain ID does not match store".into());
        }
        header.canonical_bytes()?;
        if runtime_after.genesis_balances() != &self.genesis_balances {
            return Err("runtime genesis allocations do not match block store".into());
        }

        match self.blocks.last() {
            Some(previous) => {
                let expected_height = previous.header.height.checked_add(1)
                    .ok_or_else(|| "previous block height overflow".to_string())?;
                if header.height != expected_height {
                    return Err("block height is not contiguous".into());
                }
                if header.parent_hash != previous.header.header_hash()? {
                    return Err("block parent hash does not match stored tip".into());
                }
                if header.timestamp_secs < previous.header.timestamp_secs {
                    return Err("block timestamp precedes stored tip".into());
                }
            }
            None => {
                if header.height != 0 || header.parent_hash != [0; 32] || !transactions.is_empty() {
                    return Err("first stored block must be an empty genesis block at height zero".into());
                }
            }
        }

        let txs: Vec<TransactionV1> = transactions.iter()
            .map(|envelope| envelope.transaction.clone())
            .collect();
        if header.transaction_root != transaction_merkle_root(&txs)? {
            return Err("block transaction root does not match supplied transactions".into());
        }
        if header.receipt_root != ChainRuntime::receipt_root_for(block_receipts)? {
            return Err("block receipt root does not match supplied receipts".into());
        }
        if header.state_root != runtime_after.state_root() {
            return Err("block state root does not match resulting runtime".into());
        }
        if runtime_after.receipts().len() < block_receipts.len()
            || runtime_after.receipts()[runtime_after.receipts().len() - block_receipts.len()..] != *block_receipts {
            return Err("block receipts are not the suffix of the resulting runtime log".into());
        }
        for envelope in transactions {
            envelope.transaction.validate_for(self.chain_id, header.height)?;
            envelope.transaction.verify_signature(&envelope.public_key)?;
        }

        self.blocks.push(StoredBlock { header: header.clone(), transactions: transactions.to_vec() });
        if let Err(error) = self.save_to_file(path) {
            self.blocks.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        use std::io::Write;
        let path = path.as_ref();
        let temporary = path.with_extension("zblocks.tmp");
        let result = (|| -> Result<(), String> {
            let mut file = std::fs::File::create(&temporary)
                .map_err(|error| format!("cannot create block-store temporary file: {error}"))?;
            writeln!(file, "{STORE_VERSION}").map_err(|error| format!("cannot write block store: {error}"))?;
            writeln!(file, "C\t{}", self.chain_id).map_err(|error| format!("cannot write chain ID: {error}"))?;
            for (address, balance) in &self.genesis_balances {
                writeln!(file, "G\t{address}\t{balance}")
                    .map_err(|error| format!("cannot write genesis allocation: {error}"))?;
            }
            for block in &self.blocks {
                let header_bytes = block.header.canonical_bytes()?;
                writeln!(file, "B\t{}", hex_encode(&header_bytes))
                    .map_err(|error| format!("cannot write block header: {error}"))?;
                for (index, envelope) in block.transactions.iter().enumerate() {
                    let transaction_bytes = envelope.transaction.canonical_bytes()?;
                    writeln!(file, "T\t{}\t{}\t{}\t{}",
                        block.header.height,
                        index,
                        hex_encode(&envelope.public_key),
                        hex_encode(&transaction_bytes))
                        .map_err(|error| format!("cannot write transaction envelope: {error}"))?;
                }
            }
            file.sync_all().map_err(|error| format!("cannot sync block store: {error}"))?;
            let length = file.metadata().map_err(|error| format!("cannot inspect block store: {error}"))?.len();
            if length > MAX_STORE_BYTES {
                return Err("block store exceeds maximum file size".into());
            }
            std::fs::rename(&temporary, path)
                .map_err(|error| format!("cannot replace block store: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    /// Load the block store and replay all blocks from genesis, returning the resulting runtime.
    pub fn load_from_file(
        path: impl AsRef<std::path::Path>,
        expected_chain_id: u64,
    ) -> Result<(Self, ChainRuntime), String> {
        let path = path.as_ref();
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("cannot inspect block store: {error}"))?;
        if metadata.len() > MAX_STORE_BYTES {
            return Err("block store exceeds maximum file size".into());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read block store: {error}"))?;
        let mut lines = content.lines();
        if lines.next() != Some(STORE_VERSION) {
            return Err("unsupported or missing block-store version".into());
        }

        let mut chain_id = None;
        let mut genesis_balances = BTreeMap::new();
        let mut blocks: Vec<StoredBlock> = Vec::new();
        for (index, line) in lines.enumerate() {
            let line_number = index + 2;
            if line.len() > MAX_STORE_LINE_BYTES {
                return Err(format!("block-store line {line_number} exceeds maximum length"));
            }
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["C", value] if chain_id.is_none() && blocks.is_empty() && genesis_balances.is_empty() => {
                    let parsed: u64 = value.parse()
                        .map_err(|_| format!("block-store line {line_number} has invalid chain ID"))?;
                    if parsed == 0 || parsed != expected_chain_id {
                        return Err("block-store chain ID does not match expected network".into());
                    }
                    chain_id = Some(parsed);
                }
                ["G", address, balance] if chain_id.is_some() && blocks.is_empty() => {
                    let balance: i64 = balance.parse()
                        .map_err(|_| format!("block-store line {line_number} has invalid genesis balance"))?;
                    if balance < 0 || genesis_balances.contains_key(*address)
                        || genesis_balances.len() >= MAX_GENESIS_ACCOUNTS {
                        return Err(format!("block-store line {line_number} has invalid or duplicate genesis allocation"));
                    }
                    genesis_balances.insert((*address).to_string(), balance);
                }
                ["B", encoded_header] if chain_id.is_some() && !genesis_balances.is_empty() => {
                    let bytes = hex_decode(encoded_header)?;
                    let header = BlockHeaderV1::from_canonical_bytes(&bytes)?;
                    if header.chain_id != expected_chain_id {
                        return Err(format!("block-store line {line_number} has wrong chain ID"));
                    }
                    blocks.push(StoredBlock { header, transactions: Vec::new() });
                }
                ["T", height, transaction_index, encoded_public_key, encoded_transaction]
                    if chain_id.is_some() && !blocks.is_empty() => {
                    let current = blocks.last_mut().expect("checked non-empty block list");
                    let parsed_height: u64 = height.parse()
                        .map_err(|_| format!("block-store line {line_number} has invalid transaction height"))?;
                    let parsed_index: usize = transaction_index.parse()
                        .map_err(|_| format!("block-store line {line_number} has invalid transaction index"))?;
                    if parsed_height != current.header.height || parsed_index != current.transactions.len() {
                        return Err(format!("block-store line {line_number} has non-contiguous transaction ordering"));
                    }
                    let public_key = hex_decode(encoded_public_key)?;
                    if public_key.len() != 32 {
                        return Err(format!("block-store line {line_number} has invalid public-key length"));
                    }
                    let transaction = TransactionV1::from_canonical_bytes(&hex_decode(encoded_transaction)?)?;
                    current.transactions.push(SignedTransactionEnvelope { transaction, public_key });
                }
                _ => return Err(format!("block-store line {line_number} has an invalid record or ordering")),
            }
        }

        let chain_id = chain_id.ok_or_else(|| "block store has no chain ID".to_string())?;
        let store = Self::new(chain_id, genesis_balances)?;
        let mut store = store;
        store.blocks = blocks;

        let mut runtime = ChainRuntime::new();
        for (address, balance) in &store.genesis_balances {
            runtime.set_genesis_balance(address, *balance)?;
        }
        let mut previous_header: Option<BlockHeaderV1> = None;
        for block in &store.blocks {
            execute_block(
                &mut runtime,
                &block.header,
                &block.transactions,
                expected_chain_id,
                previous_header.as_ref(),
            )?;
            previous_header = Some(block.header.clone());
        }
        Ok((store, runtime))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("invalid lowercase hexadecimal field".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        output.push(((high << 4) | low) as u8);
    }
    Ok(output)
}

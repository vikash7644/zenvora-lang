//! Durable contract-block store. The canonical runtime is rebuilt by replaying every
//! stored signed contract block from the genesis executor snapshot; cached tip state is never
//! trusted as an independent source of truth.

use std::io::{Read, Write};
use serde::{Deserialize, Serialize};
use super::contract_block::{execute_contract_block, ContractBlockExecution, ContractBlockHeaderV1, SignedContractEnvelope};
use super::contract_executor::ContractAccountState;

const STORE_FORMAT: &str = "QORVYNT_CONTRACT_BLOCK_STORE_V1";
const MAX_STORE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BLOCKS: usize = 1_000_000;
const MAX_TRANSACTIONS_PER_BLOCK: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredContractBlock {
    pub header: ContractBlockHeaderV1,
    pub transactions: Vec<SignedContractEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreFile {
    format: String,
    chain_id: u64,
    genesis_executor_snapshot: Vec<u8>,
    blocks: Vec<StoredContractBlock>,
}

#[derive(Debug, Clone)]
pub struct ContractBlockStore {
    chain_id: u64,
    genesis_executor_snapshot: Vec<u8>,
    blocks: Vec<StoredContractBlock>,
    runtime: ContractAccountState,
}

impl ContractBlockStore {
    pub fn new(chain_id: u64, genesis_state: ContractAccountState) -> Result<Self, String> {
        if chain_id == 0 { return Err("contract block store chain ID must be nonzero".into()); }
        let snapshot = genesis_state.encode_snapshot()?;
        Ok(Self { chain_id, genesis_executor_snapshot: snapshot, blocks: Vec::new(), runtime: genesis_state })
    }

    pub fn chain_id(&self) -> u64 { self.chain_id }
    pub fn blocks(&self) -> &[StoredContractBlock] { &self.blocks }
    pub fn runtime(&self) -> &ContractAccountState { &self.runtime }

    /// Execute against a candidate, persist the complete replay log, and only then advance tip.
    pub fn append_block(
        &mut self,
        path: impl AsRef<std::path::Path>,
        header: &ContractBlockHeaderV1,
        transactions: &[SignedContractEnvelope],
    ) -> Result<ContractBlockExecution, String> {
        if self.blocks.len() >= MAX_BLOCKS { return Err("contract block store block limit exceeded".into()); }
        if transactions.len() > MAX_TRANSACTIONS_PER_BLOCK { return Err("contract block transaction count exceeds limit".into()); }
        if header.chain_id != self.chain_id { return Err("contract block chain ID does not match store".into()); }
        let mut candidate = self.runtime.clone();
        let previous = self.blocks.last().map(|b| b.header.clone());
        let execution = execute_contract_block(&mut candidate, header, transactions, self.chain_id, previous.as_ref())?;
        self.blocks.push(StoredContractBlock { header: header.clone(), transactions: transactions.to_vec() });
        if let Err(error) = self.save_to_file(path) {
            self.blocks.pop();
            return Err(error);
        }
        self.runtime = candidate;
        Ok(execution)
    }

    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref();
        let file = StoreFile {
            format: STORE_FORMAT.to_string(), chain_id: self.chain_id,
            genesis_executor_snapshot: self.genesis_executor_snapshot.clone(),
            blocks: self.blocks.clone(),
        };
        let bytes = serde_json::to_vec(&file).map_err(|e| format!("cannot encode contract block store: {e}"))?;
        if bytes.len() as u64 > MAX_STORE_BYTES { return Err("contract block store exceeds file size limit".into()); }
        let temp = path.with_extension(format!("qvm-tmp-{}", std::process::id()));
        let result = (|| {
            let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)
                .map_err(|e| format!("cannot create contract block store temp file: {e}"))?;
            output.write_all(&bytes).map_err(|e| format!("cannot write contract block store: {e}"))?;
            output.sync_all().map_err(|e| format!("cannot sync contract block store: {e}"))?;
            std::fs::rename(&temp, path).map_err(|e| format!("cannot atomically install contract block store: {e}"))?;
            Ok(())
        })();
        if result.is_err() { let _ = std::fs::remove_file(&temp); }
        result
    }

    /// Reconstruct the executor exclusively by replaying the signed block log from genesis.
    pub fn load_from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let metadata = std::fs::metadata(path).map_err(|e| format!("cannot inspect contract block store: {e}"))?;
        if metadata.len() > MAX_STORE_BYTES { return Err("contract block store exceeds file size limit".into()); }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        std::fs::File::open(path).map_err(|e| format!("cannot open contract block store: {e}"))?
            .take(MAX_STORE_BYTES + 1).read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read contract block store: {e}"))?;
        if bytes.len() as u64 > MAX_STORE_BYTES { return Err("contract block store exceeds file size limit".into()); }
        let file: StoreFile = serde_json::from_slice(&bytes).map_err(|e| format!("invalid contract block store: {e}"))?;
        if file.format != STORE_FORMAT { return Err("unsupported contract block store version".into()); }
        if file.chain_id == 0 || file.blocks.len() > MAX_BLOCKS { return Err("contract block store metadata exceeds limits".into()); }
        let mut runtime = ContractAccountState::decode_snapshot(&file.genesis_executor_snapshot)?;
        let mut previous: Option<ContractBlockHeaderV1> = None;
        for (index, block) in file.blocks.iter().enumerate() {
            if block.transactions.len() > MAX_TRANSACTIONS_PER_BLOCK {
                return Err(format!("stored contract block {} exceeds transaction limit", index));
            }
            execute_contract_block(&mut runtime, &block.header, &block.transactions, file.chain_id, previous.as_ref())
                .map_err(|e| format!("stored contract block {} failed deterministic replay: {e}", index))?;
            previous = Some(block.header.clone());
        }
        Ok(Self {
            chain_id: file.chain_id, genesis_executor_snapshot: file.genesis_executor_snapshot,
            blocks: file.blocks, runtime,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ContractBlockStore;
    use super::super::contract_block::{contract_block_hash, contract_receipt_root, contract_state_root, contract_transaction_root, ContractBlockHeaderV1};
    use super::super::contract_executor::ContractAccountState;

    fn temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("qorvynt-{label}-{}.json", std::process::id()))
    }

    #[test]
    fn contract_block_store_persists_and_replays_genesis() {
        let path = temp_path("block-store");
        let _ = std::fs::remove_file(&path);
        let state = ContractAccountState::new();
        let mut store = ContractBlockStore::new(77, state.clone()).unwrap();
        let header = ContractBlockHeaderV1 {
            chain_id: 77, height: 0, timestamp_secs: 1, parent_hash: [0; 32],
            transaction_root: contract_transaction_root(&[]).unwrap(),
            receipt_root: contract_receipt_root(&[]).unwrap(),
            state_root: contract_state_root(&state).unwrap(),
        };
        store.append_block(&path, &header, &[]).unwrap();
        let restored = ContractBlockStore::load_from_file(&path).unwrap();
        assert_eq!(restored.chain_id(), 77);
        assert_eq!(restored.blocks(), store.blocks());
        assert_eq!(restored.runtime().encode_snapshot().unwrap(), store.runtime().encode_snapshot().unwrap());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn contract_block_store_rejects_tampered_persisted_header_on_replay() {
        let path = temp_path("tampered-store");
        let _ = std::fs::remove_file(&path);
        let state = ContractAccountState::new();
        let mut store = ContractBlockStore::new(77, state.clone()).unwrap();
        let header = ContractBlockHeaderV1 {
            chain_id: 77, height: 0, timestamp_secs: 1, parent_hash: [0; 32],
            transaction_root: contract_transaction_root(&[]).unwrap(),
            receipt_root: contract_receipt_root(&[]).unwrap(),
            state_root: contract_state_root(&state).unwrap(),
        };
        store.append_block(&path, &header, &[]).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["blocks"][0]["header"]["state_root"][0] = serde_json::Value::from(1);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(ContractBlockStore::load_from_file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

//! Atomic block execution and commitment verification for the Zenvora Chain prototype.
//!
//! This validates transaction, receipt, and state roots and parent linkage. It deliberately does
//! not verify the consensus proof or persist a canonical block database.

use super::chain_protocol::{transaction_merkle_root, BlockHeaderV1, TransactionV1};
use super::chain_consensus::verify_consensus_proof;
use super::chain_runtime::{ChainRuntime, TransferReceipt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedTransactionEnvelope {
    pub transaction: TransactionV1,
    pub public_key: Vec<u8>,
}

/// Execute one candidate block against a cloned state and commit only if every check succeeds.
///
/// With no previous header, this prototype expects height 1 and a zero parent hash. A consensus
/// layer must separately authenticate the proposer and consensus proof before accepting the block.
pub fn execute_block(
    runtime: &mut ChainRuntime,
    header: &BlockHeaderV1,
    transactions: &[SignedTransactionEnvelope],
    expected_chain_id: u64,
    previous_header: Option<&BlockHeaderV1>,
) -> Result<Vec<TransferReceipt>, String> {
    header.canonical_bytes()?;
    if header.chain_id != expected_chain_id {
        return Err("block chain ID does not match this network".into());
    }
    verify_consensus_proof(header, runtime.genesis_balances())?;

    match previous_header {
        Some(previous) => {
            if previous.chain_id != expected_chain_id {
                return Err("previous block belongs to a different chain".into());
            }
            let expected_height = previous.height.checked_add(1)
                .ok_or_else(|| "previous block height overflow".to_string())?;
            if header.height != expected_height {
                return Err("block height is not contiguous".into());
            }
            if header.parent_hash != previous.header_hash()? {
                return Err("block parent hash does not match previous header".into());
            }
            if header.timestamp_secs < previous.timestamp_secs {
                return Err("block timestamp precedes previous block".into());
            }
        }
        None => {
            if header.height != 0 || header.parent_hash != [0; 32] {
                return Err("genesis block must have height 0 and a zero parent hash".into());
            }
            if !transactions.is_empty() {
                return Err("genesis block cannot contain transactions".into());
            }
        }
    }

    let transactions_only: Vec<TransactionV1> =
        transactions.iter().map(|envelope| envelope.transaction.clone()).collect();
    let computed_transaction_root = transaction_merkle_root(&transactions_only)?;
    if header.transaction_root != computed_transaction_root {
        return Err("block transaction root does not match its transactions".into());
    }

    let mut candidate = runtime.clone();
    let starting_receipt_count = candidate.receipts().len();
    for envelope in transactions {
        candidate.apply_signed_transaction(
            &envelope.transaction,
            &envelope.public_key,
            expected_chain_id,
            header.height,
        )?;
    }
    let block_receipts = candidate.receipts()[starting_receipt_count..].to_vec();
    let computed_receipt_root = ChainRuntime::receipt_root_for(&block_receipts)?;
    if header.receipt_root != computed_receipt_root {
        return Err("block receipt root does not match executed receipts".into());
    }
    if header.state_root != candidate.state_root() {
        return Err("block state root does not match resulting state".into());
    }

    // The consensus proof is opaque at this layer. State is committed only after all execution
    // and commitment checks above pass.
    *runtime = candidate;
    Ok(block_receipts)
}

#[cfg(test)]
mod tests {
    use super::{execute_block, SignedTransactionEnvelope};
    use super::super::chain_protocol::{
        address_from_public_key, transaction_merkle_root, BlockHeaderV1, TransactionV1,
    };
    use super::super::chain_consensus::sign_header_as_proposer;
    use super::super::chain_runtime::ChainRuntime;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_transfer(key: &SigningKey, nonce: u64, amount: u64) -> SignedTransactionEnvelope {
        let public_key = key.verifying_key().to_bytes();
        let mut transaction = TransactionV1 {
            chain_id: 71,
            sender: address_from_public_key(&public_key).unwrap(),
            nonce,
            recipient: "bob".into(),
            amount,
            max_fee: 3,
            gas_limit: 21_000,
            valid_until_height: Some(100),
            signature: Vec::new(),
        };
        transaction.signature = key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        SignedTransactionEnvelope { transaction, public_key: public_key.to_vec() }
    }

    fn genesis_header(runtime: &ChainRuntime) -> BlockHeaderV1 {
        BlockHeaderV1 {
            chain_id: 71,
            height: 0,
            timestamp_secs: 1_800_000_000,
            parent_hash: [0; 32],
            transaction_root: transaction_merkle_root(&[]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[]).unwrap(),
            state_root: runtime.state_root(),
            proposer: "validator_1".into(),
            consensus_proof: Vec::new(),
        }
    }

    fn candidate_header(
        runtime_before: &ChainRuntime,
        envelope: &SignedTransactionEnvelope,
        key: &SigningKey,
        height: u64,
        parent_hash: [u8; 32],
        timestamp_secs: u64,
    ) -> BlockHeaderV1 {
        let mut candidate = runtime_before.clone();
        let receipt = candidate.apply_signed_transaction(
            &envelope.transaction, &envelope.public_key, 71, height,
        ).unwrap();
        let mut header = BlockHeaderV1 {
            chain_id: 71,
            height,
            timestamp_secs,
            parent_hash,
            transaction_root: transaction_merkle_root(&[envelope.transaction.clone()]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[receipt]).unwrap(),
            state_root: candidate.state_root(),
            proposer: "validator_1".into(),
            consensus_proof: Vec::new(),
        };
        sign_header_as_proposer(&mut header, key).unwrap();
        header
    }

    fn funded_runtime(key: &SigningKey) -> ChainRuntime {
        let mut runtime = ChainRuntime::new();
        let address = address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        runtime.set_genesis_balance(&address, 1_000).unwrap();
        runtime.set_genesis_balance("bob", 0).unwrap();
        runtime
    }

    #[test]
    fn block_execution_checks_roots_and_commits_atomically() {
        let key = SigningKey::from_bytes(&[71; 32]);
        let mut runtime = funded_runtime(&key);
        let genesis = genesis_header(&runtime);
        assert!(execute_block(&mut runtime, &genesis, &[], 71, None).unwrap().is_empty());
        let envelope = signed_transfer(&key, 0, 125);
        let header = candidate_header(&runtime, &envelope, &key, 1, genesis.header_hash().unwrap(), 1_800_000_001);

        let receipts = execute_block(
            &mut runtime, &header, &[envelope.clone()], 71, Some(&genesis),
        ).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(runtime.balance("bob"), 125);

        let next_envelope = signed_transfer(&key, 1, 25);
        let previous_hash = header.header_hash().unwrap();
        let next_header = candidate_header(&runtime, &next_envelope, &key, 2, previous_hash, 1_800_000_001);
        assert!(execute_block(
            &mut runtime, &next_header, &[next_envelope], 71, Some(&header),
        ).is_ok());
        assert_eq!(runtime.balance("bob"), 150);
    }

    #[test]
    fn invalid_block_root_or_transaction_keeps_original_state() {
        let key = SigningKey::from_bytes(&[72; 32]);
        let mut runtime = funded_runtime(&key);
        let original_root = runtime.state_root();
        let genesis = genesis_header(&runtime);
        let envelope = signed_transfer(&key, 0, 125);
        let mut header = candidate_header(&runtime, &envelope, &key, 1, genesis.header_hash().unwrap(), 1_800_000_001);
        header.state_root[0] ^= 1;
        sign_header_as_proposer(&mut header, &key).unwrap();

        assert!(execute_block(&mut runtime, &header, &[envelope.clone()], 71, Some(&genesis)).is_err());
        assert_eq!(runtime.state_root(), original_root);
        assert_eq!(runtime.balance("bob"), 0);
        assert_eq!(runtime.account_nonce(&envelope.transaction.sender), 0);

        let mut bad_envelope = envelope.clone();
        bad_envelope.transaction.amount += 1;
        let valid_header = candidate_header(&runtime, &envelope, &key, 1, genesis.header_hash().unwrap(), 1_800_000_001);
        assert!(execute_block(&mut runtime, &valid_header, &[envelope, bad_envelope], 71, Some(&genesis)).is_err());
        assert_eq!(runtime.state_root(), original_root);
        assert_eq!(runtime.balance("bob"), 0);
    }

    #[test]
    fn block_parent_linkage_and_chain_id_are_enforced() {
        let key = SigningKey::from_bytes(&[73; 32]);
        let runtime = funded_runtime(&key);
        let envelope = signed_transfer(&key, 0, 125);
        let mut header = candidate_header(&runtime, &envelope, &key, 1, [9; 32], 100);
        header.chain_id = 72;
        assert!(execute_block(&mut runtime.clone(), &header, &[envelope.clone()], 71, None).is_err());

        header.chain_id = 71;
        header.height = 2;
        assert!(execute_block(&mut runtime.clone(), &header, &[envelope], 71, None).is_err());
    }
}

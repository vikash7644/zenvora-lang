//! Versioned, deterministic wire-format building blocks for the Zenvora Chain prototype.
//!
//! This module validates structure and canonical encoding only. It does not verify
//! signatures, calculate cryptographic hashes, execute transactions, or implement consensus.

const TX_DOMAIN: &[u8] = b"ZENVORA_TX_SIGNING_V1\0";
const TX_WIRE_DOMAIN: &[u8] = b"ZENVORA_TX_WIRE_V1\0";
const BLOCK_DOMAIN: &[u8] = b"ZENVORA_BLOCK_HEADER_V1\0";
const MAX_ADDRESS_BYTES: usize = 64;
const MAX_SIGNATURE_BYTES: usize = 512;
const MAX_CONSENSUS_PROOF_BYTES: usize = 4096;
const MAX_BLOCK_TRANSACTIONS: usize = 10_000;
const MAX_BLOCK_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionV1 {
    pub chain_id: u64,
    pub sender: String,
    pub nonce: u64,
    pub recipient: String,
    pub amount: u64,
    pub max_fee: u64,
    pub gas_limit: u64,
    pub valid_until_height: Option<u64>,
    /// Opaque signature bytes. A future crypto layer must verify these bytes.
    pub signature: Vec<u8>,
}

impl TransactionV1 {
    /// Deterministic signing payload; the signature itself is deliberately excluded.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_fields()?;
        let mut out = Vec::new();
        out.extend_from_slice(TX_DOMAIN);
        out.extend_from_slice(&1_u16.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        put_string(&mut out, &self.sender)?;
        out.extend_from_slice(&self.nonce.to_be_bytes());
        put_string(&mut out, &self.recipient)?;
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.max_fee.to_be_bytes());
        out.extend_from_slice(&self.gas_limit.to_be_bytes());
        match self.valid_until_height {
            Some(height) => {
                out.push(1);
                out.extend_from_slice(&height.to_be_bytes());
            }
            None => out.push(0),
        }
        Ok(out)
    }

    /// Canonical transaction envelope for transport/storage; this is not a transaction hash.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        if self.signature.is_empty() || self.signature.len() > MAX_SIGNATURE_BYTES {
            return Err("signature length is outside the permitted structural range".into());
        }
        let signing = self.signing_bytes()?;
        let mut out = Vec::with_capacity(TX_WIRE_DOMAIN.len() + 4 + signing.len() + self.signature.len());
        out.extend_from_slice(TX_WIRE_DOMAIN);
        put_bytes(&mut out, &signing)?;
        put_bytes(&mut out, &self.signature)?;
        if out.len() > MAX_BLOCK_BYTES {
            return Err("transaction exceeds maximum encoded size".into());
        }
        Ok(out)
    }

    /// Checks structural fields for a specific network and block height.
    /// Cryptographic signature verification and account nonce/balance checks are separate steps.
    pub fn validate_for(&self, expected_chain_id: u64, current_height: u64) -> Result<(), String> {
        self.validate_fields()?;
        if self.chain_id != expected_chain_id {
            return Err("transaction chain ID does not match this network".into());
        }
        if self.valid_until_height.is_some_and(|expiry| expiry < current_height) {
            return Err("transaction has expired".into());
        }
        if self.signature.is_empty() || self.signature.len() > MAX_SIGNATURE_BYTES {
            return Err("signature length is outside the permitted structural range".into());
        }
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), String> {
        if self.chain_id == 0 {
            return Err("chain ID must be nonzero".into());
        }
        if !valid_address(&self.sender) || !valid_address(&self.recipient) {
            return Err("invalid address: use 1-64 ASCII letters, digits, or underscores".into());
        }
        if self.sender == self.recipient {
            return Err("self-transfers are not supported".into());
        }
        if self.amount == 0 {
            return Err("transaction amount must be positive".into());
        }
        if self.gas_limit == 0 {
            return Err("gas limit must be positive".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeaderV1 {
    pub chain_id: u64,
    pub height: u64,
    pub timestamp_secs: u64,
    pub parent_hash: [u8; 32],
    pub transaction_root: [u8; 32],
    pub receipt_root: [u8; 32],
    pub state_root: [u8; 32],
    pub proposer: String,
    /// Opaque consensus proof bytes; validation belongs to the consensus implementation.
    pub consensus_proof: Vec<u8>,
}

impl BlockHeaderV1 {
    /// Canonical version-1 header bytes. Hashing, timestamp policy, and proof verification
    /// are intentionally not implemented in this structural-format milestone.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        if self.chain_id == 0 {
            return Err("chain ID must be nonzero".into());
        }
        if !valid_address(&self.proposer) {
            return Err("invalid block proposer address".into());
        }
        if self.consensus_proof.len() > MAX_CONSENSUS_PROOF_BYTES {
            return Err("consensus proof exceeds maximum size".into());
        }
        if self.height == 0 && self.parent_hash != [0; 32] {
            return Err("genesis block must have a zero parent hash".into());
        }
        if self.height > 0 && self.parent_hash == [0; 32] {
            return Err("non-genesis block must have a parent hash".into());
        }

        let mut out = Vec::with_capacity(160 + self.proposer.len() + self.consensus_proof.len());
        out.extend_from_slice(BLOCK_DOMAIN);
        out.extend_from_slice(&1_u16.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.height.to_be_bytes());
        out.extend_from_slice(&self.timestamp_secs.to_be_bytes());
        out.extend_from_slice(&self.parent_hash);
        out.extend_from_slice(&self.transaction_root);
        out.extend_from_slice(&self.receipt_root);
        out.extend_from_slice(&self.state_root);
        put_string(&mut out, &self.proposer)?;
        put_bytes(&mut out, &self.consensus_proof)?;
        Ok(out)
    }
}

fn valid_address(address: &str) -> bool {
    !address.is_empty()
        && address.len() <= MAX_ADDRESS_BYTES
        && address.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn put_string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    put_bytes(out, value.as_bytes())
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), String> {
    let len = u32::try_from(value.len()).map_err(|_| "encoded field is too large")?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BlockHeaderV1, TransactionV1};

    fn tx() -> TransactionV1 {
        TransactionV1 {
            chain_id: 71,
            sender: "alice".into(),
            nonce: 4,
            recipient: "bob".into(),
            amount: 125,
            max_fee: 3,
            gas_limit: 21_000,
            valid_until_height: Some(100),
            signature: vec![7; 64],
        }
    }

    #[test]
    fn transaction_signing_bytes_are_deterministic_and_exclude_signature() {
        let first = tx();
        let mut second = first.clone();
        second.signature = vec![9; 64];
        assert_eq!(first.signing_bytes().unwrap(), second.signing_bytes().unwrap());
        assert_ne!(first.canonical_bytes().unwrap(), second.canonical_bytes().unwrap());
    }

    #[test]
    fn transaction_validation_rejects_wrong_network_expiry_and_invalid_fields() {
        let transaction = tx();
        assert!(transaction.validate_for(71, 99).is_ok());
        assert!(transaction.validate_for(72, 99).unwrap_err().contains("chain ID"));
        assert!(transaction.validate_for(71, 101).unwrap_err().contains("expired"));

        let mut invalid = tx();
        invalid.amount = 0;
        assert!(invalid.validate_for(71, 1).is_err());
        invalid = tx();
        invalid.sender = "alice;show".into();
        assert!(invalid.validate_for(71, 1).is_err());
        invalid = tx();
        invalid.signature.clear();
        assert!(invalid.validate_for(71, 1).is_err());
    }

    #[test]
    fn block_header_encoding_is_deterministic_and_checks_parent_rules() {
        let header = BlockHeaderV1 {
            chain_id: 71,
            height: 0,
            timestamp_secs: 1_800_000_000,
            parent_hash: [0; 32],
            transaction_root: [1; 32],
            receipt_root: [2; 32],
            state_root: [3; 32],
            proposer: "validator_1".into(),
            consensus_proof: vec![],
        };
        assert_eq!(header.canonical_bytes().unwrap(), header.canonical_bytes().unwrap());
        let mut invalid = header;
        invalid.height = 1;
        assert!(invalid.canonical_bytes().unwrap_err().contains("parent hash"));
    }
}

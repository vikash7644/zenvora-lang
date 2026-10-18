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

    /// Decode the canonical version-1 envelope and reject non-canonical or trailing data.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BLOCK_BYTES {
            return Err("transaction exceeds maximum encoded size".into());
        }
        let mut envelope = Reader::new(bytes);
        envelope.expect(TX_WIRE_DOMAIN)?;
        let signing = envelope.read_bytes(MAX_BLOCK_BYTES)?;
        let signature = envelope.read_bytes(MAX_SIGNATURE_BYTES)?;
        envelope.finish()?;

        let mut payload = Reader::new(signing);
        payload.expect(TX_DOMAIN)?;
        if payload.read_u16()? != 1 {
            return Err("unsupported transaction signing version".into());
        }
        let chain_id = payload.read_u64()?;
        let sender = payload.read_string(MAX_ADDRESS_BYTES)?;
        let nonce = payload.read_u64()?;
        let recipient = payload.read_string(MAX_ADDRESS_BYTES)?;
        let amount = payload.read_u64()?;
        let max_fee = payload.read_u64()?;
        let gas_limit = payload.read_u64()?;
        let valid_until_height = match payload.read_u8()? {
            0 => None,
            1 => Some(payload.read_u64()?),
            _ => return Err("invalid transaction expiry option tag".into()),
        };
        payload.finish()?;

        let transaction = Self {
            chain_id,
            sender,
            nonce,
            recipient,
            amount,
            max_fee,
            gas_limit,
            valid_until_height,
            signature: signature.to_vec(),
        };
        let reencoded = transaction.canonical_bytes()?;
        if reencoded != bytes {
            return Err("transaction bytes are not canonical".into());
        }
        Ok(transaction)
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

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self.offset.checked_add(length).ok_or("encoded length overflow")?;
        if end > self.bytes.len() {
            return Err("truncated canonical transaction".into());
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn expect(&mut self, expected: &[u8]) -> Result<(), String> {
        if self.read_exact(expected.len())? != expected {
            return Err("invalid canonical transaction domain tag".into());
        }
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.read_exact(2)?.try_into().map_err(|_| "invalid u16")?))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.read_exact(4)?.try_into().map_err(|_| "invalid u32")?))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.read_exact(8)?.try_into().map_err(|_| "invalid u64")?))
    }

    fn read_bytes(&mut self, maximum: usize) -> Result<&'a [u8], String> {
        let length = self.read_u32()? as usize;
        if length > maximum {
            return Err("encoded field exceeds maximum permitted size".into());
        }
        self.read_exact(length)
    }

    fn read_string(&mut self, maximum: usize) -> Result<String, String> {
        String::from_utf8(self.read_bytes(maximum)?.to_vec())
            .map_err(|_| "encoded string is not valid UTF-8".into())
    }

    fn finish(&self) -> Result<(), String> {
        if self.offset != self.bytes.len() {
            return Err("trailing bytes in canonical transaction".into());
        }
        Ok(())
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
    fn canonical_transaction_round_trips_and_rejects_truncation_or_trailing_data() {
        let original = tx();
        let bytes = original.canonical_bytes().unwrap();
        assert_eq!(TransactionV1::from_canonical_bytes(&bytes).unwrap(), original);

        for end in 0..bytes.len() {
            assert!(TransactionV1::from_canonical_bytes(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(TransactionV1::from_canonical_bytes(&trailing).is_err());
    }

    #[test]
    fn canonical_transaction_decoder_rejects_invalid_domain_and_oversized_input() {
        let mut bytes = tx().canonical_bytes().unwrap();
        bytes[0] ^= 1;
        assert!(TransactionV1::from_canonical_bytes(&bytes).is_err());
        assert!(TransactionV1::from_canonical_bytes(&vec![0; 4 * 1024 * 1024 + 1]).is_err());
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

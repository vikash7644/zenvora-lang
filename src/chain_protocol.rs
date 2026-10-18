//! Versioned, deterministic wire-format building blocks for the Zenvora Chain prototype.
//!
//! This module provides canonical encoding, domain-separated SHA-256 hashing, and Ed25519
//! signature verification. It does not execute transactions or implement consensus.

const TX_DOMAIN: &[u8] = b"ZENVORA_TX_SIGNING_V1\0";
const TX_WIRE_DOMAIN: &[u8] = b"ZENVORA_TX_WIRE_V1\0";
const BLOCK_DOMAIN: &[u8] = b"ZENVORA_BLOCK_HEADER_V1\0";
const ADDRESS_DOMAIN: &[u8] = b"ZENVORA_ADDRESS_V1\\0";
const MAX_ADDRESS_BYTES: usize = 64;
const MAX_SIGNATURE_BYTES: usize = 512;
const MAX_CONSENSUS_PROOF_BYTES: usize = 4096;
const MAX_BLOCK_TRANSACTIONS: usize = 10_000;
const MAX_BLOCK_BYTES: usize = 4 * 1024 * 1024;
const TX_ID_DOMAIN: &[u8] = b"ZENVORA_TX_ID_V1\0";
const BLOCK_HASH_DOMAIN: &[u8] = b"ZENVORA_BLOCK_HASH_V1\0";
const MERKLE_LEAF_DOMAIN: &[u8] = b"ZENVORA_MERKLE_LEAF_V1\0";
const MERKLE_NODE_DOMAIN: &[u8] = b"ZENVORA_MERKLE_NODE_V1\0";
const MERKLE_EMPTY_DOMAIN: &[u8] = b"ZENVORA_MERKLE_EMPTY_V1\0";

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

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
    /// Ed25519 signature bytes; callers must invoke `verify_signature` to authenticate them.
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

    /// SHA-256 transaction identifier over the canonical signed envelope, with domain separation.
    pub fn transaction_id(&self) -> Result<[u8; 32], String> {
        let bytes = self.canonical_bytes()?;
        Ok(hash_parts(&[TX_ID_DOMAIN, &bytes]))
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

    /// Verify the Ed25519 signature and bind the supplied public key to the sender address.
    /// Public keys are 32 bytes; signatures must be exactly 64 bytes.
    pub fn verify_signature(&self, public_key_bytes: &[u8]) -> Result<(), String> {
        self.validate_fields()?;
        if public_key_bytes.len() != 32 {
            return Err("Ed25519 public key must be exactly 32 bytes".into());
        }
        let public_key_array: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| "Ed25519 public key must be exactly 32 bytes")?;
        let verifying_key = VerifyingKey::from_bytes(&public_key_array)
            .map_err(|_| "invalid Ed25519 public key")?;
        let derived_address = address_from_public_key(public_key_bytes)?;
        if self.sender != derived_address {
            return Err("transaction sender does not match public key".into());
        }
        if self.signature.len() != 64 {
            return Err("Ed25519 signature must be exactly 64 bytes".into());
        }
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| "invalid Ed25519 signature encoding")?;
        let message = self.signing_bytes()?;
        verifying_key
            .verify_strict(&message, &signature)
            .map_err(|_| "transaction signature verification failed".into())
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

    /// SHA-256 block-header hash over canonical header bytes, with domain separation.
    pub fn header_hash(&self) -> Result<[u8; 32], String> {
        let bytes = self.canonical_bytes()?;
        Ok(hash_parts(&[BLOCK_HASH_DOMAIN, &bytes]))
    }

    /// Decode the canonical version-1 block header, rejecting truncation and trailing data.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BLOCK_BYTES {
            return Err("block header exceeds maximum encoded size".into());
        }
        let mut reader = Reader::new(bytes);
        reader.expect(BLOCK_DOMAIN)?;
        if reader.read_u16()? != 1 {
            return Err("unsupported block header version".into());
        }
        let chain_id = reader.read_u64()?;
        let height = reader.read_u64()?;
        let timestamp_secs = reader.read_u64()?;
        let parent_hash: [u8; 32] = reader.read_exact(32)?.try_into().map_err(|_| "invalid parent hash")?;
        let transaction_root: [u8; 32] = reader.read_exact(32)?.try_into().map_err(|_| "invalid transaction root")?;
        let receipt_root: [u8; 32] = reader.read_exact(32)?.try_into().map_err(|_| "invalid receipt root")?;
        let state_root: [u8; 32] = reader.read_exact(32)?.try_into().map_err(|_| "invalid state root")?;
        let proposer = reader.read_string(MAX_ADDRESS_BYTES)?;
        let consensus_proof = reader.read_bytes(MAX_CONSENSUS_PROOF_BYTES)?.to_vec();
        reader.finish()?;

        let header = Self {
            chain_id,
            height,
            timestamp_secs,
            parent_hash,
            transaction_root,
            receipt_root,
            state_root,
            proposer,
            consensus_proof,
        };
        if header.canonical_bytes()? != bytes {
            return Err("block header bytes are not canonical".into());
        }
        Ok(header)
    }

}

/// Derive a deterministic Zenvora address from a 32-byte Ed25519 public key.
/// The displayed address is the prefix zv followed by 40 lowercase hex characters.
pub fn address_from_public_key(public_key: &[u8]) -> Result<String, String> {
    if public_key.len() != 32 {
        return Err("Ed25519 public key must be exactly 32 bytes".into());
    }
    let digest = hash_parts(&[ADDRESS_DOMAIN, public_key]);
    let mut address = String::with_capacity(42);
    address.push_str("zv");
    for byte in &digest[..20] {
        use std::fmt::Write as _;
        write!(&mut address, "{byte:02x}").map_err(|_| "address formatting failed")?;
    }
    Ok(address)
}

/// Compute the transaction Merkle root. Leaves and internal nodes use distinct domain tags;
/// an odd final node is paired with itself. This function does not execute or validate signatures.
pub fn transaction_merkle_root(transactions: &[TransactionV1]) -> Result<[u8; 32], String> {
    if transactions.len() > MAX_BLOCK_TRANSACTIONS {
        return Err("block transaction limit exceeded".into());
    }
    if transactions.is_empty() {
        return Ok(hash_parts(&[MERKLE_EMPTY_DOMAIN]));
    }
    let mut level = Vec::with_capacity(transactions.len());
    for transaction in transactions {
        let bytes = transaction.canonical_bytes()?;
        level.push(hash_parts(&[MERKLE_LEAF_DOMAIN, &bytes]));
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        for pair in level.chunks(2) {
            let right = pair.get(1).unwrap_or(&pair[0]);
            next.push(hash_parts(&[MERKLE_NODE_DOMAIN, &pair[0], right]));
        }
        level = next;
    }
    Ok(level[0])
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
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
    use super::{address_from_public_key, transaction_merkle_root, BlockHeaderV1, TransactionV1, BLOCK_DOMAIN};
    use ed25519_dalek::{Signer, SigningKey};

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
    fn transaction_ids_and_block_hashes_are_deterministic_and_change_with_content() {
        let original = tx();
        assert_eq!(original.transaction_id().unwrap(), original.transaction_id().unwrap());
        let mut changed = original.clone();
        changed.amount += 1;
        assert_ne!(original.transaction_id().unwrap(), changed.transaction_id().unwrap());

        let header = BlockHeaderV1 {
            chain_id: 71,
            height: 1,
            timestamp_secs: 1_800_000_001,
            parent_hash: [4; 32],
            transaction_root: [1; 32],
            receipt_root: [2; 32],
            state_root: [3; 32],
            proposer: "validator_1".into(),
            consensus_proof: vec![8, 9, 10],
        };
        assert_eq!(header.header_hash().unwrap(), header.header_hash().unwrap());
        let mut changed_header = header.clone();
        changed_header.timestamp_secs += 1;
        assert_ne!(header.header_hash().unwrap(), changed_header.header_hash().unwrap());
    }

    #[test]
    fn transaction_merkle_root_is_deterministic_order_sensitive_and_handles_empty() {
        let a = tx();
        let mut b = tx();
        b.nonce += 1;
        assert_eq!(transaction_merkle_root(&[]).unwrap(), transaction_merkle_root(&[]).unwrap());
        assert_ne!(transaction_merkle_root(&[]).unwrap(), transaction_merkle_root(&[a.clone()]).unwrap());
        assert_eq!(transaction_merkle_root(&[a.clone(), b.clone()]).unwrap(), transaction_merkle_root(&[a.clone(), b.clone()]).unwrap());
        assert_ne!(transaction_merkle_root(&[a.clone(), b.clone()]).unwrap(), transaction_merkle_root(&[b, a.clone()]).unwrap());
        assert_ne!(transaction_merkle_root(&[a.clone()]).unwrap(), transaction_merkle_root(&[a.clone(), a]).unwrap());
    }

    #[test]
    fn ed25519_signature_verification_binds_sender_and_rejects_tampering() {
        let signing_key = SigningKey::from_bytes(&[42; 32]);
        let public_key = signing_key.verifying_key().to_bytes();
        let mut transaction = tx();
        transaction.sender = address_from_public_key(&public_key).unwrap();
        transaction.signature = signing_key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();

        assert!(transaction.verify_signature(&public_key).is_ok());

        let mut tampered = transaction.clone();
        tampered.amount += 1;
        assert!(tampered.verify_signature(&public_key).is_err());

        let other_key = SigningKey::from_bytes(&[43; 32]);
        assert!(transaction.verify_signature(&other_key.verifying_key().to_bytes()).is_err());
        assert!(transaction.verify_signature(&[0; 31]).is_err());

        let mut bad_signature = transaction;
        bad_signature.signature[0] ^= 1;
        assert!(bad_signature.verify_signature(&public_key).is_err());
    }

    #[test]
    fn address_derivation_requires_a_full_ed25519_public_key() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public_key = key.verifying_key().to_bytes();
        let address = address_from_public_key(&public_key).unwrap();
        assert!(address.starts_with("zv"));
        assert_eq!(address.len(), 42);
        assert!(address.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert!(address_from_public_key(&[1; 31]).is_err());
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
    fn canonical_block_header_round_trips_and_rejects_truncation_or_trailing_data() {
        let original = BlockHeaderV1 {
            chain_id: 71,
            height: 1,
            timestamp_secs: 1_800_000_001,
            parent_hash: [4; 32],
            transaction_root: [1; 32],
            receipt_root: [2; 32],
            state_root: [3; 32],
            proposer: "validator_1".into(),
            consensus_proof: vec![8, 9, 10],
        };
        let bytes = original.canonical_bytes().unwrap();
        assert_eq!(BlockHeaderV1::from_canonical_bytes(&bytes).unwrap(), original);
        for end in 0..bytes.len() {
            assert!(BlockHeaderV1::from_canonical_bytes(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(BlockHeaderV1::from_canonical_bytes(&trailing).is_err());
        let mut bad_version = bytes;
        let version_offset = BLOCK_DOMAIN.len();
        bad_version[version_offset + 1] = 2;
        assert!(BlockHeaderV1::from_canonical_bytes(&bad_version).is_err());
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

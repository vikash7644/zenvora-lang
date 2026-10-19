//! Genesis-authorized validator signatures for the Zenvora Chain prototype.
//!
//! This is a simple quorum-signature layer, not a BFT consensus algorithm. It does not implement
//! leader election, round changes, equivocation handling, fork choice, slashing, or finality.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use super::chain_protocol::{address_from_public_key, BlockHeaderV1};

const SIGNATURE_RECORD_BYTES: usize = 96;
const MAX_VALIDATORS: usize = 42;

/// Sign a header as its proposer and set the proposer address from the signing key.
pub fn sign_header_as_proposer(header: &mut BlockHeaderV1, key: &SigningKey) -> Result<(), String> {
    header.proposer = address_from_public_key(&key.verifying_key().to_bytes())?;
    header.consensus_proof.clear();
    add_validator_signature(header, key)
}

/// Add a validator signature to a header's proof. Records are sorted by validator address.
pub fn add_validator_signature(header: &mut BlockHeaderV1, key: &SigningKey) -> Result<(), String> {
    let public_key = key.verifying_key().to_bytes();
    let signing_bytes = header.consensus_signing_bytes()?;
    let signature = key.sign(&signing_bytes).to_bytes();
    add_signature_record(header, &public_key, &signature)
}

/// Merge a validator's public key and signature record into a header proof.
pub fn add_signature_record(
    header: &mut BlockHeaderV1,
    public_key: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    if public_key.len() != 32 || signature.len() != 64 {
        return Err("consensus signature record must contain a 32-byte key and 64-byte signature".into());
    }
    let address = address_from_public_key(public_key)?;
    let mut records: Vec<(String, Vec<u8>)> = Vec::new();
    if !header.consensus_proof.is_empty() {
        if header.consensus_proof.len() % SIGNATURE_RECORD_BYTES != 0 {
            return Err("existing consensus proof has a partial signature record".into());
        }
        for record in header.consensus_proof.chunks_exact(SIGNATURE_RECORD_BYTES) {
            let existing_key: [u8; 32] = record[..32].try_into()
                .map_err(|_| "invalid public key in consensus proof")?;
            let existing_address = address_from_public_key(&existing_key)?;
            records.push((existing_address, record.to_vec()));
        }
    }
    if records.iter().any(|(existing, _)| existing == &address) {
        return Err("validator has already signed this header".into());
    }
    let mut record = Vec::with_capacity(SIGNATURE_RECORD_BYTES);
    record.extend_from_slice(public_key);
    record.extend_from_slice(signature);
    records.push((address, record));
    records.sort_by(|left, right| left.0.cmp(&right.0));
    if records.len() > MAX_VALIDATORS {
        return Err("consensus proof exceeds the prototype validator limit".into());
    }
    header.consensus_proof = records.into_iter().flat_map(|(_, record)| record).collect();
    Ok(())
}

/// Verify all supplied signatures without requiring a quorum; useful when validating a proposal.
pub fn verify_partial_consensus_proof(
    header: &BlockHeaderV1,
    genesis_balances: &BTreeMap<String, i64>,
) -> Result<usize, String> {
    if header.height == 0 {
        return if header.consensus_proof.is_empty() {
            Ok(0)
        } else {
            Err("genesis block must not contain a consensus proof".into())
        };
    }
    if header.consensus_proof.is_empty()
        || header.consensus_proof.len() % SIGNATURE_RECORD_BYTES != 0
    {
        return Err("non-genesis block requires complete validator signature records".into());
    }
    let validators = validator_addresses(genesis_balances)?;
    let signature_count = header.consensus_proof.len() / SIGNATURE_RECORD_BYTES;
    if signature_count > validators.len() {
        return Err("consensus proof contains more signers than the validator set".into());
    }

    let message = header.consensus_signing_bytes()?;
    let mut previous_address: Option<String> = None;
    let mut signers = BTreeSet::new();
    for record in header.consensus_proof.chunks_exact(SIGNATURE_RECORD_BYTES) {
        let public_key_bytes: [u8; 32] = record[..32].try_into()
            .map_err(|_| "invalid consensus public key")?;
        let address = address_from_public_key(&public_key_bytes)?;
        if previous_address.as_ref().is_some_and(|previous| previous >= &address) {
            return Err("consensus signatures must be sorted and unique by validator address".into());
        }
        previous_address = Some(address.clone());
        if !validators.contains(&address) {
            return Err("consensus proof contains a non-validator signer".into());
        }
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
            .map_err(|_| "invalid Ed25519 validator public key")?;
        let signature = Signature::from_slice(&record[32..])
            .map_err(|_| "invalid Ed25519 consensus signature")?;
        verifying_key.verify_strict(&message, &signature)
            .map_err(|_| "invalid Ed25519 consensus signature")?;
        signers.insert(address);
    }
    if !signers.contains(&header.proposer) {
        return Err("block proposer must be included in the validator signatures".into());
    }
    Ok(signers.len())
}

/// Verify proposer authorization, sorted unique signatures, and a 2/3+1 validator quorum.
pub fn verify_consensus_proof(
    header: &BlockHeaderV1,
    genesis_balances: &BTreeMap<String, i64>,
) -> Result<(), String> {
    if header.height == 0 {
        return verify_partial_consensus_proof(header, genesis_balances).map(|_| ());
    }
    let signer_count = verify_partial_consensus_proof(header, genesis_balances)?;
    let validators = validator_addresses(genesis_balances)?;
    let quorum = (validators.len() * 2) / 3 + 1;
    if signer_count < quorum {
        return Err(format!("insufficient validator signatures: have {signer_count}, require {quorum}"));
    }
    Ok(())
}

fn validator_addresses(genesis_balances: &BTreeMap<String, i64>) -> Result<BTreeSet<String>, String> {
    let validators: BTreeSet<String> = genesis_balances.iter()
        .filter(|(address, balance)| **balance > 0 && is_public_key_address(address))
        .map(|(address, _)| address.clone())
        .collect();
    if validators.is_empty() || validators.len() > MAX_VALIDATORS {
        return Err("genesis validator set is empty or exceeds the prototype limit".into());
    }
    Ok(validators)
}

fn is_public_key_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("zv")
        && address.as_bytes()[2..].iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

#[cfg(test)]
mod tests {
    use super::{add_validator_signature, sign_header_as_proposer, verify_consensus_proof, SIGNATURE_RECORD_BYTES};
    use super::super::chain_protocol::{address_from_public_key, BlockHeaderV1};
    use ed25519_dalek::SigningKey;
    use std::collections::BTreeMap;

    fn header() -> BlockHeaderV1 {
        BlockHeaderV1 {
            chain_id: 71, height: 1, timestamp_secs: 100, parent_hash: [9; 32],
            transaction_root: [1; 32], receipt_root: [2; 32], state_root: [3; 32],
            proposer: "zv0000000000000000000000000000000000000000".into(),
            consensus_proof: Vec::new(),
        }
    }

    fn genesis(keys: &[SigningKey]) -> BTreeMap<String, i64> {
        keys.iter().map(|key| {
            let address = address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
            (address, 1_000)
        }).collect()
    }

    #[test]
    fn single_validator_signature_authenticates_header() {
        let key = SigningKey::from_bytes(&[101; 32]);
        let allocations = genesis(std::slice::from_ref(&key));
        let mut block = header();
        sign_header_as_proposer(&mut block, &key).unwrap();
        assert!(verify_consensus_proof(&block, &allocations).is_ok());
        block.state_root[0] ^= 1;
        assert!(verify_consensus_proof(&block, &allocations).is_err());
    }

    #[test]
    fn four_validators_require_three_sorted_unique_signatures() {
        let keys: Vec<SigningKey> = (111_u8..115).map(|seed| SigningKey::from_bytes(&[seed; 32])).collect();
        let allocations = genesis(&keys);
        let mut block = header();
        sign_header_as_proposer(&mut block, &keys[0]).unwrap();
        add_validator_signature(&mut block, &keys[1]).unwrap();
        add_validator_signature(&mut block, &keys[2]).unwrap();
        assert!(verify_consensus_proof(&block, &allocations).is_ok());

        let proof = block.consensus_proof.clone();
        block.consensus_proof = proof[..SIGNATURE_RECORD_BYTES * 2].to_vec();
        assert!(verify_consensus_proof(&block, &allocations).is_err());
        block.consensus_proof = proof;
        assert!(add_validator_signature(&mut block, &keys[1]).is_err());
    }

    #[test]
    fn non_validator_and_unsigned_non_genesis_blocks_are_rejected() {
        let validator = SigningKey::from_bytes(&[121; 32]);
        let outsider = SigningKey::from_bytes(&[122; 32]);
        let allocations = genesis(std::slice::from_ref(&validator));
        let mut block = header();
        assert!(verify_consensus_proof(&block, &allocations).is_err());
        sign_header_as_proposer(&mut block, &outsider).unwrap();
        assert!(verify_consensus_proof(&block, &allocations).is_err());
    }
}

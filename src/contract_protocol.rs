//! Signed, canonical contract transaction envelopes for QORVYNT's VM execution layer.
//!
//! This module defines the authenticated transaction boundary only. Contract execution,
//! state mutation, fee settlement, block replay, and deployment admission are integrated
//! by later runtime layers; a valid envelope alone does not deploy or execute a contract.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"QORVYNT_CONTRACT_TX_V1\0";
const ID_DOMAIN: &[u8] = b"QORVYNT_CONTRACT_TX_ID_V1\0";
const MAX_SENDER_BYTES: usize = 64;
const MAX_CONTRACT_ID_BYTES: usize = 64;
const MAX_METHOD_BYTES: usize = 64;
const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
const MAX_CALLDATA_BYTES: usize = 64 * 1024;
const MAX_ENCODED_BYTES: usize = MAX_ARTIFACT_BYTES + MAX_CALLDATA_BYTES + 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractOperation {
    Deploy { artifact: Vec<u8> },
    Call { contract_id: String, method: String, calldata: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractTransactionV1 {
    pub chain_id: u64,
    pub sender: String,
    pub nonce: u64,
    pub gas_limit: u64,
    /// Maximum total fee the sender authorizes for this transaction.
    pub max_fee: u64,
    /// Fee charged per consumed gas unit; protocol activation is a separate chain decision.
    pub gas_price: u64,
    pub valid_until_height: Option<u64>,
    pub operation: ContractOperation,
    /// Ed25519 signature over the canonical signing payload; excluded from that payload.
    pub signature: Vec<u8>,
}

impl ContractTransactionV1 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_fields()?;
        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(&1_u16.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        put_string(&mut out, &self.sender)?;
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.extend_from_slice(&self.gas_limit.to_be_bytes());
        out.extend_from_slice(&self.max_fee.to_be_bytes());
        out.extend_from_slice(&self.gas_price.to_be_bytes());
        match self.valid_until_height {
            Some(height) => { out.push(1); out.extend_from_slice(&height.to_be_bytes()); }
            None => out.push(0),
        }
        match &self.operation {
            ContractOperation::Deploy { artifact } => {
                out.push(0);
                put_bytes(&mut out, artifact)?;
            }
            ContractOperation::Call { contract_id, method, calldata } => {
                out.push(1);
                put_string(&mut out, contract_id)?;
                put_string(&mut out, method)?;
                put_bytes(&mut out, calldata)?;
            }
        }
        if out.len() > MAX_ENCODED_BYTES {
            return Err("contract transaction exceeds maximum encoded size".into());
        }
        Ok(out)
    }

    /// Canonical signed wire encoding: signing payload followed by the 64-byte signature.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        if self.signature.len() != 64 {
            return Err("Ed25519 signature must be exactly 64 bytes".into());
        }
        let mut out = self.signing_bytes()?;
        out.extend_from_slice(&self.signature);
        if out.len() > MAX_ENCODED_BYTES + 64 {
            return Err("contract transaction exceeds maximum encoded size".into());
        }
        Ok(out)
    }

    /// Decode and re-encode a signed transaction to enforce one canonical wire representation.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_ENCODED_BYTES + 64 {
            return Err("contract transaction exceeds maximum encoded size".into());
        }
        let mut reader = ContractTxReader { bytes, offset: 0 };
        reader.expect(DOMAIN)?;
        if reader.read_u16()? != 1 {
            return Err("unsupported contract transaction version".into());
        }
        let chain_id = reader.read_u64()?;
        let sender = reader.read_string(MAX_SENDER_BYTES)?;
        let nonce = reader.read_u64()?;
        let gas_limit = reader.read_u64()?;
        let max_fee = reader.read_u64()?;
        let gas_price = reader.read_u64()?;
        let valid_until_height = match reader.read_u8()? {
            0 => None,
            1 => Some(reader.read_u64()?),
            _ => return Err("invalid contract transaction expiry marker".into()),
        };
        let operation = match reader.read_u8()? {
            0 => ContractOperation::Deploy { artifact: reader.read_bytes(MAX_ARTIFACT_BYTES)? },
            1 => ContractOperation::Call {
                contract_id: reader.read_string(MAX_CONTRACT_ID_BYTES)?,
                method: reader.read_string(MAX_METHOD_BYTES)?,
                calldata: reader.read_bytes(MAX_CALLDATA_BYTES)?,
            },
            _ => return Err("invalid contract transaction operation tag".into()),
        };
        let signature = reader.take(64)?.to_vec();
        reader.finish()?;
        let transaction = Self {
            chain_id, sender, nonce, gas_limit, max_fee, gas_price,
            valid_until_height, operation, signature,
        };
        if transaction.canonical_bytes()? != bytes {
            return Err("contract transaction encoding is not canonical".into());
        }
        Ok(transaction)
    }

    pub fn transaction_id(&self) -> Result<[u8; 32], String> {
        if self.signature.len() != 64 {
            return Err("Ed25519 signature must be exactly 64 bytes".into());
        }
        let mut hasher = Sha256::new();
        hasher.update(ID_DOMAIN);
        hasher.update(self.signing_bytes()?);
        hasher.update(&self.signature);
        Ok(hasher.finalize().into())
    }

    pub fn verify_signature(&self, public_key_bytes: &[u8]) -> Result<(), String> {
        self.validate_fields()?;
        if public_key_bytes.len() != 32 {
            return Err("Ed25519 public key must be exactly 32 bytes".into());
        }
        if self.signature.len() != 64 {
            return Err("Ed25519 signature must be exactly 64 bytes".into());
        }
        let key_bytes: [u8; 32] = public_key_bytes.try_into()
            .map_err(|_| "Ed25519 public key must be exactly 32 bytes")?;
        let key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| "invalid Ed25519 public key")?;
        let sender = super::chain_protocol::address_from_public_key(public_key_bytes)?;
        if self.sender != sender {
            return Err("contract transaction sender does not match public key".into());
        }
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| "invalid Ed25519 signature encoding")?;
        key.verify_strict(&self.signing_bytes()?, &signature)
            .map_err(|_| "contract transaction signature verification failed".into())
    }

    pub fn validate_for(&self, chain_id: u64, height: u64) -> Result<u64, String> {
        self.validate_fields()?;
        if self.chain_id != chain_id {
            return Err("contract transaction chain ID does not match this network".into());
        }
        if self.valid_until_height.is_some_and(|expiry| expiry < height) {
            return Err("contract transaction has expired".into());
        }
        let maximum_fee = self.gas_limit.checked_mul(self.gas_price)
            .ok_or_else(|| "contract transaction maximum fee overflow".to_string())?;
        if maximum_fee > self.max_fee {
            return Err("gas limit multiplied by gas price exceeds max fee reserve".into());
        }
        Ok(maximum_fee)
    }

    fn validate_fields(&self) -> Result<(), String> {
        if self.sender.is_empty() || self.sender.len() > MAX_SENDER_BYTES {
            return Err("contract transaction sender length is invalid".into());
        }
        if self.gas_limit == 0 {
            return Err("contract transaction gas limit must be positive".into());
        }
        if self.gas_price == 0 {
            return Err("contract transaction gas price must be positive".into());
        }
        match &self.operation {
            ContractOperation::Deploy { artifact } => {
                if artifact.is_empty() || artifact.len() > MAX_ARTIFACT_BYTES {
                    return Err("contract artifact size is outside the permitted range".into());
                }
            }
            ContractOperation::Call { contract_id, method, calldata } => {
                if contract_id.is_empty() || contract_id.len() > MAX_CONTRACT_ID_BYTES {
                    return Err("contract ID length is invalid".into());
                }
                if method.is_empty() || method.len() > MAX_METHOD_BYTES
                    || !method.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err("contract method name is invalid".into());
                }
                if calldata.len() > MAX_CALLDATA_BYTES {
                    return Err("contract calldata exceeds the permitted size".into());
                }
            }
        }
        Ok(())
    }
}

struct ContractTxReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ContractTxReader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self.offset.checked_add(length).ok_or("contract transaction offset overflow")?;
        if end > self.bytes.len() { return Err("truncated contract transaction".into()); }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }
    fn expect(&mut self, expected: &[u8]) -> Result<(), String> {
        if self.take(expected.len())? != expected {
            return Err("unsupported contract transaction domain".into());
        }
        Ok(())
    }
    fn read_u8(&mut self) -> Result<u8, String> { Ok(self.take(1)?[0]) }
    fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| "invalid u16")?))
    }
    fn read_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| "invalid u64")?))
    }
    fn read_bytes(&mut self, maximum: usize) -> Result<Vec<u8>, String> {
        let length = u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| "invalid field length")?) as usize;
        if length > maximum { return Err("contract transaction field exceeds size limit".into()); }
        Ok(self.take(length)?.to_vec())
    }
    fn read_string(&mut self, maximum: usize) -> Result<String, String> {
        String::from_utf8(self.read_bytes(maximum)?).map_err(|_| "contract transaction string is not UTF-8".into())
    }
    fn finish(&self) -> Result<(), String> {
        if self.offset != self.bytes.len() { return Err("trailing bytes in contract transaction".into()); }
        Ok(())
    }
}

fn put_string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    put_bytes(out, value.as_bytes())
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), String> {
    let len = u32::try_from(value.len()).map_err(|_| "field exceeds canonical encoding limit")?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ContractOperation, ContractTransactionV1};
    use ed25519_dalek::{Signer, SigningKey};

    fn signed(operation: ContractOperation) -> (ContractTransactionV1, SigningKey) {
        let key = SigningKey::from_bytes(&[37; 32]);
        let public = key.verifying_key().to_bytes();
        let mut tx = ContractTransactionV1 {
            chain_id: 9001,
            sender: super::super::chain_protocol::address_from_public_key(&public).unwrap(),
            nonce: 4,
            gas_limit: 100_000,
            max_fee: 500_000,
            gas_price: 5,
            valid_until_height: Some(200),
            operation,
            signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        (tx, key)
    }

    #[test]
    fn signed_deployment_envelope_is_bound_to_chain_sender_and_artifact() {
        let (tx, key) = signed(ContractOperation::Deploy { artifact: b"QORVYNT-BCV2".to_vec() });
        assert!(tx.verify_signature(&key.verifying_key().to_bytes()).is_ok());
        assert_eq!(tx.validate_for(9001, 100).unwrap(), 500_000);
        assert_eq!(tx.transaction_id().unwrap().len(), 32);
        assert!(tx.validate_for(9002, 100).is_err());
        assert!(tx.validate_for(9001, 201).is_err());
    }

    #[test]
    fn signed_contract_transaction_wire_encoding_round_trips_canonically() {
        let (tx, _) = signed(ContractOperation::Call {
            contract_id: "counter".into(), method: "increment".into(), calldata: b"4".to_vec(),
        });
        let encoded = tx.canonical_bytes().unwrap();
        assert_eq!(ContractTransactionV1::from_canonical_bytes(&encoded).unwrap(), tx);
        let mut trailing = encoded;
        trailing.push(0);
        assert!(ContractTransactionV1::from_canonical_bytes(&trailing).is_err());
    }

    #[test]
    fn signed_call_binds_contract_method_and_calldata() {
        let (tx, key) = signed(ContractOperation::Call {
            contract_id: "token_01".into(),
            method: "transfer".into(),
            calldata: [b"alice".as_slice(), b"bob".as_slice(), b"25".as_slice()].concat(),
        });
        assert!(tx.verify_signature(&key.verifying_key().to_bytes()).is_ok());
        let mut altered = tx.clone();
        if let ContractOperation::Call { calldata, .. } = &mut altered.operation {
            calldata.push(0);
        }
        assert!(altered.verify_signature(&key.verifying_key().to_bytes()).is_err());
    }

    #[test]
    fn sender_key_mismatch_and_underfunded_fee_reserve_are_rejected() {
        let (tx, key) = signed(ContractOperation::Deploy { artifact: vec![1, 2, 3] });
        let wrong_key = SigningKey::from_bytes(&[38; 32]);
        assert!(tx.verify_signature(&wrong_key.verifying_key().to_bytes()).is_err());
        let mut underfunded = tx;
        underfunded.max_fee = 1;
        assert!(underfunded.validate_for(9001, 100).is_err());
        assert!(underfunded.verify_signature(&key.verifying_key().to_bytes()).is_err());
    }

    #[test]
    fn oversized_artifacts_and_invalid_method_names_fail_closed() {
        let (mut tx, _) = signed(ContractOperation::Deploy { artifact: vec![0, 1, 2] });
        if let ContractOperation::Deploy { artifact } = &mut tx.operation {
            artifact.resize(super::MAX_ARTIFACT_BYTES + 1, 0);
        }
        assert!(tx.signing_bytes().is_err());
        tx.operation = ContractOperation::Call {
            contract_id: "token".into(),
            method: "transfer;mint".into(),
            calldata: Vec::new(),
        };
        assert!(tx.signing_bytes().is_err());
    }
}

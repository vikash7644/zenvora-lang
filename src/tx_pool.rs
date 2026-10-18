//! Bounded signed-transaction pool for local development nodes.
//!
//! Transactions are admitted by replaying the existing pool against a cloned runtime. The pool
//! never mutates canonical chain state and is scoped to a specific chain ID and next block height.

use std::io::Write;

use super::chain_block::SignedTransactionEnvelope;
use super::chain_protocol::TransactionV1;
use super::chain_runtime::ChainRuntime;

const POOL_VERSION: &str = "ZENVORA_TX_POOL_V1";
const MAX_POOL_TRANSACTIONS: usize = 10_000;
const MAX_POOL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_POOL_LINE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct TransactionPool {
    chain_id: u64,
    next_height: u64,
    entries: Vec<SignedTransactionEnvelope>,
}

impl TransactionPool {
    pub fn new(chain_id: u64, next_height: u64) -> Result<Self, String> {
        if chain_id == 0 {
            return Err("chain ID must be nonzero".into());
        }
        Ok(Self { chain_id, next_height, entries: Vec::new() })
    }

    pub fn pending(&self) -> &[SignedTransactionEnvelope] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn submit(
        &mut self,
        runtime: &ChainRuntime,
        envelope: SignedTransactionEnvelope,
    ) -> Result<[u8; 32], String> {
        if self.entries.len() >= MAX_POOL_TRANSACTIONS {
            return Err("transaction pool is full".into());
        }
        if envelope.public_key.len() != 32 {
            return Err("transaction public key must be 32 bytes".into());
        }
        envelope.transaction.validate_for(self.chain_id, self.next_height)?;
        envelope.transaction.verify_signature(&envelope.public_key)?;
        let transaction_id = envelope.transaction.transaction_id()?;
        if self.entries.iter().any(|entry| entry.transaction.transaction_id().ok() == Some(transaction_id)) {
            return Err("transaction is already in the pool".into());
        }

        let mut candidate = runtime.clone();
        for entry in &self.entries {
            candidate.apply_signed_transaction(
                &entry.transaction, &entry.public_key, self.chain_id, self.next_height,
            )?;
        }
        candidate.apply_signed_transaction(
            &envelope.transaction, &envelope.public_key, self.chain_id, self.next_height,
        )?;
        self.entries.push(envelope);
        Ok(transaction_id)
    }

    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref();
        let temporary = path.with_extension("zpool.tmp");
        let result = (|| -> Result<(), String> {
            let mut file = std::fs::File::create(&temporary)
                .map_err(|error| format!("cannot create pool temporary file: {error}"))?;
            writeln!(file, "{POOL_VERSION}").map_err(|error| format!("cannot write pool: {error}"))?;
            writeln!(file, "C\t{}", self.chain_id).map_err(|error| format!("cannot write pool chain ID: {error}"))?;
            writeln!(file, "H\t{}", self.next_height).map_err(|error| format!("cannot write pool height: {error}"))?;
            for envelope in &self.entries {
                let transaction = envelope.transaction.canonical_bytes()?;
                writeln!(file, "T\t{}\t{}",
                    hex_encode(&envelope.public_key), hex_encode(&transaction))
                    .map_err(|error| format!("cannot write pool transaction: {error}"))?;
            }
            file.sync_all().map_err(|error| format!("cannot sync transaction pool: {error}"))?;
            if file.metadata().map_err(|error| error.to_string())?.len() > MAX_POOL_BYTES {
                return Err("transaction pool exceeds maximum file size".into());
            }
            std::fs::rename(&temporary, path)
                .map_err(|error| format!("cannot replace transaction pool: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    /// A missing pool file means an empty pool; an existing file must match the current chain context.
    pub fn load_from_file(
        path: impl AsRef<std::path::Path>,
        runtime: &ChainRuntime,
        chain_id: u64,
        next_height: u64,
    ) -> Result<Self, String> {
        let path = path.as_ref();
        let metadata = match std::fs::metadata(path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::new(chain_id, next_height),
            Err(error) => return Err(format!("cannot inspect transaction pool: {error}")),
        };
        if metadata.len() > MAX_POOL_BYTES {
            return Err("transaction pool exceeds maximum file size".into());
        }
        // Read at most the configured limit plus one byte so a concurrent file
        // replacement/growth cannot bypass the metadata size check.
        use std::io::Read;
        let mut content = String::new();
        std::fs::File::open(path)
            .map_err(|error| format!("cannot open transaction pool: {error}"))?
            .take(MAX_POOL_BYTES + 1)
            .read_to_string(&mut content)
            .map_err(|error| format!("cannot read transaction pool: {error}"))?;
        if content.len() as u64 > MAX_POOL_BYTES {
            return Err("transaction pool exceeds maximum file size".into());
        }
        let mut lines = content.lines();
        if lines.next() != Some(POOL_VERSION) {
            return Err("unsupported or missing transaction-pool version".into());
        }
        let chain_line = lines.next().ok_or_else(|| "transaction pool is missing chain ID".to_string())?;
        let stored_chain_id: u64 = chain_line.strip_prefix("C\t").ok_or_else(|| "invalid pool chain-ID record".to_string())?
            .parse().map_err(|_| "invalid pool chain ID")?;
        let height_line = lines.next().ok_or_else(|| "transaction pool is missing next height".to_string())?;
        let stored_height: u64 = height_line.strip_prefix("H\t").ok_or_else(|| "invalid pool height record".to_string())?
            .parse().map_err(|_| "invalid pool next height")?;
        if stored_chain_id != chain_id || stored_height != next_height {
            return Err("transaction pool context is stale; revalidate or recreate it for the current chain height".into());
        }

        let mut pool = Self::new(chain_id, next_height)?;
        for (index, line) in lines.enumerate() {
            let line_number = index + 4;
            if line.len() > MAX_POOL_LINE_BYTES {
                return Err(format!("transaction-pool line {line_number} exceeds maximum length"));
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let ["T", encoded_public_key, encoded_transaction] = fields.as_slice() else {
                return Err(format!("transaction-pool line {line_number} has an invalid record"));
            };
            let public_key = hex_decode(encoded_public_key)?;
            if public_key.len() != 32 {
                return Err(format!("transaction-pool line {line_number} has invalid public-key length"));
            }
            let transaction = TransactionV1::from_canonical_bytes(&hex_decode(encoded_transaction)?)?;
            pool.submit(runtime, SignedTransactionEnvelope { transaction, public_key })?;
        }
        Ok(pool)
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
    if value.is_empty() || value.len() % 2 != 0
        || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("invalid lowercase hexadecimal transaction-pool field".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        output.push(((high << 4) | low) as u8);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::TransactionPool;
    use super::super::chain_block::SignedTransactionEnvelope;
    use super::super::chain_protocol::{address_from_public_key, TransactionV1};
    use super::super::chain_runtime::ChainRuntime;
    use ed25519_dalek::{Signer, SigningKey};

    fn transaction(key: &SigningKey, nonce: u64, amount: u64) -> SignedTransactionEnvelope {
        let public_key = key.verifying_key().to_bytes();
        let mut transaction = TransactionV1 {
            chain_id: 71,
            sender: address_from_public_key(&public_key).unwrap(),
            nonce,
            recipient: "bob".into(),
            amount,
            max_fee: 1,
            gas_limit: 21_000,
            valid_until_height: Some(100),
            signature: Vec::new(),
        };
        transaction.signature = key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        SignedTransactionEnvelope { transaction, public_key: public_key.to_vec() }
    }

    fn runtime(key: &SigningKey) -> ChainRuntime {
        let mut runtime = ChainRuntime::new();
        let address = address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        runtime.set_genesis_balance(&address, 500).unwrap();
        runtime.set_genesis_balance("bob", 0).unwrap();
        runtime
    }

    #[test]
    fn pool_checks_signatures_nonces_balances_and_duplicate_ids() {
        let key = SigningKey::from_bytes(&[131; 32]);
        let runtime = runtime(&key);
        let mut pool = TransactionPool::new(71, 1).unwrap();
        let first = transaction(&key, 0, 100);
        pool.submit(&runtime, first.clone()).unwrap();
        assert_eq!(pool.len(), 1);
        assert!(pool.submit(&runtime, first).is_err());
        assert!(pool.submit(&runtime, transaction(&key, 2, 100)).is_err());
        assert!(pool.submit(&runtime, transaction(&key, 1, 10_000)).is_err());
        assert_eq!(runtime.balance("bob"), 0);
    }

    #[test]
    fn pool_file_round_trips_and_rejects_stale_context() {
        let key = SigningKey::from_bytes(&[132; 32]);
        let runtime = runtime(&key);
        let path = std::env::temp_dir().join(format!("zenvora-tx-pool-{}.zvp", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut pool = TransactionPool::new(71, 1).unwrap();
        pool.submit(&runtime, transaction(&key, 0, 100)).unwrap();
        pool.save_to_file(&path).unwrap();
        let loaded = TransactionPool::load_from_file(&path, &runtime, 71, 1).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(TransactionPool::load_from_file(&path, &runtime, 71, 2).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

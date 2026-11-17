//! Authenticated contract-call admission and execution on the existing QVM 2.0 runtime.
use std::collections::BTreeMap;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use super::contract_protocol::{ContractOperation, ContractTransactionV1};
use super::contract_runtime::{execute, ExecutionReceipt};
use super::contract_state::{AccessMode, ContractState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAccountState {
    pub storage: ContractState,
    deployed_contracts: BTreeMap<String, DeployedContract>,
    balances: BTreeMap<String, u64>,
    nonces: BTreeMap<String, u64>,
    pub fees_collected: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployedContract {
    pub name: String,
    pub source: String,
    pub bytecode_v2: String,
    pub deployer: String,
    pub deployment_nonce: u64,
    pub transaction_id: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentReceipt {
    pub contract_id: String,
    pub contract_name: String,
    pub transaction_id: [u8; 32],
    pub sender: String,
    pub nonce: u64,
    pub gas_charged: u64,
    pub fee_charged: u64,
    pub sender_balance: u64,
    pub next_nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedCallReceipt {
    pub transaction_id: [u8; 32],
    pub sender: String,
    pub nonce: u64,
    pub execution: Result<ExecutionReceipt, String>,
    pub gas_charged: u64,
    pub fee_charged: u64,
    pub sender_balance: u64,
    pub next_nonce: u64,
}

impl Default for ContractAccountState {
    fn default() -> Self {
        Self { storage: ContractState::new(), deployed_contracts: BTreeMap::new(), balances: BTreeMap::new(), nonces: BTreeMap::new(), fees_collected: 0 }
    }
}

impl ContractAccountState {
    pub fn new() -> Self { Self::default() }

    /// Genesis/faucet/test harness balance setter. Production issuance must be consensus-authorized.
    pub fn set_balance(&mut self, address: &str, amount: u64) -> Result<(), String> {
        if address.is_empty() || address.len() > 64 || !address.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("invalid account address".into());
        }
        self.balances.insert(address.to_owned(), amount);
        Ok(())
    }

    pub fn balance(&self, address: &str) -> u64 { self.balances.get(address).copied().unwrap_or(0) }
    pub fn nonce(&self, address: &str) -> u64 { self.nonces.get(address).copied().unwrap_or(0) }

    pub fn deployed_contract(&self, contract_id: &str) -> Option<&DeployedContract> { self.deployed_contracts.get(contract_id) }

    /// Admit and persist a signed deployment after validating the artifact against canonical
    /// QORVYNT source and Bytecode v2. The generated ID is domain-separated and deterministic.
    pub fn execute_signed_deployment(
        &mut self,
        tx: &ContractTransactionV1,
        public_key: &[u8],
        expected_chain_id: u64,
        current_height: u64,
    ) -> Result<DeploymentReceipt, String> {
        tx.verify_signature(public_key)?;
        let maximum_fee = tx.validate_for(expected_chain_id, current_height)?;
        let expected_nonce = self.nonce(&tx.sender);
        if tx.nonce != expected_nonce { return Err("contract transaction nonce mismatch or replay".into()); }
        let balance = self.balance(&tx.sender);
        if balance < tx.max_fee { return Err("insufficient balance for maximum authorized fee reserve".into()); }
        let artifact = match &tx.operation {
            ContractOperation::Deploy { artifact } => artifact,
            ContractOperation::Call { .. } => return Err("deployment executor requires a deploy operation".into()),
        };
        let (name, source, bytecode_v2) = super::validate_contract_artifact(artifact)?;
        let gas_charged = 21_000u64.checked_add(u64::try_from(artifact.len()).map_err(|_| "artifact gas size overflow")?)
            .ok_or_else(|| "deployment gas calculation overflow".to_string())?;
        if gas_charged > tx.gas_limit { return Err("out of gas during contract deployment".into()); }
        let fee_charged = gas_charged.checked_mul(tx.gas_price).ok_or_else(|| "deployment fee overflow".to_string())?;
        if fee_charged > maximum_fee || fee_charged > tx.max_fee { return Err("deployment fee exceeds authorized maximum".into()); }
        let next_nonce = expected_nonce.checked_add(1).ok_or_else(|| "account nonce overflow".to_string())?;
        let sender_balance = balance.checked_sub(fee_charged).ok_or_else(|| "sender fee balance underflow".to_string())?;
        let next_fees = self.fees_collected.checked_add(fee_charged).ok_or_else(|| "fee collector balance overflow".to_string())?;
        let transaction_id = tx.transaction_id()?;
        let mut hasher = Sha256::new();
        hasher.update(b"QORVYNT_CONTRACT_ADDRESS_V1\0");
        hasher.update(tx.sender.as_bytes());
        hasher.update(tx.nonce.to_be_bytes());
        hasher.update(transaction_id);
        let contract_id = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
        if self.deployed_contracts.contains_key(&contract_id) { return Err("contract address collision".into()); }
        self.deployed_contracts.insert(contract_id.clone(), DeployedContract {
            name: name.clone(), source, bytecode_v2, deployer: tx.sender.clone(),
            deployment_nonce: tx.nonce, transaction_id,
        });
        self.balances.insert(tx.sender.clone(), sender_balance);
        self.nonces.insert(tx.sender.clone(), next_nonce);
        self.fees_collected = next_fees;
        Ok(DeploymentReceipt { contract_id, contract_name: name, transaction_id, sender: tx.sender.clone(),
            nonce: tx.nonce, gas_charged, fee_charged, sender_balance, next_nonce })
    }

    /// Execute a signed call against the source registered by a prior verified deployment.
    /// Callers cannot substitute arbitrary source code for a deployed contract ID.
    pub fn execute_signed_registered_call(
        &mut self, tx: &ContractTransactionV1, public_key: &[u8],
        expected_chain_id: u64, current_height: u64,
    ) -> Result<SignedCallReceipt, String> {
        let contract_id = match &tx.operation {
            ContractOperation::Call { contract_id, .. } => contract_id,
            ContractOperation::Deploy { .. } => return Err("registered call executor requires a call operation".into()),
        };
        let source = self.deployed_contracts.get(contract_id)
            .map(|contract| contract.source.clone())
            .ok_or_else(|| "contract is not deployed in this QVM state".to_string())?;
        self.execute_signed_call(tx, public_key, expected_chain_id, current_height, &source)
    }
    /// Execute one signed call atomically against a candidate state. Failed VM calls consume
    /// the nonce and the full gas limit (bounded by max_fee), while contract storage rolls back.
    pub fn execute_signed_call(
        &mut self,
        tx: &ContractTransactionV1,
        public_key: &[u8],
        expected_chain_id: u64,
        current_height: u64,
        contract_source: &str,
    ) -> Result<SignedCallReceipt, String> {
        tx.verify_signature(public_key)?;
        let maximum_fee = tx.validate_for(expected_chain_id, current_height)?;
        let expected_nonce = self.nonce(&tx.sender);
        if tx.nonce != expected_nonce {
            return Err("contract transaction nonce mismatch or replay".into());
        }
        let balance = self.balance(&tx.sender);
        if balance < tx.max_fee {
            return Err("insufficient balance for maximum authorized fee reserve".into());
        }
        let (contract_id, method, calldata) = match &tx.operation {
            ContractOperation::Call { contract_id, method, calldata } => (contract_id, method, calldata),
            ContractOperation::Deploy { .. } => return Err("deployment admission is not enabled: artifact-to-contract registration is not implemented".into()),
        };
        let args = std::str::from_utf8(calldata).map_err(|_| "call calldata must be UTF-8 QORVYNT argument text")?;
        if args.contains('\n') || args.contains('\r') {
            return Err("call calldata must be a single-line argument list".into());
        }
        let invocation = format!("call {method}({args})");
        let mut candidate_storage = self.storage.clone();
        let result = execute(contract_source, contract_id, &invocation, AccessMode::Action, &mut candidate_storage, tx.gas_limit);
        let (execution, gas_charged) = match result {
            Ok(receipt) => (Ok(receipt.clone()), receipt.gas_used),
            Err(error) => (Err(error), tx.gas_limit),
        };
        let fee_charged = gas_charged.checked_mul(tx.gas_price)
            .ok_or_else(|| "actual contract fee overflow".to_string())?;
        if fee_charged > maximum_fee || fee_charged > tx.max_fee {
            return Err("actual contract fee exceeds authorized maximum".into());
        }
        let next_nonce = expected_nonce.checked_add(1).ok_or_else(|| "account nonce overflow".to_string())?;
        let sender_balance = balance.checked_sub(fee_charged).ok_or_else(|| "sender fee balance underflow".to_string())?;
        let next_fees = self.fees_collected.checked_add(fee_charged).ok_or_else(|| "fee collector balance overflow".to_string())?;
        let transaction_id = tx.transaction_id()?;

        if execution.is_ok() { self.storage = candidate_storage; }
        self.balances.insert(tx.sender.clone(), sender_balance);
        self.nonces.insert(tx.sender.clone(), next_nonce);
        self.fees_collected = next_fees;
        Ok(SignedCallReceipt {
            transaction_id, sender: tx.sender.clone(), nonce: tx.nonce, execution,
            gas_charged, fee_charged, sender_balance, next_nonce,
        })
    }

    /// Canonical snapshot includes QVM contract storage, account balances, replay-protection
    /// nonces and accumulated fees. Chain replay/state-root validation remains the caller's duty.
    pub fn encode_snapshot(&self) -> Result<Vec<u8>, String> {
        let storage = self.storage.encode_snapshot()?;
        let mut out = Vec::new();
        out.extend_from_slice(b"QORVYNT_CONTRACT_EXECUTOR_V2\0");
        put_bytes(&mut out, &storage)?;
        out.extend_from_slice(&self.fees_collected.to_be_bytes());
        put_contracts(&mut out, &self.deployed_contracts)?;
        put_map(&mut out, &self.balances)?;
        put_map(&mut out, &self.nonces)?;
        if out.len() > 4 * 1024 * 1024 { return Err("contract executor snapshot exceeds size limit".into()); }
        Ok(out)
    }

    pub fn decode_snapshot(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4 * 1024 * 1024 { return Err("contract executor snapshot exceeds size limit".into()); }
        let mut reader = ExecutorReader { bytes, offset: 0 };
        reader.expect(b"QORVYNT_CONTRACT_EXECUTOR_V2\0")?;
        let storage = ContractState::decode_snapshot(reader.read_bytes(2 * 1024 * 1024)?)?;
        let fees_collected = reader.read_u64()?;
        let deployed_contracts = reader.read_contracts()?;
        let balances = reader.read_map()?;
        let nonces = reader.read_map()?;
        reader.finish()?;
        let state = Self { storage, deployed_contracts, balances, nonces, fees_collected };
        if state.encode_snapshot()? != bytes { return Err("contract executor snapshot is not canonical".into()); }
        Ok(state)
    }

    pub fn save_snapshot(&self, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        let bytes = self.encode_snapshot()?;
        let name = path.file_name().ok_or_else(|| "snapshot path must name a file".to_string())?;
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(".tmp-{}", std::process::id()));
        let temp = path.with_file_name(temp_name);
        let result = (|| {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)
                .map_err(|e| format!("cannot create executor snapshot: {e}"))?;
            file.write_all(&bytes).map_err(|e| format!("cannot write executor snapshot: {e}"))?;
            file.sync_all().map_err(|e| format!("cannot sync executor snapshot: {e}"))?;
            std::fs::rename(&temp, path).map_err(|e| format!("cannot install executor snapshot: {e}"))?;
            Ok(())
        })();
        if result.is_err() { let _ = std::fs::remove_file(&temp); }
        result
    }

    pub fn load_snapshot(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let metadata = std::fs::metadata(path).map_err(|e| format!("cannot stat executor snapshot: {e}"))?;
        if metadata.len() > 4 * 1024 * 1024 { return Err("contract executor snapshot exceeds size limit".into()); }
        let mut file = std::fs::File::open(path).map_err(|e| format!("cannot open executor snapshot: {e}"))?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read executor snapshot: {e}"))?;
        Self::decode_snapshot(&bytes)
    }
}


fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    let len = u32::try_from(bytes.len()).map_err(|_| "snapshot field too large")?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn put_contracts(out: &mut Vec<u8>, map: &BTreeMap<String, DeployedContract>) -> Result<(), String> {
    out.extend_from_slice(&u32::try_from(map.len()).map_err(|_| "too many deployed contracts")?.to_be_bytes());
    for (id, contract) in map {
        put_bytes(out, id.as_bytes())?; put_bytes(out, contract.name.as_bytes())?;
        put_bytes(out, contract.source.as_bytes())?; put_bytes(out, contract.bytecode_v2.as_bytes())?;
        put_bytes(out, contract.deployer.as_bytes())?; out.extend_from_slice(&contract.deployment_nonce.to_be_bytes());
        out.extend_from_slice(&contract.transaction_id);
    }
    Ok(())
}

fn put_map(out: &mut Vec<u8>, map: &BTreeMap<String, u64>) -> Result<(), String> {
    out.extend_from_slice(&u32::try_from(map.len()).map_err(|_| "too many account records")?.to_be_bytes());
    for (address, value) in map {
        if address.is_empty() || address.len() > 64 || !address.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("invalid account address in snapshot".into());
        }
        put_bytes(out, address.as_bytes())?;
        out.extend_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

struct ExecutorReader<'a> { bytes: &'a [u8], offset: usize }
impl<'a> ExecutorReader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.offset.checked_add(n).ok_or("snapshot offset overflow")?;
        if end > self.bytes.len() { return Err("truncated executor snapshot".into()); }
        let value = &self.bytes[self.offset..end]; self.offset = end; Ok(value)
    }
    fn expect(&mut self, tag: &[u8]) -> Result<(), String> {
        if self.take(tag.len())? != tag { return Err("unsupported executor snapshot version".into()); } Ok(())
    }
    fn read_u32(&mut self) -> Result<u32, String> { Ok(u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| "bad u32")?)) }
    fn read_u64(&mut self) -> Result<u64, String> { Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| "bad u64")?)) }
    fn read_bytes(&mut self, max: usize) -> Result<&'a [u8], String> {
        let len = self.read_u32()? as usize;
        if len > max { return Err("executor snapshot field exceeds limit".into()); }
        self.take(len)
    }
    fn read_contracts(&mut self) -> Result<BTreeMap<String, DeployedContract>, String> {
        let count = self.read_u32()? as usize;
        if count > 100_000 { return Err("too many deployed contracts in snapshot".into()); }
        let mut map = BTreeMap::new(); let mut previous: Option<String> = None;
        for _ in 0..count {
            let id = String::from_utf8(self.read_bytes(64)?.to_vec()).map_err(|_| "contract ID is not UTF-8")?;
            if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) || previous.as_ref().is_some_and(|p| p >= &id) { return Err("deployed contract IDs are invalid or unsorted".into()); }
            previous = Some(id.clone());
            let name = String::from_utf8(self.read_bytes(64)?.to_vec()).map_err(|_| "contract name is not UTF-8")?;
            let source = String::from_utf8(self.read_bytes(1024 * 1024)?.to_vec()).map_err(|_| "contract source is not UTF-8")?;
            let bytecode_v2 = String::from_utf8(self.read_bytes(1024 * 1024)?.to_vec()).map_err(|_| "contract bytecode is not UTF-8")?;
            let deployer = String::from_utf8(self.read_bytes(64)?.to_vec()).map_err(|_| "deployer is not UTF-8")?;
            let deployment_nonce = self.read_u64()?;
            let transaction_id: [u8; 32] = self.take(32)?.try_into().map_err(|_| "invalid deployment transaction ID")?;
            if name.is_empty() || deployer.is_empty() { return Err("invalid deployed contract metadata".into()); }
            let (compiled, layout) = super::compile_with_layout(&source).map_err(|_| "deployed contract source is invalid")?;
            let canonical_bytecode = super::serialize_bytecode_v2(&compiled, &layout).map_err(|_| "cannot verify deployed contract bytecode")?;
            if canonical_bytecode != bytecode_v2 { return Err("deployed contract bytecode does not match canonical source".into()); }
            let mut hasher = Sha256::new(); hasher.update(b"QORVYNT_CONTRACT_ADDRESS_V1\0");
            hasher.update(deployer.as_bytes()); hasher.update(deployment_nonce.to_be_bytes()); hasher.update(transaction_id);
            let expected_id = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
            if id != expected_id { return Err("deployed contract ID does not match deployment metadata".into()); }
            map.insert(id, DeployedContract { name, source, bytecode_v2, deployer, deployment_nonce, transaction_id });
        }
        Ok(map)
    }
    fn read_map(&mut self) -> Result<BTreeMap<String, u64>, String> {
        let count = self.read_u32()? as usize;
        if count > 100_000 { return Err("too many account records in snapshot".into()); }
        let mut map = BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..count {
            let address = String::from_utf8(self.read_bytes(64)?.to_vec()).map_err(|_| "account address is not UTF-8")?;
            if address.is_empty() || !address.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { return Err("invalid account address in snapshot".into()); }
            if previous.as_ref().is_some_and(|p| p >= &address) { return Err("account records are not strictly sorted".into()); }
            previous = Some(address.clone());
            map.insert(address, self.read_u64()?);
        }
        Ok(map)
    }
    fn finish(&self) -> Result<(), String> {
        if self.offset != self.bytes.len() { return Err("trailing bytes in executor snapshot".into()); } Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ContractAccountState;
    use super::super::contract_protocol::{ContractOperation, ContractTransactionV1};
    use ed25519_dalek::{Signer, SigningKey};

    const SOURCE: &str = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview current() {\nreturn count\n}\n}\n";

    fn signed_call(key: &SigningKey, nonce: u64, args: &str) -> ContractTransactionV1 {
        let public = key.verifying_key().to_bytes();
        let mut tx = ContractTransactionV1 {
            chain_id: 9001,
            sender: super::super::chain_protocol::address_from_public_key(&public).unwrap(),
            nonce, gas_limit: 100_000, max_fee: 500_000, gas_price: 2,
            valid_until_height: Some(200),
            operation: ContractOperation::Call { contract_id: "counter".into(), method: "increment".into(), calldata: args.as_bytes().to_vec() },
            signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        tx
    }

    #[test]
    fn signed_call_checks_signature_nonce_executes_qvm2_and_settles_actual_fee() {
        let key = SigningKey::from_bytes(&[71; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "7");
        let receipt = state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).unwrap();
        assert!(receipt.execution.is_ok());
        assert_eq!(state.storage.get("counter", "count").unwrap(), Some(&b"7"[..]));
        assert_eq!(state.nonce(&address), 1);
        assert_eq!(receipt.fee_charged, receipt.gas_charged * 2);
        assert_eq!(state.balance(&address), 1_000_000 - receipt.fee_charged);
        assert_eq!(state.fees_collected, receipt.fee_charged);
    }

    #[test]
    fn replayed_or_wrong_nonce_transactions_are_rejected_without_charging() {
        let key = SigningKey::from_bytes(&[72; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "1");
        state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).unwrap();
        let balance = state.balance(&address);
        assert!(state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).is_err());
        assert_eq!(state.balance(&address), balance);
    }

    #[test]
    fn failed_execution_charges_gas_but_rolls_back_contract_storage() {
        let key = SigningKey::from_bytes(&[73; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "1");
        let receipt = state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, "not a contract").unwrap();
        assert!(receipt.execution.is_err());
        assert_eq!(state.storage.get("counter", "count").unwrap(), None);
        assert_eq!(state.nonce(&address), 1);
        assert_eq!(receipt.fee_charged, tx.gas_limit * tx.gas_price);
    }


    #[test]
    fn executor_snapshot_preserves_storage_balances_nonces_and_fees() {
        let key = SigningKey::from_bytes(&[75; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "6");
        state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).unwrap();
        let encoded = state.encode_snapshot().unwrap();
        let restored = ContractAccountState::decode_snapshot(&encoded).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.storage.get("counter", "count").unwrap(), Some(&b"6"[..]));
        assert_eq!(restored.nonce(&address), 1);
    }

    #[test]
    fn invalid_signature_and_underfunded_reserve_do_not_consume_nonce() {
        let key = SigningKey::from_bytes(&[74; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1).unwrap();
        let tx = signed_call(&key, 0, "1");
        assert!(state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).is_err());
        assert_eq!(state.nonce(&address), 0);
    }

    #[test]
    fn signed_deployment_registers_verified_artifact_and_survives_snapshot_restore() {
        let key = SigningKey::from_bytes(&[81; 32]);
        let public = key.verifying_key().to_bytes();
        let address = super::super::chain_protocol::address_from_public_key(&public).unwrap();
        let source = r#"contract Counter {
state count := 0
action increment(amount) {
count := count + amount
}
view current() {
return count
}
}
"#;
        let artifact = serde_json::to_vec(&super::super::contract_abi_and_artifact(source).unwrap()).unwrap();
        let mut tx = ContractTransactionV1 {
            chain_id: 9001, sender: address.clone(), nonce: 0, gas_limit: 100_000,
            max_fee: 200_000, gas_price: 2, valid_until_height: Some(200),
            operation: ContractOperation::Deploy { artifact }, signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let receipt = state.execute_signed_deployment(&tx, &public, 9001, 100).unwrap();
        let deployed = state.deployed_contract(&receipt.contract_id).cloned().unwrap();
        assert_eq!(deployed.name, "Counter");
        assert_eq!(deployed.source, source);
        assert!(deployed.bytecode_v2.starts_with("ZENVORA_BYTECODE 2"));
        assert_eq!(state.nonce(&address), 1);
        assert_eq!(state.fees_collected, receipt.fee_charged);
        assert!(state.execute_signed_deployment(&tx, &public, 9001, 100).is_err());

        let mut call = signed_call(&key, 1, "9");
        call.operation = ContractOperation::Call { contract_id: receipt.contract_id.clone(), method: "increment".into(), calldata: b"9".to_vec() };
        call.signature = key.sign(&call.signing_bytes().unwrap()).to_bytes().to_vec();
        let call_receipt = state.execute_signed_registered_call(&call, &public, 9001, 100).unwrap();
        assert!(call_receipt.execution.is_ok());
        assert_eq!(state.storage.get(&receipt.contract_id, "count").unwrap(), Some(&b"9"[..]));

        let restored = ContractAccountState::decode_snapshot(&state.encode_snapshot().unwrap()).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.deployed_contract(&receipt.contract_id).cloned().unwrap(), deployed);
    }

}

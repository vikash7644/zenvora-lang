use std::collections::{BTreeMap, HashMap, HashSet};
use std::{env, fs, process};
use std::path::Path;
use std::io::Read;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

mod module_loader;
mod chain_runtime;
mod chain_protocol;
mod chain_consensus;
mod chain_block;
mod block_store;
mod chain_rpc;
mod compiler_api;
mod chain_p2p;
mod tx_pool;
mod bytecode_v2;
mod contract_state;
mod contract_runtime;
mod contract_gas;
mod contract_protocol;

#[derive(Debug, Clone, PartialEq)]
enum Token {
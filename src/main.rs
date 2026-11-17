mod compiler_api;
mod chain_p2p;
mod tx_pool;
mod bytecode_v2;
mod contract_state;
mod contract_runtime;
mod contract_executor;
mod contract_block;
mod contract_block_store;
mod contract_gas;
pub mod contract_protocol;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64), String(String), Bool(bool), Ident(String), And, Or, Not, Plus, Minus, Star, Slash,
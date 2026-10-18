//! Loopback-only, read-only JSON-RPC for the local Zenvora Chain prototype.
//!
//! The server deliberately binds only to 127.0.0.1. It replays the block store on each request,
//! favoring correctness and simplicity over performance. It is not a public RPC service.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

use super::block_store::BlockStore;

const MAX_HTTP_REQUEST_BYTES: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(3);

pub fn serve(store_path: String, chain_id: u64, port: u16) -> Result<(), String> {
    if chain_id == 0 {
        return Err("chain ID must be nonzero".into());
    }
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address)
        .map_err(|error| format!("cannot bind local RPC at {address}: {error}"))?;
    println!("Zenvora read-only RPC listening at http://{address}");
    println!("Only loopback clients can connect. Press Ctrl+C to stop.");
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
                let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
                match read_http_body(&mut stream) {
                    Ok(body) => {
                        let response = handle_json_rpc(&body, &store_path, chain_id).to_string();
                        write_http_response(&mut stream, "200 OK", &response)?;
                    }
                    Err(error) => {
                        let response = json!({
                            "jsonrpc": "2.0",
                            "id": Value::Null,
                            "error": { "code": -32600, "message": error }
                        }).to_string();
                        write_http_response(&mut stream, "400 Bad Request", &response)?;
                    }
                }
            }
            Err(error) => eprintln!("RPC connection failed: {error}"),
        }
    }
    Ok(())
}

fn read_http_body(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        if buffer.len() > MAX_HTTP_REQUEST_BYTES {
            return Err("HTTP request exceeds maximum size".into());
        }
        if let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&buffer[..header_end])
                .map_err(|_| "HTTP headers are not valid UTF-8")?;
            let mut lines = headers.split("\r\n");
            let request_line = lines.next().unwrap_or("");
            if request_line != "POST / HTTP/1.1" && request_line != "POST / HTTP/1.0" {
                return Err("only POST / is supported".into());
            }
            let mut content_length = None;
            for line in lines {
                let (name, value) = line.split_once(':')
                    .ok_or_else(|| "malformed HTTP header".to_string())?;
                if name.eq_ignore_ascii_case("transfer-encoding") {
                    return Err("chunked transfer encoding is not supported".into());
                }
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = Some(value.trim().parse::<usize>()
                        .map_err(|_| "invalid Content-Length")?);
                }
            }
            let length = content_length.ok_or_else(|| "Content-Length is required".to_string())?;
            let body_start = header_end + 4;
            let body_end = body_start.checked_add(length)
                .ok_or_else(|| "HTTP body length overflow".to_string())?;
            if body_end > MAX_HTTP_REQUEST_BYTES {
                return Err("HTTP request exceeds maximum size".into());
            }
            if buffer.len() >= body_end {
                return Ok(buffer[body_start..body_end].to_vec());
            }
        }
        let read = stream.read(&mut chunk).map_err(|error| format!("cannot read HTTP request: {error}"))?;
        if read == 0 {
            return Err("connection closed before HTTP body completed".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).and_then(|_| stream.write_all(body.as_bytes()))
        .map_err(|error| format!("cannot write HTTP response: {error}"))
}

pub fn handle_json_rpc(body: &[u8], store_path: &str, expected_chain_id: u64) -> Value {
    let request: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return rpc_error(Value::Null, -32700, "parse error"),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return rpc_error(id, -32600, "jsonrpc must be 2.0");
    }
    let method = match request.get("method").and_then(Value::as_str) {
        Some(value) => value,
        None => return rpc_error(id, -32600, "method must be a string"),
    };
    let params = request.get("params").cloned().unwrap_or_else(|| json!([]));
    let (store, runtime) = match BlockStore::load_from_file(store_path, expected_chain_id) {
        Ok(value) => value,
        Err(error) => return rpc_error(id, -32000, &format!("block-store verification failed: {error}")),
    };

    let result = match method {
        "chain_getInfo" => {
            let tip = match store.blocks().last() {
                Some(value) => value,
                None => return rpc_error(id, -32000, "block store has no genesis block"),
            };
            json!({
                "chainId": store.chain_id(),
                "blockCount": store.blocks().len(),
                "height": tip.header.height,
                "tipHash": hex_encode(&tip.header.header_hash().unwrap_or([0; 32])),
                "stateRoot": hex_encode(&runtime.state_root()),
                "consensus": "not implemented",
                "storage": "local replay-verified file"
            })
        }
        "chain_getBalance" => {
            let address = match string_param(&params, "address", 0) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "expected address parameter"),
            };
            json!({ "address": address, "balance": runtime.balance(address), "nonce": runtime.account_nonce(address) })
        }
        "chain_getBlock" => {
            let height = match number_param(&params, "height", 0) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "expected numeric height parameter"),
            };
            let block = match store.blocks().iter().find(|block| block.header.height == height) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "block height not found"),
            };
            let transactions: Vec<Value> = block.transactions.iter().map(|envelope| {
                json!({
                    "transactionId": envelope.transaction.transaction_id().map(|id| hex_encode(&id)).unwrap_or_default(),
                    "sender": envelope.transaction.sender,
                    "recipient": envelope.transaction.recipient,
                    "nonce": envelope.transaction.nonce,
                    "amount": envelope.transaction.amount,
                    "maxFee": envelope.transaction.max_fee
                })
            }).collect();
            json!({
                "chainId": block.header.chain_id,
                "height": block.header.height,
                "timestamp": block.header.timestamp_secs,
                "parentHash": hex_encode(&block.header.parent_hash),
                "transactionRoot": hex_encode(&block.header.transaction_root),
                "receiptRoot": hex_encode(&block.header.receipt_root),
                "stateRoot": hex_encode(&block.header.state_root),
                "proposer": block.header.proposer,
                "transactions": transactions
            })
        }
        "chain_getTransaction" => {
            let id_text = match string_param(&params, "transactionId", 0) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "expected transactionId parameter"),
            };
            let found = store.blocks().iter().find_map(|block| {
                block.transactions.iter().enumerate().find_map(|(index, envelope)| {
                    envelope.transaction.transaction_id().ok().filter(|txid| hex_encode(txid) == id_text)
                        .map(|_| (block.header.height, index, &envelope.transaction))
                })
            });
            match found {
                Some((height, index, transaction)) => json!({
                    "transactionId": id_text,
                    "blockHeight": height,
                    "transactionIndex": index,
                    "chainId": transaction.chain_id,
                    "sender": transaction.sender,
                    "recipient": transaction.recipient,
                    "nonce": transaction.nonce,
                    "amount": transaction.amount,
                    "maxFee": transaction.max_fee,
                    "gasLimit": transaction.gas_limit,
                    "validUntilHeight": transaction.valid_until_height
                }),
                None => return rpc_error(id, -32602, "transaction ID not found"),
            }
        }
        _ => return rpc_error(id, -32601, "method not found"),
    };
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn string_param<'a>(params: &'a Value, object_key: &str, index: usize) -> Option<&'a str> {
    params.get(object_key).and_then(Value::as_str)
        .or_else(|| params.get(index).and_then(Value::as_str))
}

fn number_param(params: &Value, object_key: &str, index: usize) -> Option<u64> {
    params.get(object_key).and_then(Value::as_u64)
        .or_else(|| params.get(index).and_then(Value::as_u64))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::handle_json_rpc;
    use super::super::block_store::BlockStore;
    use super::super::chain_block::execute_block;
    use super::super::chain_protocol::{transaction_merkle_root, BlockHeaderV1};
    use super::super::chain_runtime::ChainRuntime;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn test_store() -> (String, String) {
        let path = std::env::temp_dir().join(format!("zenvora-rpc-{}.zvb", std::process::id()));
        let path_text = path.to_string_lossy().to_string();
        let mut allocations = BTreeMap::new();
        allocations.insert("alice".to_string(), 100);
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance("alice", 100).unwrap();
        let header = BlockHeaderV1 {
            chain_id: 71, height: 0, timestamp_secs: 1_800_000_000,
            parent_hash: [0; 32], transaction_root: transaction_merkle_root(&[]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[]).unwrap(), state_root: runtime.state_root(),
            proposer: "validator_1".into(), consensus_proof: Vec::new(),
        };
        execute_block(&mut runtime, &header, &[], 71, None).unwrap();
        let mut store = BlockStore::new(71, allocations).unwrap();
        store.append_executed_block(&path_text, &runtime, &header, &[], &[]).unwrap();
        (path_text, path.to_string_lossy().to_string())
    }

    #[test]
    fn json_rpc_info_balance_and_unknown_method_work() {
        let (path, cleanup_path) = test_store();
        let info = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":1,"method":"chain_getInfo","params":[]}"#,
            &path, 71,
        );
        assert_eq!(info["result"]["chainId"], 71);
        assert_eq!(info["result"]["height"], 0);

        let balance = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":2,"method":"chain_getBalance","params":{"address":"alice"}}"#,
            &path, 71,
        );
        assert_eq!(balance["result"]["balance"], 100);
        assert_eq!(balance["result"]["nonce"], 0);

        let missing = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":3,"method":"no_such_method","params":[]}"#,
            &path, 71,
        );
        assert_eq!(missing["error"]["code"], -32601);
        std::fs::remove_file(cleanup_path).unwrap();
    }
}

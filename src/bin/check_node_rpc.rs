//! Read-only smoke checker for the current Zenvora Chain JSON-RPC prototype.
//! Supports plain HTTP only; the current node binds to loopback by design.

use serde_json::{json, Value};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("FAIL: {message}");
    std::process::exit(1);
}

fn rpc_url() -> String {
    let url = env::var("RPC_URL").unwrap_or_else(|_| {
        fail("set RPC_URL, for example http://127.0.0.1:8545/")
    });
    let rest = url.strip_prefix("http://").unwrap_or_else(|| {
        fail("only http:// is supported; do not send credentials over an untrusted network")
    });
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty() || authority.contains('@') || authority.contains('?') || authority.contains('#') {
        fail("RPC_URL must contain a host and optional port, without user-info/query/fragment");
    }
    if !path.is_empty() {
        fail("the current Zenvora RPC accepts POST / only; RPC_URL must not include a path");
    }
    if !authority.contains(':') {
        fail("RPC_URL must include an explicit port, for example http://127.0.0.1:8545/");
    }
    authority.to_string()
}

fn request_rpc(address: &str, token: Option<&str>, payload: &Value) -> Result<Value, String> {
    if token.is_some_and(|t| t.contains('\r') || t.contains('\n')) {
        return Err("RPC_TOKEN must not contain newline characters".into());
    }
    let socket = address.to_socket_addrs().map_err(|e| format!("invalid RPC address: {e}"))?
        .next().ok_or_else(|| "RPC address resolved to no socket addresses".to_string())?;
    let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
        .map_err(|e| format!("cannot connect to {address}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;

    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let mut headers = format!(
        "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nAccept: application/json\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(value) = token {
        headers.push_str(&format!("Authorization: Bearer {value}\r\n"));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes()).map_err(|e| format!("write HTTP headers: {e}"))?;
    stream.write_all(&body).map_err(|e| format!("write JSON-RPC body: {e}"))?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|e| format!("read HTTP response: {e}"))?;
    let split = response.windows(4).position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response: missing header terminator".to_string())?;
    let header = std::str::from_utf8(&response[..split]).map_err(|_| "HTTP headers are not UTF-8")?;
    let status = header.lines().next().unwrap_or("");
    if !status.contains(" 200 ") {
        return Err(format!("HTTP status was not 200: {status}"));
    }
    let body = &response[split + 4..];
    serde_json::from_slice(body).map_err(|e| format!("response body is not valid JSON: {e}"))
}

fn call(address: &str, token: Option<&str>, id: u64, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
    let response = request_rpc(address, token, &request)
        .unwrap_or_else(|e| fail(format!("{method}: {e}")));
    if response.get("id").and_then(Value::as_u64) != Some(id) {
        fail(format!("{method}: response id did not match request"));
    }
    if let Some(error) = response.get("error") {
        fail(format!("{method}: JSON-RPC error {error}"));
    }
    response.get("result").cloned().unwrap_or_else(|| fail(format!("{method}: response has neither result nor error")))
}

fn main() {
    let address = rpc_url();
    let token = env::var("RPC_TOKEN").ok();
    println!("Zenvora read-only RPC smoke check");
    let info = call(&address, token.as_deref(), 1, "chain_getInfo", json!([]));
    let chain_id = info.get("chainId").and_then(Value::as_u64)
        .unwrap_or_else(|| fail("chain_getInfo result is missing numeric chainId"));
    let height = info.get("height").and_then(Value::as_u64)
        .unwrap_or_else(|| fail("chain_getInfo result is missing numeric height"));
    let block_count = info.get("blockCount").and_then(Value::as_u64)
        .unwrap_or_else(|| fail("chain_getInfo result is missing numeric blockCount"));
    if chain_id == 0 || block_count == 0 {
        fail("chain info reports an invalid chainId or empty block store");
    }
    println!("PASS chain_getInfo: chainId={chain_id}, height={height}, blockCount={block_count}");

    if let Ok(address_value) = env::var("RPC_TEST_ADDRESS") {
        let balance = call(&address, token.as_deref(), 2, "chain_getBalance", json!({"address":address_value}));
        if balance.get("balance").and_then(Value::as_u64).is_none() {
            fail("chain_getBalance result is missing numeric balance");
        }
        if balance.get("nonce").and_then(Value::as_u64).is_none() {
            fail("chain_getBalance result is missing numeric nonce");
        }
        println!("PASS chain_getBalance: address={address_value}, balance={}, nonce={}",
            balance["balance"], balance["nonce"]);
    } else {
        println!("SKIP chain_getBalance: set RPC_TEST_ADDRESS to test a known address");
    }

    let test_height = env::var("RPC_TEST_HEIGHT").ok()
        .map(|s| s.parse::<u64>().unwrap_or_else(|_| fail("RPC_TEST_HEIGHT must be an unsigned integer")))
        .unwrap_or(height);
    let block = call(&address, token.as_deref(), 3, "chain_getBlock", json!({"height":test_height}));
    if block.get("height").and_then(Value::as_u64) != Some(test_height) {
        fail("chain_getBlock returned a different height than requested");
    }
    println!("PASS chain_getBlock: height={test_height}");
    println!("NOTE: this checks read-only RPC connectivity and response shape; it does not prove consensus, finality, or production security.");
}

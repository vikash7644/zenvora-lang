//! Versioned HTTP API for compiling and executing Zenvora Lang source.
//! It binds to loopback by default; public binding requires bearer-token configuration.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_SOURCE_BYTES: usize = 48 * 1024;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_ACTIVE_CONNECTIONS: usize = 32;
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

struct ConnectionGuard;

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.parse::<IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

fn validate_bind_security(host: &str, token: &str) -> Result<(), String> {
    if !is_loopback_host(host) && token.len() < 32 {
        return Err("public compiler API binding requires ZENVORA_API_TOKEN with at least 32 characters".into());
    }
    if !token.is_empty() && token.len() < 16 {
        return Err("ZENVORA_API_TOKEN must contain at least 16 characters".into());
    }
    Ok(())
}

fn bearer_token_matches(expected: &str, authorization: &str) -> bool {
    let Some(provided) = authorization.strip_prefix("Bearer ") else {
        return false;
    };
    if expected.len() != provided.len() {
        return false;
    }
    expected.as_bytes().iter().zip(provided.as_bytes())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right)) == 0
}

pub fn serve(port: u16) -> Result<(), String> {
    let host = std::env::var("ZENVORA_API_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let is_loopback = is_loopback_host(&host);
    let token = std::env::var("ZENVORA_API_TOKEN").unwrap_or_default();
    validate_bind_security(&host, &token)?;
    let address = match host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::new(ip, port).to_string(),
        Err(_) => format!("{host}:{port}"),
    };
    let listener = TcpListener::bind(&address)
        .map_err(|e| format!("cannot bind compiler API at {address}: {e}"))?;
    println!("Zenvora compiler API listening at http://{address}");
    println!("API v1 routes: health, capabilities, compile, compile-bytecode-v2, verify-bytecode-v2, run-bytecode-v2, compile-run");
    if is_loopback {
        println!("Loopback mode: API is reachable only from this device.");
    } else {
        println!("Public bind enabled: bearer-token authentication is required for compile routes.");
    }
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let active = ACTIVE_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
                if active >= MAX_ACTIVE_CONNECTIONS {
                    ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
                    let _ = respond(&mut stream, "503 Service Unavailable", &json!({"ok":false,"error":"server is at connection capacity"}));
                    continue;
                }
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                std::thread::spawn(move || {
                    let _guard = ConnectionGuard;
                    if let Err(error) = handle(&mut stream) {
                        eprintln!("compiler API request error: {error}");
                    }
                });
            }
            Err(error) => eprintln!("compiler API connection error: {error}"),
        }
    }
    Ok(())
}

fn handle(stream: &mut TcpStream) -> Result<(), String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut header_end = None;
    let mut content_length = None;
    let mut route = String::new();
    let mut method = String::new();
    let mut content_type = String::new();
    let mut authorization = String::new();
    let mut content_type_seen = false;
    let mut authorization_seen = false;

    loop {
        if buffer.len() > MAX_REQUEST_BYTES {
            return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"request exceeds 2 MiB"}));
        }
        if header_end.is_none() && buffer.len() > MAX_HEADER_BYTES {
            return respond(stream, "431 Request Header Fields Too Large", &json!({"ok":false,"error":"HTTP headers exceed 16 KiB"}));
        }
        if header_end.is_none() {
            if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = match std::str::from_utf8(&buffer[..pos]) {
                    Ok(headers) => headers,
                    Err(_) => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"invalid HTTP headers"})),
                };
                let mut lines = headers.split("\r\n");
                let request_line = lines.next().unwrap_or("");
                let mut request_parts = request_line.split_whitespace();
                method = request_parts.next().unwrap_or("").to_string();
                let raw_path = request_parts.next().unwrap_or("");
                let version = request_parts.next().unwrap_or("");
                if method.is_empty() || raw_path.is_empty() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") || request_parts.next().is_some() {
                    return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"malformed HTTP request line"}));
                }
                route = raw_path.split('?').next().unwrap_or("").to_string();
                let mut length = None;
                for line in lines {
                    let Some((name, value)) = line.split_once(':') else {
                        return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"malformed HTTP header"}));
                    };
                    let name = name.trim();
                    let value = value.trim();
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"transfer-encoding is unsupported"}));
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        if length.is_some() {
                            return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"duplicate Content-Length"}));
                        }
                        length = Some(match value.parse::<usize>() {
                            Ok(length) => length,
                            Err(_) => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"invalid Content-Length"})),
                        });
                    } else if name.eq_ignore_ascii_case("content-type") {
                        if content_type_seen {
                            return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"duplicate Content-Type"}));
                        }
                        content_type_seen = true;
                        content_type = value.to_ascii_lowercase();
                    } else if name.eq_ignore_ascii_case("authorization") {
                        if authorization_seen {
                            return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"duplicate Authorization"}));
                        }
                        authorization_seen = true;
                        authorization = value.to_string();
                    }
                }

                if method == "OPTIONS" {
                    return respond(stream, "200 OK", &json!({"ok":true}));
                }
                if method == "GET" {
                    return match route.as_str() {
                        "/health" | "/v1/health" => respond(stream, "200 OK", &json!({
                            "ok":true,"service":"zenvora-compiler-api","api_version":"v1",
                            "compiler":concat!("zenvora-rust-v", env!("CARGO_PKG_VERSION")),
                            "mode": if std::env::var("ZENVORA_API_TOKEN").map(|value| !value.is_empty()).unwrap_or(false) {"authenticated"} else {"development"}
                        })),
                        "/v1/capabilities" => respond(stream, "200 OK", &json!({
                            "ok":true,"api_version":"v1",
                            "endpoints":["GET /v1/health","GET /v1/capabilities","POST /v1/compile","POST /v1/compile-bytecode-v2","POST /v1/verify-bytecode-v2","POST /v1/run-bytecode-v2","POST /v1/compile-run"],
                            "request":{"content_type":"application/json","compile_body":{"source":"string"},"bytecode_body":{"bytecode":"string"}},
                            "limits":{"request_bytes":MAX_REQUEST_BYTES,"header_bytes":MAX_HEADER_BYTES,"source_bytes":MAX_SOURCE_BYTES,"output_bytes":MAX_OUTPUT_BYTES,"active_connections":MAX_ACTIVE_CONNECTIONS},
                            "features":{"compile":true,"execute":true,"bytecode_artifact":true,"bytecode_v2_verify":true,"self_hosted_frontend":true,"rust_free_bootstrap":false}
                        })),
                        _ => respond(stream, "404 Not Found", &json!({"ok":false,"error":"route not found"})),
                    };
                }
                if method != "POST" {
                    return respond(stream, "405 Method Not Allowed", &json!({"ok":false,"error":"method not allowed"}));
                }
                if !matches!(route.as_str(), "/compile-run" | "/v1/compile" | "/v1/compile-bytecode-v2" | "/v1/verify-bytecode-v2" | "/v1/run-bytecode-v2" | "/v1/compile-run") {
                    return respond(stream, "404 Not Found", &json!({"ok":false,"error":"route not found"}));
                }
                let expected_token = std::env::var("ZENVORA_API_TOKEN").unwrap_or_default();
                if !expected_token.is_empty() && !bearer_token_matches(&expected_token, &authorization) {
                    return respond(stream, "401 Unauthorized", &json!({"ok":false,"error":"valid bearer token required"}));
                }
                if !content_type.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json") {
                    return respond(stream, "415 Unsupported Media Type", &json!({"ok":false,"error":"Content-Type must be application/json"}));
                }
                content_length = Some(match length {
                    Some(length) => length,
                    None => return respond(stream, "411 Length Required", &json!({"ok":false,"error":"Content-Length is required"})),
                });
                header_end = Some(pos + 4);
                if content_length.unwrap_or(0) > MAX_REQUEST_BYTES {
                    return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"request exceeds 2 MiB"}));
                }
            }
        }
        if let (Some(start), Some(length)) = (header_end, content_length) {
            if buffer.len() >= start + length {
                let body = &buffer[start..start + length];
                let request: Value = match serde_json::from_slice(body) {
                    Ok(value) => value,
                    Err(_) => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"body must be valid JSON"})),
                };
                if route == "/v1/verify-bytecode-v2" || route == "/v1/run-bytecode-v2" {
                    let bytecode = match request.get("bytecode").and_then(Value::as_str) {
                        Some(bytecode) => bytecode,
                        None => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"JSON field 'bytecode' must be a string"})),
                    };
                    if bytecode.len() > MAX_OUTPUT_BYTES {
                        return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"bytecode exceeds 1 MiB limit"}));
                    }
                    let code = match super::deserialize_bytecode_v2(bytecode) {
                        Ok(code) => code,
                        Err(error) => return respond(stream, "422 Unprocessable Entity", &json!({
                            "ok":false,"error":error,"phase":"bytecode_verification"
                        })),
                    };
                    if route == "/v1/verify-bytecode-v2" {
                        return respond(stream, "200 OK", &json!({
                            "ok":true,"verified":true,"format":"ZENVORA_BYTECODE 2",
                            "instruction_count":code.len()
                        }));
                    }
                    let mut lines = Vec::new();
                    let mut output_bytes = 0usize;
                    let mut output_truncated = false;
                    return match super::run_with_output(&code, &mut |line| {
                        let line_bytes = line.len().saturating_add(1);
                        if output_bytes.saturating_add(line_bytes) <= MAX_OUTPUT_BYTES {
                            output_bytes += line_bytes;
                            lines.push(line);
                        } else {
                            output_truncated = true;
                        }
                    }) {
                        Ok(()) => respond(stream, "200 OK", &json!({
                            "ok":true,"verified":true,"output":lines.join("\n"),"lines":lines,
                            "output_truncated":output_truncated,"instruction_count":code.len()
                        })),
                        Err(error) => respond(stream, "422 Unprocessable Entity", &json!({
                            "ok":false,"error":error,"phase":"runtime"
                        })),
                    };
                }
                let source = match request.get("source").and_then(Value::as_str) {
                    Some(source) => source,
                    None => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"JSON field 'source' must be a string"})),
                };
                if source.len() > MAX_SOURCE_BYTES {
                    return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"source exceeds 48 KiB"}));
                }
                if route == "/v1/compile-bytecode-v2" {
                    let bytecode = match super::compile_self_hosted_source(source) {
                        Ok(bytecode) => bytecode,
                        Err(error) => return respond(stream, "422 Unprocessable Entity", &json!({
                            "ok":false,"error":error,"phase":"self_hosted_compile",
                            "compiler":"zenvora-self-hosted-frontend"
                        })),
                    };
                    if bytecode.len() > MAX_OUTPUT_BYTES {
                        return respond(stream, "413 Payload Too Large", &json!({
                            "ok":false,"error":"compiled bytecode exceeds 1 MiB response limit"
                        }));
                    }
                    return respond(stream, "200 OK", &json!({
                        "ok":true,"format":"ZENVORA_BYTECODE 2","bytecode":bytecode,
                        "bytecode_bytes":bytecode.len(),
                        "compiler":"zenvora-self-hosted-frontend",
                        "host":"rust-bootstrap"
                    }));
                }
                let code = match super::compile(source) {
                    Ok(code) => code,
                    Err(error) => return respond(stream, "422 Unprocessable Entity", &json!({
                        "ok":false,"error":error,"phase":"compile",
                        "compiler":concat!("zenvora-rust-v", env!("CARGO_PKG_VERSION"))
                    })),
                };
                if route == "/v1/compile" {
                    return respond(stream, "200 OK", &json!({
                        "ok":true,"compiled":true,"instruction_count":code.len(),
                        "compiler":concat!("zenvora-rust-v", env!("CARGO_PKG_VERSION"))
                    }));
                }
                let mut lines = Vec::new();
                let mut output_bytes = 0usize;
                let mut output_truncated = false;
                return match super::run_with_output(&code, &mut |line| {
                    let line_bytes = line.len().saturating_add(1);
                    if output_bytes.saturating_add(line_bytes) <= MAX_OUTPUT_BYTES {
                        output_bytes += line_bytes;
                        lines.push(line);
                    } else {
                        output_truncated = true;
                    }
                }) {
                    Ok(()) => respond(stream, "200 OK", &json!({
                        "ok":true,"output":lines.join("\n"),"lines":lines,
                        "output_truncated":output_truncated,
                        "instruction_count":code.len(),
                        "compiler":concat!("zenvora-rust-v", env!("CARGO_PKG_VERSION"))
                    })),
                    Err(error) => respond(stream, "422 Unprocessable Entity", &json!({
                        "ok":false,"error":error,"phase":"runtime",
                        "compiler":concat!("zenvora-rust-v", env!("CARGO_PKG_VERSION"))
                    })),
                };
            }
        }
        let read = stream.read(&mut chunk).map_err(|e| format!("read failed: {e}"))?;
        if read == 0 {
            return Err("client closed before request completed".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn respond(stream: &mut TcpStream, status: &str, value: &Value) -> Result<(), String> {
    let body = value.to_string();
    let configured_origin = std::env::var("ZENVORA_API_ALLOWED_ORIGIN")
        .unwrap_or_else(|_| "https://vikash7644.github.io".into());
    let allowed_origin = if configured_origin.contains('\r') || configured_origin.contains('\n') {
        "https://vikash7644.github.io"
    } else {
        configured_origin.as_str()
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: {}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nVary: Origin\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len(),
        allowed_origin
    );
    stream.write_all(header.as_bytes()).and_then(|_| stream.write_all(body.as_bytes()))
        .map_err(|e| format!("write failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_for(raw_request: &[u8]) -> String {
        use std::net::Shutdown;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let worker = thread::spawn(move || handle(&mut server));
        client.write_all(raw_request).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        worker.join().unwrap().unwrap();
        response
    }

    #[test]
    fn compile_run_returns_package_version_and_output() {
        let body = r#"{"source":"show 1 + 1"}"#;
        let request = format!(
            "POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = response_for(request.as_bytes());
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let response_body = response.split("\r\n\r\n").nth(1).unwrap();
        let value: Value = serde_json::from_str(response_body).unwrap();
        assert_eq!(
            value["compiler"],
            format!("zenvora-rust-v{}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(value["output"], "2");
    }

    #[test]
    fn v1_compile_endpoint_returns_instruction_count() {
        let body = r#"{"source":"show 40 + 2"}"#;
        let request = format!(
            "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        let response = response_for(request.as_bytes());
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let value: Value = serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(value["compiled"], true);
        assert!(value["instruction_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn v1_self_hosted_endpoint_emits_executable_bytecode_v2() {
        let body = r#"{"source":"show 40 + 2\n"}"#;
        let request = format!(
            "POST /v1/compile-bytecode-v2 HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        let response = response_for(request.as_bytes());
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let value: Value = serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(value["format"], "ZENVORA_BYTECODE 2");
        let bytecode = value["bytecode"].as_str().unwrap();
        assert!(bytecode.starts_with("ZENVORA_BYTECODE 2\n"));
        assert_eq!(value["compiler"], "zenvora-self-hosted-frontend");
    }

    #[test]
    fn v1_bytecode_verify_and_run_endpoints_accept_self_hosted_artifacts() {
        let bytecode = super::compile_self_hosted_source("show 40 + 2\n").unwrap();
        let body = serde_json::json!({"bytecode":bytecode}).to_string();
        let request = format!(
            "POST /v1/run-bytecode-v2 HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        let response = response_for(request.as_bytes());
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let value: Value = serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(value["verified"], true);
        assert_eq!(value["output"], "42");
    }

    #[test]
    fn v1_capabilities_describe_real_api_routes() {
        let response = response_for(b"GET /v1/capabilities HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let value: Value = serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(value["api_version"], "v1");
        assert_eq!(value["features"]["bytecode_artifact"], true);
        assert_eq!(value["features"]["self_hosted_frontend"], true);
        assert_eq!(value["features"]["rust_free_bootstrap"], false);
    }

    #[test]
    fn public_bind_requires_a_long_token_and_loopback_does_not() {
        assert!(validate_bind_security("127.0.0.1", "").is_ok());
        assert!(validate_bind_security("0.0.0.0", "").is_err());
        assert!(validate_bind_security("0.0.0.0", "short").is_err());
        assert!(validate_bind_security("0.0.0.0", "0123456789abcdef0123456789abcdef").is_ok());
    }

    #[test]
    fn bearer_token_validation_requires_exact_header_and_constant_length() {
        let expected = "0123456789abcdef0123456789abcdef";
        assert!(bearer_token_matches(expected, "Bearer 0123456789abcdef0123456789abcdef"));
        assert!(!bearer_token_matches(expected, "0123456789abcdef0123456789abcdef"));
        assert!(!bearer_token_matches(expected, "Bearer wrong"));
        assert!(!bearer_token_matches(expected, "Basic 0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn cors_preflight_allows_authorization_header() {
        let response = response_for(b"OPTIONS /v1/compile-run HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Access-Control-Allow-Headers: Content-Type, Authorization"));
    }

    #[test]
    fn non_json_post_receives_http_415_response() {
        let response = response_for(
            b"POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain\r\nContent-Length: 1\r\n\r\nx",
        );
        assert!(response.starts_with("HTTP/1.1 415 Unsupported Media Type"));
    }

    #[test]
    fn unsupported_method_receives_http_405_response() {
        let response = response_for(b"PUT /v1/compile HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed"));
    }

    #[test]
    fn duplicate_content_length_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\ncontent-length: 2\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("duplicate Content-Length"));
    }

    #[test]
    fn duplicate_content_type_receives_http_400_response() {
        let response = response_for(
            b"POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\ncontent-type: text/plain\r\nContent-Length: 2\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("duplicate Content-Type"));
    }

    #[test]
    fn duplicate_authorization_receives_http_400_response() {
        let response = response_for(
            b"POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer first\r\nauthorization: Bearer second\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("duplicate Authorization"));
    }

    #[test]
    fn transfer_encoding_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nContent-Length: 2\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("transfer-encoding is unsupported"));
    }

    #[test]
    fn oversized_content_length_receives_http_413_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 99999999\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 413 Payload Too Large"));
    }

    #[test]
    fn oversized_incomplete_headers_receive_http_431_response() {
        let mut request = b"POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nX-Fill: ".to_vec();
        request.extend(std::iter::repeat(b'a').take(MAX_HEADER_BYTES));
        let response = response_for(&request);
        assert!(response.starts_with("HTTP/1.1 431 Request Header Fields Too Large"));
        assert!(response.contains("HTTP headers exceed 16 KiB"));
    }

    #[test]
    fn malformed_json_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 1\r\n\r\nx",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("body must be valid JSON"));
    }

    #[test]
    fn compiler_api_source_limit_is_reasonable() {
        assert!(MAX_SOURCE_BYTES < MAX_REQUEST_BYTES);
        assert!(MAX_SOURCE_BYTES >= 1024);
    }

    #[test]
    fn malformed_content_length_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: nope\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("invalid Content-Length"));
    }

    #[test]
    fn invalid_utf8_headers_receive_http_400_response() {
        let request = b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nX-Bad: "
            .iter()
            .copied()
            .chain([0xff])
            .chain(b"\r\nContent-Length: 2\r\n\r\n{}".iter().copied())
            .collect::<Vec<_>>();
        let response = response_for(&request);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("invalid HTTP headers"));
    }

    #[test]
    fn missing_content_length_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 411 Length Required"));
        assert!(response.contains("Content-Length is required"));
    }
}

//! Loopback-only HTTP API for compiling and executing Zenvora Lang source.
//! This is a development service; it intentionally binds to 127.0.0.1 by default.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_SOURCE_BYTES: usize = 48 * 1024;

pub fn serve(port: u16) -> Result<(), String> {
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address)
        .map_err(|e| format!("cannot bind compiler API at {address}: {e}"))?;
    println!("Zenvora compiler API listening at http://{address}");
    println!("Development-only: loopback binding; press Ctrl+C to stop.");
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                if let Err(error) = handle(&mut stream) {
                    eprintln!("compiler API request error: {error}");
                }
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
    loop {
        if buffer.len() > MAX_REQUEST_BYTES {
            return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"request exceeds 64 KiB"}));
        }
        if header_end.is_none() {
            if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = match std::str::from_utf8(&buffer[..pos]) {
                    Ok(headers) => headers,
                    Err(_) => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"invalid HTTP headers"})),
                };
                let mut lines = headers.split("\r\n");
                let request_line = lines.next().unwrap_or("");
                if request_line == "OPTIONS /compile-run HTTP/1.1" || request_line == "OPTIONS /compile-run HTTP/1.0" {
                    return respond(stream, "204 No Content", &json!({"ok":true}));
                }
                if request_line != "GET /health HTTP/1.1" && request_line != "GET /health HTTP/1.0"
                    && request_line != "POST /compile-run HTTP/1.1" && request_line != "POST /compile-run HTTP/1.0" {
                    return respond(stream, "404 Not Found", &json!({"ok":false,"error":"use GET /health or POST /compile-run"}));
                }
                if request_line.starts_with("GET ") {
                    return respond(stream, "200 OK", &json!({"ok":true,"service":"zenvora-compiler-api","compiler":"Rust","mode":"development"}));
                }
                let mut length = None;
                for line in lines {
                    let Some((name, value)) = line.split_once(':') else { return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"malformed HTTP header"})); };
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"transfer-encoding is unsupported"}));
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        if length.is_some() { return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"duplicate Content-Length"})); }
                        length = Some(match value.trim().parse::<usize>() {
                            Ok(length) => length,
                            Err(_) => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"invalid Content-Length"})),
                        });
                    }
                }
                content_length = Some(match length {
                    Some(length) => length,
                    None => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"Content-Length is required"})),
                });
                header_end = Some(pos + 4);
                if content_length.unwrap_or(0) > MAX_REQUEST_BYTES {
                    return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"request exceeds 64 KiB"}));
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
                let source = match request.get("source").and_then(Value::as_str) {
                    Some(source) => source,
                    None => return respond(stream, "400 Bad Request", &json!({"ok":false,"error":"JSON field 'source' must be a string"})),
                };
                if source.len() > MAX_SOURCE_BYTES {
                    return respond(stream, "413 Payload Too Large", &json!({"ok":false,"error":"source exceeds 48 KiB"}));
                }
                let result = match super::compile_for_vm(source) {
                    Ok(code) => {
                        let mut lines = Vec::new();
                        match super::run_with_output(&code, &mut |line| lines.push(line)) {
                            Ok(()) => json!({"ok":true,"output":lines.join("\n"),"lines":lines,"compiler":"zenvora-rust-v0.8"}),
                            Err(error) => json!({"ok":false,"error":error,"phase":"runtime","compiler":"zenvora-rust-v0.8"}),
                        }
                    }
                    Err(error) => json!({"ok":false,"error":error,"phase":"compile","compiler":"zenvora-rust-v0.8"}),
                };
                return respond(stream, "200 OK", &result);
            }
        }
        let read = stream.read(&mut chunk).map_err(|e| format!("read failed: {e}"))?;
        if read == 0 { return Err("client closed before request completed".into()); }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn respond(stream: &mut TcpStream, status: &str, value: &Value) -> Result<(), String> {
    let body = value.to_string();
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: https://vikash7644.github.io\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nVary: Origin\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
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
    fn duplicate_content_length_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\ncontent-length: 2\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("duplicate Content-Length"));
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
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 413 Payload Too Large"));
    }

    #[test]
    fn malformed_json_receives_http_400_response() {
        let response = response_for(
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\nx",
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
            b"POST /compile-run HTTP/1.1\r\nHost: localhost\r\nContent-Length: nope\r\n\r\n",
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
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("Content-Length is required"));
    }
}

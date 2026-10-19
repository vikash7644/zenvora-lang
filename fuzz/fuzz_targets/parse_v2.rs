#![no_main]

#[path = "../../src/bytecode_v2.rs"]
mod bytecode_v2;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Lossy UTF-8 conversion intentionally includes arbitrary and invalid byte sequences.
    // Parsing is bounded by MAX_V2_FILE_BYTES before any large decode/allocation.
    let input = String::from_utf8_lossy(data);
    if let Ok(parsed) = bytecode_v2::parse_v2_file(&input) {
        let _ = bytecode_v2::verify_v2_file(&parsed);
        let _ = bytecode_v2::serialize_v2_file(&parsed);
    }
});

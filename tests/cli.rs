use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_BYTECODE_TEST_ID: AtomicUsize = AtomicUsize::new(0);

#[test]
fn runs_arithmetic_program() {
    let path = std::env::temp_dir().join("zenvora_lang_test.zv");
    fs::write(&path, "define a := 10\ndefine b := 20\nshow a + b * 2\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zv")).arg(&path).output().unwrap();
    let _ = fs::remove_file(path);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "50");
}

#[test]
fn reports_division_by_zero() {
    let path = std::env::temp_dir().join("zenvora_lang_div_zero.zv");
    fs::write(&path, "show 5 / 0\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zv")).arg(&path).output().unwrap();
    let _ = fs::remove_file(path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("division by zero"));
}


#[test]
fn bytecode_cli_round_trip_preserves_output() {
    let id = NEXT_BYTECODE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("zenvora_bytecode_{}_{}", std::process::id(), id));
    let source_path = base.with_extension("zv");
    let bytecode_path = base.with_extension("zvb");
    fs::write(
        &source_path,
        "show twice(21)\nfn twice(n) {\nreturn n * 2\n}\nshow true\n",
    )
    .unwrap();

    let compiled = Command::new(env!("CARGO_BIN_EXE_zv"))
        .arg("--emit-bytecode")
        .arg(&source_path)
        .arg(&bytecode_path)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        fs::read_to_string(&bytecode_path)
            .unwrap()
            .starts_with("ZENVORA_BYTECODE 1\n")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_zv"))
        .arg("--run-bytecode")
        .arg(&bytecode_path)
        .output()
        .unwrap();
    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(bytecode_path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\ntrue\n");
}

#[test]
fn bytecode_cli_rejects_malformed_files() {
    let id = NEXT_BYTECODE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("zenvora_bad_bytecode_{}_{}.zvb", std::process::id(), id));
    fs::write(&path, "not a bytecode file\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zv"))
        .arg("--run-bytecode")
        .arg(&path)
        .output()
        .unwrap();
    let _ = fs::remove_file(path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid bytecode header"));
}


#[test]
fn version_flag_reports_current_language_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_zv"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "Zenvora Lang v0.6.0");
}

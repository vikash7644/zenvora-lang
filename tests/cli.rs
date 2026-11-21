use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_BYTECODE_TEST_ID: AtomicUsize = AtomicUsize::new(0);

#[test]
fn runs_arithmetic_program() {
    let path = std::env::temp_dir().join("qorvynt_lang_test.qv");
    fs::write(&path, "define a := 10\ndefine b := 20\nshow a + b * 2\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qorvynt")).arg(&path).output().unwrap();
    let _ = fs::remove_file(path);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "50");
}

#[test]
fn reports_division_by_zero() {
    let path = std::env::temp_dir().join("qorvynt_lang_div_zero.qv");
    fs::write(&path, "show 5 / 0\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qorvynt")).arg(&path).output().unwrap();
    let _ = fs::remove_file(path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("division by zero"));
}


#[test]
fn bytecode_cli_round_trip_preserves_output() {
    let id = NEXT_BYTECODE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("qorvynt_bytecode_{}_{}", std::process::id(), id));
    let source_path = base.with_extension("zv");
    let bytecode_path = base.with_extension("zvb");
    fs::write(
        &source_path,
        "show twice(21)\nfn twice(n) {\nreturn n * 2\n}\nshow true\n",
    )
    .unwrap();

    let compiled = Command::new(env!("CARGO_BIN_EXE_qorvynt"))
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
            .starts_with("QORVYNT_BYTECODE 1\n")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_qorvynt"))
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
    let path = std::env::temp_dir().join(format!("qorvynt_bad_bytecode_{}_{}.zvb", std::process::id(), id));
    fs::write(&path, "not a bytecode file\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qorvynt"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_qorvynt"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "QORVYNT v0.8.0");
}

#[test]
fn reads_legacy_zenvora_v1_bytecode_during_identity_migration() {
    let id = NEXT_BYTECODE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("qorvynt_legacy_bytecode_{}_{}.zvb", std::process::id(), id));
    fs::write(&path, "ZENVORA_BYTECODE 1\nPUSH 42\nPRINT\nJUMP 3\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_qorvynt"))
        .arg("--run-bytecode")
        .arg(&path)
        .output()
        .unwrap();
    let _ = fs::remove_file(path);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
}

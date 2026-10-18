use std::fs;
use std::process::Command;

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

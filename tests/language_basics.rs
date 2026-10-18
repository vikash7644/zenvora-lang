use std::fs;
use std::process::Command;

fn run_program(source: &str) -> std::process::Output {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("zenvora_test_{}_{}.zv", std::process::id(), source.len()));
    fs::write(&path, source).expect("write test program");
    let output = Command::new(env!("CARGO_BIN_EXE_zv"))
        .arg(&path)
        .output()
        .expect("run Zenvora");
    let _ = fs::remove_file(path);
    output
}

#[test]
fn supports_precedence_parentheses_and_negative_numbers() {
    let output = run_program(
        "define a := 10 + 20 * 2\nshow a\nshow (10 + 20) * 2\nshow -5 + 8\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "50\n60\n3\n");
}

#[test]
fn supports_assignment_using_previous_value() {
    let output = run_program("define count := 1\ncount := count + 1\nshow count\n");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2\n");
}

#[test]
fn reports_undefined_variables() {
    let output = run_program("show missing\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("undefined variable 'missing'"));
}

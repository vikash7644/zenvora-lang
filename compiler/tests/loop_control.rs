use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn run_program(source: &str) -> Output {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "zenvora_loop_test_{}_{}.zv",
        std::process::id(),
        id
    ));
    fs::write(&path, source).expect("write temporary Zenvora program");
    let output = Command::new(env!("CARGO_BIN_EXE_zenvora"))
        .arg(&path)
        .output()
        .expect("run Zenvora interpreter");
    let _ = fs::remove_file(path);
    output
}

#[test]
fn break_exits_loop_at_the_requested_value() {
    let output = run_program(
        "define count := 1\nrepeat count <= 10 {\n    show count\n    if count == 5 {\n        break\n    }\n    count := count + 1\n}\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n2\n3\n4\n5\n");
}

#[test]
fn continue_skips_remaining_statements_and_rechecks_condition() {
    let output = run_program(
        "define count := 0\nrepeat count < 5 {\n    count := count + 1\n    if count == 3 {\n        continue\n    }\n    show count\n}\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n2\n4\n5\n");
}

#[test]
fn break_inside_nested_if_exits_the_enclosing_loop() {
    let output = run_program(
        "define count := 0\nrepeat count < 8 {\n    count := count + 1\n    if count == 2 {\n        if count >= 2 {\n            break\n        }\n    }\n    show count\n}\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n");
}

#[test]
fn break_outside_loop_is_a_clear_error() {
    let output = run_program("break\n");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("'break' used outside a repeat loop"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn continue_outside_loop_is_a_clear_error() {
    let output = run_program("continue\n");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("'continue' used outside a repeat loop"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn break_in_inner_loop_does_not_exit_outer_loop() {
    let output = run_program(
        "define outer := 0\nrepeat outer < 3 {\n    outer := outer + 1\n    define inner := 0\n    repeat inner < 5 {\n        inner := inner + 1\n        if inner == 2 {\n            break\n        }\n        show outer * 10 + inner\n    }\n}\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "11\n21\n31\n"
    );
}

#[test]
fn break_inside_if_outside_loop_is_rejected() {
    let output = run_program("if true {\n    break\n}\n");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("'break' used outside a repeat loop"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn continue_inside_if_outside_loop_is_rejected() {
    let output = run_program("if true {\n    continue\n}\n");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("'continue' used outside a repeat loop"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn inner_continue_does_not_skip_outer_loop_body() {
    let output = run_program(
        "define outer := 0\nrepeat outer < 2 {\n    outer := outer + 1\n    define inner := 0\n    repeat inner < 3 {\n        inner := inner + 1\n        if inner == 2 {\n            continue\n        }\n        show outer * 10 + inner\n    }\n    show outer\n}\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "11\n13\n1\n21\n23\n2\n"
    );
}

#[test]
fn loop_control_does_not_hide_runtime_errors() {
    let output = run_program(
        "define count := 0\nrepeat count < 3 {\n    count := count + 1\n    if count == 2 {\n        continue\n    }\n    show 10 / (count - 1)\n}\n",
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("division by zero"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

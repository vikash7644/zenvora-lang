use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FILE_ID: AtomicUsize = AtomicUsize::new(0);

fn run_program(source: &str) -> std::process::Output {
    let dir = std::env::temp_dir();
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("zenvora_test_{}_{}.zv", std::process::id(), id));
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

#[test]
fn if_branch_runs_when_comparison_is_true() {
    let output = run_program(
        "define age := 20\nif age >= 18 {\nshow 1\n} otherwise {\nshow 0\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n");
}

#[test]
fn otherwise_branch_runs_when_comparison_is_false() {
    let output = run_program(
        "define age := 16\nif age >= 18 {\nshow 1\n} otherwise {\nshow 0\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0\n");
}

#[test]
fn nested_if_branches_work() {
    let output = run_program(
        "define age := 20\nif age >= 18 {\nif age == 20 {\nshow 20\n} otherwise {\nshow 18\n}\n} otherwise {\nshow 0\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "20\n");
}

#[test]
fn supports_all_comparison_operators_in_conditions() {
    let output = run_program(
        "if 2 == 2 {\nshow 1\n}\nif 2 != 3 {\nshow 2\n}\nif 2 < 3 {\nshow 3\n}\nif 2 <= 2 {\nshow 4\n}\nif 3 > 2 {\nshow 5\n}\nif 3 >= 3 {\nshow 6\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n2\n3\n4\n5\n6\n");
}

#[test]
fn repeat_loop_runs_until_condition_becomes_false() {
    let output = run_program(
        "define count := 1\nrepeat count <= 3 {\nshow count\ncount := count + 1\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n2\n3\n");
}

#[test]
fn break_exits_only_the_nearest_loop() {
    let output = run_program(
        "define outer := 0\nrepeat outer < 3 {\nouter := outer + 1\ndefine inner := 0\nrepeat inner < 5 {\ninner := inner + 1\nif inner == 2 {\nbreak\n}\nshow outer * 10 + inner\n}\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "11\n21\n31\n");
}

#[test]
fn continue_rechecks_condition_and_skips_remaining_body() {
    let output = run_program(
        "define count := 0\nrepeat count < 5 {\ncount := count + 1\nif count == 3 {\ncontinue\n}\nshow count\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n2\n4\n5\n");
}

#[test]
fn break_inside_if_exits_loop_but_not_the_program() {
    let output = run_program(
        "define count := 0\nrepeat count < 10 {\ncount := count + 1\nif count == 2 {\nbreak\n}\n}\nshow count\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2\n");
}

#[test]
fn break_and_continue_outside_loops_are_compile_errors() {
    for statement in ["break", "continue", "if 1 == 1 {\nbreak\n}"] {
        let output = run_program(statement);
        assert!(!output.status.success(), "program unexpectedly succeeded: {statement}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("used outside a repeat loop"),
            "unexpected stderr for {statement}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn repeat_with_false_initial_condition_skips_body() {
    let output = run_program(
        "define count := 5\nrepeat count < 5 {\nshow 99\n}\nshow count\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "5\n");
}

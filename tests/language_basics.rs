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

#[test]
fn continue_inside_nested_if_rechecks_the_loop_condition() {
    let output = run_program(
        "define count := 0\nrepeat count < 4 {\ncount := count + 1\nif count == 2 {\nif count > 1 {\ncontinue\n}\n}\nshow count\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n3\n4\n");
}

#[test]
fn otherwise_inside_loop_keeps_jump_targets_correct() {
    let output = run_program(
        "define count := 0\nrepeat count < 3 {\ncount := count + 1\nif count == 2 {\nshow 20\n} otherwise {\nshow count\n}\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n20\n3\n");
}

#[test]
fn infinite_repeat_is_stopped_by_the_vm_instruction_limit() {
    let output = run_program("repeat 1 == 1 {\n}\n");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("execution exceeded safety limit"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}


#[test]
fn functions_can_be_called_before_their_definition() {
    let output = run_program("call greet()\nfn greet() {\nshow 42\n}\n");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "42\n");
}

#[test]
fn functions_can_be_called_multiple_times_and_call_other_functions() {
    let output = run_program(
        "call first()\ncall first()\nfn first() {\nshow 1\ncall second()\n}\nfn second() {\nshow 2\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n2\n1\n2\n");
}

#[test]
fn function_return_exits_only_the_current_function() {
    let output = run_program(
        "call stop_early()\nshow 9\nfn stop_early() {\nshow 1\nreturn\nshow 2\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n9\n");
}

#[test]
fn function_calls_support_loops_and_conditionals() {
    let output = run_program(
        "call work()\nfn work() {\ndefine count := 0\nrepeat count < 3 {\ncount := count + 1\nif count == 2 {\ncontinue\n}\nshow count\n}\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n3\n");
}

#[test]
fn undefined_function_calls_are_compile_errors() {
    let output = run_program("call missing()\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("undefined function 'missing'"));
}

#[test]
fn return_outside_a_function_call_is_an_error() {
    let output = run_program("return\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("return used outside a function call"));
}

#[test]
fn functions_accept_integer_arguments_and_evaluate_argument_expressions() {
    let output = run_program(
        "call add_and_show(2 + 3, 4 * 2)\nfn add_and_show(left, right) {\nshow left + right\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "13\n");
}

#[test]
fn function_local_variables_do_not_leak_into_the_caller() {
    let output = run_program(
        "define count := 99\ncall change_count()\nshow count\nfn change_count() {\ndefine count := 5\ncount := count + 1\nshow count\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "6\n99\n");
}

#[test]
fn function_argument_count_must_match_parameters() {
    let output = run_program("call show_pair(1)\nfn show_pair(left, right) {\nshow left + right\n}\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expects 2 argument(s), got 1"));
}

#[test]
fn function_parameters_are_required_and_are_local() {
    let output = run_program("call show_value(7)\nfn show_value(value) {\nshow value\n}\n");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "7\n");
}

#[test]
fn functions_can_return_values_into_expressions() {
    let output = run_program(
        "define result := add(2, multiply(3, 4))\nshow result\nfn add(left, right) {\nreturn left + right\n}\nfn multiply(left, right) {\nreturn left * right\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "14\n");
}

#[test]
fn recursive_functions_use_independent_local_frames() {
    let output = run_program(
        "show sum_to(5)\nfn sum_to(n) {\nif n <= 0 {\nreturn 0\n} otherwise {\nreturn n + sum_to(n - 1)\n}\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "15\n");
}

#[test]
fn native_booleans_can_be_stored_printed_and_used_as_conditions() {
    let output = run_program(
        "define alive := true\nshow alive\ndefine dead := false\nshow dead\nif alive {\nshow 7\n} otherwise {\nshow 0\n}\nif dead {\nshow 8\n} otherwise {\nshow 9\n}\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\nfalse\n7\n9\n");
}

#[test]
fn comparisons_produce_native_boolean_values() {
    let output = run_program("show 2 == 2\nshow 3 < 1\nshow true != false\n");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\nfalse\ntrue\n");
}

#[test]
fn arithmetic_rejects_boolean_values_with_a_type_error() {
    let output = run_program("show true + 1\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("type error: '+' expects integers"));
}

#[test]
fn ordered_comparisons_reject_boolean_values() {
    let output = run_program("show true < false\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ordered comparisons expect integers"));
}

#[test]
fn boolean_not_and_or_operators_work_with_expected_precedence() {
    let output = run_program(
        "show not false\nshow true and false\nshow true or false and false\nshow (true or false) and false\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\nfalse\ntrue\nfalse\n");
}

#[test]
fn logical_operators_reject_integer_operands() {
    let output = run_program("show true and 1\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("type error: 'and' expects booleans"));
}

#[test]
fn unterminated_function_reports_declaration_line() {
    let output = run_program("show 1\nfn unfinished(value) {\nshow value\n");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("line 2: missing closing brace for function 'unfinished'"), "unexpected stderr: {stderr}");
}

#[test]
fn contract_transfer_checks_amount_and_balance_without_partial_updates() {
    let source = r#"
contract DemoToken {
    state alice := 100
    state bob := 0

    action transfer(amount) {
        if amount > 0 {
            if alice >= amount {
                alice := alice - amount
                bob := bob + amount
                show "TRANSFER_ACCEPTED"
            } otherwise {
                show "TRANSFER_REJECTED_INSUFFICIENT_BALANCE"
            }
        } otherwise {
            show "TRANSFER_REJECTED_INVALID_AMOUNT"
        }
    }

    view alice_balance() {
        return alice
    }

    view bob_balance() {
        return bob
    }
}

call transfer(30)
call transfer(1000)
call transfer(0)
call transfer(-1)
show alice_balance()
show bob_balance()
"#;
    let output = run_program(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "TRANSFER_ACCEPTED\nTRANSFER_REJECTED_INSUFFICIENT_BALANCE\nTRANSFER_REJECTED_INVALID_AMOUNT\nTRANSFER_REJECTED_INVALID_AMOUNT\n70\n30\n"
    );
}


#[test]
fn logical_operators_short_circuit_without_evaluating_the_right_operand() {
    let output = run_program(
        "if false and (1 / 0 == 0) {\nshow 0\n} otherwise {\nshow 1\n}\nif true or (1 / 0 == 0) {\nshow 2\n}\n",
    );
    assert!(
        output.status.success(),
        "short-circuit evaluation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
}

#[test]
fn arrays_maps_and_collection_helpers_preserve_expected_values() {
    let output = run_program(
        "define values := array(10, 20, 30)\nshow values[1]\nshow len(values)\nshow sum(values)\ndefine profile := map(\"name\", \"Zenvora\", \"level\", 6)\nshow get(profile, \"name\")\ndefine changed := put(profile, \"level\", 7)\nshow get(changed, \"level\")\nshow get(profile, \"level\")\n",
    );
    assert!(
        output.status.success(),
        "collection program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "20\n3\n60\nZenvora\n7\n6\n"
    );
}

#[test]
fn string_helpers_handle_unicode_and_integer_conversion() {
    let output = run_program(
        "show char_count(\"hé🙂\")\nshow to_int(\"  -42 \")\nshow upper(\"Zenvora\")\nshow trim(\"  Lang  \")\n",
    );
    assert!(
        output.status.success(),
        "string helper program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n-42\nZENVORA\nLang\n");
}

#[test]
fn checked_integer_overflow_is_reported_as_a_runtime_error() {
    let output = run_program("show 9223372036854775807 + 1\n");
    assert!(!output.status.success(), "overflowing arithmetic unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr).to_lowercase().contains("overflow"),
        "expected an overflow diagnostic, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

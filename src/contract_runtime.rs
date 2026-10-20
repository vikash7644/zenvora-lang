//! Experimental source-level contract executor backed by the Zenvora compiler and VM.
//! Version 1 deliberately supports integer state only; unsupported state types fail closed.
use super::contract_gas::GasMeter;
use super::contract_state::{AccessMode, ContractState};
use super::{compile, run_with_gas};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReceipt {
    pub output: Vec<String>,
    pub gas_used: u64,
    pub state_root: [u8; 32],
}

pub fn execute(
    source: &str,
    contract_id: &str,
    invocation: &str,
    mode: AccessMode,
    state: &mut ContractState,
    gas_limit: u64,
) -> Result<ExecutionReceipt, String> {
    let overlay = state.begin(contract_id, mode)?;
    let (contract_name, state_vars, methods, rewritten) = prepare_source(source, state, &overlay)?;
    let (invocation_kind, method_name) = parse_invocation(invocation)?;
    let expected_kind = match mode { AccessMode::Action => "action", AccessMode::View => "view" };
    if invocation_kind != expected_kind {
        return Err(format!("{expected_kind} execution requires an invocation beginning with '{expected_kind}'"));
    }
    if methods.get(method_name) != Some(&expected_kind) {
        return Err(format!("method '{method_name}' is not a {expected_kind} in contract '{contract_name}'"));
    }
    let mut program = rewritten;
    program.push_str("\n");
    program.push_str(invocation);
    program.push('\n');
    for (name, _) in &state_vars {
        program.push_str(&format!("show {name}\n"));
    }
    let code = compile(&program)?;
    let mut output = Vec::new();
    let mut meter = GasMeter::new(gas_limit);
    run_with_gas(&code, &mut |line| output.push(line), &mut meter)?;
    if output.len() < state_vars.len() {
        return Err("VM did not return all contract state values".into());
    }
    let state_output = output.split_off(output.len() - state_vars.len());
    let mut pending = state.begin(contract_id, mode)?;
    for ((name, _), value) in state_vars.iter().zip(state_output) {
        let parsed: i64 = value.parse().map_err(|_| format!(
            "contract state '{name}' must evaluate to an integer in runtime v1"
        ))?;
        if mode == AccessMode::Action {
            pending.set(name, parsed.to_string().into_bytes())?;
        }
    }
    if mode == AccessMode::Action {
        state.commit(pending)?;
    }
    Ok(ExecutionReceipt { output, gas_used: meter.used(), state_root: state.state_root() })
}

fn parse_invocation(invocation: &str) -> Result<(&str, &str), String> {
    let trimmed = invocation.trim();
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("contract invocation must be a single line".into());
    }
    let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed).trim();
    if trimmed.ends_with(';') {
        return Err("contract invocation accepts at most one trailing semicolon".into());
    }
    let (kind, rest) = if let Some(rest) = trimmed.strip_prefix("call ") {
        ("action", rest)
    } else if let Some(rest) = trimmed.strip_prefix("show ") {
        ("view", rest)
    } else {
        return Err("invocation must be 'call action_name(args)' or 'show view_name(args)'".into());
    };
    let rest = rest.trim();
    let (name, _) = rest.split_once('(')
        .ok_or_else(|| "invocation must include method parentheses".to_string())?;
    let name = name.trim();
    if !rest.ends_with(')') {
        return Err("contract invocation must end with a closing parenthesis".into());
    }
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("invalid contract method name".into());
    }
    Ok((kind, name))
}

fn prepare_source(
    source: &str,
    state: &ContractState,
    overlay: &super::contract_state::StateOverlay,
) -> Result<(String, Vec<(String, String)>, std::collections::BTreeMap<String, &'static str>, String), String> {
    let mut contract_name = None;
    let mut in_contract = false;
    let mut contract_depth = 0i32;
    let mut state_vars = Vec::new();
    let mut methods = std::collections::BTreeMap::new();
    let mut rewritten = String::new();

    for raw in source.lines() {
        let line = raw.trim();
        if !in_contract {
            if let Some(name) = line.strip_prefix("contract ").and_then(|v| v.strip_suffix('{')) {
                if contract_name.is_some() {
                    return Err("only one contract block per source file is currently supported".into());
                }
                contract_name = Some(name.trim().to_string());
                in_contract = true;
                let (opens, closes) = super::count_block_braces(raw);
                contract_depth += opens - closes;
            }
            rewritten.push_str(raw);
            rewritten.push('\n');
            continue;
        }

        if let Some(rest) = line.strip_prefix("state ") {
            let (name, initial) = rest.split_once(":=")
                .ok_or_else(|| format!("invalid state declaration: {line}"))?;
            let name = name.trim();
            let initial = initial.trim().trim_end_matches(';').trim();
            let stored = overlay.get(state, name)?.map(|bytes| {
                std::str::from_utf8(bytes).map(str::to_owned)
                    .map_err(|_| format!("stored contract state '{name}' is not UTF-8"))
            }).transpose()?;
            let value = stored.unwrap_or_else(|| initial.to_string());
            let _: i64 = value.parse().map_err(|_| format!(
                "contract state '{name}' initializer/storage must be an integer in runtime v1"
            ))?;
            state_vars.push((name.to_string(), value.clone()));
            rewritten.push_str(&format!("state {name} := {value}\n"));
            let (opens, closes) = super::count_block_braces(raw);
            contract_depth += opens - closes;
            if contract_depth <= 0 { in_contract = false; }
            continue;
        }

        for (kind, prefix) in [("action", "action "), ("view", "view ")] {
            if let Some(header) = line.strip_prefix(prefix).and_then(|v| v.strip_suffix('{')) {
                let name = header.split_once('(').map(|(name, _)| name.trim())
                    .ok_or_else(|| format!("invalid {kind} header: {line}"))?;
                if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    return Err(format!("invalid {kind} name in contract header: {line}"));
                }
                if methods.insert(name.to_string(), kind).is_some() {
                    return Err(format!("duplicate contract method '{name}'"));
                }
                break;
            }
        }

        rewritten.push_str(raw);
        rewritten.push('\n');
        let (opens, closes) = super::count_block_braces(raw);
        contract_depth += opens - closes;
        if contract_depth <= 0 { in_contract = false; }
    }

    if in_contract || contract_depth != 0 {
        return Err("contract is missing its closing brace".into());
    }
    let contract_name = contract_name.ok_or_else(|| "source contains no contract declaration".to_string())?;
    if state_vars.is_empty() { return Err("contract must declare at least one state variable in runtime v1".into()); }
    Ok((contract_name, state_vars, methods, rewritten))
}

#[cfg(test)]
mod tests {
    use super::execute;
    use super::super::contract_state::{AccessMode, ContractState};

    const COUNTER: &str = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview current() {\nreturn count\n}\n}\n";

    #[test]
    fn action_state_persists_across_vm_invocations_and_view_reads_it() {
        let mut state = ContractState::new();
        let first = execute(COUNTER, "counter", "call increment(3)", AccessMode::Action, &mut state, 100_000).unwrap();
        let second = execute(COUNTER, "counter", "call increment(4)", AccessMode::Action, &mut state, 100_000).unwrap();
        assert!(first.gas_used > 0 && second.gas_used > 0);
        let view = execute(COUNTER, "counter", "show current()", AccessMode::View, &mut state, 100_000).unwrap();
        assert_eq!(view.output, vec!["7"]);
        assert_eq!(state.get("counter", "count").unwrap(), Some(&b"7"[..]));
    }

    #[test]
    fn failed_action_and_out_of_gas_do_not_commit_state() {
        let mut state = ContractState::new();
        execute(COUNTER, "counter", "call increment(3)", AccessMode::Action, &mut state, 100_000).unwrap();
        let before = state.state_root();
        assert!(execute(COUNTER, "counter", "call increment(4)", AccessMode::Action, &mut state, 21_001).is_err());
        assert_eq!(state.state_root(), before);
    }

    #[test]
    fn action_with_nested_if_otherwise_persists_state() {
        let source = "contract Counter {\nstate count := 0\naction add(amount) {\nif amount > 0 {\ncount := count + amount\n} otherwise {\ncount := count - amount\n}\n}\nview current() {\nreturn count\n}\n}\n";
        let mut state = ContractState::new();
        execute(source, "counter", "call add(3)", AccessMode::Action, &mut state, 100_000).unwrap();
        execute(source, "counter", "call add(-2)", AccessMode::Action, &mut state, 100_000).unwrap();
        let view = execute(source, "counter", "show current()", AccessMode::View, &mut state, 100_000).unwrap();
        assert_eq!(view.output, vec!["5"]);
        assert_eq!(state.get("counter", "count").unwrap(), Some(&b"5"[..]));
    }

    #[test]
    fn invocation_rejects_multiline_statement_injection_without_state_changes() {
        let mut state = ContractState::new();
        let error = execute(
            COUNTER,
            "counter",
            "call increment(3)\ncall increment(5)",
            AccessMode::Action,
            &mut state,
            100_000,
        ).unwrap_err();
        assert!(error.contains("single line"), "{error}");
        assert_eq!(state.get("counter", "count").unwrap(), None);
    }

    #[test]
    fn view_execution_cannot_commit_or_invoke_an_action() {
        let mut state = ContractState::new();
        let before = state.state_root();
        assert!(execute(COUNTER, "counter", "call increment(5)", AccessMode::View, &mut state, 100_000).is_err());
        execute(COUNTER, "counter", "show current()", AccessMode::View, &mut state, 100_000).unwrap();
        assert_eq!(state.state_root(), before);
    }
}

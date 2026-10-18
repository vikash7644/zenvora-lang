use std::collections::HashMap;
use std::env;
use std::fs;

#[derive(Clone, Debug, PartialEq)]
enum Value { Number(f64), Bool(bool), Text(String) }

impl Value {
    fn truthy(&self) -> bool {
        match self { Value::Bool(v) => *v, Value::Number(n) => *n != 0.0, Value::Text(s) => !s.is_empty() }
    }
    fn display(&self) -> String {
        match self {
            Value::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Text(s) => s.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token { Number(f64), Text(String), Ident(String), Op(String), LParen, RParen }

fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() { i += 1; continue; }
        if c == '"' || c == '\'' {
            let quote = c; i += 1; let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    s.push(match chars[i] { 'n' => '\n', 't' => '\t', other => other });
                } else { s.push(chars[i]); }
                i += 1;
            }
            if i == chars.len() { return Err("unterminated string".into()); }
            i += 1; out.push(Token::Text(s)); continue;
        }
        if c.is_ascii_digit() {
            let start = i; i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') { i += 1; }
            let raw: String = chars[start..i].iter().collect();
            out.push(Token::Number(raw.parse().map_err(|_| format!("invalid number '{raw}'"))?)); continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i; i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            out.push(Token::Ident(chars[start..i].iter().collect())); continue;
        }
        if c == '(' { out.push(Token::LParen); i += 1; continue; }
        if c == ')' { out.push(Token::RParen); i += 1; continue; }
        if "+-*/%=!<>".contains(c) {
            let mut op = c.to_string(); i += 1;
            if i < chars.len() && (chars[i] == '=' && "=!<>".contains(c) || chars[i] == c && (c == '&' || c == '|')) {
                op.push(chars[i]); i += 1;
            }
            out.push(Token::Op(op)); continue;
        }
        if c == '&' || c == '|' { return Err(format!("unexpected operator '{c}'")); }
        return Err(format!("unexpected character '{c}'"));
    }
    Ok(out)
}

struct ExprParser<'v> { tokens: Vec<Token>, pos: usize, vars: &'v HashMap<String, Value> }
impl<'v> ExprParser<'v> {
    fn parse(src: &str, vars: &'v HashMap<String, Value>) -> Result<Value, String> {
        let tokens = tokenize(src)?;
        let mut p = Self { tokens, pos: 0, vars };
        let value = p.expr(0)?;
        if p.pos != p.tokens.len() { return Err(format!("unexpected token {:?}", p.tokens[p.pos])); }
        Ok(value)
    }
    fn expr(&mut self, min_prec: u8) -> Result<Value, String> {
        let mut left = self.atom()?;
        loop {
            let op = match self.tokens.get(self.pos) { Some(Token::Op(op)) => op.clone(), _ => break };
            let prec = precedence(&op);
            if prec < min_prec || prec == 0 { break; }
            self.pos += 1;
            let right = self.expr(prec + 1)?;
            left = apply(&op, left, right)?;
        }
        Ok(left)
    }
    fn atom(&mut self) -> Result<Value, String> {
        let token = self.tokens.get(self.pos).ok_or("expected a value")?.clone();
        self.pos += 1;
        match token {
            Token::Number(n) => Ok(Value::Number(n)),
            Token::Text(s) => Ok(Value::Text(s)),
            Token::Ident(s) if s == "true" => Ok(Value::Bool(true)),
            Token::Ident(s) if s == "false" => Ok(Value::Bool(false)),
            Token::Ident(s) => self.vars.get(&s).cloned().ok_or_else(|| format!("undefined variable '{s}'")),
            Token::Op(op) if op == "-" => match self.atom()? { Value::Number(n) => Ok(Value::Number(-n)), _ => Err("unary '-' requires a number".into()) },
            Token::Op(op) if op == "!" => Ok(Value::Bool(!self.atom()?.truthy())),
            Token::LParen => {
                let v = self.expr(0)?;
                if self.tokens.get(self.pos) != Some(&Token::RParen) { return Err("expected ')'".into()); }
                self.pos += 1; Ok(v)
            }
            other => Err(format!("expected a value, got {other:?}")),
        }
    }
}
fn precedence(op: &str) -> u8 {
    match op { "==" | "!=" | "<" | "<=" | ">" | ">=" => 1, "+" | "-" => 2, "*" | "/" | "%" => 3, _ => 0 }
}
fn apply(op: &str, a: Value, b: Value) -> Result<Value, String> {
    match (op, a, b) {
        ("+", Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
        ("-", Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
        ("*", Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
        ("/", Value::Number(_), Value::Number(b)) if b == 0.0 => Err("division by zero".into()),
        ("/", Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
        ("%", Value::Number(_), Value::Number(b)) if b == 0.0 => Err("remainder by zero".into()),
        ("%", Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
        ("+", Value::Text(a), Value::Text(b)) => Ok(Value::Text(a + &b)),
        ("==", a, b) => Ok(Value::Bool(a == b)),
        ("!=", a, b) => Ok(Value::Bool(a != b)),
        ("<", Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a < b)),
        ("<=", Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a <= b)),
        (">", Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a > b)),
        (">=", Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a >= b)),
        (op, _, _) => Err(format!("operator '{op}' received incompatible values")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Flow { Normal, Break, Continue }

fn clean(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with('#') { String::new() }
    else { trimmed.trim_end_matches(';').trim().to_string() }
}
fn collect_block(lines: &[String], start: usize) -> Result<(Vec<String>, usize), String> {
    let mut depth = 1_i32;
    let mut body = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = clean(&lines[i]);
        // A combined "} otherwise {" line closes one branch and opens
        // the next at the same nesting level, so its net depth change is zero.
        if line == "}" {
            depth -= 1;
            if depth == 0 { return Ok((body, i + 1)); }
            body.push(lines[i].clone());
        } else if line == "} otherwise {" || (line.starts_with("} otherwise if ") && line.ends_with('{')) {
            body.push(lines[i].clone());
        } else {
            if line.ends_with('{') { depth += 1; }
            body.push(lines[i].clone());
        }
        i += 1;
    }
    Err("block opened with '{' was never closed".into())
}
fn run_lines(lines: &[String], vars: &mut HashMap<String, Value>, depth: usize) -> Result<Flow, String> {
    if depth > 64 { return Err("maximum block nesting (64) exceeded".into()); }
    let mut i = 0;
    while i < lines.len() {
        let line = clean(&lines[i]);
        if line.is_empty() { i += 1; continue; }
        if line == "}" { return Err(format!("unexpected '}}' on line {}", i + 1)); }
        if line == "break" { return Ok(Flow::Break); }
        if line == "continue" { return Ok(Flow::Continue); }
        if line.starts_with("define ") {
            let rest = &line[7..];
            let (name, expr) = rest.split_once(":=").ok_or("expected: define name := expression")?;
            let name = name.trim();
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') { return Err("invalid variable name".into()); }
            let value = ExprParser::parse(expr.trim(), vars)?;
            vars.insert(name.to_string(), value); i += 1; continue;
        }
        if line.starts_with("show ") {
            let value = ExprParser::parse(line[5..].trim(), vars)?;
            println!("{}", value.display()); i += 1; continue;
        }
        if line.starts_with("if ") && line.ends_with('{') {
            let condition = line[3..line.len()-1].trim();
            let (yes, next) = collect_block(lines, i + 1)?;
            let mut cursor = next;
            while cursor < lines.len() && clean(&lines[cursor]).is_empty() { cursor += 1; }
            let mut no = Vec::new();
            if cursor < lines.len() && clean(&lines[cursor]).starts_with("otherwise") {
                let other = clean(&lines[cursor]);
                if other == "otherwise {" {
                    let (block, after) = collect_block(lines, cursor + 1)?; no = block; cursor = after;
                } else if other.starts_with("otherwise if ") && other.ends_with('{') {
                    let (nested, after) = collect_block(lines, cursor + 1)?;
                    no.push(format!("if {} {{", other[13..other.len()-1].trim()));
                    no.extend(nested); no.push("}".into()); cursor = after;
                } else { return Err("expected 'otherwise {' or 'otherwise if condition {'".into()); }
            }
            let cond = ExprParser::parse(condition, vars)?.truthy();
            let flow = run_lines(if cond { &yes } else { &no }, vars, depth + 1)?;
            if flow != Flow::Normal { return Ok(flow); }
            i = cursor; continue;
        }
        if line.starts_with("repeat ") && line.ends_with('{') {
            let condition = line[7..line.len()-1].trim().to_string();
            let (body, next) = collect_block(lines, i + 1)?;
            let mut iterations = 0_u64;
            loop {
                if !ExprParser::parse(&condition, vars)?.truthy() { break; }
                iterations += 1;
                if iterations > 1_000_000 { return Err("loop exceeded safety limit (1,000,000 iterations)".into()); }
                match run_lines(&body, vars, depth + 1)? {
                    Flow::Break => break,
                    Flow::Continue | Flow::Normal => continue,
                }
            }
            i = next; continue;
        }
        if let Some((name, expr)) = line.split_once(":=") {
            let name = name.trim();
            if !vars.contains_key(name) { return Err(format!("cannot assign to undefined variable '{name}'")); }
            let value = ExprParser::parse(expr.trim(), vars)?;
            vars.insert(name.to_string(), value); i += 1; continue;
        }
        return Err(format!("unknown statement on line {}: {line}", i + 1));
    }
    Ok(Flow::Normal)
}
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Zenvora Lang MVP\nUsage: zenvora <file.zv>");
        std::process::exit(2);
    }
    let source = match fs::read_to_string(&args[1]) {
        Ok(s) => s,
        Err(e) => { eprintln!("Zenvora error: cannot read '{}': {e}", args[1]); std::process::exit(1); }
    };
    let lines: Vec<String> = source.lines().map(str::to_string).collect();
    let mut vars = HashMap::new();
    match run_lines(&lines, &mut vars, 0) {
        Ok(Flow::Normal) => {}
        Ok(Flow::Break) => {
            eprintln!("Zenvora error: 'break' used outside a repeat loop");
            std::process::exit(1);
        }
        Ok(Flow::Continue) => {
            eprintln!("Zenvora error: 'continue' used outside a repeat loop");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Zenvora error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn arithmetic_precedence() {
        let vars = HashMap::new();
        assert_eq!(ExprParser::parse("10 + 2 * 3", &vars).unwrap(), Value::Number(16.0));
    }
    #[test] fn comparisons_and_bools() {
        let vars = HashMap::new();
        assert_eq!(ExprParser::parse("5 >= 5", &vars).unwrap(), Value::Bool(true));
        assert_eq!(ExprParser::parse("false", &vars).unwrap(), Value::Bool(false));
    }
    #[test] fn string_value() {
        let vars = HashMap::new();
        assert_eq!(ExprParser::parse("\"Zenvora\"", &vars).unwrap(), Value::Text("Zenvora".into()));
    }

    #[test] fn nested_otherwise_braces_are_collected() {
        let lines = vec![
            "if true {".to_string(),
            "show 1".to_string(),
            "} otherwise {".to_string(),
            "show 2".to_string(),
            "}".to_string(),
            "show 3".to_string(),
        ];
        let (body, next) = collect_block(&lines, 1).unwrap();
        assert_eq!(body.len(), 4);
        assert_eq!(next, 5);
    }

    #[test] fn assignment_requires_existing_variable() {
        let lines = vec!["count := 2".to_string()];
        let mut vars = HashMap::new();
        let result = run_lines(&lines, &mut vars, 0);
        assert!(result.unwrap_err().contains("undefined variable 'count'"));
    }

    #[test] fn break_propagates_out_of_nested_condition() {
        let lines = vec![
            "define count := 1".to_string(),
            "repeat count <= 3 {".to_string(),
            "if count == 2 {".to_string(),
            "break".to_string(),
            "}".to_string(),
            "count := count + 1".to_string(),
            "}".to_string(),
        ];
        let mut vars = HashMap::new();
        assert_eq!(run_lines(&lines, &mut vars, 0).unwrap(), Flow::Normal);
        assert_eq!(vars.get("count"), Some(&Value::Number(2.0)));
    }

    #[test] fn invalid_number_format_is_rejected() {
        let vars = HashMap::new();
        assert!(ExprParser::parse("1.2.3", &vars).is_err());
    }
}

use std::collections::HashMap;
use std::{env, fs, process};

#[derive(Debug, Clone, PartialEq)]
enum Token { Number(i64), Ident(String), Plus, Minus, Star, Slash, LParen, RParen, End }

fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() { i += 1; continue; }
        if c == '#' { break; }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
            let n: String = chars[start..i].iter().collect();
            out.push(Token::Number(n.parse().map_err(|_| format!("invalid integer: {n}"))?));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') { i += 1; }
            out.push(Token::Ident(chars[start..i].iter().collect()));
            continue;
        }
        out.push(match c {
            '+' => Token::Plus, '-' => Token::Minus, '*' => Token::Star, '/' => Token::Slash,
            '(' => Token::LParen, ')' => Token::RParen,
            _ => return Err(format!("unexpected character '{c}'")),
        });
        i += 1;
    }
    out.push(Token::End);
    Ok(out)
}

#[derive(Debug, Clone)]
enum Op { Push(i64), Load(String), Store(String), Add, Sub, Mul, Div, Neg, Print }

struct Parser { tokens: Vec<Token>, pos: usize, code: Vec<Op> }
impl Parser {
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn take(&mut self) -> Token { let t = self.tokens[self.pos].clone(); self.pos += 1; t }
    fn expression(&mut self) -> Result<(), String> { self.term()?; loop { match self.peek() {
        Token::Plus => { self.take(); self.term()?; self.code.push(Op::Add); }
        Token::Minus => { self.take(); self.term()?; self.code.push(Op::Sub); }
        _ => break,
    }} Ok(()) }
    fn term(&mut self) -> Result<(), String> { self.factor()?; loop { match self.peek() {
        Token::Star => { self.take(); self.factor()?; self.code.push(Op::Mul); }
        Token::Slash => { self.take(); self.factor()?; self.code.push(Op::Div); }
        _ => break,
    }} Ok(()) }
    fn factor(&mut self) -> Result<(), String> { match self.take() {
        Token::Number(n) => self.code.push(Op::Push(n)),
        Token::Ident(name) => self.code.push(Op::Load(name)),
        Token::Minus => { self.factor()?; self.code.push(Op::Neg); }
        Token::LParen => { self.expression()?; if self.take() != Token::RParen { return Err("expected ')'".into()); } }
        other => return Err(format!("expected a number, variable, or '(', found {other:?}")),
    } Ok(()) }
}

fn compile(source: &str) -> Result<Vec<Op>, String> {
    let mut code = Vec::new();
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim().trim_end_matches(';').trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let statement = if let Some(rest) = line.strip_prefix("define ") {
            let (name, expr) = rest.split_once(":=").ok_or_else(|| format!("line {}: expected 'define name := expression'", line_no + 1))?;
            let name = name.trim();
            if name.is_empty() || !name.chars().enumerate().all(|(i,c)| c == '_' || c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit())) {
                return Err(format!("line {}: invalid variable name", line_no + 1));
            }
            (name.to_string(), expr.trim(), false)
        } else if let Some(expr) = line.strip_prefix("show ") {
            ("".to_string(), expr.trim(), true)
        } else if let Some(rest) = line.strip_prefix("print(").and_then(|s| s.strip_suffix(')')) {
            ("".to_string(), rest.trim(), true)
        } else if let Some((name, expr)) = line.split_once(":=") {
            (name.trim().to_string(), expr.trim(), false)
        } else {
            return Err(format!("line {}: expected 'define name := ...', 'name := ...', or 'show ...'", line_no + 1));
        };
        if statement.1.is_empty() { return Err(format!("line {}: missing expression", line_no + 1)); }
        let tokens = lex(statement.1).map_err(|e| format!("line {}: {e}", line_no + 1))?;
        let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
        parser.expression().map_err(|e| format!("line {}: {e}", line_no + 1))?;
        if parser.peek() != &Token::End { return Err(format!("line {}: unexpected token {:?}", line_no + 1, parser.peek())); }
        code.extend(parser.code);
        if statement.2 { code.push(Op::Print); } else { code.push(Op::Store(statement.0)); }
    }
    Ok(code)
}

fn run(code: &[Op]) -> Result<(), String> {
    let mut stack: Vec<i64> = Vec::new();
    let mut vars: HashMap<String, i64> = HashMap::new();
    for op in code {
        match op {
            Op::Push(n) => stack.push(*n),
            Op::Load(name) => stack.push(*vars.get(name).ok_or_else(|| format!("undefined variable '{name}'"))?),
            Op::Store(name) => { let n = stack.pop().ok_or("VM stack underflow")?; vars.insert(name.clone(), n); }
            Op::Print => println!("{}", stack.pop().ok_or("VM stack underflow")?),
            Op::Neg => { let a = stack.pop().ok_or("VM stack underflow")?; stack.push(a.checked_neg().ok_or("integer overflow")?); }
            Op::Add | Op::Sub | Op::Mul | Op::Div => {
                let b = stack.pop().ok_or("VM stack underflow")?;
                let a = stack.pop().ok_or("VM stack underflow")?;
                let value = match op {
                    Op::Add => a.checked_add(b).ok_or("integer overflow")?,
                    Op::Sub => a.checked_sub(b).ok_or("integer overflow")?,
                    Op::Mul => a.checked_mul(b).ok_or("integer overflow")?,
                    Op::Div => { if b == 0 { return Err("division by zero".into()); } a.checked_div(b).ok_or("integer overflow")? },
                    _ => unreachable!(),
                };
                stack.push(value);
            }
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        println!("Zenvora Lang v0.1.0\nUsage: zv <file.zv>\n\nSyntax: define total := 10 + 20\n        show total * 2");
        if args.len() != 2 { process::exit(2); }
        return;
    }
    let path = &args[1];
    let source = fs::read_to_string(path).unwrap_or_else(|e| { eprintln!("Zenvora error: cannot read {path}: {e}"); process::exit(1) });
    match compile(&source).and_then(|code| run(&code)) {
        Ok(()) => {},
        Err(e) => { eprintln!("Zenvora error: {e}"); process::exit(1); }
    }
}

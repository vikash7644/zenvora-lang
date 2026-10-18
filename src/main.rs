use std::collections::HashMap;
use std::{env, fs, process};

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64), Ident(String), Plus, Minus, Star, Slash,
    LParen, RParen, EqEq, NotEq, Less, LessEq, Greater, GreaterEq, End,
}

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
        let token = match c {
            '+' => Token::Plus, '-' => Token::Minus, '*' => Token::Star, '/' => Token::Slash,
            '(' => Token::LParen, ')' => Token::RParen,
            '=' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::EqEq }
            '!' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::NotEq }
            '<' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::LessEq }
            '>' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::GreaterEq }
            '<' => Token::Less,
            '>' => Token::Greater,
            _ => return Err(format!("unexpected character '{c}'")),
        };
        out.push(token);
        i += 1;
    }
    out.push(Token::End);
    Ok(out)
}

#[derive(Debug, Clone)]
enum Op {
    Push(i64), Load(String), Store(String), Add, Sub, Mul, Div, Neg, Print,
    Eq, Ne, Lt, Le, Gt, Ge, JumpIfFalse(usize), Jump(usize),
}

struct Parser { tokens: Vec<Token>, pos: usize, code: Vec<Op> }
impl Parser {
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn take(&mut self) -> Token { let t = self.tokens[self.pos].clone(); self.pos += 1; t }

    fn expression(&mut self) -> Result<(), String> {
        self.term()?;
        loop {
            match self.peek().clone() {
                Token::Plus => { self.take(); self.term()?; self.code.push(Op::Add); }
                Token::Minus => { self.take(); self.term()?; self.code.push(Op::Sub); }
                _ => break,
            }
        }
        Ok(())
    }

    fn term(&mut self) -> Result<(), String> {
        self.factor()?;
        loop {
            match self.peek() {
                Token::Star => { self.take(); self.factor()?; self.code.push(Op::Mul); }
                Token::Slash => { self.take(); self.factor()?; self.code.push(Op::Div); }
                _ => break,
            }
        }
        Ok(())
    }

    fn factor(&mut self) -> Result<(), String> {
        match self.take() {
            Token::Number(n) => self.code.push(Op::Push(n)),
            Token::Ident(name) => self.code.push(Op::Load(name)),
            Token::Minus => { self.factor()?; self.code.push(Op::Neg); }
            Token::LParen => {
                self.expression()?;
                if self.take() != Token::RParen { return Err("expected ')'".into()); }
            }
            other => return Err(format!("expected a number, variable, or '(', found {other:?}")),
        }
        Ok(())
    }

    fn condition(&mut self) -> Result<(), String> {
        self.expression()?;
        let op = match self.take() {
            Token::EqEq => Op::Eq,
            Token::NotEq => Op::Ne,
            Token::Less => Op::Lt,
            Token::LessEq => Op::Le,
            Token::Greater => Op::Gt,
            Token::GreaterEq => Op::Ge,
            other => return Err(format!("expected comparison operator (==, !=, <, <=, >, >=), found {other:?}")),
        };
        self.expression()?;
        self.code.push(op);
        Ok(())
    }
}

struct IfFrame { false_jump: Option<usize>, end_jump: Option<usize> }

fn compile(source: &str) -> Result<Vec<Op>, String> {
    let mut code = Vec::new();
    let mut if_stack: Vec<IfFrame> = Vec::new();

    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim().trim_end_matches(';').trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        if let Some(condition) = line.strip_prefix("if ").and_then(|s| s.strip_suffix('{')) {
            let tokens = lex(condition.trim()).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
            parser.condition().map_err(|e| format!("line {}: {e}", line_no + 1))?;
            if parser.peek() != &Token::End {
                return Err(format!("line {}: unexpected token {:?}", line_no + 1, parser.peek()));
            }
            code.extend(parser.code);
            let jump = code.len();
            code.push(Op::JumpIfFalse(usize::MAX));
            if_stack.push(IfFrame { false_jump: Some(jump), end_jump: None });
            continue;
        }

        if line == "} otherwise {" {
            let frame = if_stack.last_mut().ok_or_else(|| format!("line {}: otherwise without matching if", line_no + 1))?;
            if frame.false_jump.is_none() || frame.end_jump.is_some() {
                return Err(format!("line {}: duplicate or misplaced otherwise", line_no + 1));
            }
            let end_jump = code.len();
            code.push(Op::Jump(usize::MAX));
            if let Some(false_jump) = frame.false_jump.take() {
                code[false_jump] = Op::JumpIfFalse(code.len());
            }
            frame.end_jump = Some(end_jump);
            continue;
        }

        if line == "}" {
            let frame = if_stack.pop().ok_or_else(|| format!("line {}: closing brace without matching if", line_no + 1))?;
            if let Some(end_jump) = frame.end_jump {
                code[end_jump] = Op::Jump(code.len());
            } else if let Some(false_jump) = frame.false_jump {
                code[false_jump] = Op::JumpIfFalse(code.len());
            }
            continue;
        }

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
            return Err(format!("line {}: expected a declaration, assignment, show, if, or closing brace", line_no + 1));
        };

        if statement.1.is_empty() { return Err(format!("line {}: missing expression", line_no + 1)); }
        let tokens = lex(statement.1).map_err(|e| format!("line {}: {e}", line_no + 1))?;
        let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
        parser.expression().map_err(|e| format!("line {}: {e}", line_no + 1))?;
        if parser.peek() != &Token::End {
            return Err(format!("line {}: unexpected token {:?}", line_no + 1, parser.peek()));
        }
        code.extend(parser.code);
        if statement.2 { code.push(Op::Print); } else { code.push(Op::Store(statement.0)); }
    }

    if !if_stack.is_empty() { return Err("missing closing brace for if block".into()); }
    Ok(code)
}

fn run(code: &[Op]) -> Result<(), String> {
    let mut stack: Vec<i64> = Vec::new();
    let mut vars: HashMap<String, i64> = HashMap::new();
    let mut pc = 0usize;
    while pc < code.len() {
        match &code[pc] {
            Op::Push(n) => stack.push(*n),
            Op::Load(name) => stack.push(*vars.get(name).ok_or_else(|| format!("undefined variable '{name}'"))?),
            Op::Store(name) => { let n = stack.pop().ok_or("VM stack underflow")?; vars.insert(name.clone(), n); }
            Op::Print => println!("{}", stack.pop().ok_or("VM stack underflow")?),
            Op::Neg => {
                let a = stack.pop().ok_or("VM stack underflow")?;
                stack.push(a.checked_neg().ok_or("integer overflow")?);
            }
            Op::JumpIfFalse(target) => {
                let condition = stack.pop().ok_or("VM stack underflow")?;
                if condition == 0 { pc = *target; continue; }
            }
            Op::Jump(target) => { pc = *target; continue; }
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                let b = stack.pop().ok_or("VM stack underflow")?;
                let a = stack.pop().ok_or("VM stack underflow")?;
                let value = match &code[pc] {
                    Op::Add => a.checked_add(b).ok_or("integer overflow")?,
                    Op::Sub => a.checked_sub(b).ok_or("integer overflow")?,
                    Op::Mul => a.checked_mul(b).ok_or("integer overflow")?,
                    Op::Div => { if b == 0 { return Err("division by zero".into()); } a.checked_div(b).ok_or("integer overflow")? },
                    Op::Eq => i64::from(a == b),
                    Op::Ne => i64::from(a != b),
                    Op::Lt => i64::from(a < b),
                    Op::Le => i64::from(a <= b),
                    Op::Gt => i64::from(a > b),
                    Op::Ge => i64::from(a >= b),
                    _ => unreachable!(),
                };
                stack.push(value);
            }
        }
        pc += 1;
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        println!("Zenvora Lang v0.2.0\nUsage: zv <file.zv>\n\nSyntax: define total := 10 + 20\n        show total * 2\n        if total >= 18 {{ ... }}");
        if args.len() != 2 { process::exit(2); }
        return;
    }
    let path = &args[1];
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Zenvora error: cannot read {path}: {e}");
        process::exit(1)
    });
    match compile(&source).and_then(|code| run(&code)) {
        Ok(()) => {},
        Err(e) => { eprintln!("Zenvora error: {e}"); process::exit(1); }
    }
}

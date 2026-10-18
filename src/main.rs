use std::collections::HashMap;
use std::{env, fs, process};

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64), Bool(bool), Ident(String), And, Or, Not, Plus, Minus, Star, Slash,
    LParen, RParen, Comma, EqEq, NotEq, Less, LessEq, Greater, GreaterEq, End,
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
            let ident: String = chars[start..i].iter().collect();
            match ident.as_str() {
                "true" => out.push(Token::Bool(true)),
                "false" => out.push(Token::Bool(false)),
                "and" => out.push(Token::And),
                "or" => out.push(Token::Or),
                "not" => out.push(Token::Not),
                _ => out.push(Token::Ident(ident)),
            }
            continue;
        }
        let token = match c {
            '+' => Token::Plus, '-' => Token::Minus, '*' => Token::Star, '/' => Token::Slash,
            '(' => Token::LParen, ')' => Token::RParen, ',' => Token::Comma,
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
    Push(i64), PushBool(bool), Load(String), Store(String), Add, Sub, Mul, Div, Neg, Not, And, Or, Print,
    Eq, Ne, Lt, Le, Gt, Ge, JumpIfFalse(usize), Jump(usize), Call(String, usize), CallTarget { target: usize, params: Vec<String>, arg_count: usize }, Return, ReturnValue,
}

struct Parser { tokens: Vec<Token>, pos: usize, code: Vec<Op> }
impl Parser {
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn take(&mut self) -> Token { let t = self.tokens[self.pos].clone(); self.pos += 1; t }

    fn expression(&mut self) -> Result<(), String> {
        self.logical_or()
    }

    fn logical_or(&mut self) -> Result<(), String> {
        self.logical_and()?;
        while self.peek() == &Token::Or {
            self.take();
            self.logical_and()?;
            self.code.push(Op::Or);
        }
        Ok(())
    }

    fn logical_and(&mut self) -> Result<(), String> {
        self.comparison()?;
        while self.peek() == &Token::And {
            self.take();
            self.comparison()?;
            self.code.push(Op::And);
        }
        Ok(())
    }

    fn comparison(&mut self) -> Result<(), String> {
        self.additive()?;
        let op = match self.peek().clone() {
            Token::EqEq => { self.take(); Some(Op::Eq) },
            Token::NotEq => { self.take(); Some(Op::Ne) },
            Token::Less => { self.take(); Some(Op::Lt) },
            Token::LessEq => { self.take(); Some(Op::Le) },
            Token::Greater => { self.take(); Some(Op::Gt) },
            Token::GreaterEq => { self.take(); Some(Op::Ge) },
            _ => None,
        };
        if let Some(op) = op {
            self.additive()?;
            self.code.push(op);
        }
        Ok(())
    }

    fn additive(&mut self) -> Result<(), String> {
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
            Token::Bool(value) => self.code.push(Op::PushBool(value)),
            Token::Ident(name) => {
                if self.peek() == &Token::LParen {
                    self.take();
                    let mut arg_count = 0usize;
                    if self.peek() != &Token::RParen {
                        loop {
                            self.expression()?;
                            arg_count += 1;
                            match self.take() {
                                Token::Comma => continue,
                                Token::RParen => break,
                                other => return Err(format!("expected ',' or ')' in function call, found {other:?}")),
                            }
                        }
                    } else {
                        self.take();
                    }
                    self.code.push(Op::Call(name, arg_count));
                } else {
                    self.code.push(Op::Load(name));
                }
            },
            Token::Minus => { self.factor()?; self.code.push(Op::Neg); }
            Token::Not => { self.factor()?; self.code.push(Op::Not); }
            Token::LParen => {
                self.expression()?;
                if self.take() != Token::RParen { return Err("expected ')'".into()); }
            }
            other => return Err(format!("expected a number, variable, or '(', found {other:?}")),
        }
        Ok(())
    }

    fn condition(&mut self) -> Result<(), String> {
        self.expression()
    }
}

enum BlockFrame {
    If { false_jump: Option<usize>, end_jump: Option<usize> },
    Repeat { start: usize, exit_jump: usize, breaks: Vec<usize> },
}

fn split_arguments(input: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => { args.push(input[start..i].trim()); start = i + ch.len_utf8(); }
            _ => {}
        }
    }
    if start < input.len() { args.push(input[start..].trim()); }
    args
}

fn compile_core(source: &str) -> Result<Vec<Op>, String> {
    let mut code = Vec::new();
    let mut blocks: Vec<BlockFrame> = Vec::new();

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
            blocks.push(BlockFrame::If { false_jump: Some(jump), end_jump: None });
            continue;
        }

        if let Some(condition) = line.strip_prefix("repeat ").and_then(|s| s.strip_suffix('{')) {
            let tokens = lex(condition.trim()).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
            parser.condition().map_err(|e| format!("line {}: {e}", line_no + 1))?;
            if parser.peek() != &Token::End {
                return Err(format!("line {}: unexpected token {:?}", line_no + 1, parser.peek()));
            }
            let start = code.len();
            code.extend(parser.code);
            let exit_jump = code.len();
            code.push(Op::JumpIfFalse(usize::MAX));
            blocks.push(BlockFrame::Repeat { start, exit_jump, breaks: Vec::new() });
            continue;
        }

        if let Some(rest) = line.strip_prefix("call ") {
            let call_text = rest.trim();
            let (name, args_with_close) = call_text.split_once('(').ok_or_else(|| format!("line {}: expected 'call function_name(...)'", line_no + 1))?;
            let args_text = args_with_close.strip_suffix(')').ok_or_else(|| format!("line {}: expected ')' after function arguments", line_no + 1))?;
            let name = name.trim();
            if name.is_empty() || !name.chars().enumerate().all(|(i,c)| c == '_' || c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit())) {
                return Err(format!("line {}: invalid function name", line_no + 1));
            }
            let args = split_arguments(args_text);
            let args: Vec<&str> = if args_text.trim().is_empty() { Vec::new() } else { args };
            for arg in &args {
                let tokens = lex(arg.trim()).map_err(|e| format!("line {}: {e}", line_no + 1))?;
                let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
                parser.expression().map_err(|e| format!("line {}: invalid function argument: {e}", line_no + 1))?;
                if parser.peek() != &Token::End {
                    return Err(format!("line {}: unexpected token in function argument {:?}", line_no + 1, parser.peek()));
                }
                code.extend(parser.code);
            }
            code.push(Op::Call(name.to_string(), args.len()));
            continue;
        }
        if line == "return" { code.push(Op::Return); continue; }
        if let Some(expr) = line.strip_prefix("return ") {
            let tokens = lex(expr.trim()).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
            parser.expression().map_err(|e| format!("line {}: invalid return expression: {e}", line_no + 1))?;
            if parser.peek() != &Token::End {
                return Err(format!("line {}: unexpected token in return expression {:?}", line_no + 1, parser.peek()));
            }
            code.extend(parser.code);
            code.push(Op::ReturnValue);
            continue;
        }

        if line == "break" {
            let Some(index) = blocks.iter().rposition(|block| matches!(block, BlockFrame::Repeat { .. })) else {
                return Err(format!("line {}: 'break' used outside a repeat loop", line_no + 1));
            };
            let jump = code.len();
            code.push(Op::Jump(usize::MAX));
            if let BlockFrame::Repeat { breaks, .. } = &mut blocks[index] { breaks.push(jump); }
            continue;
        }

        if line == "continue" {
            let Some(index) = blocks.iter().rposition(|block| matches!(block, BlockFrame::Repeat { .. })) else {
                return Err(format!("line {}: 'continue' used outside a repeat loop", line_no + 1));
            };
            let start = match &blocks[index] {
                BlockFrame::Repeat { start, .. } => *start,
                BlockFrame::If { .. } => unreachable!(),
            };
            code.push(Op::Jump(start));
            continue;
        }

        if line == "} otherwise {" {
            let frame = blocks.last_mut().ok_or_else(|| format!("line {}: otherwise without matching if", line_no + 1))?;
            match frame {
                BlockFrame::If { false_jump, end_jump } => {
                    if false_jump.is_none() || end_jump.is_some() {
                        return Err(format!("line {}: duplicate or misplaced otherwise", line_no + 1));
                    }
                    let jump = code.len();
                    code.push(Op::Jump(usize::MAX));
                    if let Some(false_jump) = false_jump.take() {
                        code[false_jump] = Op::JumpIfFalse(code.len());
                    }
                    *end_jump = Some(jump);
                }
                BlockFrame::Repeat { .. } => {
                    return Err(format!("line {}: otherwise must follow an if block", line_no + 1));
                }
            }
            continue;
        }

        if line == "}" {
            let frame = blocks.pop().ok_or_else(|| format!("line {}: closing brace without matching block", line_no + 1))?;
            match frame {
                BlockFrame::If { false_jump, end_jump } => {
                    if let Some(end_jump) = end_jump {
                        code[end_jump] = Op::Jump(code.len());
                    } else if let Some(false_jump) = false_jump {
                        code[false_jump] = Op::JumpIfFalse(code.len());
                    }
                }
                BlockFrame::Repeat { start, exit_jump, breaks } => {
                    code.push(Op::Jump(start));
                    let end = code.len();
                    code[exit_jump] = Op::JumpIfFalse(end);
                    for jump in breaks { code[jump] = Op::Jump(end); }
                }
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
        } else if let Some(expr) = line.strip_prefix("print(").and_then(|s| s.strip_suffix(')')) {
            ("".to_string(), expr.trim(), true)
        } else if let Some((name, expr)) = line.split_once(":=") {
            (name.trim().to_string(), expr.trim(), false)
        } else {
            return Err(format!("line {}: expected a declaration, assignment, show, call, return, if, repeat, break, continue, or closing brace", line_no + 1));
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

    if let Some(frame) = blocks.last() {
        let kind = match frame { BlockFrame::If { .. } => "if", BlockFrame::Repeat { .. } => "repeat" };
        return Err(format!("missing closing brace for {kind} block"));
    }
    Ok(code)
}


fn compile(source: &str) -> Result<Vec<Op>, String> {
    let mut main_source = String::new();
    let mut functions: Vec<(String, Vec<String>, String)> = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(header) = line.strip_prefix("fn ").and_then(|s| s.strip_suffix('{')) {
            let header = header.trim();
            let (name, params_text) = header.split_once('(').ok_or_else(|| format!("line {}: expected 'fn name(parameters) {{'", i + 1))?;
            let params_text = params_text.strip_suffix(')').ok_or_else(|| format!("line {}: expected ')' in function declaration", i + 1))?;
            let name = name.trim();
            if name.is_empty() || !name.chars().enumerate().all(|(j,c)| c == '_' || c.is_ascii_alphanumeric() && (j > 0 || !c.is_ascii_digit())) {
                return Err(format!("line {}: invalid function name", i + 1));
            }
            let params: Vec<String> = if params_text.trim().is_empty() { Vec::new() } else { params_text.split(',').map(|p| p.trim().to_string()).collect() };
            for param in &params {
                if param.is_empty() || !param.chars().enumerate().all(|(j,c)| c == '_' || c.is_ascii_alphanumeric() && (j > 0 || !c.is_ascii_digit())) {
                    return Err(format!("line {}: invalid parameter name in function '{name}'", i + 1));
                }
            }
            for (j, param) in params.iter().enumerate() {
                if params[..j].contains(param) { return Err(format!("line {}: duplicate parameter '{param}'", i + 1)); }
            }
            if functions.iter().any(|(existing, _, _)| existing == name) {
                return Err(format!("line {}: duplicate function '{name}'", i + 1));
            }
            i += 1;
            let mut depth = 1i32;
            let mut body = String::new();
            while i < lines.len() && depth > 0 {
                let current = lines[i];
                let opens = current.chars().filter(|c| *c == '{').count() as i32;
                let closes = current.chars().filter(|c| *c == '}').count() as i32;
                depth += opens - closes;
                if depth > 0 { body.push_str(current); body.push('\n'); }
                i += 1;
            }
            if depth != 0 { return Err(format!("missing closing brace for function '{name}'")); }
            functions.push((name.to_string(), params, body));
        } else {
            main_source.push_str(lines[i]);
            main_source.push('\n');
            i += 1;
        }
    }

    let mut code = compile_core(&main_source)?;
    let main_exit = code.len();
    code.push(Op::Jump(usize::MAX));
    let mut entries: HashMap<String, (usize, Vec<String>)> = HashMap::new();
    for (name, params, body) in &functions {
        let entry = code.len();
        entries.insert(name.clone(), (entry, params.clone()));
        let mut body_code = compile_core(body)?;
        for op in &mut body_code {
            match op {
                Op::Jump(target) | Op::JumpIfFalse(target) if *target != usize::MAX => *target += entry,
                _ => {}
            }
        }
        code.extend(body_code);
        code.push(Op::Return);
    }
    let end = code.len();
    code[main_exit] = Op::Jump(end);
    for op in &mut code {
        if let Op::Call(name, arg_count) = op {
            let (target, params) = entries.get(name).cloned().ok_or_else(|| format!("call to undefined function '{name}'"))?;
            if *arg_count != params.len() {
                return Err(format!("function '{name}' expects {} argument(s), got {}", params.len(), arg_count));
            }
            *op = Op::CallTarget { target, params, arg_count: *arg_count };
        }
    }
    Ok(code)
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Int(i64),
    Bool(bool),
}

fn run(code: &[Op]) -> Result<(), String> {
    let mut stack: Vec<Value> = Vec::new();
    let mut vars: HashMap<String, Value> = HashMap::new();
    let mut pc = 0usize;
    let mut call_stack: Vec<(usize, HashMap<String, Value>)> = Vec::new();
    let mut steps = 0usize;
    const MAX_STEPS: usize = 1_000_000;
    while pc < code.len() {
        steps += 1;
        if steps > MAX_STEPS {
            return Err("execution exceeded safety limit (1,000,000 instructions)".into());
        }
        match &code[pc] {
            Op::Push(n) => stack.push(Value::Int(*n)),
            Op::PushBool(value) => stack.push(Value::Bool(*value)),
            Op::Load(name) => stack.push(vars.get(name).cloned().ok_or_else(|| format!("undefined variable '{name}'"))?),
            Op::Store(name) => { let value = stack.pop().ok_or("VM stack underflow")?; vars.insert(name.clone(), value); }
            Op::Print => match stack.pop().ok_or("VM stack underflow")? {
                Value::Int(n) => println!("{n}"),
                Value::Bool(value) => println!("{value}"),
            },
            Op::Neg => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                match value {
                    Value::Int(n) => stack.push(Value::Int(n.checked_neg().ok_or("integer overflow")?)),
                    Value::Bool(_) => return Err("type error: unary '-' expects an integer".into()),
                }
            }
            Op::Not => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                match value {
                    Value::Bool(value) => stack.push(Value::Bool(!value)),
                    Value::Int(_) => return Err("type error: 'not' expects a boolean".into()),
                }
            }
            Op::JumpIfFalse(target) => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                let is_false = match value {
                    Value::Bool(value) => !value,
                    Value::Int(n) => n == 0,
                };
                if is_false { pc = *target; continue; }
            }
            Op::Jump(target) => { pc = *target; continue; }
            Op::Call(name, _) => return Err(format!("unresolved function call '{name}'")),
            Op::CallTarget { target, params, arg_count } => {
                let mut values = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count { values.push(stack.pop().ok_or("VM stack underflow while reading function arguments")?); }
                values.reverse();
                let mut local_vars = HashMap::new();
                for (name, value) in params.iter().zip(values) { local_vars.insert(name.clone(), value); }
                let caller_vars = std::mem::replace(&mut vars, local_vars);
                call_stack.push((pc + 1, caller_vars));
                pc = *target;
                continue;
            }
            Op::Return => {
                let (return_pc, caller_vars) = call_stack.pop().ok_or("return used outside a function call")?;
                vars = caller_vars;
                pc = return_pc;
                continue;
            }
            Op::ReturnValue => {
                let value = stack.pop().ok_or("VM stack underflow in return expression")?;
                let (return_pc, caller_vars) = call_stack.pop().ok_or("return used outside a function call")?;
                vars = caller_vars;
                stack.push(value);
                pc = return_pc;
                continue;
            }
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::And | Op::Or => {
                let b = stack.pop().ok_or("VM stack underflow")?;
                let a = stack.pop().ok_or("VM stack underflow")?;
                let value = match (&code[pc], a, b) {
                    (Op::Add, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_add(b).ok_or("integer overflow")?),
                    (Op::Sub, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_sub(b).ok_or("integer overflow")?),
                    (Op::Mul, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_mul(b).ok_or("integer overflow")?),
                    (Op::Div, Value::Int(_), Value::Int(0)) => return Err("division by zero".into()),
                    (Op::Div, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_div(b).ok_or("integer overflow")?),
                    (Op::Eq, a, b) => Value::Bool(a == b),
                    (Op::Ne, a, b) => Value::Bool(a != b),
                    (Op::And, Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
                    (Op::Or, Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
                    (Op::And, _, _) => return Err("type error: 'and' expects booleans".into()),
                    (Op::Or, _, _) => return Err("type error: 'or' expects booleans".into()),
                    (Op::Lt, Value::Int(a), Value::Int(b)) => Value::Bool(a < b),
                    (Op::Le, Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
                    (Op::Gt, Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
                    (Op::Ge, Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),
                    (Op::Add, _, _) => return Err("type error: '+' expects integers".into()),
                    (Op::Sub, _, _) => return Err("type error: '-' expects integers".into()),
                    (Op::Mul, _, _) => return Err("type error: '*' expects integers".into()),
                    (Op::Div, _, _) => return Err("type error: '/' expects integers".into()),
                    (Op::Lt | Op::Le | Op::Gt | Op::Ge, _, _) => return Err("type error: ordered comparisons expect integers".into()),
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
        println!("Zenvora Lang v0.5.0\nUsage: zv <file.zv>\n\nSyntax: define total := 10 + 20\n        show total * 2\n        if total >= 18 {{ ... }}");
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

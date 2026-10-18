use std::collections::HashMap;
use std::{env, fs, process};

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64), String(String), Bool(bool), Ident(String), And, Or, Not, Plus, Minus, Star, Slash,
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
        if c == '"' {
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                let ch = chars[i];
                i += 1;
                match ch {
                    '"' => { closed = true; break; }
                    '\\' => {
                        if i >= chars.len() { return Err("unterminated escape in string literal".into()); }
                        let escaped = chars[i];
                        i += 1;
                        match escaped {
                            '"' => value.push('"'),
                            '\\' => value.push('\\'),
                            'n' => value.push('\n'),
                            'r' => value.push('\r'),
                            't' => value.push('\t'),
                            _ => return Err(format!("unsupported string escape '\\{escaped}'")),
                        }
                    }
                    _ => value.push(ch),
                }
            }
            if !closed { return Err("unterminated string literal".into()); }
            out.push(Token::String(value));
            continue;
        }
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

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Push(i64), PushString(String), PushBool(bool), Load(String), Store(String), Add, Sub, Mul, Div, Neg, Not, And, Or, Print,
    Eq, Ne, Lt, Le, Gt, Ge, JumpIfFalse(usize), JumpIfFalseKeep(usize), JumpIfTrueKeep(usize), Jump(usize), Call(String, usize), Builtin(String, usize), CallTarget { target: usize, params: Vec<String>, arg_count: usize }, Return, ReturnValue,
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
            let jump = self.code.len();
            self.code.push(Op::JumpIfTrueKeep(usize::MAX));
            self.logical_and()?;
            self.code.push(Op::Or);
            let target = self.code.len();
            self.code[jump] = Op::JumpIfTrueKeep(target);
        }
        Ok(())
    }

    fn logical_and(&mut self) -> Result<(), String> {
        self.comparison()?;
        while self.peek() == &Token::And {
            self.take();
            let jump = self.code.len();
            self.code.push(Op::JumpIfFalseKeep(usize::MAX));
            self.comparison()?;
            self.code.push(Op::And);
            let target = self.code.len();
            self.code[jump] = Op::JumpIfFalseKeep(target);
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
            Token::String(value) => self.code.push(Op::PushString(value)),
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
                    if is_builtin(&name) { self.code.push(Op::Builtin(name, arg_count)); } else { self.code.push(Op::Call(name, arg_count)); }
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
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => { args.push(input[start..i].trim()); start = i + ch.len_utf8(); }
            _ => {}
        }
    }
    if start < input.len() { args.push(input[start..].trim()); }
    args
}

fn append_code(code: &mut Vec<Op>, mut appended: Vec<Op>) {
    let offset = code.len();
    for op in &mut appended {
        match op {
            Op::Jump(target)
            | Op::JumpIfFalse(target)
            | Op::JumpIfFalseKeep(target)
            | Op::JumpIfTrueKeep(target)
                if *target != usize::MAX => *target += offset,
            _ => {}
        }
    }
    code.extend(appended);
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
            append_code(&mut code, parser.code);
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
            append_code(&mut code, parser.code);
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
                append_code(&mut code, parser.code);
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
            append_code(&mut code, parser.code);
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
        if !statement.2 && !valid_identifier(&statement.0) {
            return Err(format!("line {}: invalid variable name", line_no + 1));
        }
        let tokens = lex(statement.1).map_err(|e| format!("line {}: {e}", line_no + 1))?;
        let mut parser = Parser { tokens, pos: 0, code: Vec::new() };
        parser.expression().map_err(|e| format!("line {}: {e}", line_no + 1))?;
        if parser.peek() != &Token::End {
            return Err(format!("line {}: unexpected token {:?}", line_no + 1, parser.peek()));
        }
        append_code(&mut code, parser.code);
        if statement.2 { code.push(Op::Print); } else { code.push(Op::Store(statement.0)); }
    }

    if let Some(frame) = blocks.last() {
        let kind = match frame { BlockFrame::If { .. } => "if", BlockFrame::Repeat { .. } => "repeat" };
        return Err(format!("missing closing brace for {kind} block"));
    }
    Ok(code)
}


// Count block braces only in source code, ignoring quoted strings and comments.
fn count_block_braces(line: &str) -> (i32, i32) {
    let mut opens = 0i32;
    let mut closes = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '#' {
            break;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == '{' {
            opens += 1;
        } else if ch == '}' {
            closes += 1;
        }
    }
    (opens, closes)
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
                let (opens, closes) = count_block_braces(current);
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
                Op::Jump(target) | Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) if *target != usize::MAX => *target += entry,
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
    Str(String),
    Bool(bool),
    Array(Vec<Value>),
    Map(std::collections::BTreeMap<String, Value>),
}

fn is_builtin(name: &str) -> bool { matches!(name, "array" | "map" | "get" | "len" | "contains" | "push" | "put" | "keys" | "values") }

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => format!("[{}]", items.iter().map(value_to_string).collect::<Vec<_>>().join(", ")),
        Value::Map(items) => format!("{{{}}}", items.iter().map(|(k, v)| format!("\"{}\": {}", k, value_to_string(v))).collect::<Vec<_>>().join(", ")),
    }
}

fn builtin(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let arg_len = args.len();
    let arity = |expected: usize| if arg_len == expected { Ok(()) } else { Err(format!("builtin '{name}' expects {expected} argument(s), got {arg_len}")) };
    match name {
        "array" => Ok(Value::Array(args)),
        "map" => {
            if args.len() % 2 != 0 { return Err("builtin 'map' expects alternating string keys and values".into()); }
            let mut result = std::collections::BTreeMap::new();
            let mut it = args.into_iter();
            while let Some(key) = it.next() {
                let Value::Str(key) = key else { return Err("builtin 'map' keys must be strings".into()); };
                let value = it.next().ok_or("missing map value")?;
                result.insert(key, value);
            }
            Ok(Value::Map(result))
        }
        "len" => {
            arity(1)?;
            let n = match &args[0] { Value::Array(v) => v.len(), Value::Map(v) => v.len(), Value::Str(v) => v.chars().count(), _ => return Err("type error: len expects an array, map, or string".into()) };
            Ok(Value::Int(i64::try_from(n).map_err(|_| "length exceeds integer range")?))
        }
        "get" => {
            arity(2)?;
            match (&args[0], &args[1]) {
                (Value::Array(items), Value::Int(index)) if *index >= 0 => items.get(*index as usize).cloned().ok_or_else(|| format!("array index {index} out of bounds")),
                (Value::Array(_), Value::Int(index)) => Err(format!("array index {index} out of bounds")),
                (Value::Map(items), Value::Str(key)) => items.get(key).cloned().ok_or_else(|| format!("map key '{key}' not found")),
                (Value::Array(_), _) => Err("type error: array index must be an integer".into()),
                (Value::Map(_), _) => Err("type error: map key must be a string".into()),
                _ => Err("type error: get expects an array or map as its first argument".into()),
            }
        }
        "contains" => {
            arity(2)?;
            let found = match (&args[0], &args[1]) {
                (Value::Array(items), needle) => items.contains(needle),
                (Value::Map(items), Value::Str(key)) => items.contains_key(key),
                (Value::Map(_), _) => return Err("type error: map contains expects a string key".into()),
                _ => return Err("type error: contains expects an array or map".into()),
            };
            Ok(Value::Bool(found))
        }
        "push" => {
            arity(2)?;
            let mut items = match &args[0] { Value::Array(items) => items.clone(), _ => return Err("type error: push expects an array as its first argument".into()) };
            items.push(args[1].clone());
            Ok(Value::Array(items))
        }
        "put" => {
            arity(3)?;
            let mut items = match &args[0] { Value::Map(items) => items.clone(), _ => return Err("type error: put expects a map as its first argument".into()) };
            let Value::Str(key) = &args[1] else { return Err("type error: put key must be a string".into()); };
            items.insert(key.clone(), args[2].clone());
            Ok(Value::Map(items))
        }
        "keys" | "values" => {
            arity(1)?;
            let Value::Map(items) = &args[0] else { return Err(format!("type error: {name} expects a map")); };
            let values = if name == "keys" { items.keys().cloned().map(Value::Str).collect() } else { items.values().cloned().collect() };
            Ok(Value::Array(values))
        }
        _ => Err(format!("unknown builtin '{name}'")),
    }
}

fn run(code: &[Op]) -> Result<(), String> {
    run_with_output(code, &mut |line| println!("{line}"))
}

fn run_with_output<F>(code: &[Op], output: &mut F) -> Result<(), String>
where
    F: FnMut(String),
{
    let mut stack: Vec<Value> = Vec::new();
    let mut vars: HashMap<String, Value> = HashMap::new();
    let mut pc = 0usize;
    let mut call_stack: Vec<(usize, HashMap<String, Value>)> = Vec::new();
    let mut steps = 0usize;
    const MAX_STEPS: usize = 1_000_000;
    const MAX_CALL_DEPTH: usize = 1_024;
    while pc < code.len() {
        steps += 1;
        if steps > MAX_STEPS {
            return Err("execution exceeded safety limit (1,000,000 instructions)".into());
        }
        match &code[pc] {
            Op::Push(n) => stack.push(Value::Int(*n)),
            Op::PushString(value) => stack.push(Value::Str(value.clone())),
            Op::PushBool(value) => stack.push(Value::Bool(*value)),
            Op::Load(name) => stack.push(vars.get(name).cloned().ok_or_else(|| format!("undefined variable '{name}'"))?),
            Op::Store(name) => { let value = stack.pop().ok_or("VM stack underflow")?; vars.insert(name.clone(), value); }
            Op::Print => match stack.pop().ok_or("VM stack underflow")? {
                Value::Int(n) => output(format!("{n}")),
                Value::Str(value) => output(value),
                Value::Bool(value) => output(format!("{value}")),
                value @ (Value::Array(_) | Value::Map(_)) => output(value_to_string(&value)),
            },
            Op::Neg => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                match value {
                    Value::Int(n) => stack.push(Value::Int(n.checked_neg().ok_or("integer overflow")?)),
                    Value::Bool(_) | Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: unary '-' expects an integer".into()),
                }
            }
            Op::Not => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                match value {
                    Value::Bool(value) => stack.push(Value::Bool(!value)),
                    Value::Int(_) | Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: 'not' expects a boolean".into()),
                }
            }
            Op::JumpIfFalse(target) => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                let is_false = match value {
                    Value::Bool(value) => !value,
                    Value::Int(n) => n == 0,
                    Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: condition expects a boolean or integer".into()),
                };
                if is_false { pc = *target; continue; }
            }
            Op::JumpIfFalseKeep(target) => match stack.last() {
                Some(Value::Bool(false)) => { pc = *target; continue; }
                Some(Value::Bool(true)) => {},
                Some(_) => return Err("type error: 'and' expects booleans".into()),
                None => return Err("VM stack underflow".into()),
            },
            Op::JumpIfTrueKeep(target) => match stack.last() {
                Some(Value::Bool(true)) => { pc = *target; continue; }
                Some(Value::Bool(false)) => {},
                Some(_) => return Err("type error: 'or' expects booleans".into()),
                None => return Err("VM stack underflow".into()),
            },
            Op::Jump(target) => { pc = *target; continue; }
            Op::Call(name, _) => return Err(format!("unresolved function call '{name}'")),
            Op::Builtin(name, arg_count) => {
                let mut values = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count { values.push(stack.pop().ok_or("VM stack underflow while reading builtin arguments")?); }
                values.reverse();
                stack.push(builtin(name, values)?);
            }
            Op::CallTarget { target, params, arg_count } => {
                if call_stack.len() >= MAX_CALL_DEPTH {
                    return Err(format!("maximum function call depth exceeded ({MAX_CALL_DEPTH})"));
                }
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
                    (Op::Add, Value::Str(a), Value::Str(b)) => Value::Str(format!("{a}{b}")),
                    (Op::Add, Value::Str(_), Value::Int(_)) | (Op::Add, Value::Int(_), Value::Str(_)) => return Err("type error: '+' requires two integers or two strings".into()),
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


const BYTECODE_HEADER: &str = "ZENVORA_BYTECODE 1";

fn serialize_bytecode(code: &[Op]) -> Result<String, String> {
    let mut output = String::from(BYTECODE_HEADER);
    output.push('\n');
    for op in code {
        let line = match op {
            Op::Push(n) => format!("PUSH {n}"),
            Op::PushString(value) => format!("PUSH_STRING_HEX {}", hex_encode(value)),
            Op::PushBool(value) => format!("PUSH_BOOL {value}"),
            Op::Load(name) => format!("LOAD {name}"),
            Op::Store(name) => format!("STORE {name}"),
            Op::Add => "ADD".into(),
            Op::Sub => "SUB".into(),
            Op::Mul => "MUL".into(),
            Op::Div => "DIV".into(),
            Op::Neg => "NEG".into(),
            Op::Not => "NOT".into(),
            Op::And => "AND".into(),
            Op::Or => "OR".into(),
            Op::Print => "PRINT".into(),
            Op::Eq => "EQ".into(),
            Op::Ne => "NE".into(),
            Op::Lt => "LT".into(),
            Op::Le => "LE".into(),
            Op::Gt => "GT".into(),
            Op::Ge => "GE".into(),
            Op::JumpIfFalse(target) => format!("JUMP_IF_FALSE {target}"),
            Op::JumpIfFalseKeep(target) => format!("JUMP_IF_FALSE_KEEP {target}"),
            Op::JumpIfTrueKeep(target) => format!("JUMP_IF_TRUE_KEEP {target}"),
            Op::Jump(target) => format!("JUMP {target}"),
            Op::Call(name, _) => return Err(format!("cannot serialize unresolved function call '{name}'")),
            Op::Builtin(name, count) => format!("BUILTIN {name} {count}"),
            Op::CallTarget { target, params, arg_count } => {
                let names = if params.is_empty() { "-".to_string() } else { params.join(",") };
                format!("CALL_TARGET {target} {arg_count} {names}")
            }
            Op::Return => "RETURN".into(),
            Op::ReturnValue => "RETURN_VALUE".into(),
        };
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

fn deserialize_bytecode(source: &str) -> Result<Vec<Op>, String> {
    let mut lines = source.lines();
    if lines.next() != Some(BYTECODE_HEADER) {
        return Err("invalid bytecode header or unsupported bytecode version".into());
    }
    let mut code = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line_no = index + 2;
        let parts: Vec<&str> = raw.split_whitespace().collect();
        if parts.is_empty() { continue; }
        let invalid = || format!("invalid bytecode instruction on line {line_no}");
        let op = match parts[0] {
            "PUSH" if parts.len() == 2 => Op::Push(parts[1].parse().map_err(|_| invalid())?),
            "PUSH_STRING_HEX" if parts.len() == 2 => Op::PushString(hex_decode(parts[1])?),
            "PUSH_BOOL" if parts.len() == 2 => Op::PushBool(match parts[1] {
                "true" => true,
                "false" => false,
                _ => return Err(invalid()),
            }),
            "LOAD" if parts.len() == 2 && valid_identifier(parts[1]) => Op::Load(parts[1].into()),
            "STORE" if parts.len() == 2 && valid_identifier(parts[1]) => Op::Store(parts[1].into()),
            "ADD" if parts.len() == 1 => Op::Add,
            "SUB" if parts.len() == 1 => Op::Sub,
            "MUL" if parts.len() == 1 => Op::Mul,
            "DIV" if parts.len() == 1 => Op::Div,
            "NEG" if parts.len() == 1 => Op::Neg,
            "NOT" if parts.len() == 1 => Op::Not,
            "AND" if parts.len() == 1 => Op::And,
            "OR" if parts.len() == 1 => Op::Or,
            "PRINT" if parts.len() == 1 => Op::Print,
            "EQ" if parts.len() == 1 => Op::Eq,
            "NE" if parts.len() == 1 => Op::Ne,
            "LT" if parts.len() == 1 => Op::Lt,
            "LE" if parts.len() == 1 => Op::Le,
            "GT" if parts.len() == 1 => Op::Gt,
            "GE" if parts.len() == 1 => Op::Ge,
            "JUMP_IF_FALSE" if parts.len() == 2 => Op::JumpIfFalse(parts[1].parse().map_err(|_| invalid())?),
            "JUMP_IF_FALSE_KEEP" if parts.len() == 2 => Op::JumpIfFalseKeep(parts[1].parse().map_err(|_| invalid())?),
            "JUMP_IF_TRUE_KEEP" if parts.len() == 2 => Op::JumpIfTrueKeep(parts[1].parse().map_err(|_| invalid())?),
            "JUMP" if parts.len() == 2 => Op::Jump(parts[1].parse().map_err(|_| invalid())?),
            "BUILTIN" if parts.len() == 3 && is_builtin(parts[1]) => Op::Builtin(parts[1].into(), parts[2].parse().map_err(|_| invalid())?),
            "CALL_TARGET" if parts.len() == 4 => {
                let target: usize = parts[1].parse().map_err(|_| invalid())?;
                let arg_count: usize = parts[2].parse().map_err(|_| invalid())?;
                let params: Vec<String> = if parts[3] == "-" { Vec::new() } else { parts[3].split(',').map(str::to_string).collect() };
                if arg_count != params.len() || params.iter().any(|name| !valid_identifier(name)) {
                    return Err(invalid());
                }
                Op::CallTarget { target, params, arg_count }
            }
            "RETURN" if parts.len() == 1 => Op::Return,
            "RETURN_VALUE" if parts.len() == 1 => Op::ReturnValue,
            _ => return Err(invalid()),
        };
        code.push(op);
    }
    for (index, op) in code.iter().enumerate() {
        match op {
            Op::Jump(target) | Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) if *target > code.len() =>
                return Err(format!("bytecode jump at instruction {index} targets invalid instruction {target}")),
            Op::CallTarget { target, .. } if *target >= code.len() =>
                return Err(format!("bytecode call at instruction {index} targets invalid function entry {target}")),
            _ => {}
        }
    }
    Ok(code)
}


fn hex_encode(input: &str) -> String {
    if input.is_empty() { return "-".into(); }
    input.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(input: &str) -> Result<String, String> {
    if input == "-" { return Ok(String::new()); }
    if input.len() % 2 != 0 {
        return Err("invalid hex string in bytecode".into());
    }
    let mut bytes = Vec::with_capacity(input.len() / 2);
    for pair in input.as_bytes().chunks_exact(2) {
        let digits = std::str::from_utf8(pair).map_err(|_| "invalid hex string in bytecode")?;
        bytes.push(u8::from_str_radix(digits, 16).map_err(|_| "invalid hex string in bytecode")?);
    }
    String::from_utf8(bytes).map_err(|_| "bytecode string is not valid UTF-8".into())
}

fn valid_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().enumerate().all(|(index, ch)| {
        ch == '_' || ch.is_ascii_alphanumeric() && (index > 0 || !ch.is_ascii_digit())
    })
}

fn read_source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))
}

fn write_bytecode(source_path: &str, output_path: &str) -> Result<(), String> {
    let source = read_source(source_path)?;
    let code = compile(&source)?;
    let serialized = serialize_bytecode(&code)?;
    fs::write(output_path, serialized).map_err(|error| format!("cannot write {output_path}: {error}"))
}

fn run_bytecode(path: &str) -> Result<(), String> {
    let serialized = read_source(path)?;
    let code = deserialize_bytecode(&serialized)?;
    run(&code)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("Zenvora Lang v0.6.1");
        return;
    }
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("Zenvora Lang v0.6.1\nUsage: zv <file.zv>\n       zv --emit-bytecode <file.zv> <output.zvb>\n       zv --run-bytecode <file.zvb>\n\nSyntax: define total := 10 + 20\n        show total * 2\n        if total >= 18 {{ ... }}");
        return;
    }
    let result = match args.as_slice() {
        [_, mode, source, output] if mode == "--emit-bytecode" => write_bytecode(source, output),
        [_, mode, bytecode] if mode == "--run-bytecode" => run_bytecode(bytecode),
        [_, path] if !path.starts_with("--") => {
            let source = read_source(path).unwrap_or_else(|error| {
                eprintln!("Zenvora error: {error}");
                process::exit(1)
            });
            compile(&source).and_then(|code| run(&code))
        }
        _ => {
            eprintln!("Usage: zv <file.zv> | zv --emit-bytecode <file.zv> <output.zvb> | zv --run-bytecode <file.zvb>");
            process::exit(2);
        }
    };
    if let Err(error) = result {
        eprintln!("Zenvora error: {error}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{compile, compile_core, run_with_output, Op};

    fn execute(source: &str) -> Result<Vec<String>, String> {
        let code = compile(source)?;
        let mut output = Vec::new();
        run_with_output(&code, &mut |line| output.push(line))?;
        Ok(output)
    }

    #[test]
    fn logical_and_or_short_circuit_the_right_operand() {
        let output = execute("show false and (1 / 0 == 0)\nshow true or (1 / 0 == 0)\nshow true and true\nshow false or true\nshow skip_and()\nshow skip_or()\nfn skip_and() {\nreturn false and (1 / 0 == 0)\n}\nfn skip_or() {\nreturn true or (1 / 0 == 0)\n}\n").unwrap();
        assert_eq!(output, vec!["false", "true", "true", "true", "false", "true"]);
    }

    #[test]
    fn short_circuit_jumps_work_in_repeat_conditions() {
        let output = execute("define count := 0\nrepeat count < 3 and (count != 1 or true) {\ncount := count + 1\nshow count\n}\n").unwrap();
        assert_eq!(output, vec!["1", "2", "3"]);
    }

    #[test]
    fn strings_support_escapes_concatenation_and_bytecode_round_trip() {
        let source = "define message := \"hello\\nworld\"\nshow message\nshow \"Zenvora \" + \"Lang\"\nshow \"\"\nshow \"नमस्ते 🌍\"\n";
        let code = compile(source).unwrap();
        let encoded = super::serialize_bytecode(&code).unwrap();
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        assert_eq!(decoded, code);
        let mut output = Vec::new();
        run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["hello\nworld", "Zenvora Lang", "", "नमस्ते 🌍"]);
    }

    #[test]
    fn malformed_string_literals_are_reported() {
        assert!(compile_core("show \"unterminated\n").unwrap_err().contains("unterminated string literal"));
        assert!(compile_core("show \"bad\\q\"\n").unwrap_err().contains("unsupported string escape"));
    }

    #[test]
    fn functions_accept_string_arguments_with_commas() {
        let output = execute("show echo(\"hello, world\")\nfn echo(value) {\nreturn value\n}\n").unwrap();
        assert_eq!(output, vec!["hello, world"]);
    }

    #[test]
    fn strings_are_not_valid_conditions_or_numeric_operands() {
        assert!(execute("show \"text\" - 1\n").unwrap_err().contains("expects integers"));
        assert!(execute("if \"text\" {\nshow 1\n}\n").unwrap_err().contains("condition expects"));
        assert!(super::hex_decode("0g").unwrap_err().contains("invalid hex"));
    }

    #[test]
    fn bytecode_round_trip_preserves_execution() {
        let source = "define total := 20 + 22\nshow total\nif total >= 40 {\nshow true\n} otherwise {\nshow false\n}\nfn twice(n) {\nreturn n * 2\n}\nshow twice(21)\n";
        let compiled = compile(source).unwrap();
        let serialized = super::serialize_bytecode(&compiled).unwrap();
        assert!(serialized.starts_with("ZENVORA_BYTECODE 1\n"));
        let restored = super::deserialize_bytecode(&serialized).unwrap();
        assert_eq!(restored, compiled);
        let mut output = Vec::new();
        run_with_output(&restored, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["42", "true", "42"]);
    }

    #[test]
    fn bytecode_serializes_every_instruction_variant() {
        let code = vec![
            Op::Push(-7), Op::PushString("text".into()), Op::PushBool(true), Op::Load("value".into()), Op::Store("value".into()), Op::Builtin("array".into(), 2),
            Op::Add, Op::Sub, Op::Mul, Op::Div, Op::Neg, Op::Not, Op::And, Op::Or, Op::Print,
            Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge, Op::JumpIfFalse(24), Op::JumpIfFalseKeep(27), Op::JumpIfTrueKeep(27), Op::Jump(24),
            Op::CallTarget { target: 0, params: vec!["arg".into()], arg_count: 1 },
            Op::Return, Op::ReturnValue,
        ];
        let encoded = super::serialize_bytecode(&code).unwrap();
        assert_eq!(super::deserialize_bytecode(&encoded).unwrap(), code);
        assert!(super::serialize_bytecode(&[Op::Call("unresolved".into(), 0)]).unwrap_err().contains("unresolved function call"));
    }

    #[test]
    fn bytecode_rejects_unknown_instructions_and_invalid_jumps() {
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 2\n").unwrap_err().contains("unsupported bytecode version"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nMAGIC\n").unwrap_err().contains("invalid bytecode instruction"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nJUMP 99\n").unwrap_err().contains("invalid instruction"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nCALL_TARGET 9 0 -\n").unwrap_err().contains("invalid function entry"));
    }

    #[test]
    fn arithmetic_variables_and_assignment() {
        let output = execute(
            "define a := 10\ndefine b := 20\ndefine total := a + b\nshow total\nshow total * 2\ntotal := total + 5\nshow total\n"
        ).unwrap();
        assert_eq!(output, vec!["30", "60", "35"]);
    }

    #[test]
    fn booleans_comparisons_and_nested_conditions() {
        let output = execute(
            "define alive := true\ndefine ready := false\nshow not ready\nif alive and not ready {\n if 2 < 3 {\n show 7\n } otherwise {\n show 8\n }\n} otherwise {\n show 9\n}\nshow 2 < 3 or 5 < 4\n"
        ).unwrap();
        assert_eq!(output, vec!["true", "7", "true"]);
    }

    #[test]
    fn repeat_break_and_continue() {
        let output = execute(
            "define count := 0\nrepeat count < 5 {\n count := count + 1\n if count == 2 {\n continue\n }\n show count\n if count == 4 {\n break\n }\n}\n"
        ).unwrap();
        assert_eq!(output, vec!["1", "3", "4"]);
    }

    #[test]
    fn nested_loop_break_only_exits_nearest_loop() {
        let output = execute(
            "define i := 0\nrepeat i < 3 {\n i := i + 1\n define j := 0\n repeat j < 4 {\n j := j + 1\n if j == 2 {\n break\n }\n show i * 10 + j\n }\n}\n"
        ).unwrap();
        assert_eq!(output, vec!["11", "21", "31"]);
    }

    #[test]
    fn functions_parameters_returns_and_local_scope() {
        let output = execute(
            "show add(20, 22)\ncall greet()\nfn add(left, right) {\n return left + right\n}\nfn greet() {\n show 2026\n}\n"
        ).unwrap();
        assert_eq!(output, vec!["42", "2026"]);
    }

    #[test]
    fn recursive_function_calls() {
        let output = execute(
            "show factorial(5)\nfn factorial(n) {\n if n <= 1 {\n return 1\n } otherwise {\n return n * factorial(n - 1)\n }\n}\n"
        ).unwrap();
        assert_eq!(output, vec!["120"]);
    }

    #[test]
    fn recursive_calls_stop_at_the_call_depth_limit() {
        let error = execute("call recurse()\nfn recurse() {\ncall recurse()\n}\n").unwrap_err();
        assert!(error.contains("maximum function call depth exceeded"));
    }

    #[test]
    fn invalid_assignment_names_are_rejected() {
        for source in [" := 5\n", "1bad := 5\n", "bad-name := 5\n"] {
            assert!(compile_core(source).unwrap_err().contains("invalid variable name"));
        }
    }

    #[test]
    fn invalid_loop_control_is_rejected() {
        assert!(compile_core("break\n").unwrap_err().contains("outside a repeat loop"));
        assert!(compile_core("continue\n").unwrap_err().contains("outside a repeat loop"));
    }

    #[test]
    fn undefined_function_and_wrong_argument_count_are_rejected() {
        assert!(compile("call missing()\n").unwrap_err().contains("undefined function"));
        assert!(compile("show add(1)\nfn add(a, b) {\n return a + b\n}\n").unwrap_err().contains("expects 2 argument"));
    }

    #[test]
    fn runtime_errors_are_reported_instead_of_panicking() {
        assert!(execute("show 1 / 0\n").unwrap_err().contains("division by zero"));
        assert!(execute("show missing\n").unwrap_err().contains("undefined variable"));
        assert!(execute("show 9223372036854775807 + 1\n").unwrap_err().contains("integer overflow"));
    }

    #[test]
    fn arrays_and_maps_support_construction_access_updates_and_bytecode() {
        let source = "define numbers := array(10, 20, 30)\nshow len(numbers)\nshow get(numbers, 1)\ndefine grown := push(numbers, 40)\nshow get(grown, 3)\nshow contains(grown, 20)\ndefine profile := map(\"name\", \"Zenvora\", \"level\", 6)\nshow get(profile, \"name\")\nshow get(put(profile, \"level\", 7), \"level\")\nshow len(keys(profile))\nshow get(values(profile), 1)\n";
        let code = compile(source).unwrap();
        let encoded = super::serialize_bytecode(&code).unwrap();
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        let mut output = Vec::new();
        run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["3", "20", "40", "true", "Zenvora", "7", "2", "Zenvora"]);
    }

    #[test]
    fn collections_report_invalid_types_and_indexes() {
        assert!(execute("show get(array(1), 4)\n").unwrap_err().contains("out of bounds"));
        assert!(execute("show get(map(\"x\", 1), \"missing\")\n").unwrap_err().contains("not found"));
        assert!(execute("show map(\"x\")\n").unwrap_err().contains("alternating string keys"));
        assert!(execute("show map(1, 2)\n").unwrap_err().contains("keys must be strings"));
        assert!(execute("show len(true)\n").unwrap_err().contains("len expects"));
    }

    #[test]
    fn function_brace_count_ignores_braces_in_strings_and_comments() {
        let output = execute(
            "show render()\nfn render() {\n # these braces are only comment text: } {{{\n show \"left { brace\"\n show \"right } brace\"\n return \"done { }\"\n}\n"
        ).unwrap();
        assert_eq!(output, vec!["left { brace", "right } brace", "done { }"]);
    }

    #[test]
    fn unmatched_braces_are_rejected() {
        assert!(compile_core("if true {\n show 1\n").unwrap_err().contains("missing closing brace"));
        assert!(compile_core("}\n").unwrap_err().contains("closing brace without matching block"));
    }
}

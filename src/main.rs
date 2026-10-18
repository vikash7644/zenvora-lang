use std::collections::{HashMap, HashSet};
use std::{env, fs, process};
use std::path::Path;

mod module_loader;
mod chain_runtime;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64), String(String), Bool(bool), Ident(String), And, Or, Not, Plus, Minus, Star, Slash,
    LParen, RParen, LBracket, RBracket, Comma, EqEq, NotEq, Less, LessEq, Greater, GreaterEq, End,
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
            if value.len() > MAX_BYTECODE_STRING_BYTES {
                return Err(format!("string literal exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
            }
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
            '(' => Token::LParen, ')' => Token::RParen, '[' => Token::LBracket, ']' => Token::RBracket, ',' => Token::Comma,
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
    Push(i64), PushString(String), PushBool(bool), Load(String), Store(String), Declare(String), Add, Sub, Mul, Div, Neg, Not, And, Or, Print,
    Eq, Ne, Lt, Le, Gt, Ge, Index, SetIndex(String), JumpIfFalse(usize), JumpIfFalseKeep(usize), JumpIfTrueKeep(usize), Jump(usize), Call(String, usize), Builtin(String, usize), CallTarget { target: usize, params: Vec<String>, arg_count: usize }, Return, ReturnValue,
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
        while self.peek() == &Token::LBracket {
            self.take();
            self.expression()?;
            if self.take() != Token::RBracket { return Err("expected ']' after index expression".into()); }
            self.code.push(Op::Index);
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

        if !line.starts_with("define ") {
            if let Some((left, right)) = line.split_once(":=") {
                let left = left.trim();
                if left.contains('[') {
                    let (name, index_part) = left.split_once('[').ok_or_else(|| format!("line {}: invalid indexed assignment", line_no + 1))?;
                    let index_expr = index_part.strip_suffix(']').ok_or_else(|| format!("line {}: expected ']' in indexed assignment", line_no + 1))?;
                    if index_expr.contains('[') || index_expr.contains(']') || !valid_identifier(name.trim()) {
                        return Err(format!("line {}: indexed assignment currently requires a variable and one index, such as items[0] := value", line_no + 1));
                    }
                    if right.trim().is_empty() {
                        return Err(format!("line {}: missing value in indexed assignment", line_no + 1));
                    }
                    let index_tokens = lex(index_expr.trim()).map_err(|e| format!("line {}: invalid index: {e}", line_no + 1))?;
                    let mut index_parser = Parser { tokens: index_tokens, pos: 0, code: Vec::new() };
                    index_parser.expression().map_err(|e| format!("line {}: invalid index: {e}", line_no + 1))?;
                    if index_parser.peek() != &Token::End {
                        return Err(format!("line {}: unexpected token in index {:?}", line_no + 1, index_parser.peek()));
                    }
                    append_code(&mut code, index_parser.code);
                    let value_tokens = lex(right.trim()).map_err(|e| format!("line {}: {e}", line_no + 1))?;
                    let mut value_parser = Parser { tokens: value_tokens, pos: 0, code: Vec::new() };
                    value_parser.expression().map_err(|e| format!("line {}: {e}", line_no + 1))?;
                    if value_parser.peek() != &Token::End {
                        return Err(format!("line {}: unexpected token in indexed assignment {:?}", line_no + 1, value_parser.peek()));
                    }
                    append_code(&mut code, value_parser.code);
                    code.push(Op::SetIndex(name.trim().to_string()));
                    continue;
                }
            }
        }

        let is_declaration = line.starts_with("define ");
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
        if statement.2 { code.push(Op::Print); } else if is_declaration { code.push(Op::Declare(statement.0)); } else { code.push(Op::Store(statement.0)); }
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


// Lowers the first-generation contract DSL into the existing compiler's
// global declarations and functions. This is a local language prototype,
// not a persistent blockchain contract runtime.
fn lower_contracts(source: &str) -> Result<String, String> {
    let lines: Vec<&str> = source.lines().collect();
    let state_names: HashSet<String> = lines.iter().filter_map(|raw| {
        let rest = raw.trim().strip_prefix("state ")?;
        let name = rest.split_once(":=")?.0.trim();
        if valid_identifier(name) { Some(name.to_string()) } else { None }
    }).collect();
    let mut output = String::new();
    let mut index = 0usize;
    let mut contract: Option<(String, usize)> = None;
    let mut saw_contract = false;

    while index < lines.len() {
        let line = lines[index].trim();
        if contract.is_none() {
            if let Some(name) = line.strip_prefix("contract ").and_then(|s| s.strip_suffix('{')) {
                let name = name.trim();
                if !valid_identifier(name) {
                    return Err(format!("line {}: invalid contract name", index + 1));
                }
                if saw_contract {
                    return Err(format!("line {}: only one contract block per source file is currently supported", index + 1));
                }
                saw_contract = true;
                contract = Some((name.to_string(), index + 1));
                index += 1;
                continue;
            }
            output.push_str(lines[index]);
            output.push('\n');
            index += 1;
            continue;
        }

        if line.is_empty() || line.starts_with('#') {
            output.push_str(lines[index]);
            output.push('\n');
            index += 1;
            continue;
        }

        if line == "}" {
            contract = None;
            index += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("state ") {
            let (name, expression) = rest.split_once(":=").ok_or_else(|| {
                format!("line {}: expected 'state name := expression'", index + 1)
            })?;
            let name = name.trim();
            let expression = expression.trim().trim_end_matches(';').trim();
            if !valid_identifier(name) {
                return Err(format!("line {}: invalid state name", index + 1));
            }
            if expression.is_empty() {
                return Err(format!("line {}: missing state initializer", index + 1));
            }
            output.push_str(&format!("define {name} := {expression}\n"));
            index += 1;
            continue;
        }

        let member = line.strip_prefix("action ").map(|s| ("action", s))
            .or_else(|| line.strip_prefix("view ").map(|s| ("view", s)));
        if let Some((kind, header)) = member.and_then(|(kind, text)| text.strip_suffix('{').map(|h| (kind, h.trim()))) {
            if !header.contains('(') || !header.ends_with(')') {
                return Err(format!("line {}: expected '{kind} function_name(parameters) {{'", index + 1));
            }
            output.push_str(&format!("fn {header} {{\n"));
            index += 1;
            let mut depth = 1i32;
            while index < lines.len() && depth > 0 {
                let current = lines[index];
                if kind == "view" {
                    let statement = current.trim();
                    if !statement.starts_with('#') {
                        if let Some((left, _)) = statement.split_once(":=") {
                            let base = left.trim().split('[').next().unwrap_or(left.trim()).trim();
                            if state_names.contains(base) {
                                return Err(format!("line {}: view '{header}' cannot directly assign to contract state '{base}'", index + 1));
                            }
                        }
                    }
                }
                let (opens, closes) = count_block_braces(current);
                depth += opens - closes;
                if depth > 0 {
                    output.push_str(current);
                    output.push('\n');
                } else {
                    output.push_str("}\n");
                }
                index += 1;
            }
            if depth != 0 {
                return Err(format!("line {}: missing closing brace for {kind} '{header}'", index));
            }
            continue;
        }

        return Err(format!(
            "line {}: expected state declaration, action, view, or closing contract brace",
            index + 1
        ));
    }

    if let Some((name, line)) = contract {
        return Err(format!("line {line}: contract '{name}' is missing its closing brace"));
    }
    Ok(output)
}

fn compile(source: &str) -> Result<Vec<Op>, String> {
    let lowered_source = lower_contracts(source)?;
    let source = lowered_source.as_str();
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

fn is_builtin(name: &str) -> bool { matches!(name, "array" | "map" | "get" | "len" | "contains" | "push" | "put" | "keys" | "values" | "type_of" | "to_string" | "abs" | "min" | "max" | "range" | "assert" | "sum" | "starts_with" | "ends_with" | "split" | "join" | "replace" | "first" | "last" | "reverse" | "is_empty" | "clamp") }

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
            if !args.len().is_multiple_of(2) { return Err("builtin 'map' expects alternating string keys and values".into()); }
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
        "type_of" => {
            arity(1)?;
            let kind = match &args[0] {
                Value::Int(_) => "integer",
                Value::Str(_) => "string",
                Value::Bool(_) => "boolean",
                Value::Array(_) => "array",
                Value::Map(_) => "map",
            };
            Ok(Value::Str(kind.into()))
        }
        "to_string" => {
            arity(1)?;
            Ok(Value::Str(value_to_string(&args[0])))
        }
        "abs" => {
            arity(1)?;
            match args[0] {
                Value::Int(value) => Ok(Value::Int(value.checked_abs().ok_or("integer overflow in abs")?)),
                _ => Err("type error: abs expects an integer".into()),
            }
        }
        "min" | "max" => {
            arity(2)?;
            match (&args[0], &args[1]) {
                (Value::Int(left), Value::Int(right)) => Ok(Value::Int(if name == "min" { (*left).min(*right) } else { (*left).max(*right) })),
                _ => Err(format!("type error: {name} expects two integers")),
            }
        }
        "range" => {
            arity(2)?;
            let (Value::Int(start), Value::Int(end)) = (&args[0], &args[1]) else {
                return Err("type error: range expects two integers".into());
            };
            let length = end.checked_sub(*start).ok_or("range bounds overflow")?;
            if length < 0 { return Err("range end must be greater than or equal to start".into()); }
            const MAX_RANGE_ITEMS: i64 = 100_000;
            if length > MAX_RANGE_ITEMS { return Err(format!("range exceeds safety limit ({MAX_RANGE_ITEMS} items)")); }
            let mut items = Vec::with_capacity(length as usize);
            let mut current = *start;
            while current < *end {
                items.push(Value::Int(current));
                current = current.checked_add(1).ok_or("integer overflow while building range")?;
            }
            Ok(Value::Array(items))
        }
        "first" | "last" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else { return Err(format!("type error: {name} expects an array")); };
            if items.is_empty() { return Err(format!("{name} cannot be used with an empty array")); }
            Ok(if name == "first" { items[0].clone() } else { items[items.len() - 1].clone() })
        }
        "reverse" => {
            arity(1)?;
            match &args[0] {
                Value::Array(items) => Ok(Value::Array(items.iter().rev().cloned().collect())),
                Value::Str(value) => Ok(Value::Str(value.chars().rev().collect())),
                _ => Err("type error: reverse expects an array or string".into()),
            }
        }
        "is_empty" => {
            arity(1)?;
            let empty = match &args[0] {
                Value::Array(items) => items.is_empty(),
                Value::Map(items) => items.is_empty(),
                Value::Str(value) => value.is_empty(),
                _ => return Err("type error: is_empty expects an array, map, or string".into()),
            };
            Ok(Value::Bool(empty))
        }
        "clamp" => {
            arity(3)?;
            let (Value::Int(value), Value::Int(minimum), Value::Int(maximum)) = (&args[0], &args[1], &args[2]) else {
                return Err("type error: clamp expects three integers".into());
            };
            if minimum > maximum { return Err("clamp minimum must be less than or equal to maximum".into()); }
            Ok(Value::Int((*value).clamp(*minimum, *maximum)))
        }
        "sum" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else { return Err("type error: sum expects an array of integers".into()); };
            let mut total = 0i64;
            for item in items {
                let Value::Int(value) = item else { return Err("type error: sum expects an array containing only integers".into()); };
                total = total.checked_add(*value).ok_or("integer overflow in sum")?;
            }
            Ok(Value::Int(total))
        }
        "starts_with" | "ends_with" => {
            arity(2)?;
            let (Value::Str(value), Value::Str(pattern)) = (&args[0], &args[1]) else {
                return Err(format!("type error: {name} expects two strings"));
            };
            Ok(Value::Bool(if name == "starts_with" { value.starts_with(pattern) } else { value.ends_with(pattern) }))
        }
        "replace" => {
            arity(3)?;
            let (Value::Str(value), Value::Str(from), Value::Str(to)) = (&args[0], &args[1], &args[2]) else {
                return Err("type error: replace expects three strings".into());
            };
            if from.is_empty() { return Err("replace search string cannot be empty".into()); }
            let matches = value.matches(from).count();
            let removed = matches.checked_mul(from.len()).ok_or("replace size calculation overflow")?;
            let added = matches.checked_mul(to.len()).ok_or("replace size calculation overflow")?;
            let result_len = value.len().checked_sub(removed).and_then(|len| len.checked_add(added))
                .ok_or("replace result size calculation overflow")?;
            if result_len > MAX_BYTECODE_STRING_BYTES {
                return Err(format!("replace result exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
            }
            Ok(Value::Str(value.replace(from, to)))
        }
        "split" => {
            arity(2)?;
            let (Value::Str(value), Value::Str(separator)) = (&args[0], &args[1]) else {
                return Err("type error: split expects two strings".into());
            };
            if separator.is_empty() { return Err("split separator cannot be empty".into()); }
            const MAX_SPLIT_ITEMS: usize = 100_000;
            let mut parts = Vec::new();
            for part in value.split(separator) {
                if parts.len() >= MAX_SPLIT_ITEMS {
                    return Err(format!("split exceeds safety limit ({MAX_SPLIT_ITEMS} items)"));
                }
                parts.push(Value::Str(part.to_string()));
            }
            Ok(Value::Array(parts))
        }
        "join" => {
            arity(2)?;
            let (Value::Array(items), Value::Str(separator)) = (&args[0], &args[1]) else {
                return Err("type error: join expects an array of strings and a string separator".into());
            };
            let mut parts = Vec::with_capacity(items.len());
            let mut result_len = 0usize;
            for item in items {
                let Value::Str(value) = item else { return Err("type error: join expects an array containing only strings".into()); };
                result_len = result_len.checked_add(value.len()).ok_or("join result size calculation overflow")?;
                parts.push(value.as_str());
            }
            if parts.len() > 1 {
                result_len = result_len.checked_add(separator.len().checked_mul(parts.len() - 1)
                    .ok_or("join result size calculation overflow")?).ok_or("join result size calculation overflow")?;
            }
            if result_len > MAX_BYTECODE_STRING_BYTES {
                return Err(format!("join result exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
            }
            Ok(Value::Str(parts.join(separator)))
        }
        "assert" => {
            if !(1..=2).contains(&arg_len) {
                return Err(format!("builtin 'assert' expects 1 or 2 argument(s), got {arg_len}"));
            }
            match &args[0] {
                Value::Bool(true) => Ok(Value::Bool(true)),
                Value::Bool(false) => {
                    let message = if args.len() == 2 { value_to_string(&args[1]) } else { "assertion failed".into() };
                    Err(message)
                }
                _ => Err("type error: assert expects a boolean condition".into()),
            }
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
    let mut globals: HashMap<String, Value> = HashMap::new();
    let mut pc = 0usize;
    let mut call_stack: Vec<(usize, HashMap<String, Value>)> = Vec::new();
    let mut steps = 0usize;
    const MAX_STEPS: usize = 1_000_000;
    const MAX_CALL_DEPTH: usize = 1_024;
    if code.len() > MAX_BYTECODE_INSTRUCTIONS {
        return Err(format!("program instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})"));
    }
    while pc < code.len() {
        steps += 1;
        if steps > MAX_STEPS {
            return Err("execution exceeded safety limit (1,000,000 instructions)".into());
        }
        match &code[pc] {
            Op::Push(n) => stack.push(Value::Int(*n)),
            Op::PushString(value) => stack.push(Value::Str(value.clone())),
            Op::PushBool(value) => stack.push(Value::Bool(*value)),
            Op::Load(name) => stack.push(vars.get(name).or_else(|| globals.get(name)).cloned().ok_or_else(|| format!("undefined variable '{name}'"))?),
            Op::Store(name) => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                if vars.contains_key(name) { vars.insert(name.clone(), value); }
                else if globals.contains_key(name) { globals.insert(name.clone(), value); }
                else { return Err(format!("assignment to undefined variable '{name}'; use 'define' first")); }
            }
            Op::Declare(name) => {
                let value = stack.pop().ok_or("VM stack underflow")?;
                if call_stack.is_empty() { globals.insert(name.clone(), value); }
                else { vars.insert(name.clone(), value); }
            }
            Op::Print => match stack.pop().ok_or("VM stack underflow")? {
                Value::Int(n) => output(format!("{n}")),
                Value::Str(value) => output(value),
                Value::Bool(value) => output(format!("{value}")),
                value @ (Value::Array(_) | Value::Map(_)) => output(value_to_string(&value)),
            },
            Op::SetIndex(name) => {
                let value = stack.pop().ok_or("VM stack underflow while reading indexed value")?;
                let index = stack.pop().ok_or("VM stack underflow while reading collection index")?;
                let collection = vars.get(name).or_else(|| globals.get(name)).cloned()
                    .ok_or_else(|| format!("undefined variable '{name}'"))?;
                let updated = match (collection, index) {
                    (Value::Array(mut items), Value::Int(index)) if index >= 0 => {
                        let slot = items.get_mut(index as usize).ok_or_else(|| format!("array index {index} out of bounds"))?;
                        *slot = value;
                        Value::Array(items)
                    }
                    (Value::Array(_), Value::Int(index)) => return Err(format!("array index {index} out of bounds")),
                    (Value::Array(_), _) => return Err("type error: array index must be an integer".into()),
                    (Value::Map(mut items), Value::Str(key)) => {
                        items.insert(key, value);
                        Value::Map(items)
                    }
                    (Value::Map(_), _) => return Err("type error: map index must be a string".into()),
                    _ => return Err("type error: indexed assignment expects an array or map".into()),
                };
                if vars.contains_key(name) { vars.insert(name.clone(), updated); }
                else { globals.insert(name.clone(), updated); }
            }
            Op::Index => {
                let index = stack.pop().ok_or("VM stack underflow while reading collection index")?;
                let collection = stack.pop().ok_or("VM stack underflow while reading collection")?;
                match (collection, index) {
                    (Value::Array(items), Value::Int(index)) if index >= 0 => stack.push(items.get(index as usize).cloned().ok_or_else(|| format!("array index {index} out of bounds"))?),
                    (Value::Array(_), Value::Int(index)) => return Err(format!("array index {index} out of bounds")),
                    (Value::Array(_), _) => return Err("type error: array index must be an integer".into()),
                    (Value::Map(items), Value::Str(key)) => stack.push(items.get(&key).cloned().ok_or_else(|| format!("map key '{key}' not found"))?),
                    (Value::Map(_), _) => return Err("type error: map index must be a string".into()),
                    _ => return Err("type error: indexing expects an array or map".into()),
                }
            }
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
                if *arg_count > MAX_CALL_ARGUMENTS {
                    return Err(format!("builtin argument count exceeds safety limit ({MAX_CALL_ARGUMENTS})"));
                }
                let mut values = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count { values.push(stack.pop().ok_or("VM stack underflow while reading builtin arguments")?); }
                values.reverse();
                stack.push(builtin(name, values)?);
            }
            Op::CallTarget { target, params, arg_count } => {
                if *arg_count > MAX_CALL_ARGUMENTS || params.len() > MAX_CALL_ARGUMENTS {
                    return Err(format!("function argument count exceeds safety limit ({MAX_CALL_ARGUMENTS})"));
                }
                if *arg_count != params.len() {
                    return Err("invalid function call argument count".into());
                }
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
                    (Op::Add, Value::Str(a), Value::Str(b)) => {
                        let combined_len = a.len().checked_add(b.len()).ok_or("string length overflow")?;
                        if combined_len > MAX_BYTECODE_STRING_BYTES {
                            return Err(format!("string exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
                        }
                        Value::Str(format!("{a}{b}"))
                    },
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
        if stack.len() > MAX_STACK_VALUES {
            return Err(format!("operand stack exceeds safety limit ({MAX_STACK_VALUES} values)"));
        }
        pc += 1;
    }
    Ok(())
}


const BYTECODE_HEADER: &str = "ZENVORA_BYTECODE 1";
const MAX_BYTECODE_BYTES: usize = 16 * 1024 * 1024;
const MAX_BYTECODE_INSTRUCTIONS: usize = 1_000_000;
const MAX_BYTECODE_STRING_BYTES: usize = 1024 * 1024;
const MAX_CALL_ARGUMENTS: usize = 1024;
const MAX_STACK_VALUES: usize = 65_536;

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
            Op::Declare(name) => format!("DECLARE {name}"),
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
            Op::Index => "INDEX".into(),
            Op::SetIndex(name) => format!("SET_INDEX {name}"),
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


// Verify bytecode control flow before it is executed. Stack depths are tracked
// independently for the main program and each declared function region.
fn verify_bytecode(code: &[Op]) -> Result<(), String> {
    if code.is_empty() {
        return Ok(());
    }
    if code.len() > MAX_BYTECODE_INSTRUCTIONS {
        return Err(format!("bytecode instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})"));
    }

    let main_exit = code.iter().position(|op| matches!(op, Op::Jump(target) if *target == code.len()))
        .ok_or("bytecode verifier: missing main-program exit jump")?;

    let mut entries: Vec<usize> = code.iter().filter_map(|op| {
        if let Op::CallTarget { target, .. } = op { Some(*target) } else { None }
    }).collect();
    entries.sort_unstable();
    entries.dedup();

    for entry in &entries {
        if *entry == 0 || *entry <= main_exit || *entry >= code.len() {
            return Err(format!("bytecode verifier: invalid function entry point {entry}"));
        }
    }
    for (index, op) in code.iter().enumerate() {
        match op {
            Op::CallTarget { target, .. } if entries.binary_search(target).is_err() =>
                return Err(format!("bytecode verifier: call at instruction {index} does not target a function entry")),
            Op::Jump(target) | Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target)
                if *target > code.len() =>
                return Err(format!("bytecode verifier: jump at instruction {index} has invalid target {target}")),
            _ => {}
        }
    }

    // A call's static stack effect depends on whether its target can return a
    // value. This first-generation format does not encode return types.
    let returns_value: HashSet<usize> = entries.iter().copied().filter(|entry| {
        let end = entries.iter().copied().find(|candidate| candidate > entry).unwrap_or(code.len());
        code[*entry..end].iter().any(|op| matches!(op, Op::ReturnValue))
    }).collect();

    let mut regions: Vec<(usize, usize, bool)> = vec![(0, main_exit + 1, true)];
    for (i, entry) in entries.iter().copied().enumerate() {
        let end = entries.get(i + 1).copied().unwrap_or(code.len());
        regions.push((entry, end, false));
    }

    for (start, end, is_main) in regions {
        if start >= end { continue; }
        let mut depths: HashMap<usize, usize> = HashMap::new();
        let mut pending = vec![(start, 0usize)];
        while let Some((pc, incoming_depth)) = pending.pop() {
            if pc == end {
                if !is_main {
                    return Err(format!("bytecode verifier: function control flow falls through its region ending at instruction {end}"));
                }
                continue;
            }
            if pc < start || pc > end || pc >= code.len() {
                return Err(format!("bytecode verifier: control flow escapes region at instruction {pc}"));
            }
            if let Some(previous) = depths.get(&pc) {
                if *previous != incoming_depth {
                    return Err(format!("bytecode verifier: inconsistent stack depth at instruction {pc} ({previous} vs {incoming_depth})"));
                }
                continue;
            }
            depths.insert(pc, incoming_depth);

            let op = &code[pc];
            let (required, delta): (usize, isize) = match op {
                Op::Push(_) | Op::PushString(_) | Op::PushBool(_) | Op::Load(_) => (0, 1),
                Op::Store(_) | Op::Declare(_) | Op::Print | Op::Neg | Op::Not | Op::ReturnValue | Op::JumpIfFalse(_) => (1, -1),
                Op::SetIndex(_) => (2, -2),
                Op::Index => (2, -1),
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::And | Op::Or | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => (2, -1),
                Op::JumpIfFalseKeep(_) | Op::JumpIfTrueKeep(_) => (1, 0),
                Op::Jump(_) | Op::Return => (0, 0),
                Op::Builtin(_, count) => (*count, 1 - *count as isize),
                Op::CallTarget { target, arg_count, .. } => (*arg_count, -(*arg_count as isize) + if returns_value.contains(target) { 1 } else { 0 }),
                Op::Call(name, _) => return Err(format!("bytecode verifier: unresolved call '{name}' at instruction {pc}")),
            };
            if incoming_depth < required {
                return Err(format!("bytecode verifier: operand stack underflow at instruction {pc}"));
            }
            let outgoing_depth = if delta < 0 {
                incoming_depth - (-delta) as usize
            } else {
                incoming_depth.checked_add(delta as usize).ok_or("bytecode verifier: stack depth overflow")?
            };
            if outgoing_depth > MAX_STACK_VALUES {
                return Err(format!("bytecode verifier: operand stack exceeds safety limit at instruction {pc}"));
            }

            let mut enqueue = |target: usize, depth: usize| -> Result<(), String> {
                if target == code.len() && is_main { return Ok(()); }
                if target < start || target >= end {
                    return Err(format!("bytecode verifier: jump at instruction {pc} escapes its code region to {target}"));
                }
                pending.push((target, depth));
                Ok(())
            };
            match op {
                Op::Jump(target) => {
                    if is_main && pc == main_exit && *target == code.len() {
                        if outgoing_depth != 0 { return Err(format!("bytecode verifier: main program exits with {outgoing_depth} value(s) left on the operand stack")); }
                        continue;
                    }
                    enqueue(*target, outgoing_depth)?;
                }
                Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) => {
                    enqueue(*target, outgoing_depth)?;
                    if pc + 1 < end { pending.push((pc + 1, outgoing_depth)); }
                    else if !is_main { return Err(format!("bytecode verifier: function falls through at instruction {pc}")); }
                }
                Op::Return | Op::ReturnValue => {
                    if is_main {
                        return Err(format!("bytecode verifier: return instruction outside a function at {pc}"));
                    }
                }
                Op::CallTarget { .. } => {
                    if pc + 1 < end { pending.push((pc + 1, outgoing_depth)); }
                    else if !is_main { return Err(format!("bytecode verifier: function falls through at instruction {pc}")); }
                }
                _ => {
                    if pc + 1 < end { pending.push((pc + 1, outgoing_depth)); }
                    else if is_main {
                        return Err(format!("bytecode verifier: main program falls through at instruction {pc}"));
                    } else {
                        return Err(format!("bytecode verifier: function falls through at instruction {pc}"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn deserialize_bytecode(source: &str) -> Result<Vec<Op>, String> {
    if source.len() > MAX_BYTECODE_BYTES {
        return Err(format!("bytecode file exceeds safety limit ({} bytes)", MAX_BYTECODE_BYTES));
    }
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
            "PUSH_STRING_HEX" if parts.len() == 2 => {
                if parts[1] != "-" && parts[1].len() / 2 > MAX_BYTECODE_STRING_BYTES {
                    return Err(format!("bytecode string exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
                }
                Op::PushString(hex_decode(parts[1])?)
            },
            "PUSH_BOOL" if parts.len() == 2 => Op::PushBool(match parts[1] {
                "true" => true,
                "false" => false,
                _ => return Err(invalid()),
            }),
            "LOAD" if parts.len() == 2 && valid_identifier(parts[1]) => Op::Load(parts[1].into()),
            "STORE" if parts.len() == 2 && valid_identifier(parts[1]) => Op::Store(parts[1].into()),
            "DECLARE" if parts.len() == 2 && valid_identifier(parts[1]) => Op::Declare(parts[1].into()),
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
            "INDEX" if parts.len() == 1 => Op::Index,
            "SET_INDEX" if parts.len() == 2 && valid_identifier(parts[1]) => Op::SetIndex(parts[1].into()),
            "NE" if parts.len() == 1 => Op::Ne,
            "LT" if parts.len() == 1 => Op::Lt,
            "LE" if parts.len() == 1 => Op::Le,
            "GT" if parts.len() == 1 => Op::Gt,
            "GE" if parts.len() == 1 => Op::Ge,
            "JUMP_IF_FALSE" if parts.len() == 2 => Op::JumpIfFalse(parts[1].parse().map_err(|_| invalid())?),
            "JUMP_IF_FALSE_KEEP" if parts.len() == 2 => Op::JumpIfFalseKeep(parts[1].parse().map_err(|_| invalid())?),
            "JUMP_IF_TRUE_KEEP" if parts.len() == 2 => Op::JumpIfTrueKeep(parts[1].parse().map_err(|_| invalid())?),
            "JUMP" if parts.len() == 2 => Op::Jump(parts[1].parse().map_err(|_| invalid())?),
            "BUILTIN" if parts.len() == 3 && is_builtin(parts[1]) => {
                let arg_count: usize = parts[2].parse().map_err(|_| invalid())?;
                if arg_count > MAX_CALL_ARGUMENTS {
                    return Err(format!("bytecode builtin argument count exceeds safety limit ({MAX_CALL_ARGUMENTS})"));
                }
                Op::Builtin(parts[1].into(), arg_count)
            },
            "CALL_TARGET" if parts.len() == 4 => {
                let target: usize = parts[1].parse().map_err(|_| invalid())?;
                let arg_count: usize = parts[2].parse().map_err(|_| invalid())?;
                if arg_count > MAX_CALL_ARGUMENTS {
                    return Err(format!("bytecode function argument count exceeds safety limit ({MAX_CALL_ARGUMENTS})"));
                }
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
        if code.len() >= MAX_BYTECODE_INSTRUCTIONS {
            return Err(format!("bytecode instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})"));
        }
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
    verify_bytecode(&code)?;
    Ok(code)
}


fn hex_encode(input: &str) -> String {
    if input.is_empty() { return "-".into(); }
    input.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(input: &str) -> Result<String, String> {
    if input == "-" { return Ok(String::new()); }
    if input.len() / 2 > MAX_BYTECODE_STRING_BYTES {
        return Err(format!("bytecode string exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
    }
    if !input.len().is_multiple_of(2) {
        return Err("invalid hex string in bytecode".into());
    }
    let mut bytes = Vec::with_capacity(input.len() / 2);
    for pair in input.as_bytes().as_chunks::<2>().0 {
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

fn read_program(path: &str) -> Result<String, String> {
    module_loader::load(Path::new(path))
}

fn write_bytecode(source_path: &str, output_path: &str) -> Result<(), String> {
    let source = read_program(source_path)?;
    let code = compile(&source)?;
    let serialized = serialize_bytecode(&code)?;
    fs::write(output_path, serialized).map_err(|error| format!("cannot write {output_path}: {error}"))
}

fn run_bytecode(path: &str) -> Result<(), String> {
    let serialized = read_source(path)?;
    let code = deserialize_bytecode(&serialized)?;
    run(&code)
}

fn run_chain_command(args: &[String]) -> Result<(), String> {
    use std::path::Path;
    match args {
        [_, mode, ledger_path, address, amount] if mode == "--chain-init" => {
            if Path::new(ledger_path).exists() {
                return Err(format!("ledger already exists at {ledger_path}; refusing to overwrite it"));
            }
            let amount: i64 = amount.parse().map_err(|_| "genesis balance must be a valid 64-bit integer")?;
            let mut runtime = chain_runtime::ChainRuntime::new();
            runtime.set_genesis_balance(address, amount)?;
            runtime.save_to_file(ledger_path)?;
            println!("Zenvora ledger initialized: {address} balance={amount}");
            Ok(())
        }
        [_, mode, ledger_path, from, to, amount] if mode == "--chain-transfer" => {
            let amount: i64 = amount.parse().map_err(|_| "transfer amount must be a valid 64-bit integer")?;
            let mut runtime = chain_runtime::ChainRuntime::load_from_file(ledger_path)?;
            let receipt = runtime.transfer(from, to, amount)?;
            runtime.save_to_file(ledger_path)?;
            println!("sequence={} status={} from={} to={} amount={} sender_balance={} recipient_balance={}",
                receipt.sequence, if receipt.accepted { "ACCEPTED" } else { "REJECTED" },
                receipt.from, receipt.to, receipt.amount, receipt.sender_balance, receipt.recipient_balance);
            Ok(())
        }
        [_, mode, ledger_path, address] if mode == "--chain-balance" => {
            let runtime = chain_runtime::ChainRuntime::load_from_file(ledger_path)?;
            println!("{}: {}", address, runtime.balance(address));
            Ok(())
        }
        [_, mode, ledger_path] if mode == "--chain-receipts" => {
            let runtime = chain_runtime::ChainRuntime::load_from_file(ledger_path)?;
            for receipt in runtime.receipts() {
                println!("sequence={} status={} from={} to={} amount={} sender_balance={} recipient_balance={}",
                    receipt.sequence, if receipt.accepted { "ACCEPTED" } else { "REJECTED" },
                    receipt.from, receipt.to, receipt.amount, receipt.sender_balance, receipt.recipient_balance);
            }
            Ok(())
        }
        _ => Err("invalid chain command arguments".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("Zenvora Lang v0.8.0");
        return;
    }
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("Zenvora Lang v0.8.0\nUsage: zv <file.zv>\n       zv --emit-bytecode <file.zv> <output.zvb>\n       zv --run-bytecode <file.zvb>\n       zv --chain-init <ledger.zvr> <address> <balance>\n       zv --chain-transfer <ledger.zvr> <from> <to> <amount>\n       zv --chain-balance <ledger.zvr> <address>\n       zv --chain-receipts <ledger.zvr>\n\nSyntax: define total := 10 + 20\n        show total * 2\n        if total >= 18 {{ ... }}");
        return;
    }
    if args.get(1).is_some_and(|mode| mode.starts_with("--chain-")) {
        if let Err(error) = run_chain_command(&args) {
            eprintln!("Zenvora chain error: {error}");
            process::exit(1);
        }
        return;
    }
    let result = match args.as_slice() {
        [_, mode, source, output] if mode == "--emit-bytecode" => write_bytecode(source, output),
        [_, mode, bytecode] if mode == "--run-bytecode" => run_bytecode(bytecode),
        [_, path] if !path.starts_with("--") => {
            let source = read_program(path).unwrap_or_else(|error| {
                eprintln!("Zenvora error: {error}");
                process::exit(1)
            });
            compile(&source).and_then(|code| run(&code))
        }
        _ => {
            eprintln!("Usage: zv <file.zv> | zv --emit-bytecode <file.zv> <output.zvb> | zv --run-bytecode <file.zvb> | zv --chain-init <ledger.zvr> <address> <balance> | zv --chain-transfer <ledger.zvr> <from> <to> <amount> | zv --chain-balance <ledger.zvr> <address> | zv --chain-receipts <ledger.zvr>");
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
    fn malformed_and_oversized_string_literals_are_reported() {
        assert!(compile_core("show \"unterminated\n").unwrap_err().contains("unterminated string literal"));
        assert!(compile_core("show \"bad\\q\"\n").unwrap_err().contains("unsupported string escape"));
        let oversized = format!("show \"{}\"\n", "a".repeat(super::MAX_BYTECODE_STRING_BYTES + 1));
        assert!(compile_core(&oversized).unwrap_err().contains("string literal exceeds safety limit"));
    }

    #[test]
    fn string_concatenation_cannot_exceed_the_string_limit() {
        let half = "a".repeat(super::MAX_BYTECODE_STRING_BYTES / 2 + 1);
        let source = format!("show \"{half}\" + \"{half}\"\n");
        assert!(execute(&source).unwrap_err().contains("string exceeds safety limit"));
    }

    #[test]
    fn runtime_rejects_excessive_call_arguments_before_allocating() {
        let mut output = Vec::new();
        let builtin_error = run_with_output(
            &[Op::Builtin("array".into(), super::MAX_CALL_ARGUMENTS + 1)],
            &mut |line| output.push(line),
        ).unwrap_err();
        assert!(builtin_error.contains("builtin argument count exceeds safety limit"));
        let function_error = run_with_output(
            &[Op::CallTarget {
                target: 0,
                params: vec!["x".into(); super::MAX_CALL_ARGUMENTS + 1],
                arg_count: super::MAX_CALL_ARGUMENTS + 1,
            }],
            &mut |line| output.push(line),
        ).unwrap_err();
        assert!(function_error.contains("function argument count exceeds safety limit"));
    }

    #[test]
    fn runtime_rejects_operand_stack_growth_past_the_limit() {
        let code = vec![Op::Push(1); super::MAX_STACK_VALUES + 1];
        let mut output = Vec::new();
        let error = run_with_output(&code, &mut |line| output.push(line)).unwrap_err();
        assert!(error.contains("operand stack exceeds safety limit"));
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
    fn bytecode_verifier_rejects_stack_underflow_and_bad_control_flow() {
        for bytecode in [
            "ZENVORA_BYTECODE 1\nPRINT\nJUMP 2\n",
            "ZENVORA_BYTECODE 1\nPUSH 1\nADD\nJUMP 3\n",
            "ZENVORA_BYTECODE 1\nPUSH_BOOL true\nJUMP_IF_FALSE 99\nJUMP 3\n",
        ] {
            let error = super::deserialize_bytecode(bytecode).unwrap_err();
            assert!(error.contains("verifier") || error.contains("invalid instruction") || error.contains("invalid target"), "unexpected error: {error}");
        }
    }

    #[test]
    fn bytecode_verifier_rejects_inconsistent_stack_depth_at_merge() {
        let bytecode = "ZENVORA_BYTECODE 1\nPUSH_BOOL true\nJUMP_IF_FALSE 4\nPUSH 1\nJUMP 5\nJUMP 5\nPRINT\nJUMP 7\n";
        let error = super::deserialize_bytecode(bytecode).unwrap_err();
        assert!(error.contains("inconsistent stack depth") || error.contains("invalid"), "unexpected error: {error}");
    }

    #[test]
    fn bytecode_verifier_accepts_compiler_output_with_functions_and_loops() {
        let source = "define n := 0\nrepeat n < 3 {\nn := n + 1\n}\nshow twice(n)\nfn twice(value) {\nreturn value * 2\n}\n";
        let code = super::compile(source).unwrap();
        let serialized = super::serialize_bytecode(&code).unwrap();
        let decoded = super::deserialize_bytecode(&serialized).unwrap();
        assert_eq!(decoded, code);
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
            Op::Return, Op::ReturnValue, Op::Index, Op::SetIndex("items".into()), Op::Declare("scope".into()),
        ];
        let encoded = super::serialize_bytecode(&code).unwrap();
        assert!(encoded.contains("CALL_TARGET 0 1 arg"));
        assert!(encoded.contains("JUMP_IF_FALSE 24"));
        assert!(encoded.contains("SET_INDEX items"));
        assert!(super::serialize_bytecode(&[Op::Call("unresolved".into(), 0)]).unwrap_err().contains("unresolved function call"));
    }

    #[test]
    fn bytecode_rejects_unknown_instructions_and_invalid_jumps() {
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 2\n").unwrap_err().contains("unsupported bytecode version"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nMAGIC\n").unwrap_err().contains("invalid bytecode instruction"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nJUMP 99\n").unwrap_err().contains("invalid instruction"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nCALL_TARGET 9 0 -\n").unwrap_err().contains("invalid function entry"));
        assert!(super::deserialize_bytecode(&format!("ZENVORA_BYTECODE 1\n{}", "PUSH 1\n".repeat(super::MAX_BYTECODE_INSTRUCTIONS + 1))).unwrap_err().contains("instruction count exceeds safety limit"));
        assert!(super::deserialize_bytecode(&format!("ZENVORA_BYTECODE 1\nPUSH_STRING_HEX {}", "61".repeat(super::MAX_BYTECODE_STRING_BYTES + 1))).unwrap_err().contains("string exceeds safety limit"));
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 1\nBUILTIN array 1025\n").unwrap_err().contains("argument count exceeds safety limit"));
        assert!(super::deserialize_bytecode(&format!("ZENVORA_BYTECODE 1\n{}", " ".repeat(super::MAX_BYTECODE_BYTES))).unwrap_err().contains("file exceeds safety limit"));
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
    fn direct_indexed_assignment_updates_arrays_and_maps() {
        let source = "define numbers := array(10, 20, 30)\nnumbers[1] := 99\nshow numbers[1]\ndefine profile := map(\"name\", \"Zenvora\")\nprofile[\"name\"] := \"ZVR\"\nprofile[\"level\"] := 8\nshow profile[\"name\"]\nshow profile[\"level\"]\n";
        let code = compile(source).unwrap();
        let encoded = super::serialize_bytecode(&code).unwrap();
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        let mut output = Vec::new();
        run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["99", "ZVR", "8"]);
    }

    #[test]
    fn indexed_assignment_checks_names_types_and_bounds() {
        assert!(execute("items[0] := 1\n").unwrap_err().contains("undefined variable"));
        assert!(execute("define items := array(1)\nitems[2] := 5\n").unwrap_err().contains("out of bounds"));
        assert!(execute("define items := array(1)\nitems[\"zero\"] := 5\n").unwrap_err().contains("must be an integer"));
        assert!(execute("define value := 5\nvalue[0] := 2\n").unwrap_err().contains("expects an array or map"));
    }

    #[test]
    fn contract_state_actions_and_views_execute_in_local_vm() {
        let source = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview current() {\nreturn count\n}\n}\ncall increment(3)\ncall increment(4)\nshow current()\n";
        let output = execute(source).unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn contract_views_reject_direct_state_mutation() {
        let source = "contract Counter {\nstate count := 0\nview bad() {\ncount := count + 1\nreturn count\n}\n}\n";
        assert!(compile(source).unwrap_err().contains("cannot directly assign to contract state 'count'"));
        let good = "contract Counter {\nstate count := 0\nview current() {\nreturn count\n}\n}\nshow current()\n";
        assert_eq!(execute(good).unwrap(), vec!["0"]);
    }

    #[test]
    fn contract_dsl_reports_malformed_declarations() {
        assert!(compile("contract 123 {\nstate count := 0\n}\n").unwrap_err().contains("invalid contract name"));
        assert!(compile("contract Counter {\nstate count\n}\n").unwrap_err().contains("expected 'state name := expression'"));
        assert!(compile("contract Counter {\nunknown value\n}\n").unwrap_err().contains("expected state declaration"));
        assert!(compile("contract Counter {\nstate count := 0\n").unwrap_err().contains("missing its closing brace"));
        assert!(compile("contract A {\n}\ncontract B {\n}\n").unwrap_err().contains("only one contract block"));
    }

    #[test]
    fn functions_can_read_and_update_globals_without_leaking_locals() {
        let output = execute("define total := 10\nshow add_to_total(5)\nshow total\nshow read_total()\nfn add_to_total(amount) {\ntotal := total + amount\ndefine temporary := 99\nreturn total\n}\nfn read_total() {\nreturn total\n}\n").unwrap();
        assert_eq!(output, vec!["15", "15", "15"]);
        assert!(execute("fn bad() {\nmissing := 3\n}\ncall bad()\n").unwrap_err().contains("assignment to undefined variable"));
    }

    #[test]
    fn local_declarations_shadow_globals_and_bytecode_preserves_scope_ops() {
        let source = "define value := 10\nshow shadow()\nshow value\nfn shadow() {\ndefine value := 20\nreturn value\n}\n";
        let code = compile(source).unwrap();
        let encoded = super::serialize_bytecode(&code).unwrap();
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        let mut output = Vec::new();
        run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["20", "10"]);
    }

    #[test]
    fn standard_library_type_conversion_math_range_and_assert() {
        let output = execute("show type_of(42)\nshow type_of(array(1))\nshow to_string(true)\nshow abs(-9)\nshow min(8, 3)\nshow max(8, 3)\nshow range(2, 5)\nshow len(range(0, 4))\nshow assert(2 < 3)\n").unwrap();
        assert_eq!(output, vec!["integer", "array", "true", "9", "3", "8", "[2, 3, 4]", "4", "true"]);
    }

    #[test]
    fn string_helpers_and_sum_cover_common_standard_library_operations() {
        let output = execute(r#"show starts_with("zenvora-lang", "zen")
show ends_with("zenvora-lang", "lang")
show replace("a-b-a", "a", "x")
show split("red,green,blue", ",")
show join(array("Zenvora", "Lang"), " ")
show sum(array(10, 20, 12))
"#).unwrap();
        assert_eq!(output, vec!["true", "true", "x-b-x", "[red, green, blue]", "Zenvora Lang", "42"]);
    }

    #[test]
    fn string_helpers_and_sum_reject_invalid_inputs_and_overflow() {
        assert!(execute(r#"show starts_with(1, "x")
"#).unwrap_err().contains("two strings"));
        assert!(execute(r#"show split("a", "")
"#).unwrap_err().contains("separator cannot be empty"));
        assert!(execute(r#"show replace("abc", "", "x")
"#).unwrap_err().contains("search string cannot be empty"));
        assert!(execute(r#"show join(array("ok", 2), " ")
"#).unwrap_err().contains("array containing only strings"));
        assert!(execute(r#"show sum(array(1, "two"))
"#).unwrap_err().contains("only integers"));
        assert!(execute(r#"show sum(array(9223372036854775807, 1))
"#).unwrap_err().contains("integer overflow"));
    }

    #[test]
    fn collection_and_numeric_helpers_cover_boundaries() {
        let output = execute(r#"show first(array(7, 8, 9))
show last(array(7, 8, 9))
show reverse(array(1, 2, 3))
show reverse("héllo")
show is_empty(array())
show is_empty(map())
show is_empty("")
show clamp(12, 0, 10)
show clamp(-3, 0, 10)
"#).unwrap();
        assert_eq!(output, vec!["7", "9", "[3, 2, 1]", "olléh", "true", "true", "true", "10", "0"]);
    }

    #[test]
    fn string_helpers_reject_oversized_results_before_building_them() {
        let limit = super::MAX_BYTECODE_STRING_BYTES;
        let oversized_replacement = "b".repeat(limit);
        let replace_error = super::builtin("replace", vec![
            super::Value::Str("a".repeat(limit)),
            super::Value::Str("a".into()),
            super::Value::Str(oversized_replacement),
        ]).unwrap_err();
        assert!(replace_error.contains("replace result exceeds safety limit"));

        let join_error = super::builtin("join", vec![
            super::Value::Array(vec![
                super::Value::Str("a".repeat(limit / 2 + 1)),
                super::Value::Str("b".repeat(limit / 2 + 1)),
            ]),
            super::Value::Str(",".into()),
        ]).unwrap_err();
        assert!(join_error.contains("join result exceeds safety limit"));
    }

    #[test]
    fn collection_and_numeric_helpers_reject_invalid_inputs() {
        assert!(execute("show first(array())\n").unwrap_err().contains("empty array"));
        assert!(execute("show last(4)\n").unwrap_err().contains("expects an array"));
        assert!(execute("show reverse(true)\n").unwrap_err().contains("array or string"));
        assert!(execute("show is_empty(3)\n").unwrap_err().contains("array, map, or string"));
        assert!(execute("show clamp(1, 5, 2)\n").unwrap_err().contains("minimum must be less than or equal"));
        assert!(execute("show clamp(1, false, 2)\n").unwrap_err().contains("three integers"));
    }

    #[test]
    fn standard_library_rejects_invalid_arguments_and_resource_abuse() {
        assert!(execute("show abs(true)\n").unwrap_err().contains("abs expects"));
        assert!(execute("show range(5, 2)\n").unwrap_err().contains("range end"));
        assert!(execute("show range(0, 100001)\n").unwrap_err().contains("safety limit"));
        assert!(execute("show assert(false, \"custom failure\")\n").unwrap_err().contains("custom failure"));
        assert!(execute("show assert(1)\n").unwrap_err().contains("assert expects a boolean"));
        assert!(execute("show abs(-9223372036854775807 - 1)\n").unwrap_err().contains("integer overflow"));
    }

    #[test]
    fn direct_array_and_map_indexing_supports_nested_expressions_and_bytecode() {
        let source = "define numbers := array(10, 20, 30)\nshow numbers[0]\nshow numbers[1 + 1]\ndefine profile := map(\"name\", \"Zenvora\", \"level\", 6)\nshow profile[\"name\"]\nshow array(array(7, 8), array(9, 10))[1][0]\n";
        let code = compile(source).unwrap();
        let encoded = super::serialize_bytecode(&code).unwrap();
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        let mut output = Vec::new();
        run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["10", "30", "Zenvora", "9"]);
    }

    #[test]
    fn direct_indexing_reports_invalid_indexes_and_types() {
        assert!(execute("show array(1)[2]\n").unwrap_err().contains("out of bounds"));
        assert!(execute("show array(1)[-1]\n").unwrap_err().contains("out of bounds"));
        assert!(execute("show map(\"x\", 1)[\"missing\"]\n").unwrap_err().contains("not found"));
        assert!(execute("show array(1)[\"zero\"]\n").unwrap_err().contains("must be an integer"));
        assert!(execute("show 5[0]\n").unwrap_err().contains("indexing expects"));
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
    #[test]
    fn chain_cli_initializes_transfers_and_persists_the_ledger() {
        let path = std::env::temp_dir().join(format!("zenvora-chain-cli-{}.zvr", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path_text = path.to_string_lossy().to_string();

        let init = vec!["zv".to_string(), "--chain-init".to_string(), path_text.clone(), "alice".to_string(), "1000".to_string()];
        super::run_chain_command(&init).unwrap();

        let transfer = vec!["zv".to_string(), "--chain-transfer".to_string(), path_text.clone(), "alice".to_string(), "bob".to_string(), "125".to_string()];
        super::run_chain_command(&transfer).unwrap();

        let restored = super::chain_runtime::ChainRuntime::load_from_file(&path).unwrap();
        assert_eq!(restored.balance("alice"), 875);
        assert_eq!(restored.balance("bob"), 125);
        assert_eq!(restored.receipts().len(), 1);

        let duplicate_init = vec!["zv".to_string(), "--chain-init".to_string(), path_text, "alice".to_string(), "500".to_string()];
        assert!(super::run_chain_command(&duplicate_init).unwrap_err().contains("refusing to overwrite"));
        std::fs::remove_file(path).unwrap();
    }

}

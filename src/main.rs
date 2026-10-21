use std::collections::{BTreeMap, HashMap, HashSet};
use std::{env, fs, process};
use std::path::Path;
use std::io::Read;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

mod module_loader;
mod chain_runtime;
mod chain_protocol;
mod chain_consensus;
mod chain_block;
mod block_store;
mod chain_rpc;
mod compiler_api;
mod chain_p2p;
mod tx_pool;
mod bytecode_v2;
mod contract_state;
mod contract_runtime;
mod contract_gas;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64), String(String), Bool(bool), Ident(String), And, Or, Not, Plus, Minus, Star, Slash,
    LParen, RParen, LBracket, RBracket, Comma, EqEq, NotEq, Less, LessEq, Greater, GreaterEq, End,
}

#[cfg(test)]
fn lex(input: &str) -> Result<Vec<Token>, String> {
    lex_with_columns(input).map(|(tokens, _)| tokens)
}

fn lex_with_columns(input: &str) -> Result<(Vec<Token>, Vec<usize>), String> {
    let mut out = Vec::new();
    let mut columns = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let token_column = i + 1;
        if c.is_whitespace() { i += 1; continue; }
        if c == '#' { break; }
        if out.len() >= MAX_SOURCE_TOKENS {
            return Err(format!("column {}: token count exceeds safety limit ({MAX_SOURCE_TOKENS})", token_column));
        }
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
                        if i >= chars.len() { return Err(format!("column {}: unterminated escape in string literal", i + 1)); }
                        let escaped = chars[i];
                        i += 1;
                        match escaped {
                            '"' => value.push('"'),
                            '\\' => value.push('\\'),
                            'n' => value.push('\n'),
                            'r' => value.push('\r'),
                            't' => value.push('\t'),
                            _ => return Err(format!("column {}: unsupported string escape '\\{escaped}'", i)),
                        }
                    }
                    _ => value.push(ch),
                }
            }
            if !closed { return Err(format!("column {}: unterminated string literal", chars.len() + 1)); }
            if value.len() > MAX_BYTECODE_STRING_BYTES {
                return Err(format!("string literal exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
            }
            out.push(Token::String(value));
            columns.push(token_column);
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
            let n: String = chars[start..i].iter().collect();
            out.push(Token::Number(n.parse().map_err(|_| format!("column {}: invalid integer: {n}", start + 1))?));
            columns.push(start + 1);
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
            columns.push(start + 1);
            continue;
        }
        // Handle two-character operators (==, !=, <=, >=) by peeking the next char
        // and advancing `i` one extra position inside the branch so the final
        // `i += 1` after the match moves past both characters.
        let token = match c {
            '+' => Token::Plus, '-' => Token::Minus, '*' => Token::Star, '/' => Token::Slash,
            '(' => Token::LParen, ')' => Token::RParen, '[' => Token::LBracket, ']' => Token::RBracket, ',' => Token::Comma,
            '=' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::EqEq }
            '!' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::NotEq }
            '<' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::LessEq }
            '>' if i + 1 < chars.len() && chars[i + 1] == '=' => { i += 1; Token::GreaterEq }
            '<' => Token::Less,
            '>' => Token::Greater,
            _ => return Err(format!("column {}: unexpected character '{c}'", i + 1)),
        };
        out.push(token);
        columns.push(token_column);
        i += 1;
    }
    if out.len() >= MAX_SOURCE_TOKENS {
        return Err(format!("column {}: token count exceeds safety limit ({MAX_SOURCE_TOKENS})", chars.len() + 1));
    }
    out.push(Token::End);
    columns.push(chars.len() + 1);
    Ok((out, columns))
}

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Push(i64), PushString(String), PushBool(bool), Load(String), Store(String), Declare(String), Add, Sub, Mul, Div, Neg, Not, And, Or, Print,
    Eq, Ne, Lt, Le, Gt, Ge, Index, SetIndex(String), JumpIfFalse(usize), JumpIfFalseKeep(usize), JumpIfTrueKeep(usize), Jump(usize), Call(String, usize), Builtin(String, usize), CallTarget { target: usize, params: Vec<String>, arg_count: usize }, Return, ReturnValue,
}

struct Parser { tokens: Vec<Token>, columns: Vec<usize>, pos: usize, nesting_depth: usize, error_column_override: Option<usize>, code: Vec<Op> }
impl Parser {
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn take(&mut self) -> Token { let t = self.tokens[self.pos].clone(); self.pos += 1; t }
    fn error_column(&self) -> usize { self.error_column_override.unwrap_or(self.columns[self.pos.saturating_sub(1).min(self.columns.len() - 1)]) }
    fn current_column(&self) -> usize { self.columns[self.pos.min(self.columns.len() - 1)] }

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
        if self.nesting_depth >= MAX_PARSE_NESTING {
            self.error_column_override = Some(self.current_column());
            return Err(format!("parser nesting exceeds safety limit ({MAX_PARSE_NESTING})"));
        }
        self.nesting_depth += 1;
        let result = self.factor_inner();
        self.nesting_depth -= 1;
        result
    }

    fn factor_inner(&mut self) -> Result<(), String> {
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

fn append_code(code: &mut Vec<Op>, mut appended: Vec<Op>) -> Result<(), String> {
    if code.len().saturating_add(appended.len()) > MAX_BYTECODE_INSTRUCTIONS {
        return Err(format!("compiled instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})"));
    }
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
    Ok(())
}

fn with_source_column(raw_line: &str, trimmed_line: &str, expression: &str, error: String) -> String {
    let indentation = raw_line.chars().take_while(|c| c.is_whitespace()).count();
    let expression_offset = trimmed_line.find(expression)
        .map(|prefix| trimmed_line[..prefix].chars().count())
        .unwrap_or(0);
    if let Some(rest) = error.strip_prefix("column ") {
        if let Some((column, detail)) = rest.split_once(": ") {
            if let Ok(column) = column.parse::<usize>() {
                return format!("column {}: {}", indentation + expression_offset + column, detail);
            }
        }
    }
    error
}

fn compile_core(source: &str) -> Result<Vec<Op>, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!("source exceeds safety limit ({MAX_SOURCE_BYTES} bytes)"));
    }
    let mut code = Vec::new();
    let mut blocks: Vec<BlockFrame> = Vec::new();

    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim().trim_end_matches(';').trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if code.len() >= MAX_BYTECODE_INSTRUCTIONS {
            return Err(format!("line {}: compiled instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})", line_no + 1));
        }

        if let Some(condition) = line.strip_prefix("if ").and_then(|s| s.strip_suffix('{')) {
            let (tokens, columns) = lex_with_columns(condition.trim()).map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, condition.trim(), e)))?;
            let mut parser = Parser { tokens, columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
            parser.condition().map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, condition.trim(), format!("column {}: {e}", parser.error_column()))))?;
            if parser.peek() != &Token::End {
                return Err(format!("line {}: {}", line_no + 1, with_source_column(raw, line, condition.trim(), format!("column {}: unexpected token {:?}", parser.current_column(), parser.peek()))));
            }
            append_code(&mut code, parser.code)?;
            let jump = code.len();
            code.push(Op::JumpIfFalse(usize::MAX));
            blocks.push(BlockFrame::If { false_jump: Some(jump), end_jump: None });
            continue;
        }

        if let Some(condition) = line.strip_prefix("repeat ").and_then(|s| s.strip_suffix('{')) {
            let (tokens, columns) = lex_with_columns(condition.trim()).map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, condition.trim(), e)))?;
            let mut parser = Parser { tokens, columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
            parser.condition().map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, condition.trim(), format!("column {}: {e}", parser.error_column()))))?;
            if parser.peek() != &Token::End {
                return Err(format!("line {}: {}", line_no + 1, with_source_column(raw, line, condition.trim(), format!("column {}: unexpected token {:?}", parser.current_column(), parser.peek()))));
            }
            let start = code.len();
            append_code(&mut code, parser.code)?;
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
            if !valid_identifier(name) {
                return Err(format!("line {}: invalid function name", line_no + 1));
            }
            let args = split_arguments(args_text);
            let args: Vec<&str> = if args_text.trim().is_empty() { Vec::new() } else { args };
            for arg in &args {
                let (tokens, columns) = lex_with_columns(arg.trim()).map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, arg.trim(), e)))?;
                let mut parser = Parser { tokens, columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
                parser.expression().map_err(|e| format!("line {}: invalid function argument: {}", line_no + 1, with_source_column(raw, line, arg.trim(), format!("column {}: {e}", parser.error_column()))))?;
                if parser.peek() != &Token::End {
                    return Err(format!("line {}: {}", line_no + 1, with_source_column(raw, line, arg.trim(), format!("column {}: unexpected token in function argument {:?}", parser.current_column(), parser.peek()))));
                }
                append_code(&mut code, parser.code)?;
            }
            code.push(Op::Call(name.to_string(), args.len()));
            continue;
        }
        if line == "return" { code.push(Op::Return); continue; }
        if let Some(expr) = line.strip_prefix("return ") {
            let (tokens, columns) = lex_with_columns(expr.trim()).map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, expr.trim(), e)))?;
            let mut parser = Parser { tokens, columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
            parser.expression().map_err(|e| format!("line {}: invalid return expression: {}", line_no + 1, with_source_column(raw, line, expr.trim(), format!("column {}: {e}", parser.error_column()))))?;
            if parser.peek() != &Token::End {
                return Err(format!("line {}: {}", line_no + 1, with_source_column(raw, line, expr.trim(), format!("column {}: unexpected token in return expression {:?}", parser.current_column(), parser.peek()))));
            }
            append_code(&mut code, parser.code)?;
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
                    let condition_jump = false_jump.take().ok_or_else(|| format!("line {}: otherwise without matching if", line_no + 1))?;
                    let then_start = condition_jump + 1;
                    let then_can_fall_through = code_region_can_fall_through(&code, then_start, code.len());
                    let else_start = if then_can_fall_through {
                        let jump = code.len();
                        code.push(Op::Jump(usize::MAX));
                        *end_jump = Some(jump);
                        code.len()
                    } else {
                        code.len()
                    };
                    code[condition_jump] = Op::JumpIfFalse(else_start);
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
                    let (index_tokens, index_columns) = lex_with_columns(index_expr.trim()).map_err(|e| format!("line {}: invalid index: {}", line_no + 1, with_source_column(raw, line, index_expr.trim(), e)))?;
                    let mut index_parser = Parser { tokens: index_tokens, columns: index_columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
                    index_parser.expression().map_err(|e| format!("line {}: invalid index: {}", line_no + 1, with_source_column(raw, line, index_expr.trim(), format!("column {}: {e}", index_parser.error_column()))))?;
                    if index_parser.peek() != &Token::End {
                        return Err(format!("line {}: invalid index: {}", line_no + 1, with_source_column(raw, line, index_expr.trim(), format!("column {}: unexpected token in index {:?}", index_parser.current_column(), index_parser.peek()))));
                    }
                    append_code(&mut code, index_parser.code)?;
                    let (value_tokens, value_columns) = lex_with_columns(right.trim()).map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, right.trim(), e)))?;
                    let mut value_parser = Parser { tokens: value_tokens, columns: value_columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
                    value_parser.expression().map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, right.trim(), format!("column {}: {e}", value_parser.error_column()))))?;
                    if value_parser.peek() != &Token::End {
                        return Err(format!("line {}: {}", line_no + 1, with_source_column(raw, line, right.trim(), format!("column {}: unexpected token in indexed assignment {:?}", value_parser.current_column(), value_parser.peek()))));
                    }
                    append_code(&mut code, value_parser.code)?;
                    code.push(Op::SetIndex(name.trim().to_string()));
                    continue;
                }
            }
        }

        let is_declaration = line.starts_with("define ");
        let statement = if let Some(rest) = line.strip_prefix("define ") {
            let (name, expr) = rest.split_once(":=").ok_or_else(|| format!("line {}: expected 'define name := expression'", line_no + 1))?;
            let name = name.trim();
            if !valid_identifier(name) {
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
        let (tokens, columns) = lex_with_columns(statement.1).map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, statement.1, e)))?;
        let mut parser = Parser { tokens, columns, pos: 0, nesting_depth: 0, error_column_override: None, code: Vec::new() };
        parser.expression().map_err(|e| format!("line {}: {}", line_no + 1, with_source_column(raw, line, statement.1, format!("column {}: {e}", parser.error_column()))))?;
        if parser.peek() != &Token::End {
            return Err(format!("line {}: {}", line_no + 1, with_source_column(raw, line, statement.1, format!("column {}: unexpected token {:?}", parser.current_column(), parser.peek()))));
        }
        append_code(&mut code, parser.code)?;
        if statement.2 { code.push(Op::Print); } else if is_declaration { code.push(Op::Declare(statement.0)); } else { code.push(Op::Store(statement.0)); }
    }

    if let Some(frame) = blocks.last() {
        let kind = match frame { BlockFrame::If { .. } => "if", BlockFrame::Repeat { .. } => "repeat" };
        return Err(format!("missing closing brace for {kind} block"));
    }
    if code.len() > MAX_BYTECODE_INSTRUCTIONS {
        return Err(format!("compiled instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})"));
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
fn calls_named_function(statement: &str, name: &str) -> bool {
    let bytes = statement.as_bytes();
    let mut index = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'#' {
            while index < bytes.len() && bytes[index] != b'\n' { index += 1; }
            continue;
        }
        if byte == b'"' {
            in_string = true;
            index += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
                index += 1;
            }
            if &statement[start..index] == name {
                let mut next = index;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() { next += 1; }
                if next < bytes.len() && bytes[next] == b'(' { return true; }
            }
            continue;
        }
        index += 1;
    }
    false
}

fn mutating_functions(source: &str, state_names: &HashSet<String>, action_names: &HashSet<String>) -> HashSet<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut bodies: Vec<(String, String)> = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim();
        let declaration = line.strip_prefix("fn ").map(|rest| ("fn", rest))
            .or_else(|| line.strip_prefix("action ").map(|rest| ("action", rest)))
            .or_else(|| line.strip_prefix("view ").map(|rest| ("view", rest)));
        if let Some((_kind, header)) = declaration.and_then(|(kind, rest)| rest.strip_suffix('{').map(|h| (kind, h.trim()))) {
            let Some((name, _)) = header.split_once('(') else {
                index += 1;
                continue;
            };
            let name = name.trim();
            if !valid_identifier(name) {
                index += 1;
                continue;
            }
            index += 1;
            let mut depth = 1i32;
            let mut body = String::new();
            while index < lines.len() && depth > 0 {
                let current = lines[index];
                let (opens, closes) = count_block_braces(current);
                depth += opens - closes;
                if depth > 0 {
                    body.push_str(current);
                    body.push('\n');
                }
                index += 1;
            }
            if depth == 0 {
                bodies.push((name.to_string(), body));
            }
            continue;
        }
        index += 1;
    }

    let mut mutating = action_names.clone();
    for (name, body) in &bodies {
        for line in body.lines() {
            let statement = line.trim();
            if statement.starts_with('#') { continue; }
            if let Some((left, _)) = statement.split_once(":=") {
                let base = left.trim().split('[').next().unwrap_or(left.trim()).trim();
                if state_names.contains(base) {
                    mutating.insert(name.clone());
                }
            }
        }
    }

    loop {
        let newly_mutating: Vec<String> = bodies.iter().filter_map(|(name, body)| {
            if mutating.contains(name) { return None; }
            if mutating.iter().any(|target| calls_named_function(body, target)) {
                Some(name.clone())
            } else {
                None
            }
        }).collect();
        if newly_mutating.is_empty() { break; }
        mutating.extend(newly_mutating);
    }
    mutating
}

fn lower_contracts(source: &str) -> Result<String, String> {
    let lines: Vec<&str> = source.lines().collect();
    let state_names: HashSet<String> = lines.iter().filter_map(|raw| {
        let rest = raw.trim().strip_prefix("state ")?;
        let name = rest.split_once(":=")?.0.trim();
        if valid_identifier(name) { Some(name.to_string()) } else { None }
    }).collect();
    let action_names: HashSet<String> = lines.iter().filter_map(|raw| {
        let rest = raw.trim().strip_prefix("action ")?;
        let name = rest.split_once('(')?.0.trim();
        if valid_identifier(name) { Some(name.to_string()) } else { None }
    }).collect();
    let mutating_functions = mutating_functions(source, &state_names, &action_names);
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
                        for function in &mutating_functions {
                            if calls_named_function(statement, function) {
                                if action_names.contains(function) {
                                    return Err(format!("line {}: view cannot call action '{function}'", index + 1));
                                }
                                return Err(format!("line {}: view cannot call state-mutating function '{function}'", index + 1));
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

fn code_region_can_fall_through(code: &[Op], start: usize, end: usize) -> bool {
    if start >= end {
        return true;
    }
    let mut visited = HashSet::new();
    let mut pending = vec![start];
    while let Some(pc) = pending.pop() {
        if pc == end {
            return true;
        }
        if pc < start || pc > end || !visited.insert(pc) {
            continue;
        }
        let Some(op) = code.get(pc) else { continue; };
        match op {
            Op::Return | Op::ReturnValue => {}
            Op::Jump(target) => {
                if *target == end {
                    return true;
                }
                if *target >= start && *target < end {
                    pending.push(*target);
                }
            }
            Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) => {
                if *target == end {
                    return true;
                }
                if *target >= start && *target < end {
                    pending.push(*target);
                }
                pending.push(pc + 1);
            }
            _ => pending.push(pc + 1),
        }
    }
    false
}

fn function_body_can_fall_through(code: &[Op]) -> bool {
    code_region_can_fall_through(code, 0, code.len())
}

fn compile(source: &str) -> Result<Vec<Op>, String> {
    compile_with_layout(source).map(|(code, _)| code)
}

fn compile_with_layout(source: &str) -> Result<(Vec<Op>, bytecode_v2::ProgramLayout), String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!("source exceeds safety limit ({MAX_SOURCE_BYTES} bytes)"));
    }
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
            if !valid_identifier(name) {
                return Err(format!("line {}: invalid function name", i + 1));
            }
            let params: Vec<String> = if params_text.trim().is_empty() { Vec::new() } else { params_text.split(',').map(|p| p.trim().to_string()).collect() };
            for param in &params {
                if !valid_identifier(param) {
                    return Err(format!("line {}: invalid parameter name in function '{name}'", i + 1));
                }
            }
            for (j, param) in params.iter().enumerate() {
                if params[..j].contains(param) { return Err(format!("line {}: duplicate parameter '{param}'", i + 1)); }
            }
            if functions.iter().any(|(existing, _, _)| existing == name) {
                return Err(format!("line {}: duplicate function '{name}'", i + 1));
            }
            let declaration_line = i + 1;
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
            if depth != 0 { return Err(format!("line {declaration_line}: missing closing brace for function '{name}'")); }
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
    let main_end = code.len();
    let mut entries: HashMap<String, (usize, Vec<String>)> = HashMap::new();
    let mut regions: Vec<(usize, usize, String, Vec<String>)> = Vec::new();
    for (name, params, body) in &functions {
        let entry = code.len();
        entries.insert(name.clone(), (entry, params.clone()));
        let mut body_code = compile_core(body)?;
        // Analyze local control-flow targets before relocating jumps into the
        // whole-program instruction stream.
        let can_fall_through = function_body_can_fall_through(&body_code);
        for op in &mut body_code {
            match op {
                Op::Jump(target) | Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) if *target != usize::MAX => *target += entry,
                _ => {}
            }
        }
        code.extend(body_code);
        if can_fall_through {
            code.push(Op::Return);
        }
        regions.push((entry, code.len(), name.clone(), params.clone()));
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

    // Preserve v1's existing validation for functions that are called, while
    // deriving explicit v2 return metadata for every declared function, including
    // functions that are never called by this source program.
    let mut called_entries: Vec<usize> = code.iter().filter_map(|op| {
        if let Op::CallTarget { target, .. } = op { Some(*target) } else { None }
    }).collect();
    called_entries.sort_unstable();
    called_entries.dedup();
    infer_function_return_kinds(&code, &called_entries)?;

    let all_entries: Vec<usize> = regions.iter().map(|region| region.0).collect();
    let value_returns = infer_function_return_kinds(&code, &all_entries)?;
    let function_regions = regions.into_iter().map(|(entry, region_end, _name, parameters)| {
        bytecode_v2::FunctionRegion {
            entry,
            end: region_end,
            parameters,
            return_kind: if value_returns.contains(&entry) {
                bytecode_v2::ReturnKind::Value
            } else {
                bytecode_v2::ReturnKind::Void
            },
        }
    }).collect();
    let layout = bytecode_v2::ProgramLayout {
        instruction_count: code.len(),
        main_end,
        functions: function_regions,
    };
    bytecode_v2::validate_layout(&layout)?;
    Ok((code, layout))
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    Array(Vec<Value>),
    Map(std::collections::BTreeMap<String, Value>),
}

fn is_builtin(name: &str) -> bool { bytecode_v2::is_supported_builtin(name) }

fn check_map_capacity(items: &std::collections::BTreeMap<String, Value>, key: &str) -> Result<(), String> {
    if !items.contains_key(key) && items.len() >= MAX_COLLECTION_ITEMS {
        return Err(format!("map exceeds safety limit ({MAX_COLLECTION_ITEMS} items)"));
    }
    Ok(())
}

fn set_indexed_value(collection: &mut Value, index: Value, value: Value) -> Result<(), String> {
    match (collection, index) {
        (Value::Array(items), Value::Int(index)) if index >= 0 => {
            let slot = items.get_mut(index as usize).ok_or_else(|| format!("array index {index} out of bounds"))?;
            *slot = value;
            Ok(())
        }
        (Value::Array(_), Value::Int(index)) => Err(format!("array index {index} out of bounds")),
        (Value::Array(_), _) => Err("type error: array index must be an integer".into()),
        (Value::Map(items), Value::Str(key)) => {
            check_map_capacity(items, &key)?;
            items.insert(key, value);
            Ok(())
        }
        (Value::Map(_), _) => Err("type error: map index must be a string".into()),
        _ => Err("type error: indexed assignment expects an array or map".into()),
    }
}

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
    let value = builtin_unchecked(name, args)?;
    ensure_value_depth(&value)?;
    Ok(value)
}

fn value_depth(value: &Value) -> usize {
    match value {
        Value::Int(_) | Value::Str(_) | Value::Bool(_) => 0,
        Value::Array(items) => 1 + items.iter().map(value_depth).max().unwrap_or(0),
        Value::Map(items) => 1 + items.values().map(value_depth).max().unwrap_or(0),
    }
}

fn value_size(value: &Value) -> usize {
    match value {
        Value::Int(_) => std::mem::size_of::<i64>(),
        Value::Bool(_) => std::mem::size_of::<bool>(),
        Value::Str(text) => text.len(),
        Value::Array(items) => items.iter().map(value_size).fold(
            items.len().saturating_mul(std::mem::size_of::<Value>()),
            |total, size| total.saturating_add(size),
        ),
        Value::Map(items) => items.iter().map(|(key, value)| key.len().saturating_add(value_size(value))).fold(
            items.len().saturating_mul(64),
            |total, size| total.saturating_add(size),
        ),
    }
}

fn ensure_value_depth(value: &Value) -> Result<(), String> {
    let depth = value_depth(value);
    if depth > MAX_VALUE_DEPTH {
        return Err(format!("value nesting exceeds safety limit ({MAX_VALUE_DEPTH} levels)"));
    }
    let size = value_size(value);
    if size > MAX_VALUE_BYTES {
        return Err(format!("value exceeds estimated memory safety budget ({MAX_VALUE_BYTES} bytes)"));
    }
    Ok(())
}

fn push_stack_value(stack: &mut Vec<Value>, stack_bytes: &mut usize, value: Value) -> Result<(), String> {
    let size = value_size(&value);
    let next_size = stack_bytes.saturating_add(size);
    if next_size > MAX_STACK_VALUE_BYTES {
        return Err(format!("operand stack values exceed safety limit ({MAX_STACK_VALUE_BYTES} bytes)"));
    }
    stack.push(value);
    *stack_bytes = next_size;
    Ok(())
}

fn pop_stack_value(stack: &mut Vec<Value>, stack_bytes: &mut usize, underflow: &str) -> Result<Value, String> {
    let value = stack.pop().ok_or_else(|| underflow.to_string())?;
    *stack_bytes = stack_bytes.saturating_sub(value_size(&value));
    Ok(value)
}

fn binding_size(name: &str, value: &Value) -> usize {
    name.len().saturating_add(64).saturating_add(value_size(value))
}

fn binding_map_size(bindings: &HashMap<String, Value>) -> usize {
    bindings.iter().map(|(name, value)| binding_size(name, value))
        .fold(0usize, |total, size| total.saturating_add(size))
}

fn runtime_state_bytes_after_sizes(current: usize, old_size: usize, new_size: usize) -> Result<usize, String> {
    let next = current.saturating_sub(old_size).saturating_add(new_size);
    if next > MAX_RUNTIME_STATE_BYTES {
        return Err(format!("live variable/call-frame values exceed safety limit ({MAX_RUNTIME_STATE_BYTES} estimated bytes)"));
    }
    Ok(next)
}

fn runtime_state_bytes_after_update(current: usize, name: &str, old: Option<&Value>, new: &Value) -> Result<usize, String> {
    let old_size = old.map(|value| binding_size(name, value)).unwrap_or(0);
    runtime_state_bytes_after_sizes(current, old_size, binding_size(name, new))
}

fn builtin_unchecked(name: &str, args: Vec<Value>) -> Result<Value, String> {
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
            let mut values = args.into_iter();
            let collection = values.next().ok_or("get expects a collection and an index")?;
            let index = values.next().ok_or("get expects a collection and an index")?;
            match (collection, index) {
                (Value::Array(items), Value::Int(index)) if index >= 0 => items.into_iter().nth(index as usize).ok_or_else(|| format!("array index {index} out of bounds")),
                (Value::Array(_), Value::Int(index)) => Err(format!("array index {index} out of bounds")),
                (Value::Map(mut items), Value::Str(key)) => items.remove(&key).ok_or_else(|| format!("map key '{key}' not found")),
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
            let mut values = args.into_iter();
            let original = values.next().ok_or("push expects an array and a value")?;
            let value = values.next().ok_or("push expects an array and a value")?;
            let Value::Array(mut items) = original else { return Err("type error: push expects an array as its first argument".into()); };
            if items.len() >= MAX_COLLECTION_ITEMS {
                return Err(format!("array exceeds safety limit ({MAX_COLLECTION_ITEMS} items)"));
            }
            items.push(value);
            Ok(Value::Array(items))
        }
        "put" => {
            arity(3)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("put expects a map, key, and value")?;
            let key = values.next().ok_or("put expects a map, key, and value")?;
            let value = values.next().ok_or("put expects a map, key, and value")?;
            let Value::Map(mut items) = original else { return Err("type error: put expects a map as its first argument".into()); };
            let Value::Str(key) = key else { return Err("type error: put key must be a string".into()); };
            check_map_capacity(&items, &key)?;
            items.insert(key, value);
            Ok(Value::Map(items))
        }
        "keys" | "values" => {
            arity(1)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("keys/values expects a map")?;
            let Value::Map(items) = original else { return Err(format!("type error: {name} expects a map")); };
            let values = if name == "keys" { items.into_keys().map(Value::Str).collect() } else { items.into_values().collect() };
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
        "dna_complement" => {
            arity(1)?;
            let Value::Str(sequence) = &args[0] else {
                return Err("type error: dna_complement expects a DNA string".into());
            };
            let mut result = String::with_capacity(sequence.len());
            for base in sequence.chars() {
                result.push(match base.to_ascii_uppercase() {
                    'A' => 'T', 'T' => 'A', 'C' => 'G', 'G' => 'C',
                    _ => return Err(format!("dna_complement found invalid DNA base '{base}'")),
                });
            }
            Ok(Value::Str(result))
        }
        "gc_content_percent" => {
            arity(1)?;
            let Value::Str(sequence) = &args[0] else {
                return Err("type error: gc_content_percent expects a DNA string".into());
            };
            if sequence.is_empty() { return Err("gc_content_percent cannot use an empty DNA string".into()); }
            let mut gc = 0i64;
            for base in sequence.chars() {
                match base.to_ascii_uppercase() {
                    'G' | 'C' => gc += 1,
                    'A' | 'T' => {},
                    _ => return Err(format!("gc_content_percent found invalid DNA base '{base}'")),
                }
            }
            let length = i64::try_from(sequence.chars().count()).map_err(|_| "DNA sequence is too long")?;
            Ok(Value::Int(gc.checked_mul(100).ok_or("GC percentage overflow")? / length))
        }
        "molar_mass" => {
            arity(1)?;
            let Value::Array(atomic_masses) = &args[0] else {
                return Err("type error: molar_mass expects an array of integer atomic masses".into());
            };
            if atomic_masses.is_empty() { return Err("molar_mass expects at least one atomic mass".into()); }
            let mut total = 0i64;
            for mass in atomic_masses {
                let Value::Int(mass) = mass else {
                    return Err("type error: molar_mass expects an array containing only integers".into());
                };
                if *mass < 0 { return Err("molar_mass atomic masses cannot be negative".into()); }
                total = total.checked_add(*mass).ok_or("integer overflow in molar_mass")?;
            }
            Ok(Value::Int(total))
        }
        "gcd" => {
            arity(2)?;
            let (Value::Int(mut a), Value::Int(mut b)) = (&args[0], &args[1]) else {
                return Err("type error: gcd expects two integers".into());
            };
            a = a.checked_abs().ok_or("integer overflow in gcd")?;
            b = b.checked_abs().ok_or("integer overflow in gcd")?;
            while b != 0 { let remainder = a % b; a = b; b = remainder; }
            Ok(Value::Int(a))
        }
        "lcm" => {
            arity(2)?;
            let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
                return Err("type error: lcm expects two integers".into());
            };
            if *a == 0 || *b == 0 { return Ok(Value::Int(0)); }
            let mut x = a.checked_abs().ok_or("integer overflow in lcm")?;
            let mut y = b.checked_abs().ok_or("integer overflow in lcm")?;
            let mut p = x;
            while y != 0 { let r = p % y; p = y; y = r; }
            x = a.checked_div(p).ok_or("integer overflow in lcm")?;
            Ok(Value::Int(x.checked_mul(*b).and_then(i64::checked_abs).ok_or("integer overflow in lcm")?))
        }
        "factorial" => {
            arity(1)?;
            let Value::Int(n) = args[0] else { return Err("type error: factorial expects an integer".into()); };
            if !(0..=20).contains(&n) { return Err("factorial expects an integer from 0 through 20".into()); }
            let mut result = 1i64;
            for value in 2..=n { result = result.checked_mul(value).ok_or("integer overflow in factorial")?; }
            Ok(Value::Int(result))
        }
        "is_prime" => {
            arity(1)?;
            let Value::Int(n) = args[0] else { return Err("type error: is_prime expects an integer".into()); };
            if n < 2 { return Ok(Value::Bool(false)); }
            if n == 2 { return Ok(Value::Bool(true)); }
            if n % 2 == 0 { return Ok(Value::Bool(false)); }
            let mut divisor = 3i64;
            while divisor <= n / divisor {
                if n % divisor == 0 { return Ok(Value::Bool(false)); }
                divisor += 2;
            }
            Ok(Value::Bool(true))
        }
        "fibonacci" => {
            arity(1)?;
            let Value::Int(n) = args[0] else { return Err("type error: fibonacci expects an integer".into()); };
            if !(0..=92).contains(&n) { return Err("fibonacci expects an integer from 0 through 92".into()); }
            let (mut a, mut b) = (0i64, 1i64);
            for _ in 0..n { let next = a.checked_add(b).ok_or("integer overflow in fibonacci")?; a = b; b = next; }
            Ok(Value::Int(a))
        }
        "pow_int" => {
            arity(2)?;
            let (Value::Int(base), Value::Int(exponent)) = (&args[0], &args[1]) else {
                return Err("type error: pow_int expects two integers".into());
            };
            if !(0..=63).contains(exponent) { return Err("pow_int exponent must be from 0 through 63".into()); }
            let mut result = 1i64;
            let mut factor = *base;
            let mut power = *exponent as u32;
            while power > 0 {
                if power & 1 == 1 { result = result.checked_mul(factor).ok_or("integer overflow in pow_int")?; }
                power >>= 1;
                if power > 0 { factor = factor.checked_mul(factor).ok_or("integer overflow in pow_int")?; }
            }
            Ok(Value::Int(result))
        }
        "mean" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else { return Err("type error: mean expects an array of integers".into()); };
            if items.is_empty() { return Err("mean cannot be used with an empty array".into()); }
            let mut total = 0i64;
            for item in items {
                let Value::Int(value) = item else { return Err("type error: mean expects an array containing only integers".into()); };
                total = total.checked_add(*value).ok_or("integer overflow in mean")?;
            }
            Ok(Value::Int(total / i64::try_from(items.len()).map_err(|_| "mean item count overflow")?))
        }
        "speed" | "density" | "molarity" => {
            arity(2)?;
            let (Value::Int(numerator), Value::Int(denominator)) = (&args[0], &args[1]) else {
                return Err(format!("type error: {name} expects two integers"));
            };
            if *denominator <= 0 { return Err(format!("{name} denominator must be positive")); }
            Ok(Value::Int(numerator.checked_div(*denominator).ok_or("integer overflow in scientific calculation")?))
        }
        "force" => {
            arity(2)?;
            let (Value::Int(mass), Value::Int(acceleration)) = (&args[0], &args[1]) else {
                return Err("type error: force expects mass and acceleration as integers".into());
            };
            Ok(Value::Int(mass.checked_mul(*acceleration).ok_or("integer overflow in force")?))
        }
        "kinetic_energy" => {
            arity(2)?;
            let (Value::Int(mass), Value::Int(velocity)) = (&args[0], &args[1]) else {
                return Err("type error: kinetic_energy expects mass and velocity as integers".into());
            };
            if *mass < 0 { return Err("kinetic_energy mass cannot be negative".into()); }
            let velocity_squared = velocity.checked_mul(*velocity).ok_or("integer overflow in kinetic_energy")?;
            Ok(Value::Int(mass.checked_mul(velocity_squared).ok_or("integer overflow in kinetic_energy")? / 2))
        }
        "bmi_x10" => {
            arity(2)?;
            let (Value::Int(mass_kg), Value::Int(height_cm)) = (&args[0], &args[1]) else {
                return Err("type error: bmi_x10 expects mass in kg and height in cm as integers".into());
            };
            if *mass_kg <= 0 || *height_cm <= 0 { return Err("bmi_x10 mass and height must be positive".into()); }
            let numerator = mass_kg.checked_mul(100_000).ok_or("integer overflow in bmi_x10")?;
            let denominator = height_cm.checked_mul(*height_cm).ok_or("integer overflow in bmi_x10")?;
            Ok(Value::Int(numerator / denominator))
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
        "index_of" => {
            arity(2)?;
            match (&args[0], &args[1]) {
                (Value::Str(text), Value::Str(needle)) => {
                    let index = text.find(needle).map(|byte_index| text[..byte_index].chars().count());
                    Ok(Value::Int(index.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1)))
                }
                (Value::Array(items), needle) => {
                    let index = items.iter().position(|item| item == needle);
                    Ok(Value::Int(index.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1)))
                }
                (Value::Str(_), _) => Err("type error: index_of expects a string needle when searching a string".into()),
                _ => Err("type error: index_of expects a string or array as its first argument".into()),
            }
        }
        "trim" | "lower" | "upper" | "char_count" => {
            arity(1)?;
            let Value::Str(value) = &args[0] else {
                return Err(format!("type error: {name} expects a string"));
            };
            match name {
                "trim" => Ok(Value::Str(value.trim().to_string())),
                "lower" => {
                    let result = value.to_lowercase();
                    if result.len() > MAX_BYTECODE_STRING_BYTES { return Err("lower result exceeds string safety limit".into()); }
                    Ok(Value::Str(result))
                }
                "upper" => {
                    let result = value.to_uppercase();
                    if result.len() > MAX_BYTECODE_STRING_BYTES { return Err("upper result exceeds string safety limit".into()); }
                    Ok(Value::Str(result))
                }
                "char_count" => Ok(Value::Int(i64::try_from(value.chars().count()).map_err(|_| "character count overflow")?)),
                _ => unreachable!(),
            }
        }
        "char_at" => {
            arity(2)?;
            let Value::Str(value) = &args[0] else {
                return Err("type error: char_at expects a string as its first argument".into());
            };
            let Value::Int(index) = args[1] else {
                return Err("type error: char_at expects an integer index".into());
            };
            if index < 0 {
                return Err("char_at index must be non-negative".into());
            }
            value.chars().nth(index as usize)
                .map(|ch| Value::Str(ch.to_string()))
                .ok_or_else(|| "char_at index is out of range".to_string())
        }
        "to_int" => {
            arity(1)?;
            match &args[0] {
                Value::Int(value) => Ok(Value::Int(*value)),
                Value::Str(value) => value.trim().parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| format!("cannot convert '{value}' to a 64-bit integer")),
                _ => Err("type error: to_int expects an integer or numeric string".into()),
            }
        }
        "first" | "last" => {
            arity(1)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or_else(|| format!("{name} expects an array"))?;
            let Value::Array(items) = original else { return Err(format!("type error: {name} expects an array")); };
            if items.is_empty() { return Err(format!("{name} cannot be used with an empty array")); }
            if name == "first" {
                items.into_iter().next().ok_or_else(|| format!("{name} cannot be used with an empty array"))
            } else {
                items.into_iter().last().ok_or_else(|| format!("{name} cannot be used with an empty array"))
            }
        }
        "reverse" => {
            arity(1)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("reverse expects an array or string")?;
            match original {
                Value::Array(mut items) => { items.reverse(); Ok(Value::Array(items)) }
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
        "assert_eq" => {
            if !(2..=3).contains(&arg_len) {
                return Err(format!("builtin 'assert_eq' expects 2 or 3 argument(s), got {arg_len}"));
            }
            if args[0] == args[1] {
                Ok(Value::Bool(true))
            } else {
                let message = if args.len() == 3 {
                    value_to_string(&args[2])
                } else {
                    format!("assert_eq failed: left was {}, right was {}", value_to_string(&args[0]), value_to_string(&args[1]))
                };
                Err(message)
            }
        }
        "unique" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else {
                return Err("type error: unique expects an array".into());
            };
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                if !result.contains(item) {
                    result.push(item.clone());
                }
            }
            Ok(Value::Array(result))
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
    run_with_output_metered(code, output, None)
}

fn run_with_gas<F>(code: &[Op], output: &mut F, meter: &mut contract_gas::GasMeter) -> Result<(), String>
where
    F: FnMut(String),
{
    run_with_output_metered(code, output, Some(meter))
}

fn run_with_output_metered<F>(
    code: &[Op],
    output: &mut F,
    mut gas_meter: Option<&mut contract_gas::GasMeter>,
) -> Result<(), String>
where
    F: FnMut(String),
{
    if let Some(meter) = gas_meter.as_deref_mut() {
        meter.charge(contract_gas::BASE_EXECUTION_GAS)?;
    }
    let mut stack: Vec<Value> = Vec::new();
    let mut stack_value_bytes = 0usize;
    let mut runtime_state_bytes = 0usize;
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
        if let Some(meter) = gas_meter.as_deref_mut() {
            let cost = match code.get(pc) {
                Some(Op::Load(_)) => contract_gas::LOCAL_READ_GAS,
                Some(Op::Store(_) | Op::Declare(_) | Op::SetIndex(_)) => contract_gas::LOCAL_WRITE_GAS,
                Some(Op::JumpIfFalse(_) | Op::JumpIfFalseKeep(_) | Op::JumpIfTrueKeep(_)) => contract_gas::CONDITIONAL_BRANCH_GAS,
                Some(Op::Call(_, _) | Op::CallTarget { .. }) => contract_gas::FUNCTION_CALL_GAS,
                Some(Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Neg | Op::Not | Op::And | Op::Or
                    | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) => contract_gas::INTEGER_OPERATION_GAS,
                Some(_) => 1,
                None => return Err("instruction pointer out of range".into()),
            };
            meter.charge(cost)?;
        }
        match &code[pc] {
            Op::Push(n) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Int(*n))?,
            Op::PushString(value) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Str(value.clone()))?,
            Op::PushBool(value) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Bool(*value))?,
            Op::Load(name) => { let value = vars.get(name).or_else(|| globals.get(name)).cloned().ok_or_else(|| format!("undefined variable '{name}'"))?; push_stack_value(&mut stack, &mut stack_value_bytes, value)?; },
            Op::Store(name) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                if let Some(old) = vars.get(name) {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, Some(old), &value)?;
                    vars.insert(name.clone(), value);
                    runtime_state_bytes = next;
                } else if let Some(old) = globals.get(name) {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, Some(old), &value)?;
                    globals.insert(name.clone(), value);
                    runtime_state_bytes = next;
                } else {
                    return Err(format!("assignment to undefined variable '{name}'; use 'define' first"));
                }
            }
            Op::Declare(name) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                if call_stack.is_empty() {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, globals.get(name), &value)?;
                    globals.insert(name.clone(), value);
                    runtime_state_bytes = next;
                } else {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, vars.get(name), &value)?;
                    vars.insert(name.clone(), value);
                    runtime_state_bytes = next;
                }
            }
            Op::Print => match pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")? {
                Value::Int(n) => output(format!("{n}")),
                Value::Str(value) => output(value),
                Value::Bool(value) => output(format!("{value}")),
                value @ (Value::Array(_) | Value::Map(_)) => output(value_to_string(&value)),
            },
            Op::SetIndex(name) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading indexed value")?;
                let index = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading collection index")?;
                let old_binding = vars.get(name).or_else(|| globals.get(name))
                    .ok_or_else(|| format!("undefined variable '{name}'"))?;
                let old_binding_size = binding_size(name, old_binding);
                if vars.contains_key(name) {
                    let collection = vars.get_mut(name).ok_or_else(|| format!("undefined variable '{name}'"))?;
                    set_indexed_value(collection, index, value)?;
                } else {
                    let collection = globals.get_mut(name).ok_or_else(|| format!("undefined variable '{name}'"))?;
                    set_indexed_value(collection, index, value)?;
                }
                let updated = vars.get(name).or_else(|| globals.get(name))
                    .ok_or_else(|| format!("undefined variable '{name}'"))?;
                ensure_value_depth(updated)?;
                let next = runtime_state_bytes_after_sizes(runtime_state_bytes, old_binding_size, binding_size(name, updated))?;
                runtime_state_bytes = next;
            }
            Op::Index => {
                let index = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading collection index")?;
                let collection = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading collection")?;
                match (collection, index) {
                    (Value::Array(items), Value::Int(index)) if index >= 0 => push_stack_value(&mut stack, &mut stack_value_bytes, items.get(index as usize).cloned().ok_or_else(|| format!("array index {index} out of bounds"))?)?,
                    (Value::Array(_), Value::Int(index)) => return Err(format!("array index {index} out of bounds")),
                    (Value::Array(_), _) => return Err("type error: array index must be an integer".into()),
                    (Value::Map(items), Value::Str(key)) => push_stack_value(&mut stack, &mut stack_value_bytes, items.get(&key).cloned().ok_or_else(|| format!("map key '{key}' not found"))?)?,
                    (Value::Map(_), _) => return Err("type error: map index must be a string".into()),
                    _ => return Err("type error: indexing expects an array or map".into()),
                }
            }
            Op::Neg => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                match value {
                    Value::Int(n) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Int(n.checked_neg().ok_or("integer overflow")?))?,
                    Value::Bool(_) | Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: unary '-' expects an integer".into()),
                }
            }
            Op::Not => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                match value {
                    Value::Bool(value) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Bool(!value))?,
                    Value::Int(_) | Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: 'not' expects a boolean".into()),
                }
            }
            Op::JumpIfFalse(target) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
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
                for _ in 0..*arg_count { values.push(pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading builtin arguments")?); }
                values.reverse();
                push_stack_value(&mut stack, &mut stack_value_bytes, builtin(name, values)?)?;
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
                for _ in 0..*arg_count { values.push(pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading function arguments")?); }
                values.reverse();
                let added_bytes = params.iter().zip(values.iter()).map(|(name, value)| binding_size(name, value))
                    .fold(0usize, |total, size| total.saturating_add(size));
                let next_state_bytes = runtime_state_bytes.saturating_add(added_bytes);
                if next_state_bytes > MAX_RUNTIME_STATE_BYTES {
                    return Err(format!("live variable/call-frame values exceed safety limit ({MAX_RUNTIME_STATE_BYTES} estimated bytes)"));
                }
                let mut local_vars = HashMap::new();
                for (name, value) in params.iter().zip(values) { local_vars.insert(name.clone(), value); }
                let caller_vars = std::mem::replace(&mut vars, local_vars);
                call_stack.push((pc + 1, caller_vars));
                runtime_state_bytes = next_state_bytes;
                pc = *target;
                continue;
            }
            Op::Return => {
                let local_bytes = binding_map_size(&vars);
                let (return_pc, caller_vars) = call_stack.pop().ok_or("return used outside a function call")?;
                runtime_state_bytes = runtime_state_bytes.saturating_sub(local_bytes);
                vars = caller_vars;
                pc = return_pc;
                continue;
            }
            Op::ReturnValue => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow in return expression")?;
                let local_bytes = binding_map_size(&vars);
                let (return_pc, caller_vars) = call_stack.pop().ok_or("return used outside a function call")?;
                runtime_state_bytes = runtime_state_bytes.saturating_sub(local_bytes);
                vars = caller_vars;
                push_stack_value(&mut stack, &mut stack_value_bytes, value)?;
                pc = return_pc;
                continue;
            }
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::And | Op::Or => {
                let b = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                let a = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
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
                push_stack_value(&mut stack, &mut stack_value_bytes, value)?;
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
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SOURCE_TOKENS: usize = 1_000_000;
const MAX_PARSE_NESTING: usize = 256;
const MAX_VALUE_DEPTH: usize = 256;
const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_STACK_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_RUNTIME_STATE_BYTES: usize = 64 * 1024 * 1024;
const MAX_COLLECTION_ITEMS: usize = 100_000;
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
fn infer_function_return_kinds(code: &[Op], entries: &[usize]) -> Result<HashSet<usize>, String> {
    // Infer return behavior from control flow, not from the mere presence of a
    // RETURN_VALUE opcode. Dead code after an unconditional return must not change a
    // function's call-site stack effect. Mixed reachable void/value returns are rejected
    // because bytecode v1 has no explicit return-type metadata.
    let mut returns_value: HashSet<usize> = HashSet::new();
    for (index, entry) in entries.iter().copied().enumerate() {
        let end = entries.get(index + 1).copied().unwrap_or(code.len());
        let mut visited = HashSet::new();
        let mut pending = vec![entry];
        let mut saw_void_return = false;
        let mut saw_value_return = false;
        while let Some(pc) = pending.pop() {
            if pc < entry || pc >= end || !visited.insert(pc) {
                continue;
            }
            match &code[pc] {
                Op::Return => saw_void_return = true,
                Op::ReturnValue => saw_value_return = true,
                Op::Jump(target) => {
                    if *target >= entry && *target < end { pending.push(*target); }
                }
                Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) => {
                    if *target >= entry && *target < end { pending.push(*target); }
                    if pc + 1 < end { pending.push(pc + 1); }
                }
                _ => {
                    if pc + 1 < end { pending.push(pc + 1); }
                }
            }
        }
        if saw_void_return && saw_value_return {
            return Err(format!("bytecode verifier: function at entry {entry} has inconsistent reachable return kinds"));
        }
        if saw_value_return {
            returns_value.insert(entry);
        }
    }
    Ok(returns_value)
}

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
    let mut function_params: HashMap<usize, Vec<String>> = HashMap::new();
    for (index, op) in code.iter().enumerate() {
        match op {
            Op::CallTarget { target, params, arg_count } => {
                if entries.binary_search(target).is_err() {
                    return Err(format!("bytecode verifier: call at instruction {index} does not target a function entry"));
                }
                if params.len() != *arg_count {
                    return Err(format!("bytecode verifier: function entry {target} declares {} parameter(s), but CALL_TARGET records {arg_count}", params.len()));
                }
                if let Some(previous) = function_params.get(target) {
                    if previous != params {
                        return Err(format!("bytecode verifier: function entry {target} has inconsistent parameter metadata across call sites"));
                    }
                } else {
                    function_params.insert(*target, params.clone());
                }
                let mut seen_params = HashSet::new();
                for name in params {
                    if !valid_identifier(name) {
                        return Err(format!("bytecode verifier: function entry {target} has invalid parameter name"));
                    }
                    if !seen_params.insert(name) {
                        return Err(format!("bytecode verifier: function entry {target} has duplicate parameter"));
                    }
                }
            },
            Op::Jump(target) | Op::JumpIfFalse(target) | Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target)
                if *target > code.len() =>
                return Err(format!("bytecode verifier: jump at instruction {index} has invalid target {target}")),
            _ => {}
        }
    }

    let returns_value = infer_function_return_kinds(code, &entries)?;

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
                    let expected_depth = if matches!(op, Op::ReturnValue) { 1 } else { 0 };
                    if incoming_depth != expected_depth {
                        return Err(format!(
                            "bytecode verifier: function return at instruction {pc} has {incoming_depth} operand value(s); expected {expected_depth}"
                        ));
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

fn deserialize_bytecode_v2(source: &str) -> Result<Vec<Op>, String> {
    let file = bytecode_v2::parse_v2_file(source)?;
    bytecode_v2::verify_v2_file(&file)?;
    let mut code = Vec::with_capacity(file.instructions.len());

    for (pc, line) in file.instructions.iter().enumerate() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let invalid = || format!("bytecode v2: malformed instruction at index {pc}");
        let op = match parts.as_slice() {
            ["PUSH", value] => Op::Push(value.parse().map_err(|_| invalid())?),
            ["PUSH_STRING_HEX", value] => Op::PushString(hex_decode(value)?),
            ["PUSH_BOOL", "true"] => Op::PushBool(true),
            ["PUSH_BOOL", "false"] => Op::PushBool(false),
            ["LOAD", name] if valid_identifier(name) => Op::Load((*name).into()),
            ["STORE", name] if valid_identifier(name) => Op::Store((*name).into()),
            ["DECLARE", name] if valid_identifier(name) => Op::Declare((*name).into()),
            ["ADD"] => Op::Add,
            ["SUB"] => Op::Sub,
            ["MUL"] => Op::Mul,
            ["DIV"] => Op::Div,
            ["NEG"] => Op::Neg,
            ["NOT"] => Op::Not,
            ["AND"] => Op::And,
            ["OR"] => Op::Or,
            ["PRINT"] => Op::Print,
            ["EQ"] => Op::Eq,
            ["NE"] => Op::Ne,
            ["LT"] => Op::Lt,
            ["LE"] => Op::Le,
            ["GT"] => Op::Gt,
            ["GE"] => Op::Ge,
            ["INDEX"] => Op::Index,
            ["SET_INDEX", name] if valid_identifier(name) => Op::SetIndex((*name).into()),
            ["JUMP_IF_FALSE", target] => Op::JumpIfFalse(target.parse().map_err(|_| invalid())?),
            ["JUMP_IF_FALSE_KEEP", target] => Op::JumpIfFalseKeep(target.parse().map_err(|_| invalid())?),
            ["JUMP_IF_TRUE_KEEP", target] => Op::JumpIfTrueKeep(target.parse().map_err(|_| invalid())?),
            ["JUMP", target] => {
                let target: usize = target.parse().map_err(|_| invalid())?;
                // In v2 the main exit targets main_end; the shared VM must skip
                // the function regions when the main program finishes.
                if pc + 1 == file.layout.main_end && target == file.layout.main_end {
                    Op::Jump(file.layout.instruction_count)
                } else {
                    Op::Jump(target)
                }
            }
            ["CALL", target, count] => {
                let target: usize = target.parse().map_err(|_| invalid())?;
                let arg_count: usize = count.parse().map_err(|_| invalid())?;
                let function = file.layout.functions.iter().find(|function| function.entry == target)
                    .ok_or_else(|| format!("bytecode v2: call at instruction {pc} targets undeclared function {target}"))?;
                Op::CallTarget { target, params: function.parameters.clone(), arg_count }
            }
            ["BUILTIN", name, count] if valid_identifier(name) && is_builtin(name) => {
                Op::Builtin((*name).into(), count.parse().map_err(|_| invalid())?)
            }
            ["RETURN"] => Op::Return,
            ["RETURN_VALUE"] => Op::ReturnValue,
            _ => return Err(invalid()),
        };
        code.push(op);
    }
    Ok(code)
}

fn deserialize_bytecode(source: &str) -> Result<Vec<Op>, String> {
    if source.len() > MAX_BYTECODE_BYTES {
        return Err(format!("bytecode file exceeds safety limit ({} bytes)", MAX_BYTECODE_BYTES));
    }
    if source.lines().next().map(str::trim) == Some("ZENVORA_BYTECODE 2") {
        return deserialize_bytecode_v2(source);
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
                if arg_count != params.len() {
                    return Err(format!("bytecode verifier: function entry {target} declares {} parameter(s), but CALL_TARGET records {arg_count}", params.len()));
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

// Decode hexadecimal bytecode strings in exact two-character pairs.
fn hex_decode(input: &str) -> Result<String, String> {
    if input == "-" { return Ok(String::new()); }
    if input.len() / 2 > MAX_BYTECODE_STRING_BYTES {
        return Err(format!("bytecode string exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
    }
    if input.len() % 2 != 0 {
        return Err("invalid hex string in bytecode".into());
    }
    let mut bytes = Vec::with_capacity(input.len() / 2);
    for chunk in input.as_bytes().chunks_exact(2) {
        let digits = std::str::from_utf8(chunk).map_err(|_| "invalid hex string in bytecode")?;
        bytes.push(u8::from_str_radix(digits, 16).map_err(|_| "invalid hex string in bytecode")?);
    }
    String::from_utf8(bytes).map_err(|_| "bytecode string is not valid UTF-8".into())
}

fn valid_identifier(name: &str) -> bool {
    if name.is_empty() || !name.chars().enumerate().all(|(index, ch)| {
        ch == '_' || ch.is_ascii_alphanumeric() && (index > 0 || !ch.is_ascii_digit())
    }) {
        return false;
    }
    // These words are recognized by the lexer or statement parser and cannot
    // safely act as ordinary variable, function, parameter, or state names.
    !matches!(
        name,
        "true" | "false" | "and" | "or" | "not" |
        "define" | "show" | "print" | "if" | "otherwise" | "repeat" |
        "break" | "continue" | "return" | "call" | "fn" | "use" |
        "contract" | "state" | "action" | "view"
    )
}

fn read_bytecode_source(path: &str) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| format!("cannot read bytecode {path}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTECODE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read bytecode {path}: {error}"))?;
    if bytes.len() > MAX_BYTECODE_BYTES {
        return Err(format!("bytecode file exceeds safety limit ({} bytes)", MAX_BYTECODE_BYTES));
    }
    String::from_utf8(bytes).map_err(|error| format!("bytecode file is not valid UTF-8: {error}"))
}

fn read_program(path: &str) -> Result<String, String> {
    module_loader::load(Path::new(path))
}

/// Compiles a source file and all of its imports without executing the program.
fn check_program(path: &str) -> Result<(), String> {
    let source = read_program(path)?;
    compile(&source).map(|_| ())
}

fn serialize_bytecode_v2(code: &[Op], layout: &bytecode_v2::ProgramLayout) -> Result<String, String> {
    if code.len() != layout.instruction_count {
        return Err("bytecode v2: compiler layout does not match instruction count".into());
    }
    bytecode_v2::validate_layout(layout)?;
    let mut instructions = Vec::with_capacity(code.len());
    for (pc, op) in code.iter().enumerate() {
        let line = match op {
            Op::Push(value) => format!("PUSH {value}"),
            Op::PushString(value) => format!("PUSH_STRING_HEX {}", hex_encode(value)),
            Op::PushBool(value) => format!("PUSH_BOOL {value}"),
            Op::Load(name) if valid_identifier(name) => format!("LOAD {name}"),
            Op::Store(name) if valid_identifier(name) => format!("STORE {name}"),
            Op::Declare(name) if valid_identifier(name) => format!("DECLARE {name}"),
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
            Op::Index => "INDEX".into(),
            Op::SetIndex(name) if valid_identifier(name) => format!("SET_INDEX {name}"),
            Op::JumpIfFalse(target) => format!("JUMP_IF_FALSE {target}"),
            Op::JumpIfFalseKeep(target) => format!("JUMP_IF_FALSE_KEEP {target}"),
            Op::JumpIfTrueKeep(target) => format!("JUMP_IF_TRUE_KEEP {target}"),
            Op::Jump(target) if pc + 1 == layout.main_end && *target == code.len() => format!("JUMP {}", layout.main_end),
            Op::Jump(target) => format!("JUMP {target}"),
            Op::Call(name, _) => return Err(format!("bytecode v2: unresolved function call '{name}'")),
            Op::Builtin(name, count) if valid_identifier(name) && is_builtin(name) => format!("BUILTIN {name} {count}"),
            Op::CallTarget { target, arg_count, .. } => format!("CALL {target} {arg_count}"),
            Op::Return => "RETURN".into(),
            Op::ReturnValue => "RETURN_VALUE".into(),
            _ => return Err(format!("bytecode v2: unsupported or invalid operand at instruction {pc}")),
        };
        instructions.push(line);
    }
    let file = bytecode_v2::ParsedV2File { layout: layout.clone(), instructions };
    bytecode_v2::verify_v2_file(&file)?;
    bytecode_v2::serialize_v2_file(&file)
}

fn contract_abi_and_artifact(source: &str) -> Result<serde_json::Value, String> {
    let (code, layout) = compile_with_layout(source)?;
    let bytecode = serialize_bytecode_v2(&code, &layout)?;
    let mut contract_name = None;
    let mut states = Vec::new();
    let mut state_names = HashSet::new();
    let mut methods = Vec::new();
    let mut method_names = HashSet::new();
    let mut in_contract = false;
    let mut contract_depth = 0i32;

    for raw in source.lines() {
        let line = raw.trim();
        if !in_contract {
            if let Some(name) = line.strip_prefix("contract ").and_then(|rest| rest.strip_suffix('{')) {
                if contract_name.is_some() {
                    return Err("contract artifacts currently support one contract per source file".into());
                }
                let name = name.trim();
                if !valid_identifier(name) {
                    return Err("invalid contract name in artifact source".into());
                }
                contract_name = Some(name.to_string());
                in_contract = true;
            }
            if in_contract {
                let (opens, closes) = count_block_braces(raw);
                contract_depth += opens - closes;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("state ") {
            let (name, _) = rest.split_once(":=").ok_or_else(|| "malformed contract state declaration".to_string())?;
            let name = name.trim();
            if !valid_identifier(name) {
                return Err(format!("invalid contract state name '{name}'"));
            }
            if !state_names.insert(name.to_string()) {
                return Err(format!("duplicate contract state '{name}'"));
            }
            states.push(serde_json::json!({"name": name, "type": "integer"}));
        } else {
            for (kind, prefix) in [("action", "action "), ("view", "view ")] {
                if let Some(header) = line.strip_prefix(prefix).and_then(|rest| rest.strip_suffix('{')) {
                    let (name, args) = header.trim().split_once('(')
                        .ok_or_else(|| format!("malformed {kind} declaration"))?;
                    let name = name.trim();
                    if !valid_identifier(name) {
                        return Err(format!("invalid {kind} name '{name}'"));
                    }
                    if !method_names.insert(name.to_string()) {
                        return Err(format!("duplicate contract method '{name}'"));
                    }
                    let args = args.strip_suffix(')').ok_or_else(|| format!("malformed {kind} parameters"))?;
                    let parameters: Vec<serde_json::Value> = if args.trim().is_empty() {
                        Vec::new()
                    } else {
                        args.split(',').map(|arg| {
                            let name = arg.trim();
                            if valid_identifier(name) {
                                Ok(serde_json::json!({"name": name, "type": "dynamic"}))
                            } else {
                                Err(format!("invalid {kind} parameter '{name}'"))
                            }
                        }).collect::<Result<_, _>>()?
                    };
                    methods.push(serde_json::json!({
                        "name": name,
                        "kind": kind,
                        "inputs": parameters,
                        "outputs": if kind == "view" { serde_json::json!([{"type":"dynamic"}]) } else { serde_json::json!([]) }
                    }));
                    break;
                }
            }
        }

        let (opens, closes) = count_block_braces(raw);
        contract_depth += opens - closes;
        if contract_depth <= 0 {
            in_contract = false;
        }
    }
    let name = contract_name.ok_or_else(|| "source contains no contract declaration".to_string())?;
    if in_contract || contract_depth != 0 {
        return Err("contract is missing its closing brace".into());
    }
    let source_hash = Sha256::digest(source.as_bytes());
    let source_hash = source_hash.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    Ok(serde_json::json!({
        "format": "ZENVORA_CONTRACT_ARTIFACT_V1",
        "compiler_version": env!("CARGO_PKG_VERSION"),
        "contract": name,
        "source_sha256": source_hash,
        "abi": {"version": 1, "states": states, "methods": methods},
        "bytecode_v2": bytecode
    }))
}

fn write_contract_artifact(source_path: &str, output_path: &str) -> Result<(), String> {
    let source = read_program(source_path)?;
    let artifact = contract_abi_and_artifact(&source)?;
    let json = serde_json::to_string_pretty(&artifact)
        .map_err(|error| format!("cannot encode contract artifact: {error}"))?;
    let temp_path = format!("{output_path}.tmp");
    fs::write(&temp_path, format!("{json}\n"))
        .map_err(|error| format!("cannot write temporary artifact: {error}"))?;
    fs::rename(&temp_path, output_path)
        .map_err(|error| {
            let _ = fs::remove_file(&temp_path);
            format!("cannot install artifact at {output_path}: {error}")
        })
}

fn write_bytecode_v2(source_path: &str, output_path: &str) -> Result<(), String> {
    let source = read_program(source_path)?;
    let (code, layout) = compile_with_layout(&source)?;
    let serialized = serialize_bytecode_v2(&code, &layout)?;
    fs::write(output_path, serialized).map_err(|error| format!("cannot write {output_path}: {error}"))
}

fn write_bytecode(source_path: &str, output_path: &str) -> Result<(), String> {
    let source = read_program(source_path)?;
    let code = compile(&source)?;
    let serialized = serialize_bytecode(&code)?;
    fs::write(output_path, serialized).map_err(|error| format!("cannot write {output_path}: {error}"))
}

fn run_bytecode(path: &str) -> Result<(), String> {
    let serialized = read_bytecode_source(path)?;
    let code = deserialize_bytecode(&serialized)?;
    run(&code)
}

fn run_chain_command(args: &[String]) -> Result<(), String> {
    use std::path::Path;
    match args {
        [_, mode, key_path] if mode == "--chain-keygen" => {
            if Path::new(key_path).exists() {
                return Err(format!("key file already exists at {key_path}; refusing to overwrite it"));
            }
            let mut seed = [0_u8; 32];
            getrandom::getrandom(&mut seed).map_err(|error| format!("secure random key generation failed: {error}"))?;
            let signing_key = SigningKey::from_bytes(&seed);
            let public_key = signing_key.verifying_key().to_bytes();
            let address = crate::chain_protocol::address_from_public_key(&public_key)?;
            let content = format!(
                "ZENVORA_KEY_V1\nprivate={}\npublic={}\naddress={}\n",
                hex_encode_bytes(&seed), hex_encode_bytes(&public_key), address
            );
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(key_path)
                .map_err(|error| format!("cannot create key file: {error}"))?;
            use std::io::Write;
            if let Err(error) = file.write_all(content.as_bytes()).and_then(|_| file.sync_all()) {
                let _ = std::fs::remove_file(key_path);
                return Err(format!("cannot safely write key file: {error}"));
            }
            println!("Zenvora development key created: {key_path}");
            println!("Address: {address}");
            println!("WARNING: private key is unencrypted; keep this file private and do not use it for real funds.");
            Ok(())
        }
        [_, mode, store_path, chain_id, config_path] if mode == "--chain-store-init-multi" => {
            if Path::new(store_path).exists() {
                return Err(format!("block store already exists at {store_path}; refusing to overwrite it"));
            }
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let allocations = read_genesis_config(config_path, chain_id)?;
            let proposer = allocations.iter()
                .find(|(address, balance)| **balance > 0 && is_validator_address(address))
                .map(|(address, _)| address.clone())
                .ok_or_else(|| "genesis config needs at least one funded public-key-derived validator address".to_string())?;
            let mut runtime = chain_runtime::ChainRuntime::new();
            for (address, balance) in &allocations {
                runtime.set_genesis_balance(address, *balance)?;
            }
            let mut store = crate::block_store::BlockStore::new(chain_id, allocations)?;
            let header = crate::chain_protocol::BlockHeaderV1 {
                chain_id, height: 0, timestamp_secs: 0, parent_hash: [0; 32],
                transaction_root: crate::chain_protocol::transaction_merkle_root(&[])?,
                receipt_root: chain_runtime::ChainRuntime::receipt_root_for(&[])?,
                state_root: runtime.state_root(), proposer: proposer.clone(), consensus_proof: Vec::new(),
            };
            crate::chain_block::execute_block(&mut runtime, &header, &[], chain_id, None)?;
            store.append_executed_block(store_path, &runtime, &header, &[], &[])?;
            let validator_count = runtime.genesis_balances().iter()
                .filter(|(address, balance)| **balance > 0 && is_validator_address(address))
                .count();
            println!("Zenvora multi-account genesis initialized: chain_id={chain_id} validators={validator_count}");
            println!("Genesis proposer: {proposer}");
            println!("Block store: {store_path}");
            Ok(())
        }
        [_, mode, store_path, chain_id, key_path, balance] if mode == "--chain-store-init" => {
            if Path::new(store_path).exists() {
                return Err(format!("block store already exists at {store_path}; refusing to overwrite it"));
            }
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            if chain_id == 0 { return Err("chain ID must be nonzero".into()); }
            let signing_key = read_chain_key(key_path)?;
            let public_key = signing_key.verifying_key().to_bytes();
            let address = crate::chain_protocol::address_from_public_key(&public_key)?;
            let balance: i64 = balance.parse().map_err(|_| "genesis balance must be a valid 64-bit integer")?;
            if balance <= 0 {
                return Err("genesis validator balance must be positive".into());
            }
            let mut runtime = chain_runtime::ChainRuntime::new();
            runtime.set_genesis_balance(&address, balance)?;
            let mut store = crate::block_store::BlockStore::new(chain_id, runtime.genesis_balances().clone())?;
            let header = crate::chain_protocol::BlockHeaderV1 {
                chain_id,
                height: 0,
                timestamp_secs: 0,
                parent_hash: [0; 32],
                transaction_root: crate::chain_protocol::transaction_merkle_root(&[])?,
                receipt_root: chain_runtime::ChainRuntime::receipt_root_for(&[])?,
                state_root: runtime.state_root(),
                proposer: address.clone(),
                consensus_proof: Vec::new(),
            };
            crate::chain_block::execute_block(&mut runtime, &header, &[], chain_id, None)?;
            store.append_executed_block(store_path, &runtime, &header, &[], &[])?;
            println!("Zenvora local chain initialized: chain_id={chain_id} height=0");
            println!("Genesis account: {address} balance={balance}");
            println!("Block store: {store_path}");
            Ok(())
        }
        [_, mode, store_path, chain_id, key_path, recipient, amount, max_fee] if mode == "--chain-store-transfer" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            run_signed_transfer_cli(
                store_path, chain_id, key_path, recipient, amount, max_fee,
                std::slice::from_ref(key_path),
            )
        }
        [_, mode, store_path, chain_id, key_path, recipient, amount, max_fee, validator_paths @ ..]
            if mode == "--chain-store-transfer-multi" && !validator_paths.is_empty() => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            run_signed_transfer_cli(store_path, chain_id, key_path, recipient, amount, max_fee, validator_paths)
        }
        [_, mode, store_path, chain_id, pool_path, validator_paths @ ..]
            if mode == "--chain-store-produce" && !validator_paths.is_empty() => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            run_pool_block_cli(store_path, chain_id, pool_path, validator_paths)
        }
        [_, mode, store_path, chain_id, pool_path, proposer_key, peers @ ..]
            if mode == "--chain-store-produce-peer-quorum" && !peers.is_empty() => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            run_peer_quorum_block_cli(store_path, chain_id, pool_path, proposer_key, peers)
        }
        [_, mode, store_path, chain_id, pool_path] if mode == "--chain-rpc-pool" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            crate::chain_rpc::serve_with_pool(store_path.clone(), chain_id, pool_path.clone(), 8545)
        }
        [_, mode, store_path, chain_id, pool_path, port] if mode == "--chain-rpc-pool" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let port: u16 = port.parse().map_err(|_| "RPC port must be between 1 and 65535")?;
            crate::chain_rpc::serve_with_pool(store_path.clone(), chain_id, pool_path.clone(), port)
        }
        [_, mode, store_path, chain_id, address] if mode == "--chain-store-balance" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let (_, runtime) = crate::block_store::BlockStore::load_from_file(store_path, chain_id)?;
            println!("address={address} balance={} nonce={}", runtime.balance(address), runtime.account_nonce(address));
            Ok(())
        }
        [_, mode, store_path, chain_id] if mode == "--chain-store-info" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let (store, runtime) = crate::block_store::BlockStore::load_from_file(store_path, chain_id)?;
            let tip = store.blocks().last().ok_or_else(|| "block store has no genesis block".to_string())?;
            println!("chain_id={chain_id} blocks={} height={}", store.blocks().len(), tip.header.height);
            println!("state_root={}", hex_encode_bytes(&runtime.state_root()));
            println!("tip_hash={}", hex_encode_bytes(&tip.header.header_hash()?));
            println!("Replay verification: PASS (local file only; no consensus verification)");
            Ok(())
        }
        [_, mode, store_path, chain_id] if mode == "--chain-rpc" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            crate::chain_rpc::serve(store_path.clone(), chain_id, 8545)
        }
        [_, mode, store_path, chain_id, port] if mode == "--chain-rpc" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let port: u16 = port.parse().map_err(|_| "RPC port must be between 1 and 65535")?;
            crate::chain_rpc::serve(store_path.clone(), chain_id, port)
        }
        [_, mode, store_path, chain_id, pool_path, validator_key, bind_ip, port] if mode == "--chain-p2p-serve-validator" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let port: u16 = port.parse().map_err(|_| "P2P port must be between 1 and 65535")?;
            crate::chain_p2p::serve_with_validator(
                store_path.clone(), chain_id, pool_path.clone(), validator_key.clone(), bind_ip, port,
            )
        }
        [_, mode, store_path, chain_id, pool_path, bind_ip, port] if mode == "--chain-p2p-serve-pool" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let port: u16 = port.parse().map_err(|_| "P2P port must be between 1 and 65535")?;
            crate::chain_p2p::serve_with_pool(store_path.clone(), chain_id, pool_path.clone(), bind_ip, port)
        }
        [_, mode, store_path, chain_id, pool_path, peer] if mode == "--chain-p2p-gossip" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let sent = crate::chain_p2p::gossip_pool_to_peer(store_path, chain_id, pool_path, peer)?;
            println!("Gossiped {sent} new pending transaction(s) to {peer}");
            Ok(())
        }
        [_, mode, store_path, chain_id, bind_ip, port] if mode == "--chain-p2p-serve" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let port: u16 = port.parse().map_err(|_| "P2P port must be between 1 and 65535")?;
            crate::chain_p2p::serve(store_path.clone(), chain_id, bind_ip, port)
        }
        [_, mode, store_path, chain_id, peer] if mode == "--chain-p2p-sync" => {
            let chain_id: u64 = chain_id.parse().map_err(|_| "chain ID must be a positive integer")?;
            let synced = crate::chain_p2p::sync_from_peer(store_path, chain_id, peer)?;
            println!("Verified and synced {synced} block(s) from {peer}");
            Ok(())
        }
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

fn run_peer_quorum_block_cli(
    store_path: &str,
    chain_id: u64,
    pool_path: &str,
    proposer_key_path: &str,
    peers: &[String],
) -> Result<(), String> {
    if peers.is_empty() || peers.len() > 100 {
        return Err("supply between 1 and 100 validator peer addresses".into());
    }
    let proposer_key = read_chain_key(proposer_key_path)?;
    let (mut store, mut runtime) = crate::block_store::BlockStore::load_from_file(store_path, chain_id)?;
    let previous = store.blocks().last().cloned().ok_or_else(|| "block store has no genesis block".to_string())?;
    let height = previous.header.height.checked_add(1).ok_or_else(|| "block height overflow".to_string())?;
    let pool = crate::tx_pool::TransactionPool::load_from_file(pool_path, &runtime, chain_id, height)?;
    if pool.is_empty() {
        return Err("transaction pool is empty".into());
    }
    let envelopes = pool.pending().to_vec();
    let mut candidate = runtime.clone();
    let mut candidate_receipts = Vec::with_capacity(envelopes.len());
    for envelope in &envelopes {
        candidate_receipts.push(candidate.apply_signed_transaction(
            &envelope.transaction, &envelope.public_key, chain_id, height,
        )?);
    }
    let transactions: Vec<crate::chain_protocol::TransactionV1> = envelopes.iter()
        .map(|envelope| envelope.transaction.clone()).collect();
    let proposer_address = crate::chain_protocol::address_from_public_key(
        &proposer_key.verifying_key().to_bytes()
    )?;
    let mut header = crate::chain_protocol::BlockHeaderV1 {
        chain_id,
        height,
        timestamp_secs: current_unix_seconds()?.max(previous.header.timestamp_secs),
        parent_hash: previous.header.header_hash()?,
        transaction_root: crate::chain_protocol::transaction_merkle_root(&transactions)?,
        receipt_root: chain_runtime::ChainRuntime::receipt_root_for(&candidate_receipts)?,
        state_root: candidate.state_root(),
        proposer: proposer_address,
        consensus_proof: Vec::new(),
    };
    crate::chain_consensus::sign_header_as_proposer(&mut header, &proposer_key)?;
    let mut peer_errors = Vec::new();
    for peer in peers {
        if crate::chain_consensus::verify_consensus_proof(&header, runtime.genesis_balances()).is_ok() {
            break;
        }
        match crate::chain_p2p::request_block_signature(peer, &header, &envelopes) {
            Ok(proof) => {
                let previous_proof = header.consensus_proof.clone();
                header.consensus_proof = proof;
                if let Err(error) = crate::chain_consensus::verify_partial_consensus_proof(&header, runtime.genesis_balances()) {
                    header.consensus_proof = previous_proof;
                    peer_errors.push(format!("{peer}: {error}"));
                }
            }
            Err(error) => peer_errors.push(format!("{peer}: {error}")),
        }
    }
    let receipts = match crate::chain_block::execute_block(
        &mut runtime, &header, &envelopes, chain_id, Some(&previous.header),
    ) {
        Ok(value) => value,
        Err(error) => {
            if peer_errors.is_empty() { return Err(error); }
            return Err(format!("{error}; peer errors: {}", peer_errors.join("; ")));
        }
    };
    if receipts != candidate_receipts {
        return Err("executed receipts differ from candidate block receipts".into());
    }
    store.append_executed_block(store_path, &runtime, &header, &envelopes, &receipts)?;
    let next_height = height.checked_add(1).ok_or_else(|| "block height overflow".to_string())?;
    crate::tx_pool::TransactionPool::new(chain_id, next_height)?.save_to_file(pool_path)
        .map_err(|error| format!("block committed at height {height}, but transaction pool reset failed: {error}"))?;
    println!("Peer-quorum block committed: height={height} transactions={}", receipts.len());
    println!("Validator signatures: {}", header.consensus_proof.len() / 96);
    println!("State root: {}", hex_encode_bytes(&runtime.state_root()));
    Ok(())
}

fn run_pool_block_cli(
    store_path: &str,
    chain_id: u64,
    pool_path: &str,
    validator_key_paths: &[String],
) -> Result<(), String> {
    if validator_key_paths.is_empty() || validator_key_paths.len() > 42 {
        return Err("supply between 1 and 42 validator key files".into());
    }
    let validator_keys: Vec<SigningKey> = validator_key_paths.iter()
        .map(|path| read_chain_key(path))
        .collect::<Result<_, _>>()?;
    let (mut store, mut runtime) = crate::block_store::BlockStore::load_from_file(store_path, chain_id)?;
    let previous = store.blocks().last().cloned().ok_or_else(|| "block store has no genesis block".to_string())?;
    let height = previous.header.height.checked_add(1).ok_or_else(|| "block height overflow".to_string())?;
    let pool = crate::tx_pool::TransactionPool::load_from_file(pool_path, &runtime, chain_id, height)?;
    if pool.is_empty() {
        return Err("transaction pool is empty".into());
    }
    let envelopes = pool.pending().to_vec();
    let mut candidate = runtime.clone();
    let mut candidate_receipts = Vec::with_capacity(envelopes.len());
    for envelope in &envelopes {
        candidate_receipts.push(candidate.apply_signed_transaction(
            &envelope.transaction, &envelope.public_key, chain_id, height,
        )?);
    }
    let transactions: Vec<crate::chain_protocol::TransactionV1> = envelopes.iter()
        .map(|envelope| envelope.transaction.clone()).collect();
    let proposer = crate::chain_protocol::address_from_public_key(
        &validator_keys[0].verifying_key().to_bytes()
    )?;
    let mut header = crate::chain_protocol::BlockHeaderV1 {
        chain_id,
        height,
        timestamp_secs: current_unix_seconds()?.max(previous.header.timestamp_secs),
        parent_hash: previous.header.header_hash()?,
        transaction_root: crate::chain_protocol::transaction_merkle_root(&transactions)?,
        receipt_root: chain_runtime::ChainRuntime::receipt_root_for(&candidate_receipts)?,
        state_root: candidate.state_root(),
        proposer,
        consensus_proof: Vec::new(),
    };
    crate::chain_consensus::sign_header_as_proposer(&mut header, &validator_keys[0])?;
    for key in validator_keys.iter().skip(1) {
        crate::chain_consensus::add_validator_signature(&mut header, key)?;
    }
    let receipts = crate::chain_block::execute_block(
        &mut runtime, &header, &envelopes, chain_id, Some(&previous.header),
    )?;
    if receipts != candidate_receipts {
        return Err("executed receipts differ from candidate block receipts".into());
    }
    store.append_executed_block(store_path, &runtime, &header, &envelopes, &receipts)?;
    let next_height = height.checked_add(1).ok_or_else(|| "block height overflow".to_string())?;
    crate::tx_pool::TransactionPool::new(chain_id, next_height)?.save_to_file(pool_path)
        .map_err(|error| format!("block committed at height {height}, but transaction pool reset failed: {error}"))?;
    println!("Block committed from transaction pool: height={height} transactions={}", receipts.len());
    println!("Validator signatures: {}", validator_keys.len());
    println!("State root: {}", hex_encode_bytes(&runtime.state_root()));
    Ok(())
}

fn is_validator_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("zv")
        && address.as_bytes()[2..].iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn read_genesis_config(path: &str, expected_chain_id: u64) -> Result<BTreeMap<String, i64>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| format!("cannot inspect genesis config: {error}"))?;
    if metadata.len() > 1024 * 1024 {
        return Err("genesis config exceeds 1 MiB".into());
    }
    let content = std::fs::read_to_string(path).map_err(|error| format!("cannot read genesis config: {error}"))?;
    let mut lines = content.lines();
    if lines.next() != Some("ZENVORA_GENESIS_V1") {
        return Err("genesis config must start with ZENVORA_GENESIS_V1".into());
    }
    let chain_line = lines.next().ok_or_else(|| "genesis config is missing chain_id".to_string())?;
    let chain_value = chain_line.strip_prefix("chain_id=")
        .ok_or_else(|| "genesis config must have chain_id=<number> on line two".to_string())?;
    let chain_id: u64 = chain_value.parse().map_err(|_| "invalid genesis chain ID")?;
    if chain_id != expected_chain_id || chain_id == 0 {
        return Err("genesis config chain ID does not match requested chain".into());
    }
    let mut allocations = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        let line_number = index + 3;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut fields = line.split_whitespace();
        let address = fields.next().ok_or_else(|| format!("genesis line {line_number} is missing address"))?;
        let balance: i64 = fields.next().ok_or_else(|| format!("genesis line {line_number} is missing balance"))?
            .parse().map_err(|_| format!("genesis line {line_number} has invalid balance"))?;
        if fields.next().is_some() || balance < 0 || allocations.contains_key(address) {
            return Err(format!("genesis line {line_number} has extra fields, negative balance, or duplicate address"));
        }
        allocations.insert(address.to_string(), balance);
    }
    if allocations.is_empty() {
        return Err("genesis config must contain at least one allocation".into());
    }
    let validator_count = allocations.iter()
        .filter(|(address, balance)| **balance > 0 && is_validator_address(address))
        .count();
    if validator_count == 0 || validator_count > 42 {
        return Err("genesis config must contain 1 to 42 funded public-key-derived validator addresses".into());
    }
    let mut check = chain_runtime::ChainRuntime::new();
    for (address, balance) in &allocations {
        check.set_genesis_balance(address, *balance)?;
    }
    Ok(allocations)
}

fn run_signed_transfer_cli(
    store_path: &str,
    chain_id: u64,
    transaction_key_path: &str,
    recipient: &str,
    amount_text: &str,
    max_fee_text: &str,
    validator_key_paths: &[String],
) -> Result<(), String> {
    if validator_key_paths.is_empty() || validator_key_paths.len() > 42 {
        return Err("supply between 1 and 42 validator key files".into());
    }
    let transaction_key = read_chain_key(transaction_key_path)?;
    let public_key = transaction_key.verifying_key().to_bytes();
    let sender = crate::chain_protocol::address_from_public_key(&public_key)?;
    let amount: u64 = amount_text.parse().map_err(|_| "amount must be a positive integer")?;
    let max_fee: u64 = max_fee_text.parse().map_err(|_| "maximum fee reserve must be a nonnegative integer")?;
    let validator_keys: Vec<SigningKey> = validator_key_paths.iter()
        .map(|path| read_chain_key(path))
        .collect::<Result<_, _>>()?;
    let (mut store, mut runtime) = crate::block_store::BlockStore::load_from_file(store_path, chain_id)?;
    let previous = store.blocks().last().cloned().ok_or_else(|| "block store has no genesis block".to_string())?;
    let height = previous.header.height.checked_add(1).ok_or_else(|| "block height overflow".to_string())?;
    let mut transaction = crate::chain_protocol::TransactionV1 {
        chain_id,
        sender: sender.clone(),
        nonce: runtime.account_nonce(&sender),
        recipient: recipient.to_string(),
        amount,
        max_fee,
        gas_limit: 21_000,
        valid_until_height: None,
        signature: Vec::new(),
    };
    transaction.signature = transaction_key.sign(&transaction.signing_bytes()?).to_bytes().to_vec();
    let envelope = crate::chain_block::SignedTransactionEnvelope {
        transaction: transaction.clone(),
        public_key: public_key.to_vec(),
    };
    let mut candidate = runtime.clone();
    let receipt = candidate.apply_signed_transaction(&transaction, &public_key, chain_id, height)?;
    let first_validator_address = crate::chain_protocol::address_from_public_key(
        &validator_keys[0].verifying_key().to_bytes()
    )?;
    let mut header = crate::chain_protocol::BlockHeaderV1 {
        chain_id,
        height,
        timestamp_secs: current_unix_seconds()?.max(previous.header.timestamp_secs),
        parent_hash: previous.header.header_hash()?,
        transaction_root: crate::chain_protocol::transaction_merkle_root(&[transaction])?,
        receipt_root: chain_runtime::ChainRuntime::receipt_root_for(&[receipt])?,
        state_root: candidate.state_root(),
        proposer: first_validator_address,
        consensus_proof: Vec::new(),
    };
    crate::chain_consensus::sign_header_as_proposer(&mut header, &validator_keys[0])?;
    for key in validator_keys.iter().skip(1) {
        crate::chain_consensus::add_validator_signature(&mut header, key)?;
    }
    let receipts = crate::chain_block::execute_block(
        &mut runtime, &header, &[envelope.clone()], chain_id, Some(&previous.header),
    )?;
    store.append_executed_block(store_path, &runtime, &header, &[envelope], &receipts)?;
    println!("Block committed locally: height={height} chain_id={chain_id}");
    println!("Transaction: {}", receipts[0].transaction_id.as_deref().unwrap_or("unknown"));
    println!("Validator signatures: {}", validator_keys.len());
    println!("Status: {} | {} balance={} | {} balance={}",
        if receipts[0].accepted { "ACCEPTED" } else { "REJECTED" },
        receipts[0].from, receipts[0].sender_balance,
        receipts[0].to, receipts[0].recipient_balance);
    println!("WARNING: local prototype; fees are reserved but not charged, and this is not full BFT consensus.");
    Ok(())
}

fn read_chain_key(path: &str) -> Result<SigningKey, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read key file: {error}"))?;
    let mut lines = content.lines();
    if lines.next() != Some("ZENVORA_KEY_V1") {
        return Err("unsupported or malformed Zenvora key file".into());
    }
    let private_line = lines.next().ok_or_else(|| "key file is missing private seed".to_string())?;
    let public_line = lines.next().ok_or_else(|| "key file is missing public key".to_string())?;
    let address_line = lines.next().ok_or_else(|| "key file is missing address".to_string())?;
    if lines.next().is_some() {
        return Err("key file contains unexpected extra lines".into());
    }
    let seed: [u8; 32] = decode_hex_bytes(private_line.strip_prefix("private=").ok_or_else(|| "invalid private-key field".to_string())?)?
        .try_into().map_err(|_| "private seed must be 32 bytes")?;
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key = signing_key.verifying_key().to_bytes();
    if hex_encode_bytes(&public_key) != public_line.strip_prefix("public=").unwrap_or("") {
        return Err("key file public key does not match private seed".into());
    }
    let address = crate::chain_protocol::address_from_public_key(&public_key)?;
    if address != address_line.strip_prefix("address=").unwrap_or("") {
        return Err("key file address does not match public key".into());
    }
    Ok(signing_key)
}

fn decode_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("invalid lowercase hexadecimal value".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        output.push(((high << 4) | low) as u8);
    }
    Ok(output)
}

fn hex_encode_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn current_unix_seconds() -> Result<u64, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_secs())
}

fn create_project(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    if name.is_empty() || path.components().count() != 1
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || !name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        return Err("project name must start with a letter or digit and contain only letters, digits, '-' or '_'".into());
    }
    fs::create_dir(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!("project path '{name}' already exists; refusing to overwrite it")
        } else { format!("cannot create project '{name}': {error}") }
    })?;
    let main_source = "# Welcome to your first Zenvora project!\ndefine student := \"Creator\"\nshow \"Hello, \" + student\nshow \"Your first calculation:\"\nshow 6 * 7\n\ndefine scores := array(10, 20, 30)\nshow \"Total score:\"\nshow sum(scores)\n";
    let readme = format!("# {name}\n\nA starter project written in Zenvora Lang.\n\nRun from the Zenvora Lang repository root:\n\n    cargo run -- {name}/main.zv\n    cargo run -- --check {name}/main.zv\n\nEdit main.zv to make this project your own.\n");
    if let Err(error) = fs::write(path.join("main.zv"), main_source).and_then(|_| fs::write(path.join("README.md"), readme)) {
        let _ = fs::remove_dir_all(path);
        return Err(format!("cannot write starter project files: {error}"));
    }
    println!("Created Zenvora project '{name}'");
    println!("Next: zv {name}/main.zv");
    Ok(())
}
fn run_contract_command(
    source_path: &str,
    contract_id: &str,
    snapshot_path: &str,
    mode_name: &str,
    method_call: &str,
    gas_limit: u64,
) -> Result<contract_runtime::ExecutionReceipt, String> {
    let mode = match mode_name {
        "action" => contract_state::AccessMode::Action,
        "view" => contract_state::AccessMode::View,
        _ => return Err("contract mode must be 'action' or 'view'".into()),
    };
    let source = read_program(source_path)?;
    let snapshot = Path::new(snapshot_path);
    let mut state = if snapshot.exists() {
        contract_state::ContractState::load_snapshot(snapshot)?
    } else if mode == contract_state::AccessMode::View {
        return Err("contract snapshot does not exist; run an action first to initialize persistent state".into());
    } else {
        contract_state::ContractState::new()
    };
    let verb = if mode == contract_state::AccessMode::Action { "call" } else { "show" };
    let invocation = format!("{verb} {method_call}");
    let receipt = contract_runtime::execute(&source, contract_id, &invocation, mode, &mut state, gas_limit)?;
    if mode == contract_state::AccessMode::Action {
        state.save_snapshot(snapshot)?;
    }
    Ok(receipt)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("Zenvora Lang v0.8.0");
        return;
    }
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("Zenvora Lang v0.8.0\nUsage: zv <file.zv>\n       zv --compiler-api [port]  (loopback development API, default 8787)\n       zv --new <project-name>\n       zv --check <file.zv>\n       zv --emit-bytecode <file.zv> <output.zvb>\n       zv --emit-bytecode-v2 <file.zv> <output.zv2b>\n       zv --run-bytecode <file.zvb>\n       zv --contract-run <file.zv> <contract-id> <snapshot.bin> <action|view> <method(args)> [gas-limit] (experimental local integer-state runtime)\n       zv --chain-init <ledger.zvr> <address> <balance>\n       zv --chain-transfer <ledger.zvr> <from> <to> <amount>\n       zv --chain-balance <ledger.zvr> <address>\n       zv --chain-receipts <ledger.zvr>\n       zv --chain-keygen <keyfile>\n       zv --chain-store-init <store.zvb> <chain-id> <keyfile> <balance>\n       zv --chain-store-init-multi <store.zvb> <chain-id> <genesis.txt>\n       zv --chain-store-transfer <store.zvb> <chain-id> <keyfile> <recipient> <amount> <max-fee>\n       zv --chain-store-transfer-multi <store.zvb> <chain-id> <tx-key> <recipient> <amount> <max-fee> <validator-key>...\n       zv --chain-store-balance <store.zvb> <chain-id> <address>\n       zv --chain-store-info <store.zvb> <chain-id>\n       zv --chain-store-produce <store.zvb> <chain-id> <pool-file> <validator-key>...\n       zv --chain-store-produce-peer-quorum <store.zvb> <chain-id> <pool-file> <proposer-key> <peer-ip:port>...\n       zv --chain-rpc <store.zvb> <chain-id> [port]\n       zv --chain-rpc-pool <store.zvb> <chain-id> <pool-file> [port]\n       zv --chain-p2p-serve <store.zvb> <chain-id> <bind-ip> <port>\n       zv --chain-p2p-serve-pool <store.zvb> <chain-id> <pool-file> <bind-ip> <port>\n       zv --chain-p2p-serve-validator <store.zvb> <chain-id> <pool-file> <validator-key> <bind-ip> <port>\n       zv --chain-p2p-gossip <store.zvb> <chain-id> <pool-file> <peer-ip:port>\n       zv --chain-p2p-sync <store.zvb> <chain-id> <peer-ip:port>\n\nSyntax: define total := 10 + 20\n        show total * 2\n        if total >= 18 {{ ... }}");
        return;
    }
    if args.get(1).is_some_and(|mode| mode == "--contract-run") {
        if args.len() != 7 && args.len() != 8 {
            eprintln!("Usage: zv --contract-run <file.zv> <contract-id> <snapshot.bin> <action|view> <method(args)> [gas-limit]");
            process::exit(2);
        }
        let gas_limit = if args.len() == 8 {
            match args[7].parse::<u64>() {
                Ok(limit) => limit,
                Err(_) => {
                    eprintln!("Zenvora contract error: gas-limit must be an unsigned integer");
                    process::exit(2);
                }
            }
        } else {
            100_000
        };
        match run_contract_command(&args[2], &args[3], &args[4], &args[5], &args[6], gas_limit) {
            Ok(receipt) => {
                for line in receipt.output { println!("{line}"); }
                println!("gas_used={}", receipt.gas_used);
                println!("state_root={}", hex_encode_bytes(&receipt.state_root));
            }
            Err(error) => {
                eprintln!("Zenvora contract error: {error}");
                process::exit(1);
            }
        }
        return;
    }
    if args.get(1).is_some_and(|mode| mode == "--compiler-api") {
        let port = args.get(2).map(|value| value.parse::<u16>().map_err(|_| "compiler API port must be between 1 and 65535".to_string())).transpose().unwrap_or_else(|error| { eprintln!("Zenvora API error: {error}"); process::exit(2) }).unwrap_or(8787);
        if let Err(error) = compiler_api::serve(port) { eprintln!("Zenvora API error: {error}"); process::exit(1); }
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
        [_, mode, name] if mode == "--new" => create_project(name),
        [_, mode, path] if mode == "--check" => check_program(path).map(|_| println!("Zenvora check passed: {path}")),
        [_, mode, source, output] if mode == "--emit-bytecode" => write_bytecode(source, output),
        [_, mode, source, output] if mode == "--emit-bytecode-v2" => write_bytecode_v2(source, output),
        [_, mode, source, output] if mode == "--contract-artifact" => write_contract_artifact(source, output),
        [_, mode, bytecode] if mode == "--run-bytecode" => run_bytecode(bytecode),
        [_, path] if !path.starts_with("--") => {
            let source = read_program(path).unwrap_or_else(|error| {
                eprintln!("Zenvora error: {error}");
                process::exit(1)
            });
            compile(&source).and_then(|code| run(&code))
        }
        _ => {
            eprintln!("Usage: zv <file.zv> | zv --new <project-name> | zv --check <file.zv> | zv --emit-bytecode <file.zv> <output.zvb> | zv --emit-bytecode-v2 <file.zv> <output.zv2b> | zv --contract-artifact <contract.zv> <artifact.json> | zv --run-bytecode <file.zvb> | zv --chain-init <ledger.zvr> <address> <balance> | zv --chain-transfer <ledger.zvr> <from> <to> <amount> | zv --chain-balance <ledger.zvr> <address> | zv --chain-receipts <ledger.zvr> | zv --chain-keygen <keyfile> | zv --chain-store-init <store.zvb> <chain-id> <keyfile> <balance> | zv --chain-store-transfer <store.zvb> <chain-id> <keyfile> <recipient> <amount> <max-fee> | zv --chain-store-balance <store.zvb> <chain-id> <address> | zv --chain-store-info <store.zvb> <chain-id> | zv --chain-rpc <store.zvb> <chain-id> [port] | zv --chain-p2p-serve <store.zvb> <chain-id> <bind-ip> <port> | zv --chain-p2p-sync <store.zvb> <chain-id> <peer-ip:port> | zv --chain-p2p-serve-pool <store.zvb> <chain-id> <pool-file> <bind-ip> <port> | zv --chain-p2p-serve-validator <store.zvb> <chain-id> <pool-file> <validator-key> <bind-ip> <port> | zv --chain-p2p-gossip <store.zvb> <chain-id> <pool-file> <peer-ip:port> | zv --chain-store-produce <store.zvb> <chain-id> <pool-file> <validator-key>... | zv --chain-store-produce-peer-quorum <store.zvb> <chain-id> <pool-file> <proposer-key> <peer-ip:port>... | zv --chain-rpc-pool <store.zvb> <chain-id> <pool-file> [port]");
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
    use super::{compile, compile_core, lex, read_chain_key, run_chain_command, run_with_output, Op};
    use ed25519_dalek::Signer;

    #[test]
    fn local_signed_chain_cli_keygen_transfer_and_replay_work_end_to_end() {
        let base = std::env::temp_dir();
        let key_path = base.join(format!("zenvora-cli-key-{}.zvk", std::process::id()));
        let store_path = base.join(format!("zenvora-cli-store-{}.zvb", std::process::id()));
        let second_store_path = base.join(format!("zenvora-cli-store-second-{}.zvb", std::process::id()));
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_file(&store_path);
        let _ = std::fs::remove_file(&second_store_path);
        let key_text = key_path.to_string_lossy().to_string();
        let store_text = store_path.to_string_lossy().to_string();
        let second_store_text = second_store_path.to_string_lossy().to_string();

        run_chain_command(&vec!["zv".into(), "--chain-keygen".into(), key_text.clone()]).unwrap();
        assert!(run_chain_command(&vec!["zv".into(), "--chain-keygen".into(), key_text.clone()]).is_err());
        let key = read_chain_key(&key_text).unwrap();
        let address = crate::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();

        run_chain_command(&vec![
            "zv".into(), "--chain-store-init".into(), store_text.clone(), "71".into(),
            key_text.clone(), "1000".into(),
        ]).unwrap();
        run_chain_command(&vec![
            "zv".into(), "--chain-store-init".into(), second_store_text.clone(), "71".into(),
            key_text.clone(), "1000".into(),
        ]).unwrap();
        let (first_genesis, _) = crate::block_store::BlockStore::load_from_file(&store_text, 71).unwrap();
        let (second_genesis, _) = crate::block_store::BlockStore::load_from_file(&second_store_text, 71).unwrap();
        assert_eq!(
            first_genesis.blocks()[0].header.header_hash().unwrap(),
            second_genesis.blocks()[0].header.header_hash().unwrap(),
        );
        run_chain_command(&vec![
            "zv".into(), "--chain-store-transfer".into(), store_text.clone(), "71".into(),
            key_text.clone(), "bob".into(), "100".into(), "1".into(),
        ]).unwrap();

        let (store, runtime) = crate::block_store::BlockStore::load_from_file(&store_text, 71).unwrap();
        assert_eq!(store.blocks().len(), 2);
        assert_eq!(runtime.balance(&address), 900);
        assert_eq!(runtime.account_nonce(&address), 1);
        assert_eq!(runtime.balance("bob"), 100);

        let _ = std::fs::remove_file(key_path);
        let _ = std::fs::remove_file(store_path);
        let _ = std::fs::remove_file(second_store_path);
    }

    #[test]
    fn persisted_transaction_pool_can_be_produced_into_a_validator_signed_block() {
        let base = std::env::temp_dir();
        let suffix = std::process::id();
        let key_path = base.join(format!("zenvora-pool-producer-key-{suffix}.zvk"));
        let store_path = base.join(format!("zenvora-pool-producer-store-{suffix}.zvb"));
        let pool_path = base.join(format!("zenvora-pool-producer-{suffix}.zvp"));
        for path in [&key_path, &store_path, &pool_path] {
            let _ = std::fs::remove_file(path);
        }
        let key_text = key_path.to_string_lossy().to_string();
        let store_text = store_path.to_string_lossy().to_string();
        let pool_text = pool_path.to_string_lossy().to_string();
        run_chain_command(&vec!["zv".into(), "--chain-keygen".into(), key_text.clone()]).unwrap();
        run_chain_command(&vec![
            "zv".into(), "--chain-store-init".into(), store_text.clone(), "71".into(),
            key_text.clone(), "1000".into(),
        ]).unwrap();

        let key = read_chain_key(&key_text).unwrap();
        let public_key = key.verifying_key().to_bytes();
        let address = crate::chain_protocol::address_from_public_key(&public_key).unwrap();
        let (_, runtime) = crate::block_store::BlockStore::load_from_file(&store_text, 71).unwrap();
        let mut transaction = crate::chain_protocol::TransactionV1 {
            chain_id: 71, sender: address, nonce: 0, recipient: "bob".into(),
            amount: 100, max_fee: 1, gas_limit: 21_000, valid_until_height: Some(20),
            signature: Vec::new(),
        };
        transaction.signature = key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        let envelope = crate::chain_block::SignedTransactionEnvelope {
            transaction, public_key: public_key.to_vec(),
        };
        let mut pool = crate::tx_pool::TransactionPool::new(71, 1).unwrap();
        pool.submit(&runtime, envelope).unwrap();
        pool.save_to_file(&pool_text).unwrap();

        run_chain_command(&vec![
            "zv".into(), "--chain-store-produce".into(), store_text.clone(), "71".into(),
            pool_text.clone(), key_text.clone(),
        ]).unwrap();
        let (store, runtime) = crate::block_store::BlockStore::load_from_file(&store_text, 71).unwrap();
        assert_eq!(store.blocks().len(), 2);
        assert_eq!(runtime.balance("bob"), 100);
        let next_pool = crate::tx_pool::TransactionPool::load_from_file(&pool_text, &runtime, 71, 2).unwrap();
        assert!(next_pool.is_empty());
        for path in [&key_path, &store_path, &pool_path] {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn multi_validator_cli_requires_quorum_and_replays_signed_block() {
        let base = std::env::temp_dir();
        let suffix = std::process::id();
        let key_paths: Vec<std::path::PathBuf> = (0..4)
            .map(|index| base.join(format!("zenvora-multi-key-{suffix}-{index}.zvk")))
            .collect();
        let store_path = base.join(format!("zenvora-multi-store-{suffix}.zvb"));
        let config_path = base.join(format!("zenvora-multi-genesis-{suffix}.txt"));
        for path in key_paths.iter().chain([&store_path, &config_path]) {
            let _ = std::fs::remove_file(path);
        }
        let key_texts: Vec<String> = key_paths.iter().map(|path| path.to_string_lossy().to_string()).collect();
        for path in &key_texts {
            run_chain_command(&vec!["zv".into(), "--chain-keygen".into(), path.clone()]).unwrap();
        }
        let mut allocations = Vec::new();
        for path in &key_texts {
            let key = read_chain_key(path).unwrap();
            let address = crate::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
            allocations.push(format!("{address} 1000"));
        }
        let config = format!("ZENVORA_GENESIS_V1\nchain_id=71\n{}\n", allocations.join("\n"));
        std::fs::write(&config_path, config).unwrap();
        let store_text = store_path.to_string_lossy().to_string();
        let config_text = config_path.to_string_lossy().to_string();
        run_chain_command(&vec![
            "zv".into(), "--chain-store-init-multi".into(), store_text.clone(), "71".into(), config_text,
        ]).unwrap();

        let insufficient = vec![
            "zv".into(), "--chain-store-transfer-multi".into(), store_text.clone(), "71".into(),
            key_texts[0].clone(), "bob".into(), "100".into(), "1".into(),
            key_texts[0].clone(), key_texts[1].clone(),
        ];
        assert!(run_chain_command(&insufficient).is_err());
        let (before, before_runtime) = crate::block_store::BlockStore::load_from_file(&store_text, 71).unwrap();
        assert_eq!(before.blocks().len(), 1);
        assert_eq!(before_runtime.balance("bob"), 0);

        let sufficient = vec![
            "zv".into(), "--chain-store-transfer-multi".into(), store_text.clone(), "71".into(),
            key_texts[0].clone(), "bob".into(), "100".into(), "1".into(),
            key_texts[0].clone(), key_texts[1].clone(), key_texts[2].clone(),
        ];
        run_chain_command(&sufficient).unwrap();
        let (store, runtime) = crate::block_store::BlockStore::load_from_file(&store_text, 71).unwrap();
        assert_eq!(store.blocks().len(), 2);
        assert_eq!(runtime.balance("bob"), 100);

        for path in key_paths.iter().chain([&store_path, &config_path]) {
            let _ = std::fs::remove_file(path);
        }
    }

    fn execute_source_for_test(source: &str) -> Result<Vec<String>, String> {
        let code = compile(source)?;
        let mut output = Vec::new();
        run_with_output(&code, &mut |line| output.push(line))?;
        Ok(output)
    }

    fn execute(source: &str) -> Result<Vec<String>, String> {
        let code = compile(source)?;
        let mut output = Vec::new();
        run_with_output(&code, &mut |line| output.push(line))?;
        Ok(output)
    }

    #[test]
    fn new_project_creates_runnable_files_and_refuses_overwrite() {
        let name = format!("zenvora-test-project-{}", std::process::id());
        let path = std::path::Path::new(&name);
        let _ = std::fs::remove_dir_all(path);
        super::create_project(&name).unwrap();
        assert!(path.join("main.zv").is_file());
        assert!(path.join("README.md").is_file());
        let source = std::fs::read_to_string(path.join("main.zv")).unwrap();
        assert_eq!(execute_source_for_test(&source).unwrap(), vec!["Hello, Creator", "Your first calculation:", "42", "Total score:", "60"]);
        assert!(super::create_project(&name).unwrap_err().contains("refusing to overwrite"));
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn new_project_rejects_unsafe_names() {
        assert!(super::create_project("../escape").is_err());
        assert!(super::create_project("").is_err());
    }

    #[test]
    fn new_project_name_validation_accepts_simple_names_and_rejects_paths() {
        assert!(super::create_project("../escape").is_err());
        assert!(super::create_project("nested/project").is_err());
        assert!(super::create_project("bad name").is_err());
        assert!(super::create_project("").is_err());
        assert!(super::create_project("-starts-with-dash").is_err());
    }
    #[test]
    fn check_program_compiles_without_running_side_effects() {
        let path = std::env::temp_dir().join(format!("zenvora-check-{}.zv", std::process::id()));
        std::fs::write(&path, concat!("show 1 / 0", "\n")).unwrap();
        let path_text = path.to_string_lossy().to_string();
        // Compilation succeeds even though executing this program would divide by zero.
        assert!(super::check_program(&path_text).is_ok());
        std::fs::write(&path, concat!("show (1 + )", "\n")).unwrap();
        assert!(super::check_program(&path_text).unwrap_err().contains("expected"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn advanced_math_and_science_builtins_are_checked() {
        let output = execute("show gcd(48, 18)\nshow lcm(12, 18)\nshow factorial(5)\nshow is_prime(97)\nshow fibonacci(10)\nshow pow_int(3, 4)\nshow mean(array(2, 4, 9))\nshow speed(100, 5)\nshow force(10, 9)\nshow kinetic_energy(2, 5)\nshow density(12, 3)\nshow molarity(6, 2)\nshow bmi_x10(70, 175)\n").unwrap();
        assert_eq!(output, vec!["6", "36", "120", "true", "55", "81", "5", "20", "90", "25", "4", "3", "228"]);
    }

    #[test]
    fn chemistry_and_biology_helpers_validate_scientific_inputs() {
        let output = execute("show molar_mass(array(12, 16, 1, 1, 1, 1))\nshow dna_complement(\"ACGT\")\nshow gc_content_percent(\"GCGC\")\nshow gc_content_percent(\"ATGC\")\n").unwrap();
        assert_eq!(output, vec!["32", "TGCA", "100", "50"]);
        assert!(execute("show dna_complement(\"ACNX\")\n").unwrap_err().contains("invalid DNA base"));
        assert!(execute("show gc_content_percent(\"\")\n").unwrap_err().contains("empty DNA string"));
    }

    #[test]
    fn advanced_math_builtins_reject_invalid_inputs_and_overflow() {
        assert!(execute("show factorial(21)\n").unwrap_err().contains("0 through 20"));
        assert!(execute("show pow_int(9223372036854775807, 2)\n").unwrap_err().contains("integer overflow"));
        assert!(execute("show speed(10, 0)\n").unwrap_err().contains("denominator must be positive"));
        assert!(execute("show bmi_x10(70, 0)\n").unwrap_err().contains("must be positive"));
    }


    #[test]
    fn compiler_executes_nested_functions_conditionals_loops_and_expression_precedence() {
        let source = "define answer := 0\nfn add_or_fallback(a, b) {\nif a > 0 {\nreturn a + b\n} otherwise {\nreturn b\n}\n}\nshow add_or_fallback(2, 3)\nshow add_or_fallback(0, 7)\nrepeat answer < 3 {\nanswer := answer + 1\n}\nshow answer\nshow true or false and false\nshow array(10, 20)[1]\n";
        let output = execute(source).unwrap();
        assert_eq!(output, vec!["5", "7", "3", "true", "20"]);
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
    fn compile_reports_lexer_columns_in_full_source_line() {
        let err = compile_core("   show @").unwrap_err();
        assert!(err.contains("line 1: column 9: unexpected character"), "{err}");
        let err = compile_core("show \"bad\\q\"").unwrap_err();
        assert!(err.contains("line 1: column 11: unsupported string escape"), "{err}");
        let err = compile_core("show \"नमस्ते\" + @").unwrap_err();
        assert!(err.contains("line 1: column 17: unexpected character"), "{err}");
    }

    #[test]
    fn repeat_condition_lexer_columns_include_source_prefix() {
        let err = compile_core("   repeat count < @ {").unwrap_err();
        assert!(err.contains("line 1: column 19: unexpected character"), "{err}");
    }

    #[test]
    fn parser_rejects_excessive_expression_nesting_without_overflowing_the_stack() {
        let nested = format!("show {}1{}\n", "(".repeat(super::MAX_PARSE_NESTING + 8), ")".repeat(super::MAX_PARSE_NESTING + 8));
        let error = compile_core(&nested).unwrap_err();
        assert!(error.contains("parser nesting exceeds safety limit"), "{error}");

        let unary = format!("show {}1\n", "-".repeat(super::MAX_PARSE_NESTING + 8));
        let error = compile_core(&unary).unwrap_err();
        assert!(error.contains("parser nesting exceeds safety limit"), "{error}");
    }

    #[test]
    fn parser_errors_include_one_based_source_columns() {
        let err = compile_core("   show 1 +").unwrap_err();
        assert!(err.contains("line 1: column 12: expected a number"), "{err}");

        let err = compile_core("show foo(1, )").unwrap_err();
        assert!(err.contains("line 1: column 13: expected a number"), "{err}");

        let err = compile_core("show 1 2").unwrap_err();
        assert!(err.contains("line 1: column 8: unexpected token"), "{err}");

        let err = compile_core("if true + {\n}\n").unwrap_err();
        assert!(err.contains("line 1: column 10: expected a number"), "{err}");

        let err = compile_core("repeat count < {\n}\n").unwrap_err();
        assert!(err.contains("line 1: column 15: expected a number"), "{err}");
    }

    #[test]
    fn compiler_rejects_source_over_byte_limit_before_lexing() {
        let source = " ".repeat(super::MAX_SOURCE_BYTES + 1);
        let error = super::compile(&source).unwrap_err();
        assert!(error.contains("source exceeds safety limit"), "{error}");
    }

    #[test]
    fn lexer_rejects_excessive_token_count() {
        let source = "1+".repeat(super::MAX_SOURCE_TOKENS / 2 + 1);
        let error = super::lex(&source).unwrap_err();
        assert!(error.contains("token count exceeds safety limit"), "{error}");
    }

    #[test]
    fn lexer_errors_include_one_based_source_columns() {
        assert!(lex("show @").unwrap_err().contains("column 6: unexpected character"));
        assert!(lex("show \"bad\\q\"").unwrap_err().contains("column 11: unsupported string escape"));
        assert!(lex("\"unterminated").unwrap_err().contains("column 14: unterminated string literal"));
        assert!(lex("show \"bad\\").unwrap_err().contains("column 11: unterminated escape"));
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
    fn bytecode_verifier_rejects_inconsistent_function_parameter_metadata() {
        let bytecode = "ZENVORA_BYTECODE 1\nCALL_TARGET 4 1 -\nPUSH 1\nPRINT\nJUMP 5\nRETURN\n";
        let error = super::deserialize_bytecode(bytecode).unwrap_err();
        assert!(error.contains("declares 0 parameter(s)") && error.contains("records 1"), "unexpected error: {error}");
    }

    #[test]
    fn bytecode_verifier_rejects_inconsistent_function_parameter_metadata_across_calls() {
        let bytecode = "ZENVORA_BYTECODE 1\nCALL_TARGET 4 1 first\nCALL_TARGET 4 1 second\nJUMP 5\nPUSH 1\nRETURN\n";
        let error = super::deserialize_bytecode(bytecode).unwrap_err();
        assert!(error.contains("inconsistent parameter metadata across call sites"), "unexpected error: {error}");
    }

    #[test]
    fn bytecode_verifier_rejects_duplicate_and_invalid_function_parameter_names() {
        let duplicate = "ZENVORA_BYTECODE 1\nCALL_TARGET 4 2 value,value\nPUSH 1\nPUSH 2\nJUMP 5\nRETURN\n";
        let error = super::deserialize_bytecode(duplicate).unwrap_err();
        assert!(error.contains("duplicate parameter"), "unexpected error: {error}");

        let invalid = "ZENVORA_BYTECODE 1\nCALL_TARGET 4 1 bad-name\nPUSH 1\nPRINT\nJUMP 5\nRETURN\n";
        let error = super::deserialize_bytecode(invalid).unwrap_err();
        assert!(error.contains("invalid parameter name"), "unexpected error: {error}");
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
    fn source_compilation_rejects_mixed_return_paths_before_execution() {
        let source = "fn maybe(flag) {\nif flag {\nreturn 1\n}\n}\nshow maybe(false)\n";
        let error = super::compile(source).unwrap_err();
        assert!(error.contains("inconsistent reachable return kinds"), "unexpected error: {error}");
    }

    #[test]
    fn bytecode_verifier_rejects_mixed_reachable_return_kinds() {
        let bytecode = concat!(
            "ZENVORA_BYTECODE 1\n",
            "CALL_TARGET 3 0 -\n",
            "PRINT\n",
            "JUMP 8\n",
            "PUSH_BOOL true\n",
            "JUMP_IF_FALSE 7\n",
            "PUSH 1\n",
            "RETURN_VALUE\n",
            "RETURN\n",
        );
        let error = super::deserialize_bytecode(bytecode).unwrap_err();
        assert!(error.contains("inconsistent reachable return kinds"), "unexpected error: {error}");
    }

    #[test]
    fn bytecode_verifier_rejects_function_returns_with_stack_residue() {
        let bytecode = concat!(
            "ZENVORA_BYTECODE 1\n",
            "CALL_TARGET 7 0 -\n",
            "PUSH 10\n",
            "ADD\n",
            "PUSH 20\n",
            "PRINT\n",
            "PRINT\n",
            "JUMP 10\n",
            "PUSH 1\n",
            "PUSH 2\n",
            "RETURN_VALUE\n",
        );
        let error = super::deserialize_bytecode(bytecode).unwrap_err();
        assert!(error.contains("function return") && error.contains("expected 1"), "unexpected error: {error}");
    }

    #[test]
    fn bytecode_verifier_rejects_structural_errors_in_unreachable_code() {
        let bytecode = "ZENVORA_BYTECODE 1\nJUMP 3\nJUMP 99\nJUMP 3\n";
        let error = super::deserialize_bytecode(bytecode).unwrap_err();
        assert!(error.contains("invalid instruction") || error.contains("invalid target"), "unexpected error: {error}");
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
    fn compiler_instruction_append_enforces_limit_before_growth() {
        let mut code = vec![super::Op::Push(0); super::MAX_BYTECODE_INSTRUCTIONS];
        let error = super::append_code(&mut code, vec![super::Op::Push(1)]).unwrap_err();
        assert!(error.contains("compiled instruction count exceeds safety limit"), "{error}");
        assert_eq!(code.len(), super::MAX_BYTECODE_INSTRUCTIONS);
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
    fn bytecode_file_reader_enforces_size_limit_before_unbounded_allocation() {
        let path = std::env::temp_dir().join(format!("zenvora-oversized-bytecode-{}.zvb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, vec![b' '; super::MAX_BYTECODE_BYTES + 1]).unwrap();
        let error = super::read_bytecode_source(path.to_str().unwrap()).unwrap_err();
        assert!(error.contains("bytecode file exceeds safety limit"), "{error}");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn bytecode_rejects_unknown_instructions_and_invalid_jumps() {
        assert!(super::deserialize_bytecode("ZENVORA_BYTECODE 9\n").unwrap_err().contains("unsupported bytecode version"));
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
    fn equality_compares_values_structurally_and_different_types_are_unequal() {
        let output = execute(
            "show 1 == true\nshow 1 != \"1\"\nshow array(1, 2) == array(1, 2)\nshow map(\"a\", 1) == map(\"a\", 1)\n"
        ).unwrap();
        assert_eq!(output, vec!["false", "true", "true", "true"]);
    }

    #[test]
    fn integer_conditions_treat_zero_as_false_and_nonzero_as_true() {
        let output = execute(
            "if 0 {\nshow 1\n} otherwise {\nshow 2\n}\nif -1 {\nshow 3\n} otherwise {\nshow 4\n}\nrepeat 0 {\nshow 5\n}\nrepeat 1 {\nshow 6\nbreak\n}\n"
        ).unwrap();
        assert_eq!(output, vec!["2", "3", "6"]);
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
    fn reserved_keywords_are_rejected_as_identifiers() {
        for source in [
            "define if := 1\n",
            "define true := 1\n",
            "define otherwise := 1\n",
            "fn repeat(value) {\nreturn value\n}\n",
            "fn valid(and) {\nreturn 1\n}\n",
        ] {
            let error = compile(source).unwrap_err();
            assert!(error.contains("invalid variable name") || error.contains("invalid function name") || error.contains("invalid parameter name"), "unexpected error: {error}");
        }
        assert_eq!(execute("define counter := 1\nshow counter\n").unwrap(), vec!["1"]);
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
    fn owned_collection_builtins_preserve_original_bindings() {
        let source = "define numbers := array(10, 20, 30)\nshow reverse(numbers)\nshow numbers\nshow get(numbers, 1)\ndefine grown := push(numbers, 40)\nshow len(grown)\nshow len(numbers)\ndefine profile := map(\"name\", \"Zenvora\", \"level\", 6)\ndefine updated := put(profile, \"level\", 7)\nshow get(updated, \"level\")\nshow get(profile, \"level\")\nshow get(profile, \"name\")\nshow len(keys(profile))\nshow get(values(profile), 0)\nshow get(profile, \"level\")\n";
        let output = execute(source).unwrap();
        assert_eq!(output, vec![
            "[30, 20, 10]",
            "[10, 20, 30]",
            "20",
            "4",
            "3",
            "7",
            "6",
            "Zenvora",
            "2",
            "6",
            "6",
        ]);
    }

    #[test]
    fn live_runtime_bindings_enforce_aggregate_memory_budget() {
        let mut source = format!("define giant := \"{}\"\n", "x".repeat(super::MAX_BYTECODE_STRING_BYTES));
        for index in 0..80 {
            source.push_str(&format!("define copy_{index} := giant\n"));
        }
        let error = execute(&source).unwrap_err();
        assert!(error.contains("live variable/call-frame values exceed safety limit"), "{error}");
    }

    #[test]
    fn operand_stack_enforces_aggregate_value_memory_limit() {
        let mut stack = Vec::new();
        let mut stack_bytes = 0usize;
        super::push_stack_value(&mut stack, &mut stack_bytes, super::Value::Str("a".repeat(super::MAX_STACK_VALUE_BYTES / 2))).unwrap();
        let error = super::push_stack_value(&mut stack, &mut stack_bytes, super::Value::Str("b".repeat(super::MAX_STACK_VALUE_BYTES / 2 + 1))).unwrap_err();
        assert!(error.contains("operand stack values exceed safety limit"), "{error}");
        assert_eq!(stack.len(), 1);
        let value = super::pop_stack_value(&mut stack, &mut stack_bytes, "underflow").unwrap();
        assert_eq!(stack_bytes, 0);
        assert!(matches!(value, super::Value::Str(_)));
    }

    #[test]
    fn runtime_rejects_oversized_aggregate_collection_values() {
        let large_text = super::Value::Str("x".repeat(super::MAX_BYTECODE_STRING_BYTES));
        let values = vec![large_text; super::MAX_VALUE_BYTES / super::MAX_BYTECODE_STRING_BYTES + 1];
        let error = super::builtin("array", values).unwrap_err();
        assert!(error.contains("estimated memory safety budget"), "{error}");
    }

    #[test]
    fn runtime_rejects_excessive_nested_collection_depth() {
        let source = "define count := 0\ndefine value := array()\nrepeat count < 300 {\n value := array(value)\n count := count + 1\n}\n";
        let error = execute(source).unwrap_err();
        assert!(error.contains("value nesting exceeds safety limit"), "{error}");

        let mut nested = super::Value::Int(0);
        for _ in 0..=super::MAX_VALUE_DEPTH {
            nested = super::Value::Array(vec![nested]);
        }
        assert!(super::ensure_value_depth(&nested).unwrap_err().contains("value nesting exceeds safety limit"));
    }

    #[test]
    fn collection_growth_respects_item_limits() {
        let full_array = super::Value::Array(vec![super::Value::Int(0); super::MAX_COLLECTION_ITEMS]);
        let error = super::builtin("push", vec![full_array, super::Value::Int(1)]).unwrap_err();
        assert!(error.contains("array exceeds safety limit"), "{error}");

        let full_map: std::collections::BTreeMap<String, super::Value> =
            (0..super::MAX_COLLECTION_ITEMS).map(|i| (format!("key{i}"), super::Value::Int(i as i64))).collect();
        let error = super::builtin("put", vec![super::Value::Map(full_map.clone()), super::Value::Str("new-key".into()), super::Value::Int(1)]).unwrap_err();
        assert!(error.contains("map exceeds safety limit"), "{error}");
        assert!(super::check_map_capacity(&full_map, "key0").is_ok());
        assert!(super::check_map_capacity(&full_map, "new-key").unwrap_err().contains("map exceeds safety limit"));
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
    fn contract_views_reject_direct_action_calls() {
        let direct = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview bad() {\ncall increment(1)\nreturn count\n}\n}\n";
        assert!(compile(direct).unwrap_err().contains("view cannot call action 'increment'"));
        let expression = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview bad() {\nreturn increment(1)\n}\n}\n";
        assert!(compile(expression).unwrap_err().contains("view cannot call action 'increment'"));
        let string_literal = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview label() {\nreturn \"increment()\"\n}\n}\nshow label()\n";
        assert_eq!(execute(string_literal).unwrap(), vec!["increment()"]);
    }

    #[test]
    fn contract_views_reject_indirect_state_mutation_through_helpers() {
        let source = "define count := 0\nfn mutate() {\ncount := count + 1\nreturn count\n}\nfn helper() {\n# a comment must not hide the call from effect analysis\ncall mutate()\nreturn count\n}\ncontract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview bad() {\nreturn helper()\n}\n}\n";
        assert!(compile(source).unwrap_err().contains("view cannot call state-mutating function 'helper'"));
    }

    #[test]
    fn contract_views_reject_direct_state_mutation() {
        let source = "contract Counter {\nstate count := 0\nview bad() {\ncount := count + 1\nreturn count\n}\n}\n";
        assert!(compile(source).unwrap_err().contains("cannot directly assign to contract state 'count'"));
        let good = "contract Counter {\nstate count := 0\nview current() {\nreturn count\n}\n}\nshow current()\n";
        assert_eq!(execute(good).unwrap(), vec!["0"]);
    }

    #[test]
    fn contract_artifact_tracks_methods_after_nested_blocks() {
        let source = "contract Counter {\nstate count := 0\naction increment(amount) {\nif amount > 0 {\ncount := count + amount\n} otherwise {\ncount := count - amount\n}\n}\nview current() {\nreturn count\n}\n}\n";
        let artifact = super::contract_abi_and_artifact(source).unwrap();
        assert_eq!(artifact["abi"]["methods"].as_array().unwrap().len(), 2);
        assert_eq!(artifact["abi"]["methods"][1]["name"], "current");
    }

    #[test]
    fn contract_artifact_contains_abi_source_hash_and_verified_bytecode_v2() {
        let source = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview current() {\nreturn count\n}\n}\n";
        let artifact = super::contract_abi_and_artifact(source).unwrap();
        assert_eq!(artifact["format"], "ZENVORA_CONTRACT_ARTIFACT_V1");
        assert_eq!(artifact["contract"], "Counter");
        assert_eq!(artifact["abi"]["states"][0]["name"], "count");
        assert_eq!(artifact["abi"]["methods"][0]["name"], "increment");
        assert_eq!(artifact["abi"]["methods"][0]["kind"], "action");
        assert_eq!(artifact["abi"]["methods"][1]["kind"], "view");
        let encoded = artifact["bytecode_v2"].as_str().unwrap();
        assert!(super::deserialize_bytecode_v2(encoded).is_ok());
        assert_eq!(artifact["source_sha256"].as_str().unwrap().len(), 64);
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
    fn unique_preserves_first_occurrence_order_for_mixed_values() {
        let output = execute(r#"show unique(array(3, 1, 3, "x", 1, "x", true, false, true))
show unique(array())
"#).unwrap();
        assert_eq!(output, vec!["[3, 1, x, true, false]", "[]"]);
        assert!(execute("show unique(7)\n").unwrap_err().contains("expects an array"));
    }

    #[test]
    fn assert_eq_compares_values_and_reports_actionable_failures() {
        assert_eq!(execute("show assert_eq(42, 42)\nshow assert_eq(array(1, 2), array(1, 2))\nshow assert_eq(1, 2, \"bad balance\")\n").unwrap_err(), "bad balance");
        let output = execute("show assert_eq(42, 42)\nshow assert_eq(array(1, 2), array(1, 2))\n").unwrap();
        assert_eq!(output, vec!["true", "true"]);
        assert!(execute("show assert_eq(1, 2)\n").unwrap_err().contains("left was 1, right was 2"));
        assert!(execute("show assert_eq(1)\n").unwrap_err().contains("expects 2 or 3"));
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
        assert!(replace_error.contains("replace result exceeds safety limit") || replace_error.contains("replace size calculation overflow"));

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
    fn text_normalization_character_count_and_integer_conversion_work() {
        let output = execute(r#"show trim("  Zenvora Lang  ")
show lower("ZENvora")
show upper("Zenvora")
show char_count("hé🙂")
show to_int("  -42 ")
show to_int(17)
"#).unwrap();
        assert_eq!(output, vec!["Zenvora Lang", "zenvora", "ZENVORA", "3", "-42", "17"]);
    }

    #[test]
    fn text_normalization_and_integer_conversion_reject_invalid_values() {
        assert!(execute("show trim(12)\n").unwrap_err().contains("expects a string"));
        assert!(execute(r#"show to_int("12x")
"#).unwrap_err().contains("cannot convert"));
        assert!(execute("show to_int(true)\n").unwrap_err().contains("integer or numeric string"));
        assert!(execute(r#"show to_int("9223372036854775808")
"#).unwrap_err().contains("cannot convert"));
    }

    #[test]
    fn index_of_supports_unicode_strings_and_arrays() {
        let output = execute(r#"show index_of("héllo🙂", "🙂")
show index_of("zenvora", "nope")
show index_of(array(7, "x", true), "x")
show index_of(array(7, "x", true), false)
"#).unwrap();
        assert_eq!(output, vec!["5", "-1", "1", "-1"]);
    }

    #[test]
    fn index_of_rejects_invalid_argument_types() {
        assert!(execute(r#"show index_of("abc", 1)
"#).unwrap_err().contains("string needle"));
        assert!(execute("show index_of(1, 1)\n").unwrap_err().contains("string or array"));
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
    fn v2_file_writer_creates_a_loadable_executable_file() {
        let base = std::env::temp_dir().join(format!("zenvora-v2-smoke-{}", std::process::id()));
        let source_path = base.with_extension("zv");
        let bytecode_path = base.with_extension("zv2b");
        std::fs::write(
            &source_path,
            "show double(21)\nfn double(value) {\n return value * 2\n}\n",
        ).unwrap();

        super::write_bytecode_v2(
            source_path.to_str().unwrap(),
            bytecode_path.to_str().unwrap(),
        ).unwrap();

        let encoded = std::fs::read_to_string(&bytecode_path).unwrap();
        assert!(encoded.starts_with("ZENVORA_BYTECODE 2\n"));
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        let mut output = Vec::new();
        super::run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["42"]);

        std::fs::remove_file(source_path).unwrap();
        std::fs::remove_file(bytecode_path).unwrap();
    }

    #[test]
    fn compiler_emits_v2_for_called_and_uncalled_functions() {
        let source = "show twice(21)\nfn twice(value) {\n return value * 2\n}\nfn never(value) {\n show value\n}\n";
        let (code, layout) = super::compile_with_layout(source).unwrap();
        assert_eq!(layout.functions.len(), 2);
        assert_eq!(layout.functions[0].return_kind, super::bytecode_v2::ReturnKind::Value);
        assert_eq!(layout.functions[1].return_kind, super::bytecode_v2::ReturnKind::Void);

        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        assert!(encoded.starts_with("ZENVORA_BYTECODE 2\n"));
        assert!(encoded.contains("FUNCTION 4 8 1 value value"));
        assert!(encoded.contains("FUNCTION 8 11 1 void value"));
        let decoded = super::deserialize_bytecode(&encoded).unwrap();
        let mut output = Vec::new();
        super::run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn bytecode_v2_loader_verifies_and_executes_metadata_driven_function() {
        let source = "ZENVORA_BYTECODE 2\nPROGRAM 0 4\nFUNCTION 4 8 1 value amount\nCODE\nPUSH 41\nCALL 4 1\nPRINT\nJUMP 4\nLOAD amount\nPUSH 1\nADD\nRETURN_VALUE\n";
        let parsed = super::bytecode_v2::parse_v2_file(source).unwrap();
        let serialized = super::bytecode_v2::serialize_v2_file(&parsed).unwrap();
        let decoded = super::deserialize_bytecode(&serialized).unwrap();
        let mut output = Vec::new();
        super::run_with_output(&decoded, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn bytecode_v2_executes_standard_library_builtin_parity_end_to_end() {
        let source = r#"show starts_with("zenvora", "zen")
show ends_with("zenvora", "ora")
show trim("  zv  ")
show lower("ZV")
show upper("zv")
show char_count("hé🙂")
show index_of("zenvora", "v")
show contains(array(1, 2, 3), 2)
define profile := map("answer", 42)
show get(profile, "answer")
show len(push(array(1), 2))
show keys(profile)
show values(profile)
show replace("zv", "z", "x")
show split("a,b", ",")
show join(array("a", "b"), "-")
show clamp(9, 0, 5)
show unique(array(1, 1, 2))
show to_int("42")
show range(0, 3)
show sum(array(3, 4))
"#;
        let expected = execute(source).unwrap();
        let stem = format!("zenvora-bytecode-v2-builtins-{}", std::process::id());
        let source_path = std::env::temp_dir().join(format!("{stem}.zv"));
        let bytecode_path = std::env::temp_dir().join(format!("{stem}.zv2b"));
        std::fs::write(&source_path, source).unwrap();
        super::write_bytecode_v2(source_path.to_str().unwrap(), bytecode_path.to_str().unwrap()).unwrap();
        let encoded = std::fs::read_to_string(&bytecode_path).unwrap();
        assert!(encoded.starts_with("ZENVORA_BYTECODE 2\\n"));
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();
        std::fs::remove_file(source_path).unwrap();
        std::fs::remove_file(bytecode_path).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual, vec![
            "true", "true", "zv", "zv", "ZV", "3", "3", "true", "42", "2",
            "[answer]", "[42]", "xv", "[a, b]", "a-b", "5", "[1, 2]", "42",
            "[0, 1, 2]", "7",
        ]);
    }

    #[test]
    fn bytecode_v2_loader_rejects_invalid_code_before_execution() {
        let source = "ZENVORA_BYTECODE 2\nPROGRAM 0 2\nCODE\nPRINT\nJUMP 2\n";
        assert!(super::deserialize_bytecode(source).unwrap_err().contains("stack underflow"));
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


    #[test]
    fn bytecode_v2_matches_source_execution_for_collections_builtins_and_functions() {
        let source = r#"define values := array(2, 4, 6)
show sum(values)
show values[1]
show twice(21)
fn twice(value) {
    return value * 2
}
"#;

        let expected = execute(source).unwrap();
        let (code, layout) = super::compile_with_layout(source).unwrap();
        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual, vec!["12", "4", "42"]);
    }

    #[test]
    fn bytecode_v2_round_trip_preserves_short_circuit_boolean_and_string_results() {
        let source = r#"define enabled := false
if enabled and (1 / 0 == 0) {
    show "wrong"
} otherwise {
    show "short-circuit"
}
show "zen" + "vora"
"#;

        let expected = execute(source).unwrap();
        let (code, layout) = super::compile_with_layout(source).unwrap();
        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual, vec!["short-circuit", "zenvora"]);
    }

    #[test]
    fn bytecode_v2_round_trip_preserves_branching_and_loop_results() {
        let source = r#"define i := 0
define total := 0
repeat i <= 4 {
    if i > 2 {
        total := total + i
    }
    i := i + 1
}
if total == 7 {
    show "verified"
} otherwise {
    show "mismatch"
}
"#;
        let expected = execute(source).unwrap();
        let (code, layout) = super::compile_with_layout(source).unwrap();
        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual, vec!["verified"]);
    }

    #[test]
    fn bytecode_v2_matches_source_for_nested_calls_collections_and_runtime_errors() {
        let source = r#"define profile := map("name", "Zenvora", "level", 6)
define updated := put(profile, "level", add(3, 4))
show get(updated, "level")
show get(profile, "level")
show join(array("BHYVORIX", "Chain"), "-")
show nested(5)
fn add(left, right) {
    return left + right
}
fn nested(value) {
    return add(add(value, 1), add(2, 3))
}
"#;
        let expected = execute(source).unwrap();
        let (code, layout) = super::compile_with_layout(source).unwrap();
        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual, vec!["7", "6", "BHYVORIX-Chain", "11"]);

        let failing_source = "show 1 / 0\n";
        let source_error = execute(failing_source).unwrap_err();
        let (failing_code, failing_layout) = super::compile_with_layout(failing_source).unwrap();
        let failing_encoded = super::serialize_bytecode_v2(&failing_code, &failing_layout).unwrap();
        let failing_decoded = super::deserialize_bytecode_v2(&failing_encoded).unwrap();
        let mut output = Vec::new();
        let bytecode_error = super::run_with_output(&failing_decoded, &mut |line| output.push(line)).unwrap_err();
        assert_eq!(bytecode_error, source_error);
    }




    #[test]
    fn bytecode_v2_matches_source_for_break_and_continue() {
        let source = r#"define count := 0
repeat count < 5 {
    count := count + 1
    if count == 2 {
        continue
    }
    show count
    if count == 4 {
        break
    }
}
"#;
        let expected = execute(source).unwrap();
        let (code, layout) = super::compile_with_layout(source).unwrap();
        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual, vec!["1", "3", "4"]);
    }

    #[test]
    fn bytecode_v2_matches_source_for_integer_arithmetic_failures() {
        let cases = [
            "show 9223372036854775807 + 1\n",
            "show 9223372036854775807 * 2\n",
            "show (-9223372036854775807 - 1) / -1\n",
            "show 1 / 0\n",
        ];
        for source in cases {
            let source_error = execute(source).unwrap_err();
            let (code, layout) = super::compile_with_layout(source).unwrap();
            let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
            let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
            let mut output = Vec::new();
            let bytecode_error =
                super::run_with_output(&decoded, &mut |line| output.push(line)).unwrap_err();
            assert_eq!(bytecode_error, source_error, "error mismatch for {source:?}");
            assert!(output.is_empty(), "failed arithmetic unexpectedly printed output");
        }
    }

    #[test]
    fn bytecode_v2_matches_source_for_recursive_returns() {
        let source = r#"show factorial(6)
fn factorial(n) {
    if n <= 1 {
        return 1
    } otherwise {
        return n * factorial(n - 1)
    }
}
"#;
        let expected = execute(source).unwrap();
        let (code, layout) = super::compile_with_layout(source).unwrap();
        let encoded = super::serialize_bytecode_v2(&code, &layout).unwrap();
        let decoded = super::deserialize_bytecode_v2(&encoded).unwrap();
        let mut actual = Vec::new();
        super::run_with_output(&decoded, &mut |line| actual.push(line)).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual, vec!["720"]);
    }

    #[test]
    fn vm_charges_the_deterministic_gas_schedule_per_executed_instruction() {
        let code = compile("show 1 + 2").unwrap();
        let mut meter = super::contract_gas::GasMeter::new(100_000);
        let mut output = Vec::new();
        super::run_with_gas(&code, &mut |line| output.push(line), &mut meter).unwrap();
        assert_eq!(output, vec!["3"]);
        assert!(meter.used() > super::contract_gas::BASE_EXECUTION_GAS);
        assert!(meter.used() < meter.limit());
    }

    #[test]
    fn vm_stops_before_executing_an_instruction_when_gas_is_exhausted() {
        let code = compile("show 1 + 2").unwrap();
        let mut meter = super::contract_gas::GasMeter::new(super::contract_gas::BASE_EXECUTION_GAS + 1);
        let mut output = Vec::new();
        let error = super::run_with_gas(&code, &mut |line| output.push(line), &mut meter).unwrap_err();
        assert_eq!(error, "out of gas");
        assert!(output.is_empty(), "out-of-gas execution must not print a partial result");
        assert_eq!(meter.used(), super::contract_gas::BASE_EXECUTION_GAS + 1);
    }

}

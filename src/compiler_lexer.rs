#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Number(i64), String(String), Bool(bool), Ident(String), And, Or, Not, Plus, Minus, Star, Slash,
    LParen, RParen, LBracket, RBracket, Comma, EqEq, NotEq, Less, LessEq, Greater, GreaterEq, End,
}

#[cfg(test)]
pub(crate) fn lex(input: &str) -> Result<Vec<Token>, String> {
    lex_with_columns(input).map(|(tokens, _)| tokens)
}

pub(crate) fn lex_with_columns(input: &str) -> Result<(Vec<Token>, Vec<usize>), String> {
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


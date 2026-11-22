use crate::{compiler_lexer::Token, is_builtin, qvm::Op, MAX_PARSE_NESTING};

pub(crate) struct Parser { pub(crate) tokens: Vec<Token>, pub(crate) columns: Vec<usize>, pub(crate) pos: usize, pub(crate) nesting_depth: usize, pub(crate) error_column_override: Option<usize>, pub(crate) code: Vec<Op> }
impl Parser {
    pub(crate) fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn take(&mut self) -> Token { let t = self.tokens[self.pos].clone(); self.pos += 1; t }
    pub(crate) fn error_column(&self) -> usize { self.error_column_override.unwrap_or(self.columns[self.pos.saturating_sub(1).min(self.columns.len() - 1)]) }
    pub(crate) fn current_column(&self) -> usize { self.columns[self.pos.min(self.columns.len() - 1)] }

    pub(crate) fn expression(&mut self) -> Result<(), String> {
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

    pub(crate) fn condition(&mut self) -> Result<(), String> {
        self.expression()
    }
}



#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::{compiler_lexer::lex_with_columns, qvm::Op};

    fn parse(source: &str) -> Result<Vec<Op>, String> {
        let (tokens, columns) = lex_with_columns(source)?;
        let mut parser = Parser {
            tokens,
            columns,
            pos: 0,
            nesting_depth: 0,
            error_column_override: None,
            code: Vec::new(),
        };
        parser.expression()?;
        if parser.peek() != &crate::compiler_lexer::Token::End {
            return Err(format!("unexpected token {:?}", parser.peek()));
        }
        Ok(parser.code)
    }

    #[test]
    fn parser_respects_arithmetic_precedence_and_associativity() {
        assert_eq!(parse("2 + 3 * 4").unwrap(), vec![
            Op::Push(2), Op::Push(3), Op::Push(4), Op::Mul, Op::Add,
        ]);
        assert_eq!(parse("20 - 5 - 3").unwrap(), vec![
            Op::Push(20), Op::Push(5), Op::Sub, Op::Push(3), Op::Sub,
        ]);
    }

    #[test]
    fn parser_emits_short_circuit_control_flow() {
        let code = parse("true and false or true").unwrap();
        assert!(code.iter().any(|op| matches!(op, Op::JumpIfFalseKeep(_))));
        assert!(code.iter().any(|op| matches!(op, Op::JumpIfTrueKeep(_))));
    }

    #[test]
    fn parser_handles_builtin_and_user_function_calls() {
        assert_eq!(parse("len(\"abc\")").unwrap(), vec![
            Op::PushString("abc".into()), Op::Builtin("len".into(), 1),
        ]);
        assert_eq!(parse("compute(2, 3)").unwrap(), vec![
            Op::Push(2), Op::Push(3), Op::Call("compute".into(), 2),
        ]);
    }

    #[test]
    fn parser_handles_nested_index_expressions() {
        assert_eq!(parse("items[1 + 2]").unwrap(), vec![
            Op::Load("items".into()), Op::Push(1), Op::Push(2), Op::Add, Op::Index,
        ]);
    }

    #[test]
    fn parser_rejects_unclosed_parentheses() {
        assert!(parse("(2 + 3").unwrap_err().contains("expected ')'"));
    }
}

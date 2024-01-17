use std::fmt::Display;
use std::ops::Deref;

use ruff_python_ast::{
    self as ast, BoolOp, CmpOp, ConversionFlag, Expr, ExprContext, FStringElement, Number,
    Operator, UnaryOp,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::lexer::Spanned;
use crate::parser::helpers::token_kind_to_cmp_op;
use crate::parser::{helpers, FunctionKind, Parser, ParserCtxFlags, EXPR_SET, NEWLINE_EOF_SET};
use crate::string::{
    concatenated_strings, parse_fstring_literal_element, parse_string_literal, StringType,
};
use crate::token_set::TokenSet;
use crate::{FStringErrorType, Mode, ParseErrorType, Tok, TokenKind};

/// Tokens that can appear after an expression.
const END_EXPR_SET: TokenSet = TokenSet::new(&[
    TokenKind::Newline,
    TokenKind::Semi,
    TokenKind::Colon,
    TokenKind::EndOfFile,
    TokenKind::Rbrace,
    TokenKind::Rsqb,
    TokenKind::Rpar,
    TokenKind::Comma,
    TokenKind::Dedent,
    TokenKind::Else,
    TokenKind::As,
    TokenKind::From,
    TokenKind::For,
    TokenKind::Async,
    TokenKind::In,
]);

impl<'src> Parser<'src> {
    pub(super) fn at_expr(&mut self) -> bool {
        self.at_ts(EXPR_SET)
    }

    /// Parses every Python expression.
    pub(super) fn parse_expression(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_expr();

        if self.at(TokenKind::Comma) {
            Expr::Tuple(self.parse_tuple_expr(parsed_expr.expr, start, false, Parser::parse_expr))
                .into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuple and named expressions.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    pub(super) fn parse_expr(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_expr_simple();

        if self.at(TokenKind::If) {
            Expr::IfExp(self.parse_if_expr(parsed_expr.expr, start)).into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuple.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    pub(super) fn parse_expr2(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_expr();

        if self.at(TokenKind::ColonEqual) {
            Expr::NamedExpr(self.parse_named_expr(parsed_expr.expr, start)).into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuple and `if` expression.
    fn parse_expr_simple(&mut self) -> ParsedExpr {
        self.parse_expression_with_precedence(1)
    }

    /// Tries to parse an expression (using `parse_func`), and recovers from
    /// errors by skipping until a specified set of tokens.
    ///
    /// If the current token is not part of an expression, adds the `error_msg`
    /// to the list of errors and returns an `Expr::Invalid`.
    pub(super) fn parse_expr_with_recovery(
        &mut self,
        mut parse_func: impl FnMut(&mut Parser<'src>) -> ParsedExpr,
        recover_set: impl Into<TokenSet>,
        error_msg: impl Display,
    ) -> ParsedExpr {
        if self.at_expr() {
            parse_func(self)
        } else {
            let start = self.node_start();
            self.add_error(
                ParseErrorType::OtherError(error_msg.to_string()),
                self.current_range(),
            );
            self.skip_until(NEWLINE_EOF_SET.union(recover_set.into()));

            // FIXME(micha): I don't think we should include the entire range, or the range at all because it risks including trivia
            let range = self.node_range(start);
            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(range).into(),
                range,
            })
            .into()
        }
    }

    /// Binding powers of operators for a Pratt parser.
    ///
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    fn current_op(&mut self) -> (u8, TokenKind, Associativity) {
        const NOT_AN_OP: (u8, TokenKind, Associativity) =
            (0, TokenKind::Unknown, Associativity::Left);
        let kind = self.current_kind();

        match kind {
            TokenKind::Or => (4, kind, Associativity::Left),
            TokenKind::And => (5, kind, Associativity::Left),
            TokenKind::Not if self.peek_nth(1) == TokenKind::In => (7, kind, Associativity::Left),
            TokenKind::Is
            | TokenKind::In
            | TokenKind::EqEqual
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual => (7, kind, Associativity::Left),
            TokenKind::Vbar => (8, kind, Associativity::Left),
            TokenKind::CircumFlex => (9, kind, Associativity::Left),
            TokenKind::Amper => (10, kind, Associativity::Left),
            TokenKind::LeftShift | TokenKind::RightShift => (11, kind, Associativity::Left),
            TokenKind::Plus | TokenKind::Minus => (12, kind, Associativity::Left),
            TokenKind::Star
            | TokenKind::Slash
            | TokenKind::DoubleSlash
            | TokenKind::Percent
            | TokenKind::At => (14, kind, Associativity::Left),
            TokenKind::DoubleStar => (18, kind, Associativity::Right),
            _ => NOT_AN_OP,
        }
    }

    /// Parses expression with binding power of at least bp.
    ///
    /// Uses the Pratt parser algorithm.
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    // FIXME(micha): Introduce precedence enum instead of passing cryptic u8 values.
    fn parse_expression_with_precedence(&mut self, bp: u8) -> ParsedExpr {
        let start = self.node_start();
        let mut lhs = self.parse_lhs_expression();

        loop {
            let (op_bp, op, associativity) = self.current_op();
            if op_bp < bp {
                break;
            }

            // Don't parse a `CompareExpr` if we are parsing a `Comprehension` or `ForStmt`
            if op.is_compare_operator() && self.has_ctx(ParserCtxFlags::FOR_TARGET) {
                break;
            }

            let op_bp = match associativity {
                Associativity::Left => op_bp + 1,
                Associativity::Right => op_bp,
            };

            self.bump(op);

            // We need to create a dedicated node for boolean operations,
            // even though boolean operations are infix.
            if op.is_bool_operator() {
                lhs = Expr::BoolOp(self.parse_bool_op_expr(lhs.expr, start, op, op_bp)).into();
                continue;
            }

            // Same here as well
            if op.is_compare_operator() {
                lhs = Expr::Compare(self.parse_compare_op_expr(lhs.expr, start, op, op_bp)).into();
                continue;
            }

            let rhs = if self.at_expr() {
                self.parse_expression_with_precedence(op_bp)
            } else {
                let rhs_range = self.current_range();
                self.add_error(
                    ParseErrorType::OtherError("expecting an expression after operand".into()),
                    rhs_range,
                );

                Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(rhs_range).into(),
                    range: rhs_range,
                })
                .into()
            };

            lhs.expr = Expr::BinOp(ast::ExprBinOp {
                left: Box::new(lhs.expr),
                op: Operator::try_from(op).unwrap(),
                right: Box::new(rhs.expr),
                range: self.node_range(start),
            });
        }

        lhs
    }

    pub(super) fn parse_lhs_expression(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let token = self.next_token();
        let mut lhs = match token.0 {
            token @ (Tok::Plus | Tok::Minus | Tok::Not | Tok::Tilde) => {
                Expr::UnaryOp(self.parse_unary_expr(&token, start)).into()
            }
            Tok::Star => Expr::Starred(self.parse_starred_expr(start)).into(),
            Tok::Await => Expr::Await(self.parse_await_expr(start)).into(),
            Tok::Lambda => Expr::Lambda(self.parse_lambda_expr(start)).into(),
            _ => self.parse_atom(token, start),
        };

        if self.is_current_token_postfix() {
            lhs = self.parse_postfix_expr(lhs.expr, start).into();
        }

        lhs
    }

    pub(super) fn parse_identifier(&mut self) -> ast::Identifier {
        let range = self.current_range();
        if self.current_kind() == TokenKind::Name {
            let (Tok::Name { name }, _) = self.next_token() else {
                unreachable!();
            };
            ast::Identifier { id: name, range }
        } else {
            self.add_error(
                ParseErrorType::OtherError("expecting an identifier".into()),
                range,
            );
            ast::Identifier {
                id: String::new(),
                range,
            }
        }
    }

    fn parse_atom(&mut self, (token, token_range): Spanned, start: TextSize) -> ParsedExpr {
        let lhs = match token {
            Tok::Float { value } => Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Float(value),
                range: self.node_range(start),
            }),
            Tok::Complex { real, imag } => Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Complex { real, imag },
                range: self.node_range(start),
            }),
            Tok::Int { value } => Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Int(value),
                range: self.node_range(start),
            }),
            Tok::True => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: true,
                range: self.node_range(start),
            }),
            Tok::False => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: false,
                range: self.node_range(start),
            }),
            Tok::None => Expr::NoneLiteral(ast::ExprNoneLiteral {
                range: self.node_range(start),
            }),
            Tok::Ellipsis => Expr::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: self.node_range(start),
            }),
            Tok::Name { name } => Expr::Name(ast::ExprName {
                id: name,
                ctx: ExprContext::Load,
                range: self.node_range(start),
            }),
            Tok::IpyEscapeCommand { value, kind } if self.mode == Mode::Ipython => {
                Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand {
                    range: self.node_range(start),
                    kind,
                    value,
                })
            }
            tok @ Tok::String { .. } => self.parse_string_expr((tok, token_range), start),
            Tok::FStringStart => self.parse_fstring_expr(start),
            Tok::Lpar => {
                return self.parse_parenthesized_expr(start);
            }
            Tok::Lsqb => self.parse_bracketsized_expr(start),
            Tok::Lbrace => self.parse_bracesized_expr(start),
            Tok::Yield => self.parse_yield_expr(start),
            // `Invalid` tokens are created when there's a lexical error, to
            // avoid creating an "unexpected token" error for `Tok::Invalid`
            // we handle it here. We try to parse an expression to avoid
            // creating "statements in the same line" error in some cases.
            Tok::Unknown => {
                if self.at_expr() {
                    self.parse_expression().expr
                } else {
                    Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(token_range).into(),
                        range: token_range,
                    })
                }
            }
            // Handle unexpected token
            tok => {
                // Try to parse an expression after seeing an unexpected token
                let lhs = Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(token_range).into(),
                    range: token_range,
                });

                if matches!(tok, Tok::IpyEscapeCommand { .. }) {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "IPython escape commands are only allowed in `Mode::Ipython`".into(),
                        ),
                        token_range,
                    );
                } else {
                    self.add_error(
                        ParseErrorType::OtherError(format!("unexpected token `{tok}`")),
                        token_range,
                    );
                }
                lhs
            }
        };

        lhs.into()
    }

    fn parse_postfix_expr(&mut self, mut lhs: Expr, start: TextSize) -> Expr {
        loop {
            lhs = match self.current_kind() {
                TokenKind::Lpar => Expr::Call(self.parse_call_expr(lhs, start)),
                TokenKind::Lsqb => Expr::Subscript(self.parse_subscript_expr(lhs, start)),
                TokenKind::Dot => Expr::Attribute(self.parse_attribute_expr(lhs, start)),
                _ => break lhs,
            };
        }
    }

    fn parse_call_expr(&mut self, lhs: Expr, start: TextSize) -> ast::ExprCall {
        assert_eq!(self.current_kind(), TokenKind::Lpar);
        let arguments = self.parse_arguments();

        ast::ExprCall {
            func: Box::new(lhs),
            arguments,
            range: self.node_range(start),
        }
    }

    pub(super) fn parse_arguments(&mut self) -> ast::Arguments {
        let start = self.node_start();

        self.set_ctx(ParserCtxFlags::ARGUMENTS);

        let mut args: Vec<Expr> = vec![];
        let mut keywords: Vec<ast::Keyword> = vec![];
        let mut has_seen_kw_arg = false;
        let mut has_seen_kw_unpack = false;

        self.parse_delimited(
            true,
            TokenKind::Lpar,
            TokenKind::Comma,
            TokenKind::Rpar,
            |parser| {
                let argument_start = parser.node_start();
                if parser.at(TokenKind::DoubleStar) {
                    parser.eat(TokenKind::DoubleStar);

                    let value = parser.parse_expr();
                    keywords.push(ast::Keyword {
                        arg: None,
                        value: value.expr,
                        range: parser.node_range(argument_start),
                    });

                    has_seen_kw_unpack = true;
                } else {
                    let start = parser.node_start();
                    let mut parsed_expr = parser.parse_expr2();

                    match parser.current_kind() {
                        TokenKind::Async | TokenKind::For => {
                            parsed_expr = Expr::GeneratorExp(parser.parse_generator_expr(
                                parsed_expr.expr,
                                start,
                                false,
                            ))
                            .into();
                        }
                        _ => {}
                    }

                    if has_seen_kw_unpack && matches!(parsed_expr.expr, Expr::Starred(_)) {
                        parser.add_error(ParseErrorType::UnpackedArgumentError, &parsed_expr);
                    }

                    if parser.eat(TokenKind::Equal) {
                        has_seen_kw_arg = true;
                        let arg = if let Expr::Name(ident_expr) = parsed_expr.expr {
                            ast::Identifier {
                                id: ident_expr.id,
                                range: ident_expr.range,
                            }
                        } else {
                            // FIXME(micha): This recovery looks fishy, it drops the parsed expression.
                            parser.add_error(
                                ParseErrorType::OtherError(
                                    "cannot be used as a keyword argument!".to_string(),
                                ),
                                &parsed_expr,
                            );
                            ast::Identifier {
                                id: String::new(),
                                range: parsed_expr.range(),
                            }
                        };

                        let value = parser.parse_expr();

                        keywords.push(ast::Keyword {
                            arg: Some(arg),
                            value: value.expr,
                            range: parser.node_range(argument_start),
                        });
                    } else {
                        if has_seen_kw_arg
                            && !(has_seen_kw_unpack || matches!(parsed_expr.expr, Expr::Starred(_)))
                        {
                            parser.add_error(ParseErrorType::PositionalArgumentError, &parsed_expr);
                        }
                        args.push(parsed_expr.expr);
                    }
                }
            },
        );
        self.clear_ctx(ParserCtxFlags::ARGUMENTS);

        let arguments = ast::Arguments {
            range: self.node_range(start),
            args,
            keywords,
        };

        if let Err(error) = helpers::validate_arguments(&arguments) {
            self.errors.push(error);
        }

        arguments
    }

    fn parse_subscript_expr(&mut self, mut value: Expr, start: TextSize) -> ast::ExprSubscript {
        self.bump(TokenKind::Lsqb);

        // To prevent the `value` context from being `Del` within a `del` statement,
        // we set the context as `Load` here.
        helpers::set_expr_ctx(&mut value, ExprContext::Load);

        // Create an error when receiving a empty slice to parse, e.g. `l[]`
        if !self.at(TokenKind::Colon) && !self.at_expr() {
            let slice_range = TextRange::empty(self.current_range().start());
            self.expect_and_recover(TokenKind::Rsqb, TokenSet::EMPTY);

            let range = self.node_range(start);
            self.add_error(ParseErrorType::EmptySlice, range);
            return ast::ExprSubscript {
                value: Box::new(value),
                slice: Box::new(Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(slice_range).into(),
                    range: slice_range,
                })),
                ctx: ExprContext::Load,
                range,
            };
        }

        let slice_start = self.node_start();
        let mut slice = self.parse_slice();

        if self.eat(TokenKind::Comma) {
            let mut slices = vec![slice];
            self.parse_separated(
                true,
                TokenKind::Comma,
                TokenSet::new(&[TokenKind::Rsqb]),
                |parser| {
                    slices.push(parser.parse_slice());
                },
            );

            slice = Expr::Tuple(ast::ExprTuple {
                elts: slices,
                ctx: ExprContext::Load,
                range: self.node_range(slice_start),
                parenthesized: false,
            });
        }

        self.expect_and_recover(TokenKind::Rsqb, TokenSet::EMPTY);

        ast::ExprSubscript {
            value: Box::new(value),
            slice: Box::new(slice),
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    const UPPER_END_SET: TokenSet =
        TokenSet::new(&[TokenKind::Comma, TokenKind::Colon, TokenKind::Rsqb])
            .union(NEWLINE_EOF_SET);
    const STEP_END_SET: TokenSet =
        TokenSet::new(&[TokenKind::Comma, TokenKind::Rsqb]).union(NEWLINE_EOF_SET);

    fn parse_slice(&mut self) -> Expr {
        let start = self.node_start();

        let lower = if self.at_expr() {
            let lower = self.parse_expr2();

            if !self.at(TokenKind::Colon) || lower.expr.is_named_expr_expr() {
                return lower.expr;
            }

            Some(lower.expr)
        } else {
            None
        };

        self.expect(TokenKind::Colon);

        let lower = lower.map(Box::new);
        let upper = if self.at_ts(Self::UPPER_END_SET) {
            None
        } else {
            Some(Box::new(self.parse_expr().expr))
        };

        let step = if self.eat(TokenKind::Colon) {
            if self.at_ts(Self::STEP_END_SET) {
                None
            } else {
                Some(Box::new(self.parse_expr().expr))
            }
        } else {
            None
        };

        Expr::Slice(ast::ExprSlice {
            range: self.node_range(start),
            lower,
            upper,
            step,
        })
    }

    fn parse_unary_expr(&mut self, operator: &Tok, start: TextSize) -> ast::ExprUnaryOp {
        let op =
            UnaryOp::try_from(operator).expect("Expected operator to be a unary operator token.");
        let rhs = if matches!(op, UnaryOp::Not) {
            self.parse_expression_with_precedence(6)
        } else {
            // plus, minus and tilde
            self.parse_expression_with_precedence(17)
        };

        ast::ExprUnaryOp {
            op,
            operand: Box::new(rhs.expr),
            range: self.node_range(start),
        }
    }

    pub(super) fn parse_attribute_expr(
        &mut self,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprAttribute {
        self.bump(TokenKind::Dot);

        let attr = self.parse_identifier();

        ast::ExprAttribute {
            value: Box::new(value),
            attr,
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    fn parse_bool_op_expr(
        &mut self,
        lhs: Expr,
        start: TextSize,
        op: TokenKind,
        op_bp: u8,
    ) -> ast::ExprBoolOp {
        let mut values = vec![lhs];

        // Keep adding `expr` to `values` until we see a different
        // boolean operation than `op`.
        loop {
            let parsed_expr = self.parse_expression_with_precedence(op_bp);
            values.push(parsed_expr.expr);

            if self.current_kind() != op {
                break;
            }

            self.next_token();
        }

        ast::ExprBoolOp {
            values,
            op: BoolOp::try_from(op).unwrap(),
            range: self.node_range(start),
        }
    }

    fn parse_compare_op_expr(
        &mut self,
        lhs: Expr,
        start: TextSize,
        op: TokenKind,
        op_bp: u8,
    ) -> ast::ExprCompare {
        let mut comparators = vec![];
        let op = token_kind_to_cmp_op([op, self.current_kind()]).unwrap();
        let mut ops = vec![op];

        if matches!(op, CmpOp::IsNot | CmpOp::NotIn) {
            self.next_token();
        }

        loop {
            let parsed_expr = self.parse_expression_with_precedence(op_bp);
            comparators.push(parsed_expr.expr);

            if let Ok(op) = token_kind_to_cmp_op([self.current_kind(), self.peek_nth(1)]) {
                if matches!(op, CmpOp::IsNot | CmpOp::NotIn) {
                    self.next_token();
                }

                ops.push(op);
            } else {
                break;
            }

            self.next_token();
        }

        ast::ExprCompare {
            left: Box::new(lhs),
            ops,
            comparators,
            range: self.node_range(start),
        }
    }

    pub(super) fn parse_string_expr(
        &mut self,
        (mut tok, mut tok_range): Spanned,
        start: TextSize,
    ) -> Expr {
        let mut strings = vec![];
        while let Tok::String {
            value,
            kind,
            triple_quoted,
        } = tok
        {
            match parse_string_literal(&value, kind, triple_quoted, tok_range) {
                Ok(string) => {
                    strings.push(string);
                }
                Err(error) => {
                    strings.push(StringType::Invalid(ast::StringLiteral {
                        value,
                        range: tok_range,
                        unicode: kind.is_unicode(),
                    }));
                    self.add_error(ParseErrorType::Lexical(error.error), error.location);
                }
            }

            if !self.at(TokenKind::String) {
                break;
            }

            (tok, tok_range) = self.next_token();
        }

        // This handles the case where the string is implicit concatenated with
        // a fstring, e.g., `"hello " f"{x}"`.
        if self.at(TokenKind::FStringStart) {
            self.handle_implicit_concatenated_strings(&mut strings);
        }

        let range = self.node_range(start);

        if strings.len() == 1 {
            return match strings.pop().unwrap() {
                StringType::Str(string) => Expr::StringLiteral(ast::ExprStringLiteral {
                    value: ast::StringLiteralValue::single(string),
                    range,
                }),
                StringType::Bytes(bytes) => {
                    // TODO(micha): Is this valid? I thought string and byte literals can't be concatenated? Maybe not a syntax erro?
                    Expr::BytesLiteral(ast::ExprBytesLiteral {
                        value: ast::BytesLiteralValue::single(bytes),
                        range,
                    })
                }
                StringType::Invalid(invalid) => Expr::Invalid(ast::ExprInvalid {
                    value: invalid.value,
                    range,
                }),
                StringType::FString(_) => unreachable!(),
            };
        }

        concatenated_strings(strings, range).unwrap_or_else(|error| {
            self.add_error(ParseErrorType::Lexical(error.error), error.location);
            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(error.location).into(),
                range: error.location,
            })
        })
    }

    const FSTRING_SET: TokenSet = TokenSet::new(&[TokenKind::FStringStart, TokenKind::String]);
    /// Handles implicit concatenated f-strings, e.g. `f"{x}" f"hello"`, and
    /// implicit concatenated f-strings with strings, e.g. `f"{x}" "xyz" f"{x}"`.
    fn handle_implicit_concatenated_strings(&mut self, strings: &mut Vec<StringType>) {
        loop {
            let start = self.node_start();

            if self.eat(TokenKind::FStringStart) {
                strings.push(StringType::FString(self.parse_fstring(start)));
            } else if self.at(TokenKind::String) {
                let (
                    Tok::String {
                        value,
                        kind,
                        triple_quoted,
                    },
                    _,
                ) = self.next_token()
                else {
                    unreachable!()
                };

                let range = self.node_range(start);

                match parse_string_literal(&value, kind, triple_quoted, range) {
                    Ok(string) => {
                        strings.push(string);
                    }
                    Err(error) => {
                        strings.push(StringType::Invalid(ast::StringLiteral {
                            value,
                            range,
                            unicode: kind.is_unicode(),
                        }));
                        self.add_error(ParseErrorType::Lexical(error.error), error.location);
                    }
                }
            } else {
                break;
            }
        }
    }

    fn parse_fstring_expr(&mut self, start: TextSize) -> Expr {
        let fstring = self.parse_fstring(start);

        if !self.at_ts(Self::FSTRING_SET) {
            return Expr::FString(ast::ExprFString {
                value: ast::FStringValue::single(fstring),
                range: self.node_range(start),
            });
        }

        let mut strings = vec![StringType::FString(fstring)];
        self.handle_implicit_concatenated_strings(&mut strings);

        let range = self.node_range(start);

        concatenated_strings(strings, range).unwrap_or_else(|error| {
            self.add_error(ParseErrorType::Lexical(error.error), error.location);

            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(error.location).into(),
                range: error.location,
            })
        })
    }

    fn parse_fstring(&mut self, start: TextSize) -> ast::FString {
        let elements = self.parse_fstring_elements();

        self.expect(TokenKind::FStringEnd);

        ast::FString {
            elements,
            range: self.node_range(start),
        }
    }

    const FSTRING_END_SET: TokenSet =
        TokenSet::new(&[TokenKind::FStringEnd, TokenKind::Rbrace]).union(NEWLINE_EOF_SET);
    fn parse_fstring_elements(&mut self) -> Vec<FStringElement> {
        let mut elements = vec![];

        while !self.at_ts(Self::FSTRING_END_SET) {
            let element = match self.current_kind() {
                TokenKind::Lbrace => FStringElement::Expression(self.parse_fstring_expr_element()),
                TokenKind::FStringMiddle => {
                    let (Tok::FStringMiddle { value, is_raw }, range) = self.next_token() else {
                        unreachable!()
                    };
                    let fstring_literal = parse_fstring_literal_element(&value, is_raw, range)
                        .unwrap_or_else(|lex_error| {
                            self.add_error(
                                ParseErrorType::Lexical(lex_error.error),
                                lex_error.location,
                            );

                            ast::FStringElement::Invalid(ast::FStringInvalidElement {
                                value: self.src_text(lex_error.location).into(),
                                range: lex_error.location,
                            })
                        });
                    fstring_literal
                }
                // `Invalid` tokens are created when there's a lexical error, so
                // we ignore it here to avoid creating unexpected token errors
                TokenKind::Unknown => {
                    self.next_token();
                    continue;
                }
                // Handle an unexpected token
                _ => {
                    let (tok, range) = self.next_token();
                    self.add_error(
                        ParseErrorType::OtherError(format!("f-string: unexpected token `{tok:?}`")),
                        range,
                    );
                    continue;
                }
            };
            elements.push(element);
        }

        elements
    }

    fn parse_fstring_expr_element(&mut self) -> ast::FStringExpressionElement {
        let range = self.current_range();

        let has_open_brace = self.eat(TokenKind::Lbrace);
        let value = self.parse_expr_with_recovery(
            Parser::parse_expression,
            [
                TokenKind::Exclamation,
                TokenKind::Colon,
                TokenKind::Rbrace,
                TokenKind::FStringEnd,
            ]
            .as_slice(),
            "f-string: expecting expression",
        );
        if !value.is_parenthesized && matches!(value.expr, Expr::Lambda(_)) {
            self.add_error(
                ParseErrorType::FStringError(FStringErrorType::LambdaWithoutParentheses),
                value.range(),
            );
        }
        let debug_text = if self.eat(TokenKind::Equal) {
            let leading_range = range
                .add_start("{".text_len())
                .cover_offset(value.range().start());
            let trailing_range = TextRange::new(value.range().end(), self.current_range().start());
            Some(ast::DebugText {
                leading: self.src_text(leading_range).to_string(),
                trailing: self.src_text(trailing_range).to_string(),
            })
        } else {
            None
        };

        let conversion = if self.eat(TokenKind::Exclamation) {
            let (_, range) = self.next_token();
            match self.src_text(range) {
                "s" => ConversionFlag::Str,
                "r" => ConversionFlag::Repr,
                "a" => ConversionFlag::Ascii,
                _ => {
                    self.add_error(
                        ParseErrorType::FStringError(FStringErrorType::InvalidConversionFlag),
                        range,
                    );
                    ConversionFlag::None
                }
            }
        } else {
            ConversionFlag::None
        };

        let format_spec = if self.eat(TokenKind::Colon) {
            let spec_start = self.node_start();
            let elements = self.parse_fstring_elements();
            Some(Box::new(ast::FStringFormatSpec {
                range: self.node_range(spec_start),
                elements,
            }))
        } else {
            None
        };

        let close_brace_range = self.current_range();
        if has_open_brace && !self.eat(TokenKind::Rbrace) {
            self.add_error(
                ParseErrorType::FStringError(FStringErrorType::UnclosedLbrace),
                close_brace_range,
            );
        }

        ast::FStringExpressionElement {
            expression: Box::new(value.expr),
            debug_text,
            conversion,
            format_spec,
            range: range.cover(close_brace_range),
        }
    }

    fn parse_bracketsized_expr(&mut self, start: TextSize) -> Expr {
        // Nice error message when having a unclosed open bracket `[`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("missing closing bracket `]`".to_string()),
                self.current_range(),
            );
        }

        // Return an empty `ListExpr` when finding a `]` right after the `[`
        if self.eat(TokenKind::Rsqb) {
            return Expr::List(ast::ExprList {
                elts: vec![],
                ctx: ExprContext::Load,
                range: self.node_range(start),
            });
        }

        let parsed_expr = self.parse_expr2();

        match self.current_kind() {
            TokenKind::Async | TokenKind::For => {
                Expr::ListComp(self.parse_list_comprehension_expr(parsed_expr.expr, start))
            }
            _ => Expr::List(self.parse_list_expr(parsed_expr.expr, start)),
        }
    }

    fn parse_bracesized_expr(&mut self, start: TextSize) -> Expr {
        // Nice error message when having a unclosed open brace `{`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("missing closing brace `}`".to_string()),
                self.current_range(),
            );
        }

        // Return an empty `DictExpr` when finding a `}` right after the `{`
        if self.eat(TokenKind::Rbrace) {
            return Expr::Dict(ast::ExprDict {
                keys: vec![],
                values: vec![],
                range: self.node_range(start),
            });
        }

        if self.eat(TokenKind::DoubleStar) {
            // Handle dict unpack
            let value = self.parse_expr();
            return Expr::Dict(self.parse_dict_expr(None, value.expr, start));
        }

        let key_or_value = self.parse_expr2();

        match self.current_kind() {
            TokenKind::Async | TokenKind::For => {
                Expr::SetComp(self.parse_set_comprehension_expr(key_or_value.expr, start))
            }
            TokenKind::Colon => {
                self.bump(TokenKind::Colon);
                let value = self.parse_expr();

                if matches!(self.current_kind(), TokenKind::Async | TokenKind::For) {
                    Expr::DictComp(self.parse_dict_comprehension_expr(
                        key_or_value.expr,
                        value.expr,
                        start,
                    ))
                } else {
                    Expr::Dict(self.parse_dict_expr(Some(key_or_value.expr), value.expr, start))
                }
            }
            _ => Expr::Set(self.parse_set_expr(key_or_value.expr, start)),
        }
    }

    fn parse_parenthesized_expr(&mut self, start: TextSize) -> ParsedExpr {
        self.set_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

        // Nice error message when having a unclosed open parenthesis `(`
        if self.at_ts(NEWLINE_EOF_SET) {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("missing closing parenthesis `)`".to_string()),
                range,
            );
        }

        // Return an empty `TupleExpr` when finding a `)` right after the `(`
        if self.eat(TokenKind::Rpar) {
            self.clear_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

            return Expr::Tuple(ast::ExprTuple {
                elts: vec![],
                ctx: ExprContext::Load,
                range: self.node_range(start),
                parenthesized: true,
            })
            .into();
        }

        let mut parsed_expr = self.parse_expr2();

        let parsed = match self.current_kind() {
            TokenKind::Comma => {
                let tuple =
                    self.parse_tuple_expr(parsed_expr.expr, start, true, Parser::parse_expr2);

                ParsedExpr {
                    expr: tuple.into(),
                    is_parenthesized: false,
                }
            }
            TokenKind::Async | TokenKind::For => {
                let generator =
                    Expr::GeneratorExp(self.parse_generator_expr(parsed_expr.expr, start, true));

                ParsedExpr {
                    expr: generator,
                    is_parenthesized: false,
                }
            }
            _ => {
                self.expect_and_recover(TokenKind::Rpar, TokenSet::EMPTY);

                parsed_expr.is_parenthesized = true;
                parsed_expr
            }
        };

        self.clear_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

        parsed
    }

    const END_SEQUENCE_SET: TokenSet = END_EXPR_SET.remove(TokenKind::Comma);
    /// Parses multiple items separated by a comma into a `TupleExpr` node.
    /// Uses `parse_func` to parse each item.
    pub(super) fn parse_tuple_expr(
        &mut self,
        first_element: Expr,
        start: TextSize,
        parenthesized: bool,
        mut parse_func: impl FnMut(&mut Parser<'src>) -> ParsedExpr,
    ) -> ast::ExprTuple {
        // In case of the tuple only having one element, we need to cover the
        // range of the comma.
        if !self.at_ts(Self::END_SEQUENCE_SET) {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_separated(true, TokenKind::Comma, Self::END_SEQUENCE_SET, |parser| {
            elts.push(parse_func(parser).expr);
        });

        if parenthesized {
            self.expect(TokenKind::Rpar);
        }

        ast::ExprTuple {
            elts,
            ctx: ExprContext::Load,
            range: self.node_range(start),
            parenthesized,
        }
    }

    fn parse_list_expr(&mut self, first_element: Expr, start: TextSize) -> ast::ExprList {
        if !self.at_ts(Self::END_SEQUENCE_SET) {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_separated(true, TokenKind::Comma, Self::END_SEQUENCE_SET, |parser| {
            elts.push(parser.parse_expr2().expr);
        });

        self.expect(TokenKind::Rsqb);

        ast::ExprList {
            elts,
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    fn parse_set_expr(&mut self, first_element: Expr, start: TextSize) -> ast::ExprSet {
        if !self.at_ts(Self::END_SEQUENCE_SET) {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_separated(true, TokenKind::Comma, Self::END_SEQUENCE_SET, |parser| {
            elts.push(parser.parse_expr2().expr);
        });

        self.expect(TokenKind::Rbrace);

        ast::ExprSet {
            range: self.node_range(start),
            elts,
        }
    }

    fn parse_dict_expr(
        &mut self,
        key: Option<Expr>,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprDict {
        if !self.at_ts(Self::END_SEQUENCE_SET) {
            self.expect(TokenKind::Comma);
        }

        let mut keys = vec![key];
        let mut values = vec![value];

        self.parse_separated(true, TokenKind::Comma, Self::END_SEQUENCE_SET, |parser| {
            if parser.eat(TokenKind::DoubleStar) {
                keys.push(None);
            } else {
                keys.push(Some(parser.parse_expr().expr));

                parser.expect_and_recover(
                    TokenKind::Colon,
                    TokenSet::new(&[TokenKind::Comma]).union(EXPR_SET),
                );
            }
            values.push(parser.parse_expr().expr);
        });

        self.expect(TokenKind::Rbrace);

        ast::ExprDict {
            range: self.node_range(start),
            keys,
            values,
        }
    }

    fn parse_comprehension(&mut self) -> ast::Comprehension {
        let start = self.node_start();

        let is_async = self.eat(TokenKind::Async);

        self.bump(TokenKind::For);

        self.set_ctx(ParserCtxFlags::FOR_TARGET);
        let mut target = self.parse_expr_with_recovery(
            Parser::parse_expression,
            [TokenKind::In, TokenKind::Colon].as_slice(),
            "expecting expression after `for` keyword",
        );
        self.clear_ctx(ParserCtxFlags::FOR_TARGET);

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        self.expect_and_recover(TokenKind::In, TokenSet::new(&[TokenKind::Rsqb]));

        let iter = self.parse_expr_with_recovery(
            Parser::parse_expr_simple,
            EXPR_SET.union(
                [
                    TokenKind::Rpar,
                    TokenKind::Rsqb,
                    TokenKind::Rbrace,
                    TokenKind::If,
                    TokenKind::Async,
                    TokenKind::For,
                ]
                .as_slice()
                .into(),
            ),
            "expecting an expression after `in` keyword",
        );

        let mut ifs = vec![];
        while self.eat(TokenKind::If) {
            ifs.push(self.parse_expr_simple().expr);
        }

        ast::Comprehension {
            range: self.node_range(start),
            target: target.expr,
            iter: iter.expr,
            ifs,
            is_async,
        }
    }

    fn parse_generators(&mut self) -> Vec<ast::Comprehension> {
        const GENERATOR_SET: TokenSet = TokenSet::new(&[TokenKind::For, TokenKind::Async]);
        let mut generators = vec![];
        while self.at_ts(GENERATOR_SET) {
            generators.push(self.parse_comprehension());
        }

        generators
    }

    fn parse_generator_expr(
        &mut self,
        element: Expr,
        start: TextSize,
        in_parentheses: bool,
    ) -> ast::ExprGeneratorExp {
        let generators = self.parse_generators();

        if in_parentheses {
            self.expect(TokenKind::Rpar);
        }

        ast::ExprGeneratorExp {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
        }
    }

    fn parse_list_comprehension_expr(
        &mut self,
        element: Expr,
        start: TextSize,
    ) -> ast::ExprListComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rsqb);

        ast::ExprListComp {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
        }
    }

    fn parse_dict_comprehension_expr(
        &mut self,
        key: Expr,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprDictComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rbrace);

        ast::ExprDictComp {
            key: Box::new(key),
            value: Box::new(value),
            generators,
            range: self.node_range(start),
        }
    }

    fn parse_set_comprehension_expr(&mut self, element: Expr, start: TextSize) -> ast::ExprSetComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rbrace);

        ast::ExprSetComp {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
        }
    }

    fn parse_starred_expr(&mut self, start: TextSize) -> ast::ExprStarred {
        let parsed_expr = self.parse_expr();

        ast::ExprStarred {
            value: Box::new(parsed_expr.expr),
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    fn parse_await_expr(&mut self, start: TextSize) -> ast::ExprAwait {
        let parsed_expr = self.parse_expression_with_precedence(19);

        if matches!(parsed_expr.expr, Expr::Starred(_)) {
            self.add_error(
                ParseErrorType::OtherError(
                    "starred expression is not allowed in an `await` statement".to_string(),
                ),
                &parsed_expr,
            );
        }

        ast::ExprAwait {
            value: Box::new(parsed_expr.expr),
            range: self.node_range(start),
        }
    }

    fn parse_yield_expr(&mut self, start: TextSize) -> Expr {
        if self.eat(TokenKind::From) {
            return self.parse_yield_from_expr(start);
        }

        let value = self
            .at_expr()
            .then(|| Box::new(self.parse_expression().expr));

        Expr::Yield(ast::ExprYield {
            value,
            range: self.node_range(start),
        })
    }

    fn parse_yield_from_expr(&mut self, start: TextSize) -> Expr {
        let parsed_expr = self.parse_expression();

        match &parsed_expr.expr {
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                // Should we make `expr` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(
                        "starred expression is not allowed in a `yield from` statement".to_string(),
                    ),
                    value.as_ref(),
                );
            }
            Expr::Tuple(tuple) if !tuple.parenthesized => {
                // Should we make `expr` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized tuple is not allowed in a `yield from` statement"
                            .to_string(),
                    ),
                    tuple,
                );
            }
            _ => {}
        }

        Expr::YieldFrom(ast::ExprYieldFrom {
            value: Box::new(parsed_expr.expr),
            range: self.node_range(start),
        })
    }

    fn parse_named_expr(&mut self, mut target: Expr, start: TextSize) -> ast::ExprNamedExpr {
        self.bump(TokenKind::ColonEqual);

        if !helpers::is_valid_assignment_target(&target) {
            self.add_error(ParseErrorType::NamedAssignmentError, target.range());
        }
        helpers::set_expr_ctx(&mut target, ExprContext::Store);

        let value = self.parse_expr();

        ast::ExprNamedExpr {
            target: Box::new(target),
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    fn parse_lambda_expr(&mut self, start: TextSize) -> ast::ExprLambda {
        let parameters: Option<Box<ast::Parameters>> = if self.at(TokenKind::Colon) {
            None
        } else {
            Some(Box::new(self.parse_parameters(FunctionKind::Lambda)))
        };

        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        // Check for forbidden tokens in the `lambda`'s body
        match self.current_kind() {
            TokenKind::Yield => self.add_error(
                ParseErrorType::OtherError(
                    "`yield` not allowed in a `lambda` expression".to_string(),
                ),
                self.current_range(),
            ),
            TokenKind::Star => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "starred expression not allowed in a `lambda` expression".to_string(),
                    ),
                    self.current_range(),
                );
            }
            TokenKind::DoubleStar => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "double starred expression not allowed in a `lambda` expression"
                            .to_string(),
                    ),
                    self.current_range(),
                );
            }
            _ => {}
        }

        let body = self.parse_expr();

        ast::ExprLambda {
            body: Box::new(body.expr),
            parameters,
            range: self.node_range(start),
        }
    }

    fn parse_if_expr(&mut self, body: Expr, start: TextSize) -> ast::ExprIfExp {
        self.bump(TokenKind::If);

        let test = self.parse_expr_simple();

        self.expect_and_recover(TokenKind::Else, TokenSet::EMPTY);

        let orelse = self.parse_expr_with_recovery(
            Parser::parse_expr,
            TokenSet::EMPTY,
            "expecting expression after `else` keyword",
        );

        ast::ExprIfExp {
            body: Box::new(body),
            test: Box::new(test.expr),
            orelse: Box::new(orelse.expr),
            range: self.node_range(start),
        }
    }
}

#[derive(Debug)]
pub(super) struct ParsedExpr {
    pub(super) expr: Expr,
    pub(super) is_parenthesized: bool,
}

impl From<Expr> for ParsedExpr {
    #[inline]
    fn from(expr: Expr) -> Self {
        ParsedExpr {
            expr,
            is_parenthesized: false,
        }
    }
}

impl Deref for ParsedExpr {
    type Target = Expr;

    fn deref(&self) -> &Self::Target {
        &self.expr
    }
}

impl Ranged for ParsedExpr {
    #[inline]
    fn range(&self) -> TextRange {
        self.expr.range()
    }
}

/// Binding power associativity
enum Associativity {
    Left,
    Right,
}

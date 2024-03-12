use std::ops::Deref;

use ruff_python_ast::{
    self as ast, BoolOp, CmpOp, ConversionFlag, Expr, ExprContext, FStringElement, Number,
    Operator, UnaryOp,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::parser::helpers::token_kind_to_cmp_op;
use crate::parser::progress::ParserProgress;
use crate::parser::{helpers, FunctionKind, Parser, ParserCtxFlags, EXPR_SET, NEWLINE_EOF_SET};
use crate::string::{
    concatenated_strings, parse_fstring_literal_element, parse_string_literal, StringType,
};
use crate::token_set::TokenSet;
use crate::{FStringErrorType, Mode, ParseErrorType, Tok, TokenKind};

use super::{RecoveryContextKind, TupleParenthesized};

/// Tokens that can appear after an expression.
/// FIXME: this isn't exhaustive.
pub(super) const END_EXPR_SET: TokenSet = TokenSet::new([
    // Ex) `expr`
    TokenKind::Newline,
    // Ex) `expr;`
    TokenKind::Semi,
    // Ex) `data[expr:]`
    TokenKind::Colon,
    // Ex) `expr` (without a newline)
    TokenKind::EndOfFile,
    // Ex) `{expr}`
    TokenKind::Rbrace,
    // Ex) `[expr]`
    TokenKind::Rsqb,
    // Ex) `(expr)`
    TokenKind::Rpar,
    // Ex) `expr,`
    TokenKind::Comma,
    // Ex) ??
    TokenKind::Dedent,
    // Ex) `expr if expr else expr`
    TokenKind::If,
    TokenKind::Else,
    TokenKind::As,
    TokenKind::From,
    TokenKind::For,
    TokenKind::Async,
    TokenKind::In,
    // Ex) `f"{expr=}"`
    TokenKind::Equal,
    // Ex) `f"{expr!s}"`
    TokenKind::Exclamation,
]);

const END_SEQUENCE_SET: TokenSet = END_EXPR_SET.remove(TokenKind::Comma);

impl<'src> Parser<'src> {
    pub(super) fn at_expr(&self) -> bool {
        self.at_ts(EXPR_SET)
    }

    #[allow(dead_code)]
    pub(super) fn at_expr_end(&self) -> bool {
        self.at_ts(END_EXPR_SET)
    }

    pub(super) fn at_sequence_end(&self) -> bool {
        self.at_ts(END_SEQUENCE_SET)
    }

    /// Parses every Python expression.
    /// Matches the `expressions` rule in the Python grammar.
    pub(super) fn parse_expression(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_conditional_expression_or_higher();

        if self.at(TokenKind::Comma) {
            Expr::Tuple(self.parse_tuple_expression(
                parsed_expr.expr,
                start,
                TupleParenthesized::No,
                Parser::parse_conditional_expression_or_higher,
            ))
            .into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuple.
    /// Matches the `named_expression` rule in the Python grammar.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    pub(super) fn parse_named_expression_or_higher(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_conditional_expression_or_higher();

        if self.at(TokenKind::ColonEqual) {
            Expr::Named(self.parse_named_expression(parsed_expr.expr, start)).into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuple and named expressions.
    /// Matches the `expression` rule in the Python grammar.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    pub(super) fn parse_conditional_expression_or_higher(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_simple_expression();

        if self.at(TokenKind::If) {
            Expr::If(self.parse_if_expression(parsed_expr.expr, start)).into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuples, named expressions, and `if` expression.
    /// This is a combination of the `disjunction` and `starred_expression` rules of the Python
    /// grammar.
    ///
    /// When parsing an AST node that only uses one of the rules (`disjunction` or `starred_expression`),
    /// you need to **explicitly** check if an invalid node for that AST node was parsed. Check the
    /// `parse_yield_from_expression` function for an example of this situation.
    fn parse_simple_expression(&mut self) -> ParsedExpr {
        self.parse_expression_with_precedence(Precedence::Initial)
    }

    /// Binding powers of operators for a Pratt parser.
    ///
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    fn current_op(&mut self) -> (Precedence, TokenKind, Associativity) {
        const NOT_AN_OP: (Precedence, TokenKind, Associativity) =
            (Precedence::Unknown, TokenKind::Unknown, Associativity::Left);
        let kind = self.current_token_kind();

        match kind {
            TokenKind::Or => (Precedence::Or, kind, Associativity::Left),
            TokenKind::And => (Precedence::And, kind, Associativity::Left),
            TokenKind::Not if self.peek_nth(1) == TokenKind::In => (
                Precedence::ComparisonsMembershipIdentity,
                kind,
                Associativity::Left,
            ),
            TokenKind::Is
            | TokenKind::In
            | TokenKind::EqEqual
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual => (
                Precedence::ComparisonsMembershipIdentity,
                kind,
                Associativity::Left,
            ),
            TokenKind::Vbar => (Precedence::BitOr, kind, Associativity::Left),
            TokenKind::CircumFlex => (Precedence::BitXor, kind, Associativity::Left),
            TokenKind::Amper => (Precedence::BitAnd, kind, Associativity::Left),
            TokenKind::LeftShift | TokenKind::RightShift => {
                (Precedence::LeftRightShift, kind, Associativity::Left)
            }
            TokenKind::Plus | TokenKind::Minus => (Precedence::AddSub, kind, Associativity::Left),
            TokenKind::Star
            | TokenKind::Slash
            | TokenKind::DoubleSlash
            | TokenKind::Percent
            | TokenKind::At => (Precedence::MulDivRemain, kind, Associativity::Left),
            TokenKind::DoubleStar => (Precedence::Exponent, kind, Associativity::Right),
            _ => NOT_AN_OP,
        }
    }

    /// Parses expression with binding power of at least bp.
    ///
    /// Uses the Pratt parser algorithm.
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    fn parse_expression_with_precedence(&mut self, previous_precedence: Precedence) -> ParsedExpr {
        let start = self.node_start();
        let mut lhs = self.parse_lhs_expression();

        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            let (current_precedence, op, associativity) = self.current_op();
            if current_precedence < previous_precedence {
                break;
            }

            // Don't parse a `CompareExpr` if we are parsing a `Comprehension` or `ForStmt`
            if op.is_compare_operator() && self.has_ctx(ParserCtxFlags::FOR_TARGET) {
                break;
            }

            let op_bp = match associativity {
                Associativity::Left => current_precedence.increment_precedence(),
                Associativity::Right => current_precedence,
            };

            self.bump(op);

            // We need to create a dedicated node for boolean operations,
            // even though boolean operations are infix.
            if op.is_bool_operator() {
                lhs =
                    Expr::BoolOp(self.parse_bool_operation_expression(lhs.expr, start, op, op_bp))
                        .into();
                continue;
            }

            // Same here as well
            if op.is_compare_operator() {
                lhs =
                    Expr::Compare(self.parse_compare_expression(lhs.expr, start, op, op_bp)).into();
                continue;
            }

            let rhs = self.parse_expression_with_precedence(op_bp);

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
        let mut lhs = match self.current_token_kind() {
            TokenKind::Plus | TokenKind::Minus | TokenKind::Not | TokenKind::Tilde => {
                Expr::UnaryOp(self.parse_unary_expression()).into()
            }
            TokenKind::Star => Expr::Starred(self.parse_starred_expression()).into(),
            TokenKind::Await => Expr::Await(self.parse_await_expression()).into(),
            TokenKind::Lambda => Expr::Lambda(self.parse_lambda_expr()).into(),
            _ => self.parse_atom(),
        };

        if self.is_current_token_postfix() {
            lhs = self.parse_postfix_expression(lhs.expr, start).into();
        }

        lhs
    }

    pub(super) fn parse_name(&mut self) -> ast::ExprName {
        let identifier = self.parse_identifier();

        ast::ExprName {
            range: identifier.range,
            id: identifier.id,
            ctx: ExprContext::Load,
        }
    }

    pub(super) fn parse_identifier(&mut self) -> ast::Identifier {
        let range = self.current_token_range();

        if self.at(TokenKind::Name) {
            let (Tok::Name { name }, _) = self.next_token() else {
                unreachable!();
            };
            ast::Identifier {
                id: name.to_string(),
                range,
            }
        } else {
            if self.current_token_kind().is_keyword() {
                let (tok, range) = self.next_token();
                self.add_error(
                    ParseErrorType::OtherError(format!(
                        "Identifier expected. '{tok}' is a keyword that cannot be used here."
                    )),
                    range,
                );

                ast::Identifier {
                    id: tok.to_string(),
                    range,
                }
            } else {
                self.add_error(
                    ParseErrorType::OtherError("expecting an identifier".into()),
                    range,
                );
                ast::Identifier {
                    id: String::new(),
                    range: self.missing_node_range(),
                }
            }
        }
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#atoms>
    fn parse_atom(&mut self) -> ParsedExpr {
        let start = self.node_start();

        let lhs = match self.current_token_kind() {
            TokenKind::Float => {
                let (Tok::Float { value }, _) = self.bump(TokenKind::Float) else {
                    unreachable!()
                };

                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Float(value),
                    range: self.node_range(start),
                })
            }
            TokenKind::Complex => {
                let (Tok::Complex { real, imag }, _) = self.bump(TokenKind::Complex) else {
                    unreachable!()
                };
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Complex { real, imag },
                    range: self.node_range(start),
                })
            }
            TokenKind::Int => {
                let (Tok::Int { value }, _) = self.bump(TokenKind::Int) else {
                    unreachable!()
                };
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Int(value),
                    range: self.node_range(start),
                })
            }
            TokenKind::True => {
                self.bump(TokenKind::True);
                Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                    value: true,
                    range: self.node_range(start),
                })
            }
            TokenKind::False => {
                self.bump(TokenKind::False);
                Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                    value: false,
                    range: self.node_range(start),
                })
            }
            TokenKind::None => {
                self.bump(TokenKind::None);
                Expr::NoneLiteral(ast::ExprNoneLiteral {
                    range: self.node_range(start),
                })
            }
            TokenKind::Ellipsis => {
                self.bump(TokenKind::Ellipsis);
                Expr::EllipsisLiteral(ast::ExprEllipsisLiteral {
                    range: self.node_range(start),
                })
            }
            TokenKind::Name => Expr::Name(self.parse_name()),
            TokenKind::EscapeCommand => {
                let (Tok::IpyEscapeCommand { value, kind }, _) =
                    self.bump(TokenKind::EscapeCommand)
                else {
                    unreachable!()
                };

                let command = ast::ExprIpyEscapeCommand {
                    range: self.node_range(start),
                    kind,
                    value,
                };

                if self.mode != Mode::Ipython {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "IPython escape commands are only allowed in `Mode::Ipython`".into(),
                        ),
                        &command,
                    );
                }

                Expr::IpyEscapeCommand(command)
            }
            TokenKind::String => self.parse_string_expression(),
            TokenKind::FStringStart => self.parse_fstring_expression(),
            TokenKind::Lpar => {
                return self.parse_parenthesized_expression();
            }

            TokenKind::Lsqb => self.parse_list_like_expression(),
            TokenKind::Lbrace => self.parse_set_or_dict_like_expression(),
            TokenKind::Yield => self.parse_yield_expression(),

            kind => {
                if kind.is_keyword() {
                    Expr::Name(self.parse_name())
                } else {
                    self.add_error(
                        ParseErrorType::OtherError("Expression expected.".to_string()),
                        self.current_token_range(),
                    );
                    Expr::Name(ast::ExprName {
                        range: self.missing_node_range(),
                        id: String::new(),
                        ctx: ExprContext::Load,
                    })
                }
            }
        };

        lhs.into()
    }

    fn parse_postfix_expression(&mut self, mut lhs: Expr, start: TextSize) -> Expr {
        loop {
            lhs = match self.current_token_kind() {
                TokenKind::Lpar => Expr::Call(self.parse_call_expression(lhs, start)),
                TokenKind::Lsqb => Expr::Subscript(self.parse_subscript_expression(lhs, start)),
                TokenKind::Dot => Expr::Attribute(self.parse_attribute_expression(lhs, start)),
                _ => break lhs,
            };
        }
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#calls>
    fn parse_call_expression(&mut self, lhs: Expr, start: TextSize) -> ast::ExprCall {
        assert_eq!(self.current_token_kind(), TokenKind::Lpar);
        let arguments = self.parse_arguments();

        ast::ExprCall {
            func: Box::new(lhs),
            arguments,
            range: self.node_range(start),
        }
    }

    /// Parses an argument list.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `(` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#grammar-token-python-grammar-argument_list>
    pub(super) fn parse_arguments(&mut self) -> ast::Arguments {
        let start = self.node_start();
        self.bump(TokenKind::Lpar);

        let saved_context = self.set_ctx(ParserCtxFlags::ARGUMENTS);

        let mut args: Vec<Expr> = vec![];
        let mut keywords: Vec<ast::Keyword> = vec![];
        let mut has_seen_kw_arg = false;
        let mut has_seen_kw_unpack = false;

        self.parse_comma_separated_list(
            RecoveryContextKind::Arguments,
            |parser| {
                let argument_start = parser.node_start();
                if parser.at(TokenKind::DoubleStar) {
                    parser.eat(TokenKind::DoubleStar);

                    let value = parser.parse_conditional_expression_or_higher();
                    keywords.push(ast::Keyword {
                        arg: None,
                        value: value.expr,
                        range: parser.node_range(argument_start),
                    });

                    has_seen_kw_unpack = true;
                } else {
                    let start = parser.node_start();
                    let mut parsed_expr = parser.parse_named_expression_or_higher();

                    match parser.current_token_kind() {
                        TokenKind::Async | TokenKind::For => {
                            parsed_expr = Expr::Generator(parser.parse_generator_expression(
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

                        let value = parser.parse_conditional_expression_or_higher();

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
            true,
        );

        self.restore_ctx(ParserCtxFlags::ARGUMENTS, saved_context);

        self.expect(TokenKind::Rpar);

        let arguments = ast::Arguments {
            range: self.node_range(start),
            args: args.into_boxed_slice(),
            keywords: keywords.into_boxed_slice(),
        };

        if let Err(error) = helpers::validate_arguments(&arguments) {
            self.add_error(error.error, error.location);
        }

        arguments
    }

    /// Parses a subscript expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#subscriptions>
    fn parse_subscript_expression(
        &mut self,
        mut value: Expr,
        start: TextSize,
    ) -> ast::ExprSubscript {
        self.bump(TokenKind::Lsqb);

        // To prevent the `value` context from being `Del` within a `del` statement,
        // we set the context as `Load` here.
        helpers::set_expr_ctx(&mut value, ExprContext::Load);

        // Create an error when receiving a empty slice to parse, e.g. `l[]`
        if !self.at(TokenKind::Colon) && !self.at_expr() {
            let slice_range = TextRange::empty(self.current_token_range().start());
            self.expect(TokenKind::Rsqb);

            let range = self.node_range(start);
            self.add_error(ParseErrorType::EmptySlice, range);
            #[allow(deprecated)]
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

        // If there are more than one element in the slice, we need to create a tuple
        // expression to represent it.
        if self.eat(TokenKind::Comma) {
            let mut slices = vec![slice];

            self.parse_comma_separated_list(
                RecoveryContextKind::Slices,
                |parser| slices.push(parser.parse_slice()),
                true,
            );

            slice = Expr::Tuple(ast::ExprTuple {
                elts: slices,
                ctx: ExprContext::Load,
                range: self.node_range(slice_start),
                parenthesized: false,
            });
        }

        self.expect(TokenKind::Rsqb);

        ast::ExprSubscript {
            value: Box::new(value),
            slice: Box::new(slice),
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#slicings>
    fn parse_slice(&mut self) -> Expr {
        const UPPER_END_SET: TokenSet =
            TokenSet::new([TokenKind::Comma, TokenKind::Colon, TokenKind::Rsqb])
                .union(NEWLINE_EOF_SET);
        const STEP_END_SET: TokenSet =
            TokenSet::new([TokenKind::Comma, TokenKind::Rsqb]).union(NEWLINE_EOF_SET);

        let start = self.node_start();

        let lower = if self.at_expr() {
            let lower = self.parse_named_expression_or_higher();
            if self.at_ts(NEWLINE_EOF_SET.union([TokenKind::Rsqb, TokenKind::Comma].into())) {
                return lower.expr;
            }

            Some(lower.expr)
        } else {
            None
        };

        self.expect(TokenKind::Colon);

        let lower = lower.map(Box::new);
        let upper = if self.at_ts(UPPER_END_SET) {
            None
        } else {
            Some(Box::new(self.parse_conditional_expression_or_higher().expr))
        };

        let step = if self.eat(TokenKind::Colon) {
            if self.at_ts(STEP_END_SET) {
                None
            } else {
                Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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

    fn parse_unary_expression(&mut self) -> ast::ExprUnaryOp {
        let start = self.node_start();

        let op = UnaryOp::try_from(self.current_token_kind())
            .expect("Expected operator to be a unary operator token.");
        self.bump(self.current_token_kind());

        let operand = if matches!(op, UnaryOp::Not) {
            self.parse_expression_with_precedence(Precedence::Not)
        } else {
            // plus, minus and tilde
            self.parse_expression_with_precedence(Precedence::PosNegBitNot)
        };

        ast::ExprUnaryOp {
            op,
            operand: Box::new(operand.expr),
            range: self.node_range(start),
        }
    }

    pub(super) fn parse_attribute_expression(
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

    fn parse_bool_operation_expression(
        &mut self,
        lhs: Expr,
        start: TextSize,
        op: TokenKind,
        op_bp: Precedence,
    ) -> ast::ExprBoolOp {
        let mut values = vec![lhs];
        let mut progress = ParserProgress::default();

        // Keep adding `expr` to `values` until we see a different
        // boolean operation than `op`.
        loop {
            progress.assert_progressing(self);
            let parsed_expr = self.parse_expression_with_precedence(op_bp);
            values.push(parsed_expr.expr);

            if self.current_token_kind() != op {
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

    fn parse_compare_expression(
        &mut self,
        lhs: Expr,
        start: TextSize,
        op: TokenKind,
        op_bp: Precedence,
    ) -> ast::ExprCompare {
        let mut comparators = vec![];
        let op = token_kind_to_cmp_op([op, self.current_token_kind()]).unwrap();
        let mut ops = vec![op];

        if matches!(op, CmpOp::IsNot | CmpOp::NotIn) {
            self.next_token();
        }

        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);
            let parsed_expr = self.parse_expression_with_precedence(op_bp);
            comparators.push(parsed_expr.expr);

            if let Ok(op) = token_kind_to_cmp_op([self.current_token_kind(), self.peek_nth(1)]) {
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
            ops: ops.into_boxed_slice(),
            comparators: comparators.into_boxed_slice(),
            range: self.node_range(start),
        }
    }

    pub(super) fn parse_string_expression(&mut self) -> Expr {
        let start = self.node_start();
        let mut strings = vec![];
        let mut progress = ParserProgress::default();

        while self.at(TokenKind::String) {
            progress.assert_progressing(self);
            let (
                Tok::String {
                    value,
                    kind,
                    triple_quoted,
                },
                tok_range,
            ) = self.bump(TokenKind::String)
            else {
                unreachable!()
            };

            match parse_string_literal(value, kind, triple_quoted, tok_range) {
                Ok(string) => {
                    strings.push(string);
                }
                Err(error) => {
                    strings.push(StringType::Invalid(ast::StringLiteral {
                        value: self.src_text(tok_range).to_string().into_boxed_str(),
                        range: tok_range,
                        unicode: kind.is_unicode(),
                    }));
                    let location = error.location();
                    self.add_error(ParseErrorType::Lexical(error.into_error()), location);
                }
            }
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
                    // TODO(micha): Is this valid? I thought string and byte literals can't be concatenated? Maybe not a syntax error?
                    Expr::BytesLiteral(ast::ExprBytesLiteral {
                        value: ast::BytesLiteralValue::single(bytes),
                        range,
                    })
                }
                #[allow(deprecated)]
                StringType::Invalid(invalid) => Expr::Invalid(ast::ExprInvalid {
                    value: invalid.value.to_string(),
                    range,
                }),
                StringType::FString(_) => unreachable!(),
            };
        }

        concatenated_strings(strings, range).unwrap_or_else(|error| {
            let location = error.location();
            self.add_error(ParseErrorType::Lexical(error.into_error()), location);
            #[allow(deprecated)]
            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(location).into(),
                range: location,
            })
        })
    }

    /// Handles implicit concatenated f-strings, e.g. `f"{x}" f"hello"`, and
    /// implicit concatenated f-strings with strings, e.g. `f"{x}" "xyz" f"{x}"`.
    fn handle_implicit_concatenated_strings(&mut self, strings: &mut Vec<StringType>) {
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);
            let start = self.node_start();

            if self.at(TokenKind::FStringStart) {
                strings.push(StringType::FString(self.parse_fstring()));
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

                match parse_string_literal(value, kind, triple_quoted, range) {
                    Ok(string) => {
                        strings.push(string);
                    }
                    Err(error) => {
                        strings.push(StringType::Invalid(ast::StringLiteral {
                            value: self.src_text(error.location()).to_string().into_boxed_str(),
                            range,
                            unicode: kind.is_unicode(),
                        }));
                        let location = error.location();
                        self.add_error(ParseErrorType::Lexical(error.into_error()), location);
                    }
                }
            } else {
                break;
            }
        }
    }

    /// Parses a f-string expression.
    fn parse_fstring_expression(&mut self) -> Expr {
        const FSTRING_SET: TokenSet = TokenSet::new([TokenKind::FStringStart, TokenKind::String]);

        let start = self.node_start();
        let fstring = self.parse_fstring();

        if !self.at_ts(FSTRING_SET) {
            return Expr::FString(ast::ExprFString {
                value: ast::FStringValue::single(fstring),
                range: self.node_range(start),
            });
        }

        let mut strings = vec![StringType::FString(fstring)];
        self.handle_implicit_concatenated_strings(&mut strings);

        let range = self.node_range(start);

        concatenated_strings(strings, range).unwrap_or_else(|error| {
            let location = error.location();
            self.add_error(ParseErrorType::Lexical(error.into_error()), location);

            #[allow(deprecated)]
            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(location).into(),
                range: location,
            })
        })
    }

    /// Parses a f-string.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `FStringStart` token.
    ///
    /// See: <https://docs.python.org/3/reference/grammar.html> (Search "fstring:")
    fn parse_fstring(&mut self) -> ast::FString {
        let start = self.node_start();

        self.bump(TokenKind::FStringStart);
        let elements = self.parse_fstring_elements();

        self.expect(TokenKind::FStringEnd);

        ast::FString {
            elements,
            range: self.node_range(start),
        }
    }

    /// Parses a list of f-string elements.
    fn parse_fstring_elements(&mut self) -> Vec<FStringElement> {
        let mut elements = vec![];

        self.parse_list(RecoveryContextKind::FStringElements, |parser| {
            let element = match parser.current_token_kind() {
                TokenKind::Lbrace => {
                    FStringElement::Expression(parser.parse_fstring_expression_element())
                }
                TokenKind::FStringMiddle => {
                    let (Tok::FStringMiddle { value, is_raw, .. }, range) = parser.next_token()
                    else {
                        unreachable!()
                    };
                    FStringElement::Literal(
                        parse_fstring_literal_element(value, is_raw, range).unwrap_or_else(
                            |lex_error| {
                                let location = lex_error.location();
                                parser.add_error(
                                    ParseErrorType::Lexical(lex_error.into_error()),
                                    location,
                                );
                                ast::FStringLiteralElement {
                                    value: "".into(),
                                    range: parser.missing_node_range(),
                                }
                            },
                        ),
                    )
                }
                // `Invalid` tokens are created when there's a lexical error, so
                // we ignore it here to avoid creating unexpected token errors
                TokenKind::Unknown => {
                    parser.next_token();
                    return;
                }
                tok => {
                    // This should never happen because the list parsing will only
                    // call this closure for the above token kinds which are the same
                    // as in the FIRST set.
                    unreachable!(
                        "f-string: unexpected token `{tok:?}` at {:?}",
                        parser.current_token_range()
                    );
                }
            };
            elements.push(element);
        });

        elements
    }

    /// Parses a f-string expression element.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` token.
    fn parse_fstring_expression_element(&mut self) -> ast::FStringExpressionElement {
        let start = self.node_start();

        self.bump(TokenKind::Lbrace);

        let value = self.parse_expression();
        if !value.is_parenthesized && value.expr.is_lambda_expr() {
            self.add_error(
                ParseErrorType::FStringError(FStringErrorType::LambdaWithoutParentheses),
                value.range(),
            );
        }
        let debug_text = if self.eat(TokenKind::Equal) {
            let leading_range = TextRange::new(start + "{".text_len(), value.start());
            let trailing_range = TextRange::new(value.end(), self.current_token_range().start());
            Some(ast::DebugText {
                leading: self.src_text(leading_range).to_string(),
                trailing: self.src_text(trailing_range).to_string(),
            })
        } else {
            None
        };

        let conversion = if self.eat(TokenKind::Exclamation) {
            let conversion_flag_range = self.current_token_range();
            if let Tok::Name { name } = self.next_token().0 {
                match &*name {
                    "s" => ConversionFlag::Str,
                    "r" => ConversionFlag::Repr,
                    "a" => ConversionFlag::Ascii,
                    _ => {
                        self.add_error(
                            ParseErrorType::FStringError(FStringErrorType::InvalidConversionFlag),
                            conversion_flag_range,
                        );
                        ConversionFlag::None
                    }
                }
            } else {
                self.add_error(
                    ParseErrorType::FStringError(FStringErrorType::InvalidConversionFlag),
                    conversion_flag_range,
                );
                ConversionFlag::None
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

        // We're using `eat` here instead of `expect` to use the f-string specific error type.
        if !self.eat(TokenKind::Rbrace) {
            self.add_error(
                ParseErrorType::FStringError(FStringErrorType::UnclosedLbrace),
                self.current_token_range(),
            );
        }

        ast::FStringExpressionElement {
            expression: Box::new(value.expr),
            debug_text,
            conversion,
            format_spec,
            range: self.node_range(start),
        }
    }

    /// Parses a list or a list comprehension expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `[` token.
    fn parse_list_like_expression(&mut self) -> Expr {
        let start = self.node_start();

        self.bump(TokenKind::Lsqb);

        // Nice error message when having a unclosed open bracket `[`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("missing closing bracket `]`".to_string()),
                self.current_token_range(),
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

        let first_element = self.parse_named_expression_or_higher();

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                Expr::ListComp(self.parse_list_comprehension_expression(first_element.expr, start))
            }
            _ => Expr::List(self.parse_list_expression(first_element.expr, start)),
        }
    }

    /// Parses a set, dict, set comprehension, or dict comprehension.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` token.
    fn parse_set_or_dict_like_expression(&mut self) -> Expr {
        let start = self.node_start();
        self.bump(TokenKind::Lbrace);

        // Nice error message when having a unclosed open brace `{`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("missing closing brace `}`".to_string()),
                self.current_token_range(),
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
            let value = self.parse_conditional_expression_or_higher();
            return Expr::Dict(self.parse_dictionary_expression(None, value.expr, start));
        }

        let key_or_value = self.parse_named_expression_or_higher();

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                Expr::SetComp(self.parse_set_comprehension_expression(key_or_value.expr, start))
            }
            TokenKind::Colon => {
                self.bump(TokenKind::Colon);
                let value = self.parse_conditional_expression_or_higher();

                if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                    Expr::DictComp(self.parse_dictionary_comprehension_expression(
                        key_or_value.expr,
                        value.expr,
                        start,
                    ))
                } else {
                    Expr::Dict(self.parse_dictionary_expression(
                        Some(key_or_value.expr),
                        value.expr,
                        start,
                    ))
                }
            }
            _ => Expr::Set(self.parse_set_expression(key_or_value.expr, start)),
        }
    }

    /// Parses an expression in parentheses, a tuple expression, or a generator expression.
    fn parse_parenthesized_expression(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let saved_context = self.set_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

        self.bump(TokenKind::Lpar);

        // Nice error message when having a unclosed open parenthesis `(`
        if self.at_ts(NEWLINE_EOF_SET) {
            let range = self.current_token_range();
            self.add_error(
                ParseErrorType::OtherError("missing closing parenthesis `)`".to_string()),
                range,
            );
        }

        // Return an empty `TupleExpr` when finding a `)` right after the `(`
        if self.eat(TokenKind::Rpar) {
            self.restore_ctx(ParserCtxFlags::PARENTHESIZED_EXPR, saved_context);

            return Expr::Tuple(ast::ExprTuple {
                elts: vec![],
                ctx: ExprContext::Load,
                range: self.node_range(start),
                parenthesized: true,
            })
            .into();
        }

        let mut parsed_expr = self.parse_named_expression_or_higher();

        let parsed = match self.current_token_kind() {
            TokenKind::Comma => {
                let tuple = self.parse_tuple_expression(
                    parsed_expr.expr,
                    start,
                    TupleParenthesized::Yes,
                    Parser::parse_named_expression_or_higher,
                );

                ParsedExpr {
                    expr: tuple.into(),
                    is_parenthesized: false,
                }
            }
            TokenKind::Async | TokenKind::For => {
                let generator =
                    Expr::Generator(self.parse_generator_expression(parsed_expr.expr, start, true));

                ParsedExpr {
                    expr: generator,
                    is_parenthesized: false,
                }
            }
            _ => {
                self.expect(TokenKind::Rpar);

                parsed_expr.is_parenthesized = true;
                parsed_expr
            }
        };

        self.restore_ctx(ParserCtxFlags::PARENTHESIZED_EXPR, saved_context);

        parsed
    }

    /// Parses multiple items separated by a comma into a `TupleExpr` node.
    /// Uses `parse_func` to parse each item.
    pub(super) fn parse_tuple_expression(
        &mut self,
        first_element: Expr,
        start: TextSize,
        parenthesized: TupleParenthesized,
        // TODO: I would have expected that `parse_func` is the same depending on whether `parenthesized` is true or not, but that's not the case
        // verify precedence.
        mut parse_func: impl FnMut(&mut Parser<'src>) -> ParsedExpr,
    ) -> ast::ExprTuple {
        // In case of the tuple only having one element, we need to cover the
        // range of the comma.
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_comma_separated_list(
            RecoveryContextKind::TupleElements(parenthesized),
            |p| elts.push(parse_func(p).expr),
            true,
        );

        if parenthesized.is_yes() {
            self.expect(TokenKind::Rpar);
        }

        ast::ExprTuple {
            elts,
            ctx: ExprContext::Load,
            range: self.node_range(start),
            parenthesized: parenthesized.is_yes(),
        }
    }

    /// Parses a list expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#list-displays>
    fn parse_list_expression(&mut self, first_element: Expr, start: TextSize) -> ast::ExprList {
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_comma_separated_list(
            RecoveryContextKind::ListElements,
            |parser| elts.push(parser.parse_named_expression_or_higher().expr),
            true,
        );

        self.expect(TokenKind::Rsqb);

        ast::ExprList {
            elts,
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    /// Parses a set expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#set-displays>
    fn parse_set_expression(&mut self, first_element: Expr, start: TextSize) -> ast::ExprSet {
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_comma_separated_list(
            RecoveryContextKind::SetElements,
            |parser| elts.push(parser.parse_named_expression_or_higher().expr),
            true,
        );

        self.expect(TokenKind::Rbrace);

        ast::ExprSet {
            range: self.node_range(start),
            elts,
        }
    }

    /// Parses a dictionary expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#dictionary-displays>
    fn parse_dictionary_expression(
        &mut self,
        key: Option<Expr>,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprDict {
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let mut keys = vec![key];
        let mut values = vec![value];

        self.parse_comma_separated_list(
            RecoveryContextKind::DictElements,
            |parser| {
                if parser.eat(TokenKind::DoubleStar) {
                    keys.push(None);
                } else {
                    keys.push(Some(parser.parse_conditional_expression_or_higher().expr));

                    parser.expect(TokenKind::Colon);
                }
                values.push(parser.parse_conditional_expression_or_higher().expr);
            },
            true,
        );

        self.expect(TokenKind::Rbrace);

        ast::ExprDict {
            range: self.node_range(start),
            keys,
            values,
        }
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_comprehension(&mut self) -> ast::Comprehension {
        let start = self.node_start();

        let is_async = self.eat(TokenKind::Async);

        self.bump(TokenKind::For);

        let saved_context = self.set_ctx(ParserCtxFlags::FOR_TARGET);
        let mut target = self.parse_expression();
        self.restore_ctx(ParserCtxFlags::FOR_TARGET, saved_context);

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        self.expect(TokenKind::In);

        let iter = self.parse_simple_expression();

        let mut ifs = vec![];
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::If) {
            progress.assert_progressing(self);
            ifs.push(self.parse_simple_expression().expr);
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
        const GENERATOR_SET: TokenSet = TokenSet::new([TokenKind::For, TokenKind::Async]);

        let mut generators = vec![];
        let mut progress = ParserProgress::default();

        while self.at_ts(GENERATOR_SET) {
            progress.assert_progressing(self);
            generators.push(self.parse_comprehension());
        }

        generators
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#generator-expressions>
    fn parse_generator_expression(
        &mut self,
        element: Expr,
        start: TextSize,
        in_parentheses: bool,
    ) -> ast::ExprGenerator {
        let generators = self.parse_generators();

        if in_parentheses {
            self.expect(TokenKind::Rpar);
        }

        ast::ExprGenerator {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
            parenthesized: in_parentheses,
        }
    }

    fn parse_list_comprehension_expression(
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

    fn parse_dictionary_comprehension_expression(
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

    fn parse_set_comprehension_expression(
        &mut self,
        element: Expr,
        start: TextSize,
    ) -> ast::ExprSetComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rbrace);

        ast::ExprSetComp {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
        }
    }

    fn parse_starred_expression(&mut self) -> ast::ExprStarred {
        let start = self.node_start();
        self.bump(TokenKind::Star);
        let parsed_expr = self.parse_conditional_expression_or_higher();

        ast::ExprStarred {
            value: Box::new(parsed_expr.expr),
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#await-expression>
    fn parse_await_expression(&mut self) -> ast::ExprAwait {
        let start = self.node_start();
        self.bump(TokenKind::Await);
        let parsed_expr = self.parse_expression_with_precedence(Precedence::Await);

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

    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_expression(&mut self) -> Expr {
        let start = self.node_start();
        self.bump(TokenKind::Yield);

        if self.eat(TokenKind::From) {
            return self.parse_yield_from_expression(start);
        }

        let value = self
            .at_expr()
            .then(|| Box::new(self.parse_expression().expr));

        Expr::Yield(ast::ExprYield {
            value,
            range: self.node_range(start),
        })
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_from_expression(&mut self, start: TextSize) -> Expr {
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

    /// Parses a named expression (`:=`).
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `:=` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#assignment-expressions>
    fn parse_named_expression(&mut self, mut target: Expr, start: TextSize) -> ast::ExprNamed {
        self.bump(TokenKind::ColonEqual);

        if !target.is_name_expr() {
            self.add_error(ParseErrorType::InvalidNamedAssignmentTarget, target.range());
        }
        helpers::set_expr_ctx(&mut target, ExprContext::Store);

        let value = self.parse_conditional_expression_or_higher();

        ast::ExprNamed {
            target: Box::new(target),
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/expressions.html#lambda>
    fn parse_lambda_expr(&mut self) -> ast::ExprLambda {
        let start = self.node_start();
        self.bump(TokenKind::Lambda);

        let parameters: Option<Box<ast::Parameters>> = if self.at(TokenKind::Colon) {
            None
        } else {
            Some(Box::new(self.parse_parameters(FunctionKind::Lambda)))
        };

        self.expect(TokenKind::Colon);

        // Check for forbidden tokens in the `lambda`'s body
        match self.current_token_kind() {
            TokenKind::Yield => self.add_error(
                ParseErrorType::OtherError(
                    "`yield` not allowed in a `lambda` expression".to_string(),
                ),
                self.current_token_range(),
            ),
            TokenKind::Star => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "starred expression not allowed in a `lambda` expression".to_string(),
                    ),
                    self.current_token_range(),
                );
            }
            TokenKind::DoubleStar => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "double starred expression not allowed in a `lambda` expression"
                            .to_string(),
                    ),
                    self.current_token_range(),
                );
            }
            _ => {}
        }

        let body = self.parse_conditional_expression_or_higher();

        ast::ExprLambda {
            body: Box::new(body.expr),
            parameters,
            range: self.node_range(start),
        }
    }

    fn parse_if_expression(&mut self, body: Expr, start: TextSize) -> ast::ExprIf {
        self.bump(TokenKind::If);

        let test = self.parse_simple_expression();

        self.expect(TokenKind::Else);

        let orelse = self.parse_conditional_expression_or_higher();

        ast::ExprIf {
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

/// Represents the precedence levels for various operators and expressions of Python.
/// Variants at the top have lower precedence and variants at the bottom have
/// higher precedence.
///
/// Note: Some expressions like if-else, named expression (`:=`), lambda, subscription,
/// slicing, call and attribute reference expressions, that are mentioned in the link
/// below are better handled in other parts of the parser.
///
/// See: <https://docs.python.org/3/reference/expressions.html#operator-precedence>
#[derive(Debug, Ord, Eq, PartialEq, PartialOrd, Copy, Clone)]
enum Precedence {
    /// Precedence for an unknown operator.
    Unknown,
    /// The initital precedence when parsing an expression.
    Initial,
    /// Precedence of boolean `or` operator.
    Or,
    /// Precedence of boolean `and` operator.
    And,
    /// Precedence of boolean `not` unary operator.
    Not,
    /// Precedence of comparisons operators (`<`, `<=`, `>`, `>=`, `!=`, `==`),
    /// memberships tests (`in`, `not in`) and identity tests (`is` `is not`).
    ComparisonsMembershipIdentity,
    /// Precedence of `Bitwise OR` (`|`) operator.
    BitOr,
    /// Precedence of `Bitwise XOR` (`^`) operator.
    BitXor,
    /// Precedence of `Bitwise AND` (`&`) operator.
    BitAnd,
    /// Precedence of left and right shift operators (`<<`, `>>`).
    LeftRightShift,
    /// Precedence of addition (`+`) and subtraction (`-`) operators.
    AddSub,
    /// Precedence of multiplication (`*`), matrix multiplication (`@`), division (`/`), floor
    /// division (`//`) and remainder operators (`%`).
    MulDivRemain,
    /// Precedence of positive (`+`), negative (`-`), `Bitwise NOT` (`~`) unary operators.
    PosNegBitNot,
    /// Precedence of exponentiation operator (`**`).
    Exponent,
    /// Precedence of `await` expression.
    Await,
}

impl Precedence {
    fn increment_precedence(self) -> Precedence {
        match self {
            Precedence::Or => Precedence::And,
            Precedence::And => Precedence::Not,
            Precedence::Not => Precedence::ComparisonsMembershipIdentity,
            Precedence::ComparisonsMembershipIdentity => Precedence::BitOr,
            Precedence::BitOr => Precedence::BitXor,
            Precedence::BitXor => Precedence::BitAnd,
            Precedence::BitAnd => Precedence::LeftRightShift,
            Precedence::LeftRightShift => Precedence::AddSub,
            Precedence::AddSub => Precedence::MulDivRemain,
            Precedence::MulDivRemain => Precedence::PosNegBitNot,
            Precedence::PosNegBitNot => Precedence::Exponent,
            Precedence::Exponent => Precedence::Await,
            // We've reached the highest precedence, we can't increment anymore,
            // so return the same precedence.
            Precedence::Await => Precedence::Await,
            // When this function is invoked, the precedence will never be 'Unknown' or 'Initial'.
            // This is due to their lower precedence values, causing them to exit the loop in the
            // `parse_expression_with_precedence` function before the execution of this function.
            Precedence::Unknown | Precedence::Initial => unreachable!(),
        }
    }
}

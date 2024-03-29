use std::cmp::Ordering;
use std::ops::Deref;

use ruff_python_ast::{
    self as ast, BoolOp, CmpOp, ConversionFlag, Expr, ExprContext, FStringElement, Number,
    Operator, UnaryOp,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::parser::helpers::token_kind_to_cmp_op;
use crate::parser::progress::ParserProgress;
use crate::parser::{helpers, FunctionKind, Parser, ParserCtxFlags, EXPR_SET, NEWLINE_EOF_SET};
use crate::string::{parse_fstring_literal_element, parse_string_literal, StringType};
use crate::token_set::TokenSet;
use crate::{FStringErrorType, Mode, ParseErrorType, Tok, TokenKind};

use super::{RecoveryContextKind, TupleParenthesized};

/// Tokens that can appear after an expression.
pub(super) const END_EXPR_SET: TokenSet = TokenSet::new([
    // Ex) `expr` (without a newline)
    TokenKind::EndOfFile,
    // Ex) `expr`
    TokenKind::Newline,
    // Ex) `expr;`
    TokenKind::Semi,
    // Ex) `data[expr:]`
    // Ex) `def foo() -> expr:`
    // Ex) `{expr: expr}`
    TokenKind::Colon,
    // Ex) `{expr}`
    TokenKind::Rbrace,
    // Ex) `[expr]`
    TokenKind::Rsqb,
    // Ex) `(expr)`
    TokenKind::Rpar,
    // Ex) `expr,`
    TokenKind::Comma,
    // Ex)
    //
    // if True:
    //     expr
    //     # <- Dedent
    // x
    TokenKind::Dedent,
    // Ex) `expr if expr else expr`
    TokenKind::If,
    TokenKind::Else,
    // Ex) `with expr as target:`
    // Ex) `except expr as NAME:`
    TokenKind::As,
    // Ex) `raise expr from expr`
    TokenKind::From,
    // Ex) `[expr for expr in iter]`
    TokenKind::For,
    // Ex) `[expr async for expr in iter]`
    TokenKind::Async,
    // Ex) `expr in expr`
    TokenKind::In,
    // Ex) `name: expr = expr`
    // Ex) `f"{expr=}"`
    TokenKind::Equal,
    // Ex) `f"{expr!s}"`
    TokenKind::Exclamation,
]);

/// Tokens that can appear at the end of a sequence.
const END_SEQUENCE_SET: TokenSet = END_EXPR_SET.remove(TokenKind::Comma);

impl<'src> Parser<'src> {
    /// Returns `true` if the current token is the start of an expression.
    pub(super) fn at_expr(&self) -> bool {
        self.at_ts(EXPR_SET)
    }

    // Returns `true` if the current token ends a sequence.
    pub(super) fn at_sequence_end(&self) -> bool {
        self.at_ts(END_SEQUENCE_SET)
    }

    /// Parses every Python expression.
    ///
    /// Matches the `expressions` rule in the [Python grammar].
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_expression_list(&mut self) -> ParsedExpr {
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

    /// Parses every Python expression.
    ///
    /// Matches the `star_expressions` rule in the [Python grammar].
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_star_expression_list(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_star_expression_or_higher(AllowNamedExpression::No);

        if self.at(TokenKind::Comma) {
            Expr::Tuple(self.parse_tuple_expression(
                parsed_expr.expr,
                start,
                TupleParenthesized::No,
                |parser| parser.parse_star_expression_or_higher(AllowNamedExpression::No),
            ))
            .into()
        } else {
            parsed_expr
        }
    }

    /// Parses a star expression or any other expression.
    ///
    /// Matches either the `star_expression` or `star_named_expression` rule in
    /// the [Python grammar] depending on whether named expressions are allowed.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_star_expression_or_higher(
        &mut self,
        allow_named_expression: AllowNamedExpression,
    ) -> ParsedExpr {
        if self.at(TokenKind::Star) {
            Expr::Starred(self.parse_starred_expression(StarredExpressionPrecedence::BitOr)).into()
        } else if allow_named_expression.is_yes() {
            self.parse_named_expression_or_higher()
        } else {
            self.parse_conditional_expression_or_higher()
        }
    }

    /// Parses every Python expression except unparenthesized tuple.
    ///
    /// Matches the `named_expression` rule in the [Python grammar].
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
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
    ///
    /// Matches the `expression` rule in the [Python grammar].
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_conditional_expression_or_higher(&mut self) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_simple_expression();

        if self.at(TokenKind::If) {
            Expr::If(self.parse_if_expression(parsed_expr.expr, start)).into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuples, named expressions,
    /// and `if` expression.
    ///
    /// This is a combination of the `disjunction`, `starred_expression`, `yield_expr`
    /// and `lambdef` rules of the [Python grammar].
    ///
    /// When parsing an AST node that only uses some of the above mentioned rules,
    /// you need to **explicitly** check if an invalid node for that AST node was parsed.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
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
            TokenKind::Not if self.peek() == TokenKind::In => (
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
            TokenKind::Star => Expr::Starred(
                self.parse_starred_expression(StarredExpressionPrecedence::Conditional),
            )
            .into(),
            TokenKind::Await => Expr::Await(self.parse_await_expression()).into(),
            TokenKind::Lambda => Expr::Lambda(self.parse_lambda_expr()).into(),
            _ => self.parse_atom(),
        };

        if self.is_current_token_postfix() {
            lhs = self.parse_postfix_expression(lhs.expr, start).into();
        }

        lhs
    }

    /// Parses a bitwise `or` expression or higher.
    ///
    /// Matches the `bitwise_or` rule in the [Python grammar].
    ///
    /// This method delegates the parsing to [`Parser::parse_expression_with_precedence`]
    /// with the minimum binding power of [`Precedence::BitOr`]. The main purpose of this
    /// method is to report an error when certain expressions are parsed successfully
    /// but are not allowed here.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_expression_with_bitwise_or_precedence(&mut self) -> ParsedExpr {
        let parsed_expr = self.parse_expression_with_precedence(Precedence::BitOr);

        // Parenthesized expressions have a higher precedence than bitwise `or` expressions,
        // so they're allowed here.
        if parsed_expr.is_parenthesized {
            return parsed_expr;
        }

        match parsed_expr.expr {
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                ..
            }) => {
                self.add_error(
                    ParseErrorType::OtherError("unary `not` expression cannot be used here".into()),
                    &parsed_expr,
                );
            }
            Expr::Lambda(_) => {
                self.add_error(
                    ParseErrorType::OtherError("lambda expression cannot be used here".into()),
                    &parsed_expr,
                );
            }
            _ => {}
        }

        parsed_expr
    }

    /// Parses a name.
    ///
    /// For an invalid name, the `id` field will be an empty string and the `ctx`
    /// field will be [`ExprContext::Invalid`].
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#atom-identifiers>
    pub(super) fn parse_name(&mut self) -> ast::ExprName {
        let identifier = self.parse_identifier();

        let ctx = if identifier.is_valid() {
            ExprContext::Load
        } else {
            ExprContext::Invalid
        };

        ast::ExprName {
            range: identifier.range,
            id: identifier.id,
            ctx,
        }
    }

    /// Parses an identifier.
    ///
    /// For an invalid identifier, the `id` field will be an empty string.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#atom-identifiers>
    pub(super) fn parse_identifier(&mut self) -> ast::Identifier {
        let range = self.current_token_range();

        if self.at(TokenKind::Name) {
            let (Tok::Name { name }, _) = self.bump(TokenKind::Name) else {
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
                        "Expected an identifier, but found a keyword '{tok}' that cannot be used here"
                    )),
                    range,
                );

                ast::Identifier {
                    id: tok.to_string(),
                    range,
                }
            } else {
                self.add_error(
                    ParseErrorType::OtherError("Expected an identifier".into()),
                    range,
                );
                ast::Identifier {
                    id: String::new(),
                    range: self.missing_node_range(),
                }
            }
        }
    }

    /// Parses an atom.
    ///
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
            TokenKind::String | TokenKind::FStringStart => self.parse_strings(),
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
                        ParseErrorType::OtherError("Expected an expression".to_string()),
                        self.current_token_range(),
                    );
                    Expr::Name(ast::ExprName {
                        range: self.missing_node_range(),
                        id: String::new(),
                        ctx: ExprContext::Invalid,
                    })
                }
            }
        };

        lhs.into()
    }

    pub(super) fn parse_postfix_expression(&mut self, mut lhs: Expr, start: TextSize) -> Expr {
        loop {
            lhs = match self.current_token_kind() {
                TokenKind::Lpar => Expr::Call(self.parse_call_expression(lhs, start)),
                TokenKind::Lsqb => Expr::Subscript(self.parse_subscript_expression(lhs, start)),
                TokenKind::Dot => Expr::Attribute(self.parse_attribute_expression(lhs, start)),
                _ => break lhs,
            };
        }
    }

    /// Parse a call expression.
    ///
    /// The function name is parsed by the caller and passed as `func` along with
    /// the `start` position of the call expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't position at a `(` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#calls>
    fn parse_call_expression(&mut self, func: Expr, start: TextSize) -> ast::ExprCall {
        let arguments = self.parse_arguments();

        ast::ExprCall {
            func: Box::new(func),
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

        self.parse_comma_separated_list(RecoveryContextKind::Arguments, |parser| {
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
                            GeneratorExpressionInParentheses::No(start),
                        ))
                        .into();
                    }
                    _ => {}
                }

                if has_seen_kw_unpack && parsed_expr.expr.is_starred_expr() {
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
                        parser.add_error(
                            ParseErrorType::OtherError("Expected a parameter name".to_string()),
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
        });

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

        // Slice range doesn't include the `[` token.
        let slice_start = self.node_start();

        // Create an error when receiving an empty slice to parse, e.g. `x[]`
        if self.eat(TokenKind::Rsqb) {
            let slice_range = self.node_range(slice_start);
            self.add_error(ParseErrorType::EmptySlice, slice_range);

            return ast::ExprSubscript {
                value: Box::new(value),
                slice: Box::new(Expr::Name(ast::ExprName {
                    range: slice_range,
                    id: String::new(),
                    ctx: ExprContext::Invalid,
                })),
                ctx: ExprContext::Load,
                range: self.node_range(start),
            };
        }

        let mut slice = self.parse_slice();

        // If there are more than one element in the slice, we need to create a tuple
        // expression to represent it.
        if self.eat(TokenKind::Comma) {
            let mut slices = vec![slice];

            self.parse_comma_separated_list(RecoveryContextKind::Slices, |parser| {
                slices.push(parser.parse_slice());
            });

            slice = Expr::Tuple(ast::ExprTuple {
                elts: slices,
                ctx: ExprContext::Load,
                range: self.node_range(slice_start),
                parenthesized: false,
            });
        } else if slice.is_starred_expr() {
            // If the only slice element is a starred expression, that is represented
            // using a tuple expression with a single element. This is the second case
            // in the `slices` rule in the Python grammar.
            slice = Expr::Tuple(ast::ExprTuple {
                elts: vec![slice],
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

    /// Parses a slice expression.
    ///
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

            if !lower.is_parenthesized {
                match lower.expr {
                    Expr::Starred(_) => {
                        self.add_error(ParseErrorType::StarredExpressionUsage, &lower);
                    }
                    Expr::Named(_) => {
                        self.add_error(ParseErrorType::UnparenthesizedNamedExpression, &lower);
                    }
                    _ => {}
                }
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
            let upper = self.parse_conditional_expression_or_higher();
            if upper.is_unparenthesized_starred_expr() {
                self.add_error(ParseErrorType::StarredExpressionUsage, &upper);
            }
            Some(Box::new(upper.expr))
        };

        let step = if self.eat(TokenKind::Colon) {
            if self.at_ts(STEP_END_SET) {
                None
            } else {
                let step = self.parse_conditional_expression_or_higher();
                if step.is_unparenthesized_starred_expr() {
                    self.add_error(ParseErrorType::StarredExpressionUsage, &step);
                }
                Some(Box::new(step.expr))
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

    pub(super) fn parse_unary_expression(&mut self) -> ast::ExprUnaryOp {
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

            if let Ok(op) = token_kind_to_cmp_op([self.current_token_kind(), self.peek()]) {
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

    /// Parses all kinds of strings and implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `String` or `FStringStart` token.
    ///
    /// See: <https://docs.python.org/3/reference/grammar.html> (Search "strings:")
    pub(super) fn parse_strings(&mut self) -> Expr {
        let start = self.node_start();
        let mut strings = vec![];

        self.parse_list(RecoveryContextKind::Strings, |parser| {
            let string = match parser.current_token_kind() {
                TokenKind::String => parser.parse_string(),
                TokenKind::FStringStart => StringType::FString(parser.parse_fstring()),
                _ => unreachable!("Expected `String` or `FStringStart` token"),
            };
            strings.push(string);
        });

        let range = self.node_range(start);

        match strings.len() {
            // This is not possible as the function was called by matching against a
            // `String` or `FStringStart` token.
            0 => unreachable!("Expected to parse at least one string"),
            // We need a owned value, hence the `pop` here.
            1 => match strings.pop().unwrap() {
                StringType::Str(string) => Expr::StringLiteral(ast::ExprStringLiteral {
                    value: ast::StringLiteralValue::single(string),
                    range,
                }),
                StringType::Bytes(bytes) => Expr::BytesLiteral(ast::ExprBytesLiteral {
                    value: ast::BytesLiteralValue::single(bytes),
                    range,
                }),
                StringType::FString(fstring) => Expr::FString(ast::ExprFString {
                    value: ast::FStringValue::single(fstring),
                    range,
                }),
            },
            _ => self.handle_implicitly_concatenated_strings(strings, range),
        }
    }

    /// Handles implicitly concatenated strings.
    fn handle_implicitly_concatenated_strings(
        &mut self,
        strings: Vec<StringType>,
        range: TextRange,
    ) -> Expr {
        debug_assert!(strings.len() > 1);

        let mut has_fstring = false;
        let mut byte_literal_count = 0;
        for string in &strings {
            match string {
                StringType::FString(_) => has_fstring = true,
                StringType::Bytes(_) => byte_literal_count += 1,
                StringType::Str(_) => {}
            }
        }
        let has_bytes = byte_literal_count > 0;

        if has_bytes {
            match byte_literal_count.cmp(&strings.len()) {
                Ordering::Less => {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "cannot mix bytes and non-bytes literals".to_string(),
                        ),
                        range,
                    );
                }
                // Only construct a byte expression if all the literals are bytes
                // otherwise, we'll try either string or f-string. This is to retain
                // as much information as possible.
                Ordering::Equal => {
                    let mut values = Vec::with_capacity(strings.len());
                    for string in strings {
                        values.push(match string {
                            StringType::Bytes(value) => value,
                            _ => ast::BytesLiteral::invalid(string.range()),
                        });
                    }
                    return Expr::from(ast::ExprBytesLiteral {
                        value: ast::BytesLiteralValue::concatenated(values),
                        range,
                    });
                }
                Ordering::Greater => unreachable!(),
            }
        }

        if !has_fstring {
            let mut values = Vec::with_capacity(strings.len());
            for string in strings {
                values.push(match string {
                    StringType::Str(value) => value,
                    _ => ast::StringLiteral::invalid(string.range()),
                });
            }
            return Expr::from(ast::ExprStringLiteral {
                value: ast::StringLiteralValue::concatenated(values),
                range,
            });
        }

        let mut parts = Vec::with_capacity(strings.len());
        for string in strings {
            match string {
                StringType::FString(fstring) => parts.push(ast::FStringPart::FString(fstring)),
                StringType::Str(string) => parts.push(ast::FStringPart::Literal(string)),
                StringType::Bytes(bytes) => parts.push(ast::FStringPart::Literal(
                    ast::StringLiteral::invalid(bytes.range()),
                )),
            }
        }

        Expr::from(ast::ExprFString {
            value: ast::FStringValue::concatenated(parts),
            range,
        })
    }

    /// Parses a single string literal.
    ///
    /// This does not handle implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `String` token.
    fn parse_string(&mut self) -> StringType {
        let (Tok::String { value, kind }, range) = self.bump(TokenKind::String) else {
            unreachable!()
        };

        match parse_string_literal(value, kind, range) {
            Ok(string) => string,
            Err(error) => {
                let location = error.location();
                self.add_error(ParseErrorType::Lexical(error.into_error()), location);

                if kind.is_byte_string() {
                    StringType::Bytes(ast::BytesLiteral {
                        value: Box::new([]),
                        range,
                        flags: ast::BytesLiteralFlags::from(kind).with_invalid(),
                    })
                } else {
                    StringType::Str(ast::StringLiteral {
                        value: "".into(),
                        range,
                        flags: ast::StringLiteralFlags::from(kind).with_invalid(),
                    })
                }
            }
        }
    }

    /// Parses a f-string.
    ///
    /// This does not handle implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `FStringStart` token.
    ///
    /// See: <https://docs.python.org/3/reference/grammar.html> (Search "fstring:")
    fn parse_fstring(&mut self) -> ast::FString {
        let start = self.node_start();

        let (Tok::FStringStart(kind), _) = self.bump(TokenKind::FStringStart) else {
            unreachable!()
        };
        let elements = self.parse_fstring_elements();

        self.expect(TokenKind::FStringEnd);

        ast::FString {
            elements,
            range: self.node_range(start),
            flags: kind.into(),
        }
    }

    /// Parses a list of f-string elements.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` or `FStringMiddle` token.
    fn parse_fstring_elements(&mut self) -> Vec<FStringElement> {
        let mut elements = vec![];

        self.parse_list(RecoveryContextKind::FStringElements, |parser| {
            let element = match parser.current_token_kind() {
                TokenKind::Lbrace => {
                    FStringElement::Expression(parser.parse_fstring_expression_element())
                }
                TokenKind::FStringMiddle => {
                    let (Tok::FStringMiddle { value, kind, .. }, range) = parser.next_token()
                    else {
                        unreachable!()
                    };
                    FStringElement::Literal(
                        parse_fstring_literal_element(value, kind, range).unwrap_or_else(
                            |lex_error| {
                                let location = lex_error.location();
                                parser.add_error(
                                    ParseErrorType::Lexical(lex_error.into_error()),
                                    location,
                                );
                                ast::FStringLiteralElement {
                                    value: "".into(),
                                    range,
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

        let value = self.parse_expression_list();
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
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#list-displays>
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

        // Parse the first element with a more general rule and limit it later.
        let first_element = self.parse_star_expression_or_higher(AllowNamedExpression::Yes);

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                // Parenthesized starred expression isn't allowed either but that is
                // handled by the `parse_parenthesized_expression` method.
                if !first_element.is_parenthesized && first_element.is_starred_expr() {
                    self.add_error(
                        ParseErrorType::IterableUnpackingInComprehension,
                        &first_element,
                    );
                }

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
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/expressions.html#set-displays>
    /// - <https://docs.python.org/3/reference/expressions.html#dictionary-displays>
    /// - <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
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
            // Handle dictionary unpacking
            let value = self.parse_expression_with_bitwise_or_precedence();
            return Expr::Dict(self.parse_dictionary_expression(None, value.expr, start));
        }

        // For dictionary expressions, the key uses the `expression` rule while for
        // set expressions, the element uses the `star_expression` rule. So, use the
        // one that is more general and limit it later.
        let key_or_element = self.parse_star_expression_or_higher(AllowNamedExpression::Yes);

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                if !key_or_element.is_parenthesized && key_or_element.expr.is_starred_expr() {
                    self.add_error(
                        ParseErrorType::IterableUnpackingInComprehension,
                        &key_or_element,
                    );
                }

                Expr::SetComp(self.parse_set_comprehension_expression(key_or_element.expr, start))
            }
            TokenKind::Colon => {
                // Now, we know that it's either a dictionary expression or a dictionary comprehension.
                // In either case, the key is limited to an `expression`.
                if !key_or_element.is_parenthesized {
                    match key_or_element.expr {
                        Expr::Starred(_) => self.add_error(
                            ParseErrorType::StarredExpressionUsage,
                            &key_or_element.expr,
                        ),
                        Expr::Named(_) => self.add_error(
                            ParseErrorType::UnparenthesizedNamedExpression,
                            &key_or_element,
                        ),
                        _ => {}
                    }
                }

                self.bump(TokenKind::Colon);
                let value = self.parse_conditional_expression_or_higher();

                if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                    Expr::DictComp(self.parse_dictionary_comprehension_expression(
                        key_or_element.expr,
                        value.expr,
                        start,
                    ))
                } else {
                    Expr::Dict(self.parse_dictionary_expression(
                        Some(key_or_element.expr),
                        value.expr,
                        start,
                    ))
                }
            }
            _ => Expr::Set(self.parse_set_expression(key_or_element.expr, start)),
        }
    }

    /// Parses an expression in parentheses, a tuple expression, or a generator expression.
    ///
    /// Matches the `(tuple | group | genexp)` rule in the [Python grammar].
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_parenthesized_expression(&mut self) -> ParsedExpr {
        let start = self.node_start();
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
            return Expr::Tuple(ast::ExprTuple {
                elts: vec![],
                ctx: ExprContext::Load,
                range: self.node_range(start),
                parenthesized: true,
            })
            .into();
        }

        // Use the more general rule of the three to parse the first element
        // and limit it later.
        let mut parsed_expr = self.parse_star_expression_or_higher(AllowNamedExpression::Yes);

        match self.current_token_kind() {
            TokenKind::Comma => {
                // grammar: `tuple`
                let tuple = self.parse_tuple_expression(
                    parsed_expr.expr,
                    start,
                    TupleParenthesized::Yes,
                    |parser| parser.parse_star_expression_or_higher(AllowNamedExpression::Yes),
                );

                ParsedExpr {
                    expr: tuple.into(),
                    is_parenthesized: false,
                }
            }
            TokenKind::Async | TokenKind::For => {
                // grammar: `genexp`
                if !parsed_expr.is_parenthesized && parsed_expr.expr.is_starred_expr() {
                    self.add_error(
                        ParseErrorType::IterableUnpackingInComprehension,
                        &parsed_expr,
                    );
                }

                let generator = Expr::Generator(self.parse_generator_expression(
                    parsed_expr.expr,
                    GeneratorExpressionInParentheses::Yes(start),
                ));

                ParsedExpr {
                    expr: generator,
                    is_parenthesized: false,
                }
            }
            _ => {
                // grammar: `group`
                if parsed_expr.expr.is_starred_expr() {
                    self.add_error(ParseErrorType::StarredExpressionUsage, &parsed_expr);
                }

                self.expect(TokenKind::Rpar);

                parsed_expr.is_parenthesized = true;
                parsed_expr
            }
        }
    }

    /// Parses multiple items separated by a comma into a tuple expression.
    ///
    /// Uses the `parse_func` to parse each item in the tuple.
    pub(super) fn parse_tuple_expression(
        &mut self,
        first_element: Expr,
        start: TextSize,
        parenthesized: TupleParenthesized,
        mut parse_func: impl FnMut(&mut Parser<'src>) -> ParsedExpr,
    ) -> ast::ExprTuple {
        // TODO(dhruvmanila): Can we remove `parse_func` and use `parenthesized` to
        // determine the parsing function?

        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let mut elts = vec![first_element];

        self.parse_comma_separated_list(RecoveryContextKind::TupleElements(parenthesized), |p| {
            elts.push(parse_func(p).expr);
        });

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

        self.parse_comma_separated_list(RecoveryContextKind::ListElements, |parser| {
            elts.push(
                parser
                    .parse_star_expression_or_higher(AllowNamedExpression::Yes)
                    .expr,
            );
        });

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

        self.parse_comma_separated_list(RecoveryContextKind::SetElements, |parser| {
            elts.push(
                parser
                    .parse_star_expression_or_higher(AllowNamedExpression::Yes)
                    .expr,
            );
        });

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

        self.parse_comma_separated_list(RecoveryContextKind::DictElements, |parser| {
            if parser.eat(TokenKind::DoubleStar) {
                keys.push(None);
                values.push(parser.parse_expression_with_bitwise_or_precedence().expr);
            } else {
                let key = parser.parse_conditional_expression_or_higher();
                if !key.is_parenthesized && key.expr.is_starred_expr() {
                    parser.add_error(ParseErrorType::StarredExpressionUsage, &key);
                }

                keys.push(Some(key.expr));
                parser.expect(TokenKind::Colon);

                let value = parser.parse_conditional_expression_or_higher();
                if !value.is_parenthesized && value.expr.is_starred_expr() {
                    parser.add_error(ParseErrorType::StarredExpressionUsage, &value);
                }

                values.push(value.expr);
            }
        });

        self.expect(TokenKind::Rbrace);

        ast::ExprDict {
            range: self.node_range(start),
            keys,
            values,
        }
    }

    /// Parses a list of comprehension generators.
    ///
    /// These are the `for` and `async for` clauses in a comprehension, optionally
    /// followed by `if` clauses.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#grammar-token-python-grammar-comp_for>
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

    /// Parses a comprehension.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `async` or `for` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_comprehension(&mut self) -> ast::Comprehension {
        let start = self.node_start();

        let is_async = self.eat(TokenKind::Async);
        self.bump(TokenKind::For);

        let saved_context = self.set_ctx(ParserCtxFlags::FOR_TARGET);
        let mut target = self.parse_expression_list();
        self.restore_ctx(ParserCtxFlags::FOR_TARGET, saved_context);

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);
        if !helpers::is_valid_assignment_target(&target.expr) {
            self.add_error(ParseErrorType::InvalidAssignmentTarget, &target);
        }

        self.expect(TokenKind::In);
        let iter = self.parse_simple_expression();
        self.validate_comprehension_iter_and_if_expr(&iter);

        let mut ifs = vec![];
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::If) {
            progress.assert_progressing(self);

            let parsed_expr = self.parse_simple_expression();
            self.validate_comprehension_iter_and_if_expr(&parsed_expr);

            ifs.push(parsed_expr.expr);
        }

        ast::Comprehension {
            range: self.node_range(start),
            target: target.expr,
            iter: iter.expr,
            ifs,
            is_async,
        }
    }

    /// Helper function to validate the comprehension iter and `if` expressions.
    fn validate_comprehension_iter_and_if_expr(&mut self, parsed_expr: &ParsedExpr) {
        let ParsedExpr {
            expr,
            is_parenthesized,
        } = parsed_expr;

        if *is_parenthesized {
            return;
        }

        match expr {
            Expr::Starred(_) => {
                self.add_error(ParseErrorType::StarredExpressionUsage, expr);
            }
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized yield expression cannot be used here".to_string(),
                    ),
                    expr,
                );
            }
            Expr::Lambda(_) => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized lambda expression cannot be used here".to_string(),
                    ),
                    expr,
                );
            }
            _ => {}
        }
    }

    /// Parses a generator expression.
    ///
    /// The given `in_parentheses` parameter is used to determine whether the generator
    /// expression is enclosed in parentheses or not:
    /// - `Yes`, expect the `)` token after the generator expression.
    /// - `No`, no parentheses are expected.
    /// - `Maybe`, consume the `)` token if it's present.
    ///
    /// The contained start position in each variant is used to determine the range
    /// of the generator expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#generator-expressions>
    pub(super) fn parse_generator_expression(
        &mut self,
        element: Expr,
        in_parentheses: GeneratorExpressionInParentheses,
    ) -> ast::ExprGenerator {
        let generators = self.parse_generators();

        let (parenthesized, start) = match in_parentheses {
            GeneratorExpressionInParentheses::Yes(lpar_start) => {
                self.expect(TokenKind::Rpar);
                (true, lpar_start)
            }
            GeneratorExpressionInParentheses::No(expr_start) => (false, expr_start),
            GeneratorExpressionInParentheses::Maybe {
                lpar_start,
                expr_start,
            } => {
                if self.eat(TokenKind::Rpar) {
                    (true, lpar_start)
                } else {
                    (false, expr_start)
                }
            }
        };

        ast::ExprGenerator {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
            parenthesized,
        }
    }

    /// Parses a list comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
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

    /// Parses a dictionary comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
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

    /// Parses a set comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
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

    /// Parses a starred expression with the given precedence.
    ///
    /// If the precedence is bitwise or, the expression is parsed using the `bitwise_or`
    /// rule. Otherwise, it's parsed using the `expression` rule. Refer to the
    /// [Python grammar] for more information.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `*` token.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    #[allow(dead_code)]
    fn parse_starred_expression(
        &mut self,
        precedence: StarredExpressionPrecedence,
    ) -> ast::ExprStarred {
        let start = self.node_start();
        self.bump(TokenKind::Star);

        let parsed_expr = match precedence {
            StarredExpressionPrecedence::BitOr => {
                self.parse_expression_with_bitwise_or_precedence()
            }
            StarredExpressionPrecedence::Conditional => {
                self.parse_conditional_expression_or_higher()
            }
        };

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
            .then(|| Box::new(self.parse_expression_list().expr));

        Expr::Yield(ast::ExprYield {
            value,
            range: self.node_range(start),
        })
    }

    /// Parses a `yield from` expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_from_expression(&mut self, start: TextSize) -> Expr {
        // The grammar rule to use here is `expression` but we're using `expressions`
        // for better error recovery.
        let parsed_expr = self.parse_expression_list();

        match &parsed_expr.expr {
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "starred expression is not allowed in a `yield from` statement".to_string(),
                    ),
                    value.as_ref(),
                );
            }
            Expr::Tuple(tuple) if !tuple.parenthesized => {
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
    pub(super) fn parse_named_expression(
        &mut self,
        mut target: Expr,
        start: TextSize,
    ) -> ast::ExprNamed {
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

impl ParsedExpr {
    #[inline]
    const fn is_unparenthesized_starred_expr(&self) -> bool {
        !self.is_parenthesized && self.expr.is_starred_expr()
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GeneratorExpressionInParentheses {
    /// The generator expression is in parentheses. The given [`TextSize`] is the
    /// start of the left parenthesis. E.g., `(x for x in range(10))`.
    Yes(TextSize),

    /// The generator expression is not in parentheses. The given [`TextSize`] is the
    /// start of the expression. E.g., `x for x in range(10)`.
    No(TextSize),

    /// The generator expression may or may not be in parentheses. The given [`TextSize`]s
    /// are the start of the left parenthesis and the start of the expression, respectively.
    Maybe {
        /// The start of the left parenthesis.
        lpar_start: TextSize,
        /// The start of the expression.
        expr_start: TextSize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StarredExpressionPrecedence {
    BitOr,
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AllowNamedExpression {
    Yes,
    No,
}

impl AllowNamedExpression {
    const fn is_yes(self) -> bool {
        matches!(self, AllowNamedExpression::Yes)
    }
}

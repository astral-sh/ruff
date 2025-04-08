use std::cmp::Ordering;
use std::ops::Deref;

use bitflags::bitflags;
use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_python_ast::name::Name;
use ruff_python_ast::{
    self as ast, BoolOp, CmpOp, ConversionFlag, Expr, ExprContext, FStringElement, FStringElements,
    IpyEscapeKind, Number, Operator, OperatorPrecedence, StringFlags, UnaryOp,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::error::{FStringKind, StarTupleKind, UnparenthesizedNamedExprKind};
use crate::parser::progress::ParserProgress;
use crate::parser::{helpers, FunctionKind, Parser};
use crate::string::{parse_fstring_literal_element, parse_string_literal, StringType};
use crate::token::{TokenKind, TokenValue};
use crate::token_set::TokenSet;
use crate::{
    FStringErrorType, Mode, ParseErrorType, UnsupportedSyntaxError, UnsupportedSyntaxErrorKind,
};

use super::{FStringElementsKind, Parenthesized, RecoveryContextKind};

/// A token set consisting of a newline or end of file.
const NEWLINE_EOF_SET: TokenSet = TokenSet::new([TokenKind::Newline, TokenKind::EndOfFile]);

/// Tokens that represents a literal expression.
const LITERAL_SET: TokenSet = TokenSet::new([
    TokenKind::Int,
    TokenKind::Float,
    TokenKind::Complex,
    TokenKind::String,
    TokenKind::Ellipsis,
    TokenKind::True,
    TokenKind::False,
    TokenKind::None,
]);

/// Tokens that represents either an expression or the start of one.
pub(super) const EXPR_SET: TokenSet = TokenSet::new([
    TokenKind::Name,
    TokenKind::Minus,
    TokenKind::Plus,
    TokenKind::Tilde,
    TokenKind::Star,
    TokenKind::DoubleStar,
    TokenKind::Lpar,
    TokenKind::Lbrace,
    TokenKind::Lsqb,
    TokenKind::Lambda,
    TokenKind::Await,
    TokenKind::Not,
    TokenKind::Yield,
    TokenKind::FStringStart,
    TokenKind::IpyEscapeCommand,
])
.union(LITERAL_SET);

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
    /// Returns `true` if the parser is at a name or keyword (including soft keyword) token.
    pub(super) fn at_name_or_keyword(&self) -> bool {
        self.at(TokenKind::Name) || self.current_token_kind().is_keyword()
    }

    /// Returns `true` if the parser is at a name or soft keyword token.
    pub(super) fn at_name_or_soft_keyword(&self) -> bool {
        self.at(TokenKind::Name) || self.at_soft_keyword()
    }

    /// Returns `true` if the parser is at a soft keyword token.
    pub(super) fn at_soft_keyword(&self) -> bool {
        self.current_token_kind().is_soft_keyword()
    }

    /// Returns `true` if the current token is the start of an expression.
    pub(super) fn at_expr(&self) -> bool {
        self.at_ts(EXPR_SET) || self.at_soft_keyword()
    }

    /// Returns `true` if the current token ends a sequence.
    pub(super) fn at_sequence_end(&self) -> bool {
        self.at_ts(END_SEQUENCE_SET)
    }

    /// Parses every Python expression.
    ///
    /// Matches the `expressions` rule in the [Python grammar]. The [`ExpressionContext`] can be
    /// used to match the `star_expressions` rule.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_expression_list(&mut self, context: ExpressionContext) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_conditional_expression_or_higher_impl(context);

        if self.at(TokenKind::Comma) {
            Expr::Tuple(self.parse_tuple_expression(
                parsed_expr.expr,
                start,
                Parenthesized::No,
                |p| p.parse_conditional_expression_or_higher_impl(context),
            ))
            .into()
        } else {
            parsed_expr
        }
    }

    /// Parses every Python expression except unparenthesized tuple.
    ///
    /// Matches the `named_expression` rule in the [Python grammar]. The [`ExpressionContext`] can
    /// be used to match the `star_named_expression` rule.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually
    /// instead of as a tuple, as done by [`Parser::parse_expression_list`], use this function.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_named_expression_or_higher(
        &mut self,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let start = self.node_start();
        let parsed_expr = self.parse_conditional_expression_or_higher_impl(context);

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
    /// This uses the default [`ExpressionContext`]. Use
    /// [`Parser::parse_conditional_expression_or_higher_impl`] if you prefer to pass in the
    /// context.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually
    /// instead of as a tuple, as done by [`Parser::parse_expression_list`] use this function.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    pub(super) fn parse_conditional_expression_or_higher(&mut self) -> ParsedExpr {
        self.parse_conditional_expression_or_higher_impl(ExpressionContext::default())
    }

    pub(super) fn parse_conditional_expression_or_higher_impl(
        &mut self,
        context: ExpressionContext,
    ) -> ParsedExpr {
        if self.at(TokenKind::Lambda) {
            Expr::Lambda(self.parse_lambda_expr()).into()
        } else {
            let start = self.node_start();
            let parsed_expr = self.parse_simple_expression(context);

            if self.at(TokenKind::If) {
                Expr::If(self.parse_if_expression(parsed_expr.expr, start)).into()
            } else {
                parsed_expr
            }
        }
    }

    /// Parses every Python expression except unparenthesized tuples, named expressions,
    /// and `if` expression.
    ///
    /// This is a combination of the `disjunction`, `starred_expression`, `yield_expr`
    /// and `lambdef` rules of the [Python grammar].
    ///
    /// Note that this function parses lambda expression but reports an error as they're not
    /// allowed in this context. This is done for better error recovery.
    /// Use [`Parser::parse_conditional_expression_or_higher`] or any methods which calls into the
    /// specified method to allow parsing lambda expression.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_simple_expression(&mut self, context: ExpressionContext) -> ParsedExpr {
        self.parse_binary_expression_or_higher(OperatorPrecedence::None, context)
    }

    /// Parses a binary expression using the [Pratt parsing algorithm].
    ///
    /// [Pratt parsing algorithm]: https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
    fn parse_binary_expression_or_higher(
        &mut self,
        left_precedence: OperatorPrecedence,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let start = self.node_start();
        let lhs = self.parse_lhs_expression(left_precedence, context);
        self.parse_binary_expression_or_higher_recursive(lhs, left_precedence, context, start)
    }

    pub(super) fn parse_binary_expression_or_higher_recursive(
        &mut self,
        mut left: ParsedExpr,
        left_precedence: OperatorPrecedence,
        context: ExpressionContext,
        start: TextSize,
    ) -> ParsedExpr {
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            let current_token = self.current_token_kind();

            if matches!(current_token, TokenKind::In) && context.is_in_excluded() {
                // Omit the `in` keyword when parsing the target expression in a comprehension or
                // a `for` statement.
                break;
            }

            let Some(operator) = BinaryLikeOperator::try_from_tokens(current_token, self.peek())
            else {
                // Not an operator.
                break;
            };

            let new_precedence = operator.precedence();

            let stop_at_current_operator = if new_precedence.is_right_associative() {
                new_precedence < left_precedence
            } else {
                new_precedence <= left_precedence
            };

            if stop_at_current_operator {
                break;
            }

            left.expr = match operator {
                BinaryLikeOperator::Boolean(bool_op) => {
                    Expr::BoolOp(self.parse_boolean_expression(left.expr, start, bool_op, context))
                }
                BinaryLikeOperator::Comparison(cmp_op) => Expr::Compare(
                    self.parse_comparison_expression(left.expr, start, cmp_op, context),
                ),
                BinaryLikeOperator::Binary(bin_op) => {
                    self.bump(TokenKind::from(bin_op));

                    let right = self.parse_binary_expression_or_higher(new_precedence, context);

                    Expr::BinOp(ast::ExprBinOp {
                        left: Box::new(left.expr),
                        op: bin_op,
                        right: Box::new(right.expr),
                        range: self.node_range(start),
                    })
                }
            };
        }

        left
    }

    /// Parses the left-hand side of an expression.
    ///
    /// This includes prefix expressions such as unary operators, boolean `not`,
    /// `await`, `lambda`. It also parses atoms and postfix expressions.
    ///
    /// The given [`OperatorPrecedence`] is used to determine if the parsed expression
    /// is valid in that context. For example, a unary operator is not valid
    /// in an `await` expression in which case the `left_precedence` would
    /// be [`OperatorPrecedence::Await`].
    fn parse_lhs_expression(
        &mut self,
        left_precedence: OperatorPrecedence,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let start = self.node_start();
        let token = self.current_token_kind();

        if let Some(unary_op) = token.as_unary_operator() {
            let expr = self.parse_unary_expression(unary_op, context);

            if matches!(unary_op, UnaryOp::Not) {
                if left_precedence > OperatorPrecedence::Not {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "Boolean 'not' expression cannot be used here".to_string(),
                        ),
                        &expr,
                    );
                }
            } else {
                if left_precedence > OperatorPrecedence::PosNegBitNot
                    // > The power operator `**` binds less tightly than an arithmetic
                    // > or bitwise unary operator on its right, that is, 2**-1 is 0.5.
                    //
                    // Reference: https://docs.python.org/3/reference/expressions.html#id21
                    && left_precedence != OperatorPrecedence::Exponent
                {
                    self.add_error(
                        ParseErrorType::OtherError(format!(
                            "Unary '{unary_op}' expression cannot be used here",
                        )),
                        &expr,
                    );
                }
            }

            return Expr::UnaryOp(expr).into();
        }

        match self.current_token_kind() {
            TokenKind::Star => {
                let starred_expr = self.parse_starred_expression(context);

                if left_precedence > OperatorPrecedence::None
                    || !context.is_starred_expression_allowed()
                {
                    self.add_error(ParseErrorType::InvalidStarredExpressionUsage, &starred_expr);
                }

                return Expr::Starred(starred_expr).into();
            }
            TokenKind::Await => {
                let await_expr = self.parse_await_expression();

                // `await` expressions cannot be nested
                if left_precedence >= OperatorPrecedence::Await {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "Await expression cannot be used here".to_string(),
                        ),
                        &await_expr,
                    );
                }

                return Expr::Await(await_expr).into();
            }
            TokenKind::Lambda => {
                // Lambda expression isn't allowed in this context but we'll still parse it and
                // report an error for better recovery.
                let lambda_expr = self.parse_lambda_expr();
                self.add_error(ParseErrorType::InvalidLambdaExpressionUsage, &lambda_expr);
                return Expr::Lambda(lambda_expr).into();
            }
            TokenKind::Yield => {
                let expr = self.parse_yield_expression();

                if left_precedence > OperatorPrecedence::None
                    || !context.is_yield_expression_allowed()
                {
                    self.add_error(ParseErrorType::InvalidYieldExpressionUsage, &expr);
                }

                return expr.into();
            }
            _ => {}
        }

        let lhs = self.parse_atom();

        ParsedExpr {
            expr: self.parse_postfix_expression(lhs.expr, start),
            is_parenthesized: lhs.is_parenthesized,
        }
    }

    /// Parses an expression with a minimum precedence of bitwise `or`.
    ///
    /// This methods actually parses the expression using the `expression` rule
    /// of the [Python grammar] and then validates the parsed expression. In a
    /// sense, it matches the `bitwise_or` rule of the [Python grammar].
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_expression_with_bitwise_or_precedence(&mut self) -> ParsedExpr {
        let parsed_expr = self.parse_conditional_expression_or_higher();

        if parsed_expr.is_parenthesized {
            // Parentheses resets the precedence, so we don't need to validate it.
            return parsed_expr;
        }

        let expr_name = match parsed_expr.expr {
            Expr::Compare(_) => "Comparison",
            Expr::BoolOp(_)
            | Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                ..
            }) => "Boolean",
            Expr::If(_) => "Conditional",
            Expr::Lambda(_) => "Lambda",
            _ => return parsed_expr,
        };

        self.add_error(
            ParseErrorType::OtherError(format!("{expr_name} expression cannot be used here")),
            &parsed_expr,
        );

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
            let TokenValue::Name(name) = self.bump_value(TokenKind::Name) else {
                unreachable!();
            };
            return ast::Identifier { id: name, range };
        }

        if self.current_token_kind().is_soft_keyword() {
            let id = Name::new(self.src_text(range));
            self.bump_soft_keyword_as_name();
            return ast::Identifier { id, range };
        }

        if self.current_token_kind().is_keyword() {
            // Non-soft keyword
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "Expected an identifier, but found a keyword {} that cannot be used here",
                    self.current_token_kind()
                )),
                range,
            );

            let id = Name::new(self.src_text(range));
            self.bump_any();
            ast::Identifier { id, range }
        } else {
            self.add_error(
                ParseErrorType::OtherError("Expected an identifier".into()),
                range,
            );

            ast::Identifier {
                id: Name::empty(),
                range: self.missing_node_range(),
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
                let TokenValue::Float(value) = self.bump_value(TokenKind::Float) else {
                    unreachable!()
                };

                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Float(value),
                    range: self.node_range(start),
                })
            }
            TokenKind::Complex => {
                let TokenValue::Complex { real, imag } = self.bump_value(TokenKind::Complex) else {
                    unreachable!()
                };
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Complex { real, imag },
                    range: self.node_range(start),
                })
            }
            TokenKind::Int => {
                let TokenValue::Int(value) = self.bump_value(TokenKind::Int) else {
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
            TokenKind::IpyEscapeCommand => {
                Expr::IpyEscapeCommand(self.parse_ipython_escape_command_expression())
            }
            TokenKind::String | TokenKind::FStringStart => self.parse_strings(),
            TokenKind::Lpar => {
                return self.parse_parenthesized_expression();
            }
            TokenKind::Lsqb => self.parse_list_like_expression(),
            TokenKind::Lbrace => self.parse_set_or_dict_like_expression(),

            kind => {
                if kind.is_keyword() {
                    Expr::Name(self.parse_name())
                } else {
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    Expr::Name(ast::ExprName {
                        range: self.missing_node_range(),
                        id: Name::empty(),
                        ctx: ExprContext::Invalid,
                    })
                }
            }
        };

        lhs.into()
    }

    /// Parses a postfix expression in a loop until there are no postfix expressions left to parse.
    ///
    /// For a given left-hand side, a postfix expression can begin with either `(` for a call
    /// expression, `[` for a subscript expression, or `.` for an attribute expression.
    ///
    /// This method does nothing if the current token is not a candidate for a postfix expression.
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
    pub(super) fn parse_call_expression(&mut self, func: Expr, start: TextSize) -> ast::ExprCall {
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

        let mut args = vec![];
        let mut keywords = vec![];
        let mut seen_keyword_argument = false; // foo = 1
        let mut seen_keyword_unpacking = false; // **foo

        self.parse_comma_separated_list(RecoveryContextKind::Arguments, |parser| {
            let argument_start = parser.node_start();
            if parser.eat(TokenKind::DoubleStar) {
                let value = parser.parse_conditional_expression_or_higher();

                keywords.push(ast::Keyword {
                    arg: None,
                    value: value.expr,
                    range: parser.node_range(argument_start),
                });

                seen_keyword_unpacking = true;
            } else {
                let start = parser.node_start();
                let mut parsed_expr = parser
                    .parse_named_expression_or_higher(ExpressionContext::starred_conditional());

                match parser.current_token_kind() {
                    TokenKind::Async | TokenKind::For => {
                        if parsed_expr.is_unparenthesized_starred_expr() {
                            parser.add_error(
                                ParseErrorType::IterableUnpackingInComprehension,
                                &parsed_expr,
                            );
                        }

                        parsed_expr = Expr::Generator(parser.parse_generator_expression(
                            parsed_expr.expr,
                            start,
                            Parenthesized::No,
                        ))
                        .into();
                    }
                    _ => {
                        if seen_keyword_unpacking && parsed_expr.is_unparenthesized_starred_expr() {
                            parser.add_error(
                                ParseErrorType::InvalidArgumentUnpackingOrder,
                                &parsed_expr,
                            );
                        }
                    }
                }

                let arg_range = parser.node_range(start);
                if parser.eat(TokenKind::Equal) {
                    seen_keyword_argument = true;
                    let arg = if let ParsedExpr {
                        expr: Expr::Name(ident_expr),
                        is_parenthesized,
                    } = parsed_expr
                    {
                        // test_ok parenthesized_kwarg_py37
                        // # parse_options: {"target-version": "3.7"}
                        // f((a)=1)

                        // test_err parenthesized_kwarg_py38
                        // # parse_options: {"target-version": "3.8"}
                        // f((a)=1)
                        // f((a) = 1)
                        // f( ( a ) = 1)

                        if is_parenthesized {
                            parser.add_unsupported_syntax_error(
                                UnsupportedSyntaxErrorKind::ParenthesizedKeywordArgumentName,
                                arg_range,
                            );
                        }

                        ast::Identifier {
                            id: ident_expr.id,
                            range: ident_expr.range,
                        }
                    } else {
                        // TODO(dhruvmanila): Parser shouldn't drop the `parsed_expr` if it's
                        // not a name expression. We could add the expression into `args` but
                        // that means the error is a missing comma instead.
                        parser.add_error(
                            ParseErrorType::OtherError("Expected a parameter name".to_string()),
                            &parsed_expr,
                        );
                        ast::Identifier {
                            id: Name::empty(),
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
                    if !parsed_expr.is_unparenthesized_starred_expr() {
                        if seen_keyword_unpacking {
                            parser.add_error(
                                ParseErrorType::PositionalAfterKeywordUnpacking,
                                &parsed_expr,
                            );
                        } else if seen_keyword_argument {
                            parser.add_error(
                                ParseErrorType::PositionalAfterKeywordArgument,
                                &parsed_expr,
                            );
                        }
                    }
                    args.push(parsed_expr.expr);
                }
            }
        });

        self.expect(TokenKind::Rpar);

        let arguments = ast::Arguments {
            range: self.node_range(start),
            args: args.into_boxed_slice(),
            keywords: keywords.into_boxed_slice(),
        };

        self.validate_arguments(&arguments);

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
                    id: Name::empty(),
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

        // test_ok star_index_py311
        // # parse_options: {"target-version": "3.11"}
        // lst[*index]  # simple index
        // class Array(Generic[DType, *Shape]): ...  # motivating example from the PEP
        // lst[a, *b, c]  # different positions
        // lst[a, b, *c]  # different positions
        // lst[*a, *b]  # multiple unpacks
        // array[3:5, *idxs]  # mixed with slices

        // test_err star_index_py310
        // # parse_options: {"target-version": "3.10"}
        // lst[*index]  # simple index
        // class Array(Generic[DType, *Shape]): ...  # motivating example from the PEP
        // lst[a, *b, c]  # different positions
        // lst[a, b, *c]  # different positions
        // lst[*a, *b]  # multiple unpacks
        // array[3:5, *idxs]  # mixed with slices

        // test_err star_slices
        // array[*start:*end]

        // test_ok parenthesized_star_index_py310
        // # parse_options: {"target-version": "3.10"}
        // out[(*(slice(None) for _ in range(2)), *ind)] = 1
        if let Expr::Tuple(ast::ExprTuple {
            elts,
            parenthesized: false,
            ..
        }) = &slice
        {
            for elt in elts.iter().filter(|elt| elt.is_starred_expr()) {
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::StarExpressionInIndex,
                    elt.range(),
                );
            }
        }

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

        // test_err named_expr_slice
        // # even after 3.9, an unparenthesized named expression is not allowed in a slice
        // lst[x:=1:-1]
        // lst[1:x:=1]
        // lst[1:3:x:=1]

        // test_err named_expr_slice_parse_error
        // # parse_options: {"target-version": "3.8"}
        // # before 3.9, only emit the parse error, not the unsupported syntax error
        // lst[x:=1:-1]

        let start = self.node_start();

        let lower = if self.at_expr() {
            let lower =
                self.parse_named_expression_or_higher(ExpressionContext::starred_conditional());

            // This means we're in a subscript.
            if self.at_ts(NEWLINE_EOF_SET.union([TokenKind::Rsqb, TokenKind::Comma].into())) {
                // test_ok parenthesized_named_expr_index_py38
                // # parse_options: {"target-version": "3.8"}
                // lst[(x:=1)]

                // test_ok unparenthesized_named_expr_index_py39
                // # parse_options: {"target-version": "3.9"}
                // lst[x:=1]

                // test_err unparenthesized_named_expr_index_py38
                // # parse_options: {"target-version": "3.8"}
                // lst[x:=1]
                if lower.is_unparenthesized_named_expr() {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                            UnparenthesizedNamedExprKind::SequenceIndex,
                        ),
                        lower.range(),
                    );
                }
                return lower.expr;
            }

            // Now we know we're in a slice.
            if !lower.is_parenthesized {
                match lower.expr {
                    Expr::Starred(_) => {
                        self.add_error(ParseErrorType::InvalidStarredExpressionUsage, &lower);
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

    /// Parses a unary expression.
    ///
    /// This includes the unary arithmetic `+` and `-`, bitwise `~`, and the
    /// boolean `not` operators.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at any of the unary operators.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#unary-arithmetic-and-bitwise-operations>
    pub(super) fn parse_unary_expression(
        &mut self,
        op: UnaryOp,
        context: ExpressionContext,
    ) -> ast::ExprUnaryOp {
        let start = self.node_start();
        self.bump(TokenKind::from(op));

        let operand = self.parse_binary_expression_or_higher(OperatorPrecedence::from(op), context);

        ast::ExprUnaryOp {
            op,
            operand: Box::new(operand.expr),
            range: self.node_range(start),
        }
    }

    /// Parses an attribute expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `.` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#attribute-references>
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

    /// Parses a boolean operation expression.
    ///
    /// Note that the boolean `not` operator is parsed as a unary expression and
    /// not as a boolean expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `or` or `and` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#boolean-operations>
    fn parse_boolean_expression(
        &mut self,
        lhs: Expr,
        start: TextSize,
        op: BoolOp,
        context: ExpressionContext,
    ) -> ast::ExprBoolOp {
        self.bump(TokenKind::from(op));

        let mut values = vec![lhs];
        let mut progress = ParserProgress::default();

        // Keep adding the expression to `values` until we see a different
        // token than `operator_token`.
        loop {
            progress.assert_progressing(self);

            let parsed_expr =
                self.parse_binary_expression_or_higher(OperatorPrecedence::from(op), context);
            values.push(parsed_expr.expr);

            if !self.eat(TokenKind::from(op)) {
                break;
            }
        }

        ast::ExprBoolOp {
            values,
            op,
            range: self.node_range(start),
        }
    }

    /// Bump the appropriate token(s) for the given comparison operator.
    fn bump_cmp_op(&mut self, op: CmpOp) {
        let (first, second) = match op {
            CmpOp::Eq => (TokenKind::EqEqual, None),
            CmpOp::NotEq => (TokenKind::NotEqual, None),
            CmpOp::Lt => (TokenKind::Less, None),
            CmpOp::LtE => (TokenKind::LessEqual, None),
            CmpOp::Gt => (TokenKind::Greater, None),
            CmpOp::GtE => (TokenKind::GreaterEqual, None),
            CmpOp::Is => (TokenKind::Is, None),
            CmpOp::IsNot => (TokenKind::Is, Some(TokenKind::Not)),
            CmpOp::In => (TokenKind::In, None),
            CmpOp::NotIn => (TokenKind::Not, Some(TokenKind::In)),
        };

        self.bump(first);
        if let Some(second) = second {
            self.bump(second);
        }
    }

    /// Parse a comparison expression.
    ///
    /// This includes the following operators:
    /// - Value comparisons: `==`, `!=`, `<`, `<=`, `>`, and `>=`.
    /// - Membership tests: `in` and `not in`.
    /// - Identity tests: `is` and `is not`.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at any of the comparison operators.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#comparisons>
    fn parse_comparison_expression(
        &mut self,
        lhs: Expr,
        start: TextSize,
        op: CmpOp,
        context: ExpressionContext,
    ) -> ast::ExprCompare {
        self.bump_cmp_op(op);

        let mut comparators = vec![];
        let mut operators = vec![op];

        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            comparators.push(
                self.parse_binary_expression_or_higher(
                    OperatorPrecedence::ComparisonsMembershipIdentity,
                    context,
                )
                .expr,
            );

            let next_token = self.current_token_kind();
            if matches!(next_token, TokenKind::In) && context.is_in_excluded() {
                break;
            }

            let Some(next_op) = helpers::token_kind_to_cmp_op([next_token, self.peek()]) else {
                break;
            };

            self.bump_cmp_op(next_op);
            operators.push(next_op);
        }

        ast::ExprCompare {
            left: Box::new(lhs),
            ops: operators.into_boxed_slice(),
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
        const STRING_START_SET: TokenSet =
            TokenSet::new([TokenKind::String, TokenKind::FStringStart]);

        let start = self.node_start();
        let mut strings = vec![];

        let mut progress = ParserProgress::default();

        while self.at_ts(STRING_START_SET) {
            progress.assert_progressing(self);

            if self.at(TokenKind::String) {
                strings.push(self.parse_string_or_byte_literal());
            } else {
                strings.push(StringType::FString(self.parse_fstring()));
            }
        }

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
    ///
    /// # Panics
    ///
    /// If the length of `strings` is less than 2.
    fn handle_implicitly_concatenated_strings(
        &mut self,
        strings: Vec<StringType>,
        range: TextRange,
    ) -> Expr {
        assert!(strings.len() > 1);

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
                    // TODO(dhruvmanila): This is not an ideal recovery because the parser
                    // replaces the byte literals with an invalid string literal node. Any
                    // downstream tools can extract the raw bytes from the range.
                    //
                    // We could convert the node into a string and mark it as invalid
                    // and would be clever to mark the type which is fewer in quantity.

                    // test_err mixed_bytes_and_non_bytes_literals
                    // 'first' b'second'
                    // f'first' b'second'
                    // 'first' f'second' b'third'
                    self.add_error(
                        ParseErrorType::OtherError(
                            "Bytes literal cannot be mixed with non-bytes literals".to_string(),
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
                            _ => unreachable!("Expected `StringType::Bytes`"),
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

        // TODO(dhruvmanila): Parser drops unterminated strings here as well
        // because the lexer doesn't emit them.

        // test_err implicitly_concatenated_unterminated_string
        // 'hello' 'world
        // 1 + 1
        // 'hello' f'world {x}
        // 2 + 2

        // test_err implicitly_concatenated_unterminated_string_multiline
        // (
        //     'hello'
        //     f'world {x}
        // )
        // 1 + 1
        // (
        //     'first'
        //     'second
        //     f'third'
        // )
        // 2 + 2

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

    /// Parses a single string or byte literal.
    ///
    /// This does not handle implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `String` token.
    ///
    /// See: <https://docs.python.org/3.13/reference/lexical_analysis.html#string-and-bytes-literals>
    fn parse_string_or_byte_literal(&mut self) -> StringType {
        let range = self.current_token_range();
        let flags = self.tokens.current_flags().as_any_string_flags();

        let TokenValue::String(value) = self.bump_value(TokenKind::String) else {
            unreachable!()
        };

        match parse_string_literal(value, flags, range) {
            Ok(string) => string,
            Err(error) => {
                let location = error.location();
                self.add_error(ParseErrorType::Lexical(error.into_error()), location);

                if flags.is_byte_string() {
                    // test_err invalid_byte_literal
                    // b'123a𝐁c'
                    // rb"a𝐁c123"
                    // b"""123a𝐁c"""
                    StringType::Bytes(ast::BytesLiteral {
                        value: Box::new([]),
                        range,
                        flags: ast::BytesLiteralFlags::from(flags).with_invalid(),
                    })
                } else {
                    // test_err invalid_string_literal
                    // 'hello \N{INVALID} world'
                    // """hello \N{INVALID} world"""
                    StringType::Str(ast::StringLiteral {
                        value: "".into(),
                        range,
                        flags: ast::StringLiteralFlags::from(flags).with_invalid(),
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
    /// See: <https://docs.python.org/3/reference/lexical_analysis.html#formatted-string-literals>
    fn parse_fstring(&mut self) -> ast::FString {
        let start = self.node_start();
        let flags = self.tokens.current_flags().as_any_string_flags();

        self.bump(TokenKind::FStringStart);
        let elements = self.parse_fstring_elements(flags, FStringElementsKind::Regular);

        self.expect(TokenKind::FStringEnd);

        // test_ok pep701_f_string_py312
        // # parse_options: {"target-version": "3.12"}
        // f'Magic wand: { bag['wand'] }'     # nested quotes
        // f"{'\n'.join(a)}"                  # escape sequence
        // f'''A complex trick: {
        //     bag['bag']                     # comment
        // }'''
        // f"{f"{f"{f"{f"{f"{1+1}"}"}"}"}"}"  # arbitrary nesting
        // f"{f'''{"nested"} inner'''} outer" # nested (triple) quotes
        // f"test {a \
        //     } more"                        # line continuation

        // test_ok pep701_f_string_py311
        // # parse_options: {"target-version": "3.11"}
        // f"outer {'# not a comment'}"
        // f'outer {x:{"# not a comment"} }'
        // f"""{f'''{f'{"# not a comment"}'}'''}"""
        // f"""{f'''# before expression {f'# aro{f"#{1+1}#"}und #'}'''} # after expression"""
        // f"escape outside of \t {expr}\n"
        // f"test\"abcd"

        // test_err pep701_f_string_py311
        // # parse_options: {"target-version": "3.11"}
        // f'Magic wand: { bag['wand'] }'     # nested quotes
        // f"{'\n'.join(a)}"                  # escape sequence
        // f'''A complex trick: {
        //     bag['bag']                     # comment
        // }'''
        // f"{f"{f"{f"{f"{f"{1+1}"}"}"}"}"}"  # arbitrary nesting
        // f"{f'''{"nested"} inner'''} outer" # nested (triple) quotes
        // f"test {a \
        //     } more"                        # line continuation
        // f"""{f"""{x}"""}"""                # mark the whole triple quote
        // f"{'\n'.join(['\t', '\v', '\r'])}"  # multiple escape sequences, multiple errors

        let range = self.node_range(start);

        if !self.options.target_version.supports_pep_701() {
            let quote_bytes = flags.quote_str().as_bytes();
            let quote_len = flags.quote_len();
            for expr in elements.expressions() {
                for slash_position in memchr::memchr_iter(b'\\', self.source[expr.range].as_bytes())
                {
                    let slash_position = TextSize::try_from(slash_position).unwrap();
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::Backslash),
                        TextRange::at(expr.range.start() + slash_position, '\\'.text_len()),
                    );
                }

                if let Some(quote_position) =
                    memchr::memmem::find(self.source[expr.range].as_bytes(), quote_bytes)
                {
                    let quote_position = TextSize::try_from(quote_position).unwrap();
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::NestedQuote),
                        TextRange::at(expr.range.start() + quote_position, quote_len),
                    );
                }
            }

            self.check_fstring_comments(range);
        }

        ast::FString {
            elements,
            range,
            flags: ast::FStringFlags::from(flags),
        }
    }

    /// Check `range` for comment tokens and report an `UnsupportedSyntaxError` for each one found.
    fn check_fstring_comments(&mut self, range: TextRange) {
        self.unsupported_syntax_errors
            .extend(self.tokens.in_range(range).iter().filter_map(|token| {
                token.kind().is_comment().then_some(UnsupportedSyntaxError {
                    kind: UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::Comment),
                    range: token.range(),
                    target_version: self.options.target_version,
                })
            }));
    }

    /// Parses a list of f-string elements.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` or `FStringMiddle` token.
    fn parse_fstring_elements(
        &mut self,
        flags: ast::AnyStringFlags,
        kind: FStringElementsKind,
    ) -> FStringElements {
        let mut elements = vec![];

        self.parse_list(RecoveryContextKind::FStringElements(kind), |parser| {
            let element = match parser.current_token_kind() {
                TokenKind::Lbrace => {
                    FStringElement::Expression(parser.parse_fstring_expression_element(flags))
                }
                TokenKind::FStringMiddle => {
                    let range = parser.current_token_range();
                    let TokenValue::FStringMiddle(value) =
                        parser.bump_value(TokenKind::FStringMiddle)
                    else {
                        unreachable!()
                    };
                    FStringElement::Literal(
                        parse_fstring_literal_element(value, flags, range).unwrap_or_else(
                            |lex_error| {
                                // test_err invalid_fstring_literal_element
                                // f'hello \N{INVALID} world'
                                // f"""hello \N{INVALID} world"""
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
                    parser.bump_any();
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

        FStringElements::from(elements)
    }

    /// Parses a f-string expression element.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` token.
    fn parse_fstring_expression_element(
        &mut self,
        flags: ast::AnyStringFlags,
    ) -> ast::FStringExpressionElement {
        let start = self.node_start();
        self.bump(TokenKind::Lbrace);

        // test_err f_string_empty_expression
        // f"{}"
        // f"{  }"

        // test_err f_string_invalid_starred_expr
        // # Starred expression inside f-string has a minimum precedence of bitwise or.
        // f"{*}"
        // f"{*x and y}"
        // f"{*yield x}"
        let value = self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

        if !value.is_parenthesized && value.expr.is_lambda_expr() {
            // TODO(dhruvmanila): This requires making some changes in lambda expression
            // parsing logic to handle the emitted `FStringMiddle` token in case the
            // lambda expression is not parenthesized.

            // test_err f_string_lambda_without_parentheses
            // f"{lambda x: x}"
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
            if self.at(TokenKind::Name) {
                let TokenValue::Name(name) = self.bump_value(TokenKind::Name) else {
                    unreachable!();
                };
                match &*name {
                    "s" => ConversionFlag::Str,
                    "r" => ConversionFlag::Repr,
                    "a" => ConversionFlag::Ascii,
                    _ => {
                        // test_err f_string_invalid_conversion_flag_name_tok
                        // f"{x!z}"
                        self.add_error(
                            ParseErrorType::FStringError(FStringErrorType::InvalidConversionFlag),
                            conversion_flag_range,
                        );
                        ConversionFlag::None
                    }
                }
            } else {
                // test_err f_string_invalid_conversion_flag_other_tok
                // f"{x!123}"
                // f"{x!'a'}"
                self.add_error(
                    ParseErrorType::FStringError(FStringErrorType::InvalidConversionFlag),
                    conversion_flag_range,
                );
                // TODO(dhruvmanila): Avoid dropping this token
                self.bump_any();
                ConversionFlag::None
            }
        } else {
            ConversionFlag::None
        };

        let format_spec = if self.eat(TokenKind::Colon) {
            let spec_start = self.node_start();
            let elements = self.parse_fstring_elements(flags, FStringElementsKind::FormatSpec);
            Some(Box::new(ast::FStringFormatSpec {
                range: self.node_range(spec_start),
                elements,
            }))
        } else {
            None
        };

        // We're using `eat` here instead of `expect` to use the f-string specific error type.
        if !self.eat(TokenKind::Rbrace) {
            // TODO(dhruvmanila): This requires some changes in the lexer. One of them
            // would be to emit `FStringEnd`. Currently, the following test cases doesn't
            // really work as expected. Refer https://github.com/astral-sh/ruff/pull/10372

            // test_err f_string_unclosed_lbrace
            // f"{"
            // f"{foo!r"
            // f"{foo="
            // f"{"
            // f"""{"""

            // The lexer does emit `FStringEnd` for the following test cases:

            // test_err f_string_unclosed_lbrace_in_format_spec
            // f"hello {x:"
            // f"hello {x:.3f"
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
        let first_element =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                // Parenthesized starred expression isn't allowed either but that is
                // handled by the `parse_parenthesized_expression` method.
                if first_element.is_unparenthesized_starred_expr() {
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
                items: vec![],
                range: self.node_range(start),
            });
        }

        if self.eat(TokenKind::DoubleStar) {
            // Handle dictionary unpacking. Here, the grammar is `'**' bitwise_or`
            // which requires limiting the expression.
            let value = self.parse_expression_with_bitwise_or_precedence();

            return Expr::Dict(self.parse_dictionary_expression(None, value.expr, start));
        }

        // For dictionary expressions, the key uses the `expression` rule while for
        // set expressions, the element uses the `star_expression` rule. So, use the
        // one that is more general and limit it later.
        let key_or_element =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                if key_or_element.is_unparenthesized_starred_expr() {
                    self.add_error(
                        ParseErrorType::IterableUnpackingInComprehension,
                        &key_or_element,
                    );
                } else if key_or_element.is_unparenthesized_named_expr() {
                    // test_ok parenthesized_named_expr_py38
                    // # parse_options: {"target-version": "3.8"}
                    // {(x := 1), 2, 3}
                    // {(last := x) for x in range(3)}

                    // test_ok unparenthesized_named_expr_py39
                    // # parse_options: {"target-version": "3.9"}
                    // {x := 1, 2, 3}
                    // {last := x for x in range(3)}

                    // test_err unparenthesized_named_expr_set_comp_py38
                    // # parse_options: {"target-version": "3.8"}
                    // {last := x for x in range(3)}
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                            UnparenthesizedNamedExprKind::SetComprehension,
                        ),
                        key_or_element.range(),
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
                            ParseErrorType::InvalidStarredExpressionUsage,
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
            _ => Expr::Set(self.parse_set_expression(key_or_element, start)),
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
        let mut parsed_expr =
            self.parse_named_expression_or_higher(ExpressionContext::yield_or_starred_bitwise_or());

        match self.current_token_kind() {
            TokenKind::Comma => {
                // grammar: `tuple`
                let tuple =
                    self.parse_tuple_expression(parsed_expr.expr, start, Parenthesized::Yes, |p| {
                        p.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
                    });

                ParsedExpr {
                    expr: tuple.into(),
                    is_parenthesized: false,
                }
            }
            TokenKind::Async | TokenKind::For => {
                // grammar: `genexp`
                if parsed_expr.is_unparenthesized_starred_expr() {
                    self.add_error(
                        ParseErrorType::IterableUnpackingInComprehension,
                        &parsed_expr,
                    );
                }

                let generator = Expr::Generator(self.parse_generator_expression(
                    parsed_expr.expr,
                    start,
                    Parenthesized::Yes,
                ));

                ParsedExpr {
                    expr: generator,
                    is_parenthesized: false,
                }
            }
            _ => {
                // grammar: `group`
                if parsed_expr.expr.is_starred_expr() {
                    self.add_error(ParseErrorType::InvalidStarredExpressionUsage, &parsed_expr);
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
        parenthesized: Parenthesized,
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
                    .parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
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
    fn parse_set_expression(&mut self, first_element: ParsedExpr, start: TextSize) -> ast::ExprSet {
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        // test_err unparenthesized_named_expr_set_literal_py38
        // # parse_options: {"target-version": "3.8"}
        // {x := 1, 2, 3}
        // {1, x := 2, 3}
        // {1, 2, x := 3}

        if first_element.is_unparenthesized_named_expr() {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                    UnparenthesizedNamedExprKind::SetLiteral,
                ),
                first_element.range(),
            );
        }

        let mut elts = vec![first_element.expr];

        self.parse_comma_separated_list(RecoveryContextKind::SetElements, |parser| {
            let parsed_expr =
                parser.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

            if parsed_expr.is_unparenthesized_named_expr() {
                parser.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                        UnparenthesizedNamedExprKind::SetLiteral,
                    ),
                    parsed_expr.range(),
                );
            }

            elts.push(parsed_expr.expr);
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

        let mut items = vec![ast::DictItem { key, value }];

        self.parse_comma_separated_list(RecoveryContextKind::DictElements, |parser| {
            if parser.eat(TokenKind::DoubleStar) {
                // Handle dictionary unpacking. Here, the grammar is `'**' bitwise_or`
                // which requires limiting the expression.
                items.push(ast::DictItem {
                    key: None,
                    value: parser.parse_expression_with_bitwise_or_precedence().expr,
                });
            } else {
                let key = parser.parse_conditional_expression_or_higher().expr;
                parser.expect(TokenKind::Colon);

                items.push(ast::DictItem {
                    key: Some(key),
                    value: parser.parse_conditional_expression_or_higher().expr,
                });
            }
        });

        self.expect(TokenKind::Rbrace);

        ast::ExprDict {
            range: self.node_range(start),
            items,
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

        if is_async {
            // test_err comprehension_missing_for_after_async
            // (async)
            // (x async x in iter)
            self.expect(TokenKind::For);
        } else {
            self.bump(TokenKind::For);
        }

        let mut target =
            self.parse_expression_list(ExpressionContext::starred_conditional().with_in_excluded());

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);
        self.validate_assignment_target(&target.expr);

        self.expect(TokenKind::In);
        let iter = self.parse_simple_expression(ExpressionContext::default());

        let mut ifs = vec![];
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::If) {
            progress.assert_progressing(self);

            let parsed_expr = self.parse_simple_expression(ExpressionContext::default());

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

    /// Parses a generator expression.
    ///
    /// The given `start` offset is the start of either the opening parenthesis if the generator is
    /// parenthesized or the first token of the expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#generator-expressions>
    pub(super) fn parse_generator_expression(
        &mut self,
        element: Expr,
        start: TextSize,
        parenthesized: Parenthesized,
    ) -> ast::ExprGenerator {
        let generators = self.parse_generators();

        if parenthesized.is_yes() {
            self.expect(TokenKind::Rpar);
        }

        ast::ExprGenerator {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
            parenthesized: parenthesized.is_yes(),
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
    /// The expression is parsed with the highest precedence. If the precedence
    /// of the parsed expression is lower than the given precedence, an error
    /// is reported.
    ///
    /// For example, if the given precedence is [`StarredExpressionPrecedence::BitOr`],
    /// the comparison expression is not allowed.
    ///
    /// Refer to the [Python grammar] for more information.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `*` token.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_starred_expression(&mut self, context: ExpressionContext) -> ast::ExprStarred {
        let start = self.node_start();
        self.bump(TokenKind::Star);

        let parsed_expr = match context.starred_expression_precedence() {
            StarredExpressionPrecedence::Conditional => {
                self.parse_conditional_expression_or_higher_impl(context)
            }
            StarredExpressionPrecedence::BitwiseOr => {
                self.parse_expression_with_bitwise_or_precedence()
            }
        };

        ast::ExprStarred {
            value: Box::new(parsed_expr.expr),
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
    }

    /// Parses an `await` expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `await` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#await-expression>
    fn parse_await_expression(&mut self) -> ast::ExprAwait {
        let start = self.node_start();
        self.bump(TokenKind::Await);

        let parsed_expr = self.parse_binary_expression_or_higher(
            OperatorPrecedence::Await,
            ExpressionContext::default(),
        );

        ast::ExprAwait {
            value: Box::new(parsed_expr.expr),
            range: self.node_range(start),
        }
    }

    /// Parses a `yield` expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `yield` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_expression(&mut self) -> Expr {
        let start = self.node_start();
        self.bump(TokenKind::Yield);

        if self.eat(TokenKind::From) {
            return self.parse_yield_from_expression(start);
        }

        let value = self.at_expr().then(|| {
            let parsed_expr = self.parse_expression_list(ExpressionContext::starred_bitwise_or());

            // test_ok iter_unpack_yield_py37
            // # parse_options: {"target-version": "3.7"}
            // rest = (4, 5, 6)
            // def g(): yield (1, 2, 3, *rest)

            // test_ok iter_unpack_yield_py38
            // # parse_options: {"target-version": "3.8"}
            // rest = (4, 5, 6)
            // def g(): yield 1, 2, 3, *rest
            // def h(): yield 1, (yield 2, *rest), 3

            // test_err iter_unpack_yield_py37
            // # parse_options: {"target-version": "3.7"}
            // rest = (4, 5, 6)
            // def g(): yield 1, 2, 3, *rest
            // def h(): yield 1, (yield 2, *rest), 3
            self.check_tuple_unpacking(
                &parsed_expr,
                UnsupportedSyntaxErrorKind::StarTuple(StarTupleKind::Yield),
            );

            Box::new(parsed_expr.expr)
        });

        Expr::Yield(ast::ExprYield {
            value,
            range: self.node_range(start),
        })
    }

    /// Parses a `yield from` expression.
    ///
    /// This method should not be used directly. Use [`Parser::parse_yield_expression`]
    /// even when parsing a `yield from` expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_from_expression(&mut self, start: TextSize) -> Expr {
        // Grammar:
        //     'yield' 'from' expression
        //
        // Here, a tuple expression isn't allowed without the parentheses. But, we
        // allow it here to report better error message.
        //
        // Now, this also solves another problem. Take the following example:
        //
        // ```python
        // yield from x, y
        // ```
        //
        // If we didn't use the `parse_expression_list` method here, the parser
        // would have stopped at the comma. Then, the outer expression would
        // have been a tuple expression with two elements: `yield from x` and `y`.
        let expr = self
            .parse_expression_list(ExpressionContext::default())
            .expr;

        match &expr {
            Expr::Tuple(tuple) if !tuple.parenthesized => {
                self.add_error(ParseErrorType::UnparenthesizedTupleExpression, &expr);
            }
            _ => {}
        }

        Expr::YieldFrom(ast::ExprYieldFrom {
            value: Box::new(expr),
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

        let range = self.node_range(start);

        // test_err walrus_py37
        // # parse_options: { "target-version": "3.7" }
        // (x := 1)

        // test_ok walrus_py38
        // # parse_options: { "target-version": "3.8" }
        // (x := 1)

        self.add_unsupported_syntax_error(UnsupportedSyntaxErrorKind::Walrus, range);

        ast::ExprNamed {
            target: Box::new(target),
            value: Box::new(value.expr),
            range,
        }
    }

    /// Parses a lambda expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `lambda` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#lambda>
    fn parse_lambda_expr(&mut self) -> ast::ExprLambda {
        let start = self.node_start();
        self.bump(TokenKind::Lambda);

        let parameters = if self.at(TokenKind::Colon) {
            // test_ok lambda_with_no_parameters
            // lambda: 1
            None
        } else {
            Some(Box::new(self.parse_parameters(FunctionKind::Lambda)))
        };

        self.expect(TokenKind::Colon);

        // test_ok lambda_with_valid_body
        // lambda x: x
        // lambda x: x if True else y
        // lambda x: await x
        // lambda x: lambda y: x + y
        // lambda x: (yield x)  # Parenthesized `yield` is fine
        // lambda x: x, *y

        // test_err lambda_body_with_starred_expr
        // lambda x: *y
        // lambda x: *y,
        // lambda x: *y, z
        // lambda x: *y and z

        // test_err lambda_body_with_yield_expr
        // lambda x: yield y
        // lambda x: yield from y
        let body = self.parse_conditional_expression_or_higher();

        ast::ExprLambda {
            body: Box::new(body.expr),
            parameters,
            range: self.node_range(start),
        }
    }

    /// Parses an `if` expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `if` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#conditional-expressions>
    pub(super) fn parse_if_expression(&mut self, body: Expr, start: TextSize) -> ast::ExprIf {
        self.bump(TokenKind::If);

        let test = self.parse_simple_expression(ExpressionContext::default());

        self.expect(TokenKind::Else);

        let orelse = self.parse_conditional_expression_or_higher();

        ast::ExprIf {
            body: Box::new(body),
            test: Box::new(test.expr),
            orelse: Box::new(orelse.expr),
            range: self.node_range(start),
        }
    }

    /// Parses an IPython escape command at the expression level.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `IpyEscapeCommand` token.
    /// If the escape command kind is not `%` or `!`.
    fn parse_ipython_escape_command_expression(&mut self) -> ast::ExprIpyEscapeCommand {
        let start = self.node_start();

        let TokenValue::IpyEscapeCommand { value, kind } =
            self.bump_value(TokenKind::IpyEscapeCommand)
        else {
            unreachable!()
        };

        if !matches!(kind, IpyEscapeKind::Magic | IpyEscapeKind::Shell) {
            // This should never occur as the lexer won't allow it.
            unreachable!("IPython escape command expression is only allowed for % and !");
        }

        let command = ast::ExprIpyEscapeCommand {
            range: self.node_range(start),
            kind,
            value,
        };

        if self.options.mode != Mode::Ipython {
            self.add_error(ParseErrorType::UnexpectedIpythonEscapeCommand, &command);
        }

        command
    }

    /// Performs the following validations on the function call arguments:
    /// 1. There aren't any duplicate keyword argument
    /// 2. If there are more than one argument (positional or keyword), all generator expressions
    ///    present should be parenthesized.
    fn validate_arguments(&mut self, arguments: &ast::Arguments) {
        let mut all_arg_names =
            FxHashSet::with_capacity_and_hasher(arguments.keywords.len(), FxBuildHasher);

        for (name, range) in arguments
            .keywords
            .iter()
            .filter_map(|argument| argument.arg.as_ref().map(|arg| (arg, argument.range)))
        {
            let arg_name = name.as_str();
            if !all_arg_names.insert(arg_name) {
                self.add_error(
                    ParseErrorType::DuplicateKeywordArgumentError(arg_name.to_string()),
                    range,
                );
            }
        }

        if arguments.len() > 1 {
            for arg in &*arguments.args {
                if let Some(ast::ExprGenerator {
                    range,
                    parenthesized: false,
                    ..
                }) = arg.as_generator_expr()
                {
                    // test_ok args_unparenthesized_generator
                    // sum(x for x in range(10))

                    // test_err args_unparenthesized_generator
                    // sum(x for x in range(10), 5)
                    // total(1, 2, x for x in range(5), 6)
                    self.add_error(ParseErrorType::UnparenthesizedGeneratorExpression, range);
                }
            }
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
    pub(super) const fn is_unparenthesized_starred_expr(&self) -> bool {
        !self.is_parenthesized && self.expr.is_starred_expr()
    }

    #[inline]
    pub(super) const fn is_unparenthesized_named_expr(&self) -> bool {
        !self.is_parenthesized && self.expr.is_named_expr()
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

#[derive(Debug)]
enum BinaryLikeOperator {
    Boolean(BoolOp),
    Comparison(CmpOp),
    Binary(Operator),
}

impl BinaryLikeOperator {
    /// Attempts to convert the two tokens into the corresponding binary-like operator. Returns
    /// [None] if it's not a binary-like operator.
    fn try_from_tokens(current: TokenKind, next: TokenKind) -> Option<BinaryLikeOperator> {
        if let Some(bool_op) = current.as_bool_operator() {
            Some(BinaryLikeOperator::Boolean(bool_op))
        } else if let Some(bin_op) = current.as_binary_operator() {
            Some(BinaryLikeOperator::Binary(bin_op))
        } else {
            helpers::token_kind_to_cmp_op([current, next]).map(BinaryLikeOperator::Comparison)
        }
    }

    /// Returns the [`OperatorPrecedence`] for the given operator token or [None] if the token
    /// isn't an operator token.
    fn precedence(&self) -> OperatorPrecedence {
        match self {
            BinaryLikeOperator::Boolean(bool_op) => OperatorPrecedence::from(*bool_op),
            BinaryLikeOperator::Comparison(_) => OperatorPrecedence::ComparisonsMembershipIdentity,
            BinaryLikeOperator::Binary(bin_op) => OperatorPrecedence::from(*bin_op),
        }
    }
}

/// Represents the precedence used for parsing the value part of a starred expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StarredExpressionPrecedence {
    /// Matches `'*' bitwise_or` which is part of the `star_expression` rule in the
    /// [Python grammar](https://docs.python.org/3/reference/grammar.html).
    BitwiseOr,

    /// Matches `'*' expression` which is part of the `starred_expression` rule in the
    /// [Python grammar](https://docs.python.org/3/reference/grammar.html).
    Conditional,
}

/// Represents the expression parsing context.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct ExpressionContext(ExpressionContextFlags);

bitflags! {
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    struct ExpressionContextFlags: u8 {
        /// This flag is set when the `in` keyword should be excluded from a comparison expression.
        /// It is to avoid ambiguity in `for ... in ...` statements.
        const EXCLUDE_IN = 1 << 0;

        /// This flag is set when a starred expression should be allowed. This doesn't affect the
        /// parsing of a starred expression as it will be parsed nevertheless. But, if it is not
        /// allowed, an error is reported.
        const ALLOW_STARRED_EXPRESSION = 1 << 1;

        /// This flag is set when the value of a starred expression should be limited to bitwise OR
        /// precedence. Matches the `* bitwise_or` grammar rule if set.
        const STARRED_BITWISE_OR_PRECEDENCE = 1 << 2;

        /// This flag is set when a yield expression should be allowed. This doesn't affect the
        /// parsing of a yield expression as it will be parsed nevertheless. But, if it is not
        /// allowed, an error is reported.
        const ALLOW_YIELD_EXPRESSION = 1 << 3;
    }
}

impl ExpressionContext {
    /// Create a new context allowing starred expression at conditional precedence.
    pub(super) fn starred_conditional() -> Self {
        ExpressionContext::default()
            .with_starred_expression_allowed(StarredExpressionPrecedence::Conditional)
    }

    /// Create a new context allowing starred expression at bitwise OR precedence.
    pub(super) fn starred_bitwise_or() -> Self {
        ExpressionContext::default()
            .with_starred_expression_allowed(StarredExpressionPrecedence::BitwiseOr)
    }

    /// Create a new context allowing starred expression at bitwise OR precedence or yield
    /// expression.
    pub(super) fn yield_or_starred_bitwise_or() -> Self {
        ExpressionContext::starred_bitwise_or().with_yield_expression_allowed()
    }

    /// Returns a new [`ExpressionContext`] which allows starred expression with the given
    /// precedence.
    fn with_starred_expression_allowed(self, precedence: StarredExpressionPrecedence) -> Self {
        let mut flags = self.0 | ExpressionContextFlags::ALLOW_STARRED_EXPRESSION;
        match precedence {
            StarredExpressionPrecedence::BitwiseOr => {
                flags |= ExpressionContextFlags::STARRED_BITWISE_OR_PRECEDENCE;
            }
            StarredExpressionPrecedence::Conditional => {
                flags -= ExpressionContextFlags::STARRED_BITWISE_OR_PRECEDENCE;
            }
        }
        ExpressionContext(flags)
    }

    /// Returns a new [`ExpressionContext`] which allows yield expression.
    fn with_yield_expression_allowed(self) -> Self {
        ExpressionContext(self.0 | ExpressionContextFlags::ALLOW_YIELD_EXPRESSION)
    }

    /// Returns a new [`ExpressionContext`] which excludes `in` as part of a comparison expression.
    pub(super) fn with_in_excluded(self) -> Self {
        ExpressionContext(self.0 | ExpressionContextFlags::EXCLUDE_IN)
    }

    /// Returns `true` if the `in` keyword should be excluded from a comparison expression.
    const fn is_in_excluded(self) -> bool {
        self.0.contains(ExpressionContextFlags::EXCLUDE_IN)
    }

    /// Returns `true` if starred expressions are allowed.
    const fn is_starred_expression_allowed(self) -> bool {
        self.0
            .contains(ExpressionContextFlags::ALLOW_STARRED_EXPRESSION)
    }

    /// Returns `true` if yield expressions are allowed.
    const fn is_yield_expression_allowed(self) -> bool {
        self.0
            .contains(ExpressionContextFlags::ALLOW_YIELD_EXPRESSION)
    }

    /// Returns the [`StarredExpressionPrecedence`] for the context, regardless of whether starred
    /// expressions are allowed or not.
    const fn starred_expression_precedence(self) -> StarredExpressionPrecedence {
        if self
            .0
            .contains(ExpressionContextFlags::STARRED_BITWISE_OR_PRECEDENCE)
        {
            StarredExpressionPrecedence::BitwiseOr
        } else {
            StarredExpressionPrecedence::Conditional
        }
    }
}

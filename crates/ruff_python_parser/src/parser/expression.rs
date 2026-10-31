use crate::parser::syntax::{self as id, Construct, IntoList};
use ruff_python_ast::flat::{self, build};
use std::ops::Deref;

use bitflags::bitflags;
use thin_vec::ThinVec;

use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    self as ast, AnyStringFlags, BoolOp, CmpOp, ConversionFlag, ExprContext, IpyEscapeKind, Number,
    Operator, OperatorPrecedence, StringFlags, UnaryOp,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::error::{
    ComprehensionUnpackingKind, FStringKind, StarTupleKind, UnparenthesizedNamedExprKind,
};
use crate::parser::progress::ParserProgress;
use crate::parser::{FunctionKind, IpyEscapeContext, Parser, helpers};
use crate::string::{
    InterpolatedStringKind, StringType, parse_interpolated_string_literal_element,
    parse_string_literal,
};
use crate::token_set::TokenSet;
use crate::{
    InterpolatedStringErrorType, Mode, ParseErrorType, UnsupportedSyntaxError,
    UnsupportedSyntaxErrorKind,
};

use super::{InterpolatedStringElementsKind, Parenthesized, RecoveryContextKind};

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
    TokenKind::Identifier,
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
    TokenKind::TStringStart,
    TokenKind::IpyEscapeCommand,
])
.union(LITERAL_SET);

/// Tokens that can appear after an expression.
const END_EXPR_SET: TokenSet = TokenSet::new([
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
    /// Returns `true` if the parser is at an identifier or keyword (including soft keyword) token.
    pub(super) fn at_identifier_or_keyword(&self) -> bool {
        self.at(TokenKind::Identifier) || self.current_token_kind().is_keyword()
    }

    /// Returns `true` if the parser is at an identifier or soft keyword token.
    pub(super) fn at_identifier_or_soft_keyword(&self) -> bool {
        self.at(TokenKind::Identifier) || self.at_soft_keyword()
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
            let subsequent_context = context.disallow_yield_expressions();
            id::Expr::from(self.parse_tuple_expression(
                parsed_expr.expr,
                start,
                Parenthesized::No,
                |p| p.parse_conditional_expression_or_higher_impl(subsequent_context),
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
            id::Expr::from(self.parse_named_expression(parsed_expr.expr, start)).into()
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
            id::Expr::from(self.parse_lambda_expr()).into()
        } else {
            let start = self.node_start();
            let parsed_expr = self.parse_simple_expression(context);

            if self.at(TokenKind::If) {
                id::Expr::from(self.parse_if_expression(parsed_expr.expr, start)).into()
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
        self.with_recursion(|parser| {
            let start = parser.node_start();
            let lhs = parser.parse_lhs_expression(left_precedence, context);
            parser.parse_binary_expression_or_higher_recursive(lhs, left_precedence, context, start)
        })
    }

    fn parse_binary_expression_or_higher_recursive(
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

            let next_token =
                matches!(current_token, TokenKind::Is | TokenKind::Not).then(|| self.peek());
            let Some(operator) = BinaryLikeOperator::try_from_tokens(current_token, next_token)
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
                BinaryLikeOperator::Boolean(bool_op) => id::Expr::from(
                    self.parse_boolean_expression(left.expr, start, bool_op, context),
                ),
                BinaryLikeOperator::Comparison(cmp_op) => id::Expr::from(
                    self.parse_comparison_expression(left.expr, start, cmp_op, context),
                ),
                BinaryLikeOperator::Binary(bin_op) => {
                    self.bump(TokenKind::from(bin_op));

                    let right = self.parse_binary_expression_or_higher(new_precedence, context);

                    let [left, right] = [left.expr, right.expr];
                    id::Expr::from(
                        build::ExprBinOp {
                            left: left.id,
                            op: bin_op,
                            right: right.id,
                            range: self.node_range(start),
                        }
                        .parsed(&mut self.storage),
                    )
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
        let token = self.current_token_kind();

        if token != TokenKind::Identifier {
            if let Some(unary_op) = token.as_unary_operator() {
                let expr = self.parse_unary_expression(unary_op, context);

                if matches!(unary_op, UnaryOp::Not) {
                    if left_precedence > OperatorPrecedence::Not {
                        self.add_error(
                            ParseErrorType::OtherError(
                                "Boolean 'not' expression cannot be used here".to_string(),
                            ),
                            expr,
                        );
                    }
                } else {
                    // > The power operator `**` binds less tightly than an arithmetic
                    // > or bitwise unary operator on its right, that is, 2**-1 is 0.5.
                    //
                    // Reference: https://docs.python.org/3/reference/expressions.html#id21
                    if left_precedence > OperatorPrecedence::PosNegBitNot
                        && left_precedence != OperatorPrecedence::Exponent
                    {
                        self.add_error(
                            ParseErrorType::OtherError(format!(
                                "Unary '{unary_op}' expression cannot be used here",
                            )),
                            expr,
                        );
                    }
                }

                return id::Expr::from(expr).into();
            }

            match token {
                TokenKind::Star => {
                    let starred_expr = self.parse_starred_expression(context);

                    if left_precedence > OperatorPrecedence::None
                        || !context.is_starred_expression_allowed()
                    {
                        self.add_error(ParseErrorType::InvalidStarredExpressionUsage, starred_expr);
                    }

                    return id::Expr::from(starred_expr).into();
                }
                TokenKind::Await => {
                    let await_expr = self.parse_await_expression();

                    // `await` expressions cannot be nested
                    if left_precedence >= OperatorPrecedence::Await {
                        self.add_error(
                            ParseErrorType::OtherError(
                                "Await expression cannot be used here".to_string(),
                            ),
                            await_expr,
                        );
                    }

                    return id::Expr::from(await_expr).into();
                }
                TokenKind::Lambda => {
                    // Lambda expression isn't allowed in this context but we'll still parse it and
                    // report an error for better recovery.
                    let lambda_expr = self.parse_lambda_expr();
                    self.add_error(ParseErrorType::InvalidLambdaExpressionUsage, lambda_expr);
                    return id::Expr::from(lambda_expr).into();
                }
                TokenKind::Yield => {
                    let expr = self.parse_yield_expression();

                    if left_precedence > OperatorPrecedence::None
                        || !context.is_yield_expression_allowed()
                    {
                        self.add_error(ParseErrorType::InvalidYieldExpressionUsage, expr);
                    }

                    return expr.into();
                }
                _ => {}
            }
        }

        self.parse_primary_expression(context)
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

        let expr_name = match parsed_expr.expr.view(&self.storage).classify() {
            flat::ExprKind::Compare(_) => "Comparison",
            flat::ExprKind::BoolOp(_) => "Boolean",
            flat::ExprKind::UnaryOp(node) if node.op() == ast::UnaryOp::Not => "Boolean",
            flat::ExprKind::If(_) => "Conditional",
            flat::ExprKind::Lambda(_) => "Lambda",
            _ => return parsed_expr,
        };

        self.add_error(
            ParseErrorType::OtherError(format!("{expr_name} expression cannot be used here")),
            parsed_expr,
        );

        parsed_expr
    }

    /// Parses a name.
    ///
    /// For an invalid name, the `id` field will be an empty string and the `ctx`
    /// field will be [`ExprContext::Invalid`].
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#atom-identifiers>
    pub(super) fn parse_name(&mut self, context: ExpressionContext) -> id::ExprName {
        let value = self.parse_identifier_with_context(context);
        let ctx = if value.range.is_empty() {
            ExprContext::Invalid
        } else {
            ExprContext::Load
        };
        build::ExprName {
            range: value.range,
            id: value.id,
            ctx,
        }
        .parsed(&mut self.storage)
    }

    pub(super) fn parse_missing_name(&mut self) -> id::ExprName {
        let value = self.missing_identifier();
        build::ExprName {
            range: value.range,
            id: value.id,
            ctx: ExprContext::Invalid,
        }
        .parsed(&mut self.storage)
    }

    /// Parses an identifier.
    ///
    /// For an invalid identifier, the `id` field will be an empty string.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#atom-identifiers>
    pub(super) fn parse_identifier(&mut self) -> id::Identifier {
        self.parse_identifier_with_context(ExpressionContext::default())
            .parsed(&mut self.storage)
    }

    pub(super) fn parse_identifier_with_context(
        &mut self,
        context: ExpressionContext,
    ) -> build::Identifier {
        let range = self.current_token_range();

        if self.at(TokenKind::Identifier) {
            let name = self.bump_identifier();
            return build::Identifier { id: name, range };
        }

        if self.current_token_kind().is_soft_keyword() {
            let text = self.src_text(range);
            let id = self.intern_name(text);
            self.bump_soft_keyword_as_identifier();
            return build::Identifier { id, range };
        }

        // test_err incomplete_attribute_before_for_in_delimiter
        // [item. for item in xs]
        // [item. async for item in xs]
        // {item. for item in xs}
        // (item. for item in xs)
        // [item for item. in xs]
        // for item. in xs: ...
        if (context.is_for_excluded())
            && (self.at(TokenKind::For)
                || (self.at(TokenKind::Async) && self.peek() == TokenKind::For))
            || (context.is_in_excluded() && self.at(TokenKind::In))
        {
            return self.missing_identifier();
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

            let text = self.src_text(range);
            let id = self.intern_name(text);
            self.bump_any();
            build::Identifier { id, range }
        } else {
            self.missing_identifier()
        }
    }

    fn missing_identifier(&mut self) -> build::Identifier {
        self.add_error(
            ParseErrorType::OtherError("Expected an identifier".into()),
            self.current_token_range(),
        );

        build::Identifier {
            id: self.storage.intern_name(""),
            range: self.missing_node_range(),
        }
    }

    /// Parses an atom and any following calls, subscripts, or attributes.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#primaries>
    fn parse_primary_expression(&mut self, context: ExpressionContext) -> ParsedExpr {
        let mut is_parenthesized = false;
        let start = self.node_start();

        let mut lhs = match self.current_token_kind() {
            TokenKind::Float => {
                let value = self.bump_float();

                id::Expr::from(
                    build::ExprNumberLiteral {
                        value: Number::Float(value),
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Complex => {
                let (real, imag) = self.bump_complex();
                id::Expr::from(
                    build::ExprNumberLiteral {
                        value: Number::Complex { real, imag },
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Int => {
                let value = self.bump_int();
                id::Expr::from(
                    build::ExprNumberLiteral {
                        value: Number::Int(value),
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::True => {
                self.bump(TokenKind::True);
                id::Expr::from(
                    build::ExprBooleanLiteral {
                        value: true,
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::False => {
                self.bump(TokenKind::False);
                id::Expr::from(
                    build::ExprBooleanLiteral {
                        value: false,
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::None => {
                self.bump(TokenKind::None);
                id::Expr::from(
                    build::ExprNoneLiteral {
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Ellipsis => {
                self.bump(TokenKind::Ellipsis);
                id::Expr::from(
                    build::ExprEllipsisLiteral {
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Identifier => id::Expr::from(self.parse_name(context)),
            TokenKind::IpyEscapeCommand => {
                id::Expr::from(self.parse_ipython_escape_command_expression())
            }
            TokenKind::String | TokenKind::FStringStart | TokenKind::TStringStart => {
                self.parse_strings()
            }
            TokenKind::Lpar => {
                let parsed = self.parse_parenthesized_expression();
                is_parenthesized = parsed.is_parenthesized;
                parsed.expr
            }
            TokenKind::Lsqb => self.parse_list_like_expression(),
            TokenKind::Lbrace => self.parse_set_or_dict_like_expression(),

            kind => {
                if kind.is_keyword() {
                    id::Expr::from(self.parse_name(context))
                } else {
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    id::Expr::from(
                        build::ExprName {
                            range: self.missing_node_range(),
                            id: self.storage.intern_name(""),
                            ctx: ExprContext::Invalid,
                        }
                        .parsed(&mut self.storage),
                    )
                }
            }
        };

        loop {
            lhs = match self.current_token_kind() {
                TokenKind::Lpar => id::Expr::from(self.parse_call_expression(lhs, start)),
                TokenKind::Lsqb => id::Expr::from(self.parse_subscript_expression(lhs, start)),
                TokenKind::Dot => {
                    id::Expr::from(self.parse_attribute_expression(lhs, start, context))
                }
                _ => break,
            };
        }

        ParsedExpr {
            expr: lhs,
            is_parenthesized,
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
    fn parse_call_expression(&mut self, func: id::Expr, start: TextSize) -> id::ExprCall {
        let arguments = self.parse_arguments(ArgumentsContext::Call);
        debug_assert_eq!(self.node_range(start).end(), arguments.end());

        build::ExprCall {
            func: func.id,
            arguments: arguments.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an argument list.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `(` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#grammar-token-python-grammar-argument_list>
    pub(super) fn parse_arguments(&mut self, context: ArgumentsContext) -> id::Arguments {
        let start = self.node_start();
        self.bump(TokenKind::Lpar);

        if self.eat(TokenKind::Rpar) {
            return build::Arguments {
                range: self.node_range(start),

                args: flat::ListId::default(),
                keywords: flat::ListId::default(),
            }
            .parsed(&mut self.storage);
        }

        let args_snapshot = self.expr_scratch.snapshot();
        let keywords_snapshot = self.keyword_scratch.snapshot();
        let mut seen_keyword_argument = false; // foo = 1
        let mut seen_keyword_unpacking = false; // **foo

        let has_trailing_comma =
            self.parse_comma_separated_list(RecoveryContextKind::Arguments, |parser| {
                let argument_start = parser.node_start();
                if parser.eat(TokenKind::DoubleStar) {
                    parser.parse_keyword_argument(argument_start, None);
                    seen_keyword_unpacking = true;
                } else {
                    let start = parser.node_start();
                    let mut parsed_expr = parser
                        .parse_named_expression_or_higher(ExpressionContext::starred_conditional());

                    match parser.current_token_kind() {
                        TokenKind::Async | TokenKind::For => {
                            if parsed_expr.is_unparenthesized_starred_expr() {
                                parser.add_unsupported_syntax_error(
                                    UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                                        ComprehensionUnpackingKind::IterableInGenerator,
                                    ),
                                    parsed_expr.range(),
                                );
                            }

                            parsed_expr = id::Expr::from(parser.parse_generator_expression(
                                parsed_expr.expr,
                                start,
                                Parenthesized::No,
                            ))
                            .into();
                        }
                        _ => {
                            if seen_keyword_unpacking
                                && parsed_expr.is_unparenthesized_starred_expr()
                            {
                                parser.add_error(
                                    ParseErrorType::InvalidArgumentUnpackingOrder,
                                    parsed_expr,
                                );
                            }
                        }
                    }

                    let arg_range = parser.node_range(start);
                    if parser.eat(TokenKind::Equal) {
                        seen_keyword_argument = true;
                        let arg = if let Some(ident_expr) = parsed_expr
                            .expr
                            .view(&parser.storage)
                            .cast::<ast::ExprName>()
                        {
                            // test_ok parenthesized_kwarg_py37
                            // # parse_options: {"target-version": "3.7"}
                            // f((a)=1)

                            // test_err parenthesized_kwarg_py38
                            // # parse_options: {"target-version": "3.8"}
                            // f((a)=1)
                            // f((a) = 1)
                            // f( ( a ) = 1)

                            let name = ident_expr.build_id();
                            if parsed_expr.is_parenthesized {
                                parser.add_unsupported_syntax_error(
                                    UnsupportedSyntaxErrorKind::ParenthesizedKeywordArgumentName,
                                    arg_range,
                                );
                            }

                            parser.storage.take_name(name).parsed(&mut parser.storage)
                        } else {
                            // TODO(dhruvmanila): Parser shouldn't drop the `parsed_expr` if it's
                            // not a name expression. We could add the expression into `args` but
                            // that means the error is a missing comma instead.
                            parser.add_error(
                                ParseErrorType::OtherError("Expected a parameter name".to_string()),
                                parsed_expr,
                            );
                            build::Identifier {
                                id: parser.storage.intern_name(""),
                                range: parsed_expr.range(),
                            }
                            .parsed(&mut parser.storage)
                        };

                        parser.parse_keyword_argument(argument_start, Some(arg));
                    } else {
                        if !parsed_expr.is_unparenthesized_starred_expr() {
                            if seen_keyword_unpacking {
                                parser.add_error(
                                    ParseErrorType::PositionalAfterKeywordUnpacking,
                                    parsed_expr,
                                );
                            } else if seen_keyword_argument {
                                parser.add_error(
                                    ParseErrorType::PositionalAfterKeywordArgument,
                                    parsed_expr,
                                );
                            }
                        }
                        parser.expr_scratch.push(parsed_expr.expr.id);
                    }
                }
            });

        self.expect(TokenKind::Rpar);

        let keywords = self
            .keyword_scratch
            .take_range(keywords_snapshot, &mut self.storage);
        let arguments = build::Arguments {
            range: self.node_range(start),

            args: self
                .expr_scratch
                .take_range(args_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
            keywords: keywords.into_list(&mut self.storage),
        }
        .parsed(&mut self.storage);

        self.validate_arguments(&arguments, has_trailing_comma, context);

        arguments
    }

    fn parse_keyword_argument(&mut self, start: TextSize, name: Option<id::Identifier>) {
        let value = self.parse_conditional_expression_or_higher();
        self.keyword_scratch.push(
            build::Keyword {
                arg: name.map(|node| node.id),
                value: value.expr.id,
                range: self.node_range(start),
            }
            .parsed(&mut self.storage)
            .id,
        );
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
        value: id::Expr,
        start: TextSize,
    ) -> id::ExprSubscript {
        self.bump(TokenKind::Lsqb);

        // To prevent the `value` context from being `Del` within a `del` statement,
        // we set the context as `Load` here.
        self.storage.set_expr_context(value.id, ExprContext::Load);

        // Slice range doesn't include the `[` token.
        let slice_start = self.node_start();

        // Create an error when receiving an empty slice to parse, e.g. `x[]`
        if self.eat(TokenKind::Rsqb) {
            let slice_range = self.node_range(slice_start);
            self.add_error(ParseErrorType::EmptySlice, slice_range);

            return build::ExprSubscript {
                value: value.id,
                slice: id::Expr::from(
                    build::ExprName {
                        range: slice_range,
                        id: self.storage.intern_name(""),
                        ctx: ExprContext::Invalid,
                    }
                    .parsed(&mut self.storage),
                )
                .id,
                ctx: ExprContext::Load,
                range: self.node_range(start),
            }
            .parsed(&mut self.storage);
        }

        let mut slice = self.parse_slice();

        // If there are more than one element in the slice, we need to create a tuple
        // expression to represent it.
        if self.eat(TokenKind::Comma) {
            let slices_snapshot = self.expr_scratch.snapshot();
            self.expr_scratch.push(slice.id);

            self.parse_comma_separated_list(RecoveryContextKind::Slices, |parser| {
                let slice = parser.parse_slice();
                parser.expr_scratch.push(slice.id);
            });

            slice = id::Expr::from(
                build::ExprTuple {
                    elts: self
                        .expr_scratch
                        .take_range(slices_snapshot, &mut self.storage)
                        .into_list(&mut self.storage),
                    ctx: ExprContext::Load,
                    range: self.node_range(slice_start),
                    parenthesized: false,
                }
                .parsed(&mut self.storage),
            );
        } else if slice.is_starred_expr() {
            // If the only slice element is a starred expression, that is represented
            // using a tuple expression with a single element. This is the second case
            // in the `slices` rule in the Python grammar.
            slice = id::Expr::from(
                build::ExprTuple {
                    elts: self.storage.list([slice]),
                    ctx: ExprContext::Load,
                    range: self.node_range(slice_start),
                    parenthesized: false,
                }
                .parsed(&mut self.storage),
            );
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
        if let Some(tuple) = slice.view(&self.storage).cast::<ast::ExprTuple>()
            && !tuple.parenthesized()
        {
            let elts = tuple.elts().id();
            for index in 0..elts.len() {
                if let Some(elt) = self.storage.get_list(elts).get(index)
                    && elt.node_kind() == ast::NodeKind::ExprStarred
                {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::StarExpressionInIndex,
                        elt.range(),
                    );
                }
            }
        }

        let [value, slice] = [value, slice];
        build::ExprSubscript {
            value: value.id,
            slice: slice.id,
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a slice expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#slicings>
    fn parse_slice(&mut self) -> id::Expr {
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
                match lower.expr.view(&self.storage).classify() {
                    flat::ExprKind::Starred(_) => {
                        self.add_error(ParseErrorType::InvalidStarredExpressionUsage, lower);
                    }
                    flat::ExprKind::Named(_) => {
                        self.add_error(ParseErrorType::UnparenthesizedNamedExpression, lower);
                    }
                    _ => {}
                }
            }

            Some(lower.expr)
        } else {
            None
        };

        self.expect(TokenKind::Colon);

        let upper = if self.at_ts(UPPER_END_SET) {
            None
        } else {
            Some(self.parse_conditional_expression_or_higher().expr)
        };

        let step = if self.eat(TokenKind::Colon) {
            if self.at_ts(STEP_END_SET) {
                None
            } else {
                Some(self.parse_conditional_expression_or_higher().expr)
            }
        } else {
            None
        };

        id::Expr::from(
            build::ExprSlice {
                range: self.node_range(start),
                lower: lower.map(|node| node.id),
                upper: upper.map(|node| node.id),
                step: step.map(|node| node.id),
            }
            .parsed(&mut self.storage),
        )
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
    ) -> id::ExprUnaryOp {
        let start = self.node_start();
        self.bump(TokenKind::from(op));

        let operand = self.parse_binary_expression_or_higher(OperatorPrecedence::from(op), context);

        build::ExprUnaryOp {
            op,
            operand: operand.expr.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
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
        value: id::Expr,
        start: TextSize,
        context: ExpressionContext,
    ) -> id::ExprAttribute {
        self.bump(TokenKind::Dot);

        let attr = self
            .parse_identifier_with_context(context)
            .parsed(&mut self.storage);

        build::ExprAttribute {
            value: value.id,
            attr: attr.id,
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
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
        lhs: id::Expr,
        start: TextSize,
        op: BoolOp,
        context: ExpressionContext,
    ) -> id::ExprBoolOp {
        self.bump(TokenKind::from(op));

        let values_snapshot = self.expr_scratch.snapshot();
        self.expr_scratch.push(lhs.id);
        let mut progress = ParserProgress::default();

        // Keep adding the expression to `values` until we see a different
        // token than `operator_token`.
        loop {
            progress.assert_progressing(self);

            let parsed_expr =
                self.parse_binary_expression_or_higher(OperatorPrecedence::from(op), context);
            self.expr_scratch.push(parsed_expr.expr.id);

            if !self.eat(TokenKind::from(op)) {
                break;
            }
        }

        build::ExprBoolOp {
            values: self
                .expr_scratch
                .take_range(values_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
            op,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
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
        lhs: id::Expr,
        start: TextSize,
        op: CmpOp,
        context: ExpressionContext,
    ) -> id::ExprCompare {
        self.bump_cmp_op(op);

        let operands_snapshot = self.expr_scratch.snapshot();
        self.expr_scratch.push(lhs.id);
        let mut operators = thin_vec::thin_vec![op];

        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            let comparator = self
                .parse_binary_expression_or_higher(
                    OperatorPrecedence::ComparisonsMembershipIdentity,
                    context,
                )
                .expr;
            self.expr_scratch.push(comparator.id);

            let next_token = self.current_token_kind();
            if matches!(next_token, TokenKind::In) && context.is_in_excluded() {
                break;
            }

            let next_next_token =
                matches!(next_token, TokenKind::Is | TokenKind::Not).then(|| self.peek());
            let Some(next_op) = helpers::token_kind_to_cmp_op(next_token, next_next_token) else {
                break;
            };

            self.bump_cmp_op(next_op);
            operators.push(next_op);
        }

        build::ExprCompare {
            ops: operators.into_list(&mut self.storage),
            operands: self
                .expr_scratch
                .take_range(operands_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses all kinds of strings and implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `String`, `FStringStart`, or `TStringStart` token.
    ///
    /// See: <https://docs.python.org/3/reference/grammar.html> (Search "strings:")
    pub(super) fn parse_strings(&mut self) -> id::Expr {
        const STRING_START_SET: TokenSet = TokenSet::new([
            TokenKind::String,
            TokenKind::FStringStart,
            TokenKind::TStringStart,
        ]);

        let start = self.node_start();
        let first = self.parse_string();

        if !self.at_ts(STRING_START_SET) {
            let range = first.range();

            return match first {
                StringType::Str(value) => build::ExprStringLiteral {
                    range,

                    value: self.storage.list([value]),
                }
                .parsed(&mut self.storage)
                .into(),
                StringType::Bytes(value) => build::ExprBytesLiteral {
                    range,

                    value: self.storage.list([value]),
                }
                .parsed(&mut self.storage)
                .into(),
                StringType::FString(value) => build::ExprFString {
                    range,

                    value: self.storage.list([flat::FStringPartId::FString(value.id)]),
                }
                .parsed(&mut self.storage)
                .into(),
                StringType::TString(value) => build::ExprTString {
                    range,

                    value: self.storage.list([value]),
                }
                .parsed(&mut self.storage)
                .into(),
            };
        }

        let mut strings = Vec::with_capacity(2);
        strings.push(first);

        let mut progress = ParserProgress::default();

        while self.at_ts(STRING_START_SET) {
            progress.assert_progressing(self);
            strings.push(self.parse_string());
        }

        let range = self.node_range(start);
        self.handle_implicitly_concatenated_strings(strings, range)
    }

    /// Parses a single string, byte literal, f-string, or t-string.
    fn parse_string(&mut self) -> StringType {
        if self.at(TokenKind::String) {
            self.parse_string_or_byte_literal()
        } else if self.at(TokenKind::FStringStart) {
            StringType::FString(
                self.parse_interpolated_string(InterpolatedStringKind::FString)
                    .into_fstring(&mut self.storage),
            )
        } else if self.at(TokenKind::TStringStart) {
            // test_ok template_strings_py314
            // # parse_options: {"target-version": "3.14"}
            // t"{hey}"
            // t'{there}'
            // t"""what's
            // happening?"""

            // test_err template_strings_py313
            // # parse_options: {"target-version": "3.13"}
            // t"{hey}"
            // t'{there}'
            // t"""what's
            // happening?"""
            let string_type = StringType::TString(
                self.parse_interpolated_string(InterpolatedStringKind::TString)
                    .into_tstring(&mut self.storage),
            );
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::TemplateStrings,
                string_type.range(),
            );
            string_type
        } else {
            unreachable!("Expected to parse a string")
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
    ) -> id::Expr {
        assert!(strings.len() > 1);

        let mut has_fstring = false;
        let mut byte_literal_count = 0;
        let mut tstring_count = 0;
        for string in &strings {
            match string {
                StringType::FString(_) => has_fstring = true,
                StringType::TString(_) => tstring_count += 1,
                StringType::Bytes(_) => byte_literal_count += 1,
                StringType::Str(_) => {}
            }
        }
        let has_bytes = byte_literal_count > 0;
        let has_tstring = tstring_count > 0;

        if has_bytes {
            if byte_literal_count < strings.len() {
                // TODO(dhruvmanila): This is not an ideal recovery because the parser
                // replaces the byte literals with an invalid string literal node. Any
                // downstream tools can extract the raw bytes from the range.
                //
                // We could convert the node into a string and mark it as invalid
                // and would be clever to mark the type which is fewer in quantity.

                // test_err mixed_tstring_and_bytes_literals
                // t'first' b'second'
                // b'first' t'second'
                // t'first' br'second'
                // 'first' b'second' t'third'
                // b'first' 'second' t'third'
                // 'first' t'second' 'third' b'fourth'
                // b'first' t'second' f'third'

                // test_err mixed_bytes_and_non_bytes_literals
                // 'first' b'second'
                // f'first' b'second'
                // 'first' f'second' b'third'
                self.report_mixed_string_literal_error(&strings, range);
            }
            // Only construct a byte expression if all the literals are bytes
            // otherwise, we'll try either string, t-string, or f-string. This is to retain
            // as much information as possible.
            else {
                let mut values = Vec::with_capacity(strings.len());
                for string in strings {
                    values.push(match string {
                        StringType::Bytes(value) => value,
                        _ => unreachable!("Expected `StringType::Bytes`"),
                    });
                }
                return id::Expr::from(
                    build::ExprBytesLiteral {
                        value: self.storage.list(values),
                        range,
                    }
                    .parsed(&mut self.storage),
                );
            }
        } else if has_tstring {
            if tstring_count < strings.len() {
                self.report_mixed_string_literal_error(&strings, range);
            }
            // Only construct a t-string expression if all the literals are t-strings
            // otherwise, we'll try either string or f-string. This is to retain
            // as much information as possible.
            else {
                let mut values = Vec::with_capacity(strings.len());
                for string in strings {
                    values.push(match string {
                        StringType::TString(value) => value,
                        _ => unreachable!("Expected `StringType::TString`"),
                    });
                }
                return id::Expr::from(
                    build::ExprTString {
                        value: self.storage.list(values),
                        range,
                    }
                    .parsed(&mut self.storage),
                );
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

        if !has_fstring && !has_tstring {
            let mut values = Vec::with_capacity(strings.len());
            for string in strings {
                values.push(match string {
                    StringType::Str(value) => value,
                    _ => self.invalid_string_literal(string.range()),
                });
            }
            return id::Expr::from(
                build::ExprStringLiteral {
                    value: self.storage.list(values),
                    range,
                }
                .parsed(&mut self.storage),
            );
        }

        let mut parts = Vec::with_capacity(strings.len());
        for string in strings {
            match string {
                StringType::FString(fstring) => {
                    parts.push(flat::FStringPartId::FString(fstring.id));
                }
                StringType::Str(string) => parts.push(flat::FStringPartId::Literal(string.id)),
                // Bytes and Template strings are invalid at this point
                // and stored as invalid string literal parts in the
                // f-string
                StringType::TString(tstring) => parts.push(flat::FStringPartId::Literal(
                    self.invalid_string_literal(tstring.range()).id,
                )),
                StringType::Bytes(bytes) => parts.push(flat::FStringPartId::Literal(
                    self.invalid_string_literal(bytes.range()).id,
                )),
            }
        }

        id::Expr::from(
            build::ExprFString {
                value: self.storage.list(parts),
                range,
            }
            .parsed(&mut self.storage),
        )
    }

    fn invalid_string_literal(&mut self, range: TextRange) -> id::StringLiteral {
        build::StringLiteral {
            range,
            value: "",
            flags: ast::StringLiteralFlags::empty().with_invalid(),
        }
        .parsed(&mut self.storage)
    }

    fn report_mixed_string_literal_error(&mut self, strings: &[StringType], range: TextRange) {
        // CPython reports the first incompatible pair. A t-string mismatch takes
        // precedence over a bytes mismatch within that pair.
        for pair in strings.windows(2) {
            let message = match pair {
                [StringType::TString(_), StringType::TString(_)]
                | [StringType::Bytes(_), StringType::Bytes(_)] => continue,
                [StringType::TString(_), _] | [_, StringType::TString(_)] => {
                    "Cannot mix t-string literals with string or bytes literals"
                }
                [StringType::Bytes(_), _] | [_, StringType::Bytes(_)] => {
                    "Bytes literal cannot be mixed with non-bytes literals"
                }
                _ => continue,
            };
            self.add_error(ParseErrorType::OtherError(message.to_string()), range);
            break;
        }
    }

    /// Parses a single string or byte literal.
    ///
    /// This does not handle implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `String` token.
    ///
    /// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
    fn parse_string_or_byte_literal(&mut self) -> StringType {
        let range = self.current_token_range();
        let flags = self.tokens.current_flags().as_any_string_flags();

        let value = self.bump_string_value();

        match parse_string_literal(&mut self.storage, value, flags, range) {
            Ok(string) => string,
            Err(error) => {
                let location = error.location();
                self.add_error(ParseErrorType::Lexical(error.into_error()), location);

                if flags.is_byte_string() {
                    // test_err invalid_byte_literal
                    // b'123a𝐁c'
                    // rb"a𝐁c123"
                    // b"""123a𝐁c"""
                    StringType::Bytes(
                        build::BytesLiteral {
                            value: &[],
                            range,
                            flags: ast::BytesLiteralFlags::from(flags).with_invalid(),
                        }
                        .parsed(&mut self.storage),
                    )
                } else {
                    // test_err invalid_string_literal
                    // 'hello \N{INVALID} world'
                    // """hello \N{INVALID} world"""
                    StringType::Str(
                        build::StringLiteral {
                            value: "",
                            range,
                            flags: ast::StringLiteralFlags::from(flags).with_invalid(),
                        }
                        .parsed(&mut self.storage),
                    )
                }
            }
        }
    }

    /// Parses an f/t-string.
    ///
    /// This does not handle implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `FStringStart` or
    /// `TStringStart` token.
    ///
    /// See: <https://docs.python.org/3/reference/grammar.html> (Search "fstring:" or "tstring:")
    /// See: <https://docs.python.org/3/reference/lexical_analysis.html#formatted-string-literals>
    fn parse_interpolated_string(
        &mut self,
        kind: InterpolatedStringKind,
    ) -> InterpolatedStringData {
        let start = self.node_start();
        let mut flags = self.tokens.current_flags().as_any_string_flags();

        self.bump(kind.start_token());
        let elements = self.parse_interpolated_string_elements(
            flags,
            InterpolatedStringElementsKind::Regular(kind),
            kind,
        );

        if !self.expect(kind.end_token()) {
            flags = flags.with_unclosed(true);
        }

        InterpolatedStringData {
            elements,
            range: self.node_range(start),
            flags,
        }
    }

    /// Check `range` for comment tokens, report an `UnsupportedSyntaxError` for each one found,
    /// and return whether any comments were found.
    fn check_fstring_comments(&mut self, range: TextRange) -> bool {
        let mut has_comments = false;

        self.unsupported_syntax_errors.extend(
            self.tokens
                .in_range(range)
                .iter()
                .filter(|token| token.kind().is_comment())
                .map(|token| {
                    has_comments = true;
                    UnsupportedSyntaxError {
                        kind: UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::Comment),
                        range: token.range(),
                        target_version: self.options.target_version,
                    }
                }),
        );

        has_comments
    }

    /// Parses a list of f/t-string elements.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{`, `FStringMiddle`,
    /// or `TStringMiddle` token.
    fn parse_interpolated_string_elements(
        &mut self,
        flags: ast::AnyStringFlags,
        elements_kind: InterpolatedStringElementsKind,
        string_kind: InterpolatedStringKind,
    ) -> flat::ListId<flat::Handle<ast::InterpolatedStringElement>> {
        let mut elements = vec![];
        let middle_token_kind = string_kind.middle_token();

        self.parse_list(
            RecoveryContextKind::InterpolatedStringElements(elements_kind),
            |parser| {
                let element = match parser.current_token_kind() {
                    TokenKind::Lbrace => id::InterpolatedStringElement::from(
                        parser.parse_interpolated_element(flags, string_kind),
                    ),
                    tok if tok == middle_token_kind => {
                        let range = parser.current_token_range();
                        let value = parser.current_token_text();
                        parser.bump(middle_token_kind);
                        id::InterpolatedStringElement::from(
                            parse_interpolated_string_literal_element(
                                &mut parser.storage,
                                value,
                                flags,
                                range,
                            )
                            .unwrap_or_else(|lex_error| {
                                // test_err invalid_fstring_literal_element
                                // f'hello \N{INVALID} world'
                                // f"""hello \N{INVALID} world"""
                                let location = lex_error.location();
                                parser.add_error(
                                    ParseErrorType::Lexical(lex_error.into_error()),
                                    location,
                                );
                                build::InterpolatedStringLiteralElement { value: "", range }
                                    .parsed(&mut parser.storage)
                            }),
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
                            "{}: unexpected token `{tok:?}` at {:?}",
                            string_kind,
                            parser.current_token_range()
                        );
                    }
                };
                elements.push(element);
            },
        );

        self.storage.list(elements)
    }

    /// Parses an f/t-string expression element.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` token.
    fn parse_interpolated_element(
        &mut self,
        flags: ast::AnyStringFlags,
        string_kind: InterpolatedStringKind,
    ) -> id::InterpolatedElement {
        let start = self.node_start();
        self.bump(TokenKind::Lbrace);

        self.tokens
            .re_lex_string_token_in_interpolation_element(string_kind);

        // test_err f_string_empty_expression
        // f"{}"
        // f"{  }"

        // test_err t_string_empty_expression
        // # parse_options: {"target-version": "3.14"}
        // t"{}"
        // t"{  }"

        // test_err f_string_invalid_starred_expr
        // # Starred expression inside f-string has a minimum precedence of bitwise or.
        // f"{*}"
        // f"{*x and y}"
        // f"{*yield x}"

        // test_err t_string_invalid_starred_expr
        // # parse_options: {"target-version": "3.14"}
        // # Starred expression inside t-string has a minimum precedence of bitwise or.
        // t"{*}"
        // t"{*x and y}"
        // t"{*yield x}"

        let value = self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

        if !value.is_parenthesized && value.expr.is_lambda_expr() {
            // TODO(dhruvmanila): This requires making some changes in lambda expression
            // parsing logic to handle the emitted `FStringMiddle` token in case the
            // lambda expression is not parenthesized.

            // test_err f_string_lambda_without_parentheses
            // f"{lambda x: x}"

            // test_err t_string_lambda_without_parentheses
            // # parse_options: {"target-version": "3.14"}
            // t"{lambda x: x}"
            self.add_error(
                ParseErrorType::from_interpolated_string_error(
                    InterpolatedStringErrorType::LambdaWithoutParentheses,
                    string_kind,
                ),
                value.range(),
            );
        }
        let debug_text = if self.eat(TokenKind::Equal) {
            let leading_range = TextRange::new(start + "{".text_len(), value.start());
            let trailing_range = TextRange::new(value.end(), self.current_token_range().start());
            Some(ast::DebugText::new(
                self.src_text(leading_range),
                self.src_text(value.range()),
                self.src_text(trailing_range),
            ))
        } else {
            None
        };

        let conversion = if self.eat(TokenKind::Exclamation) {
            // Ensure that the `r` is lexed as an identifier token instead of a raw string
            // in `f{abc!r"` (note the missing `}`).
            self.tokens.re_lex_raw_string_in_format_spec();

            let conversion_flag_range = self.current_token_range();
            if self.at(TokenKind::Identifier) {
                // test_err f_string_conversion_follows_exclamation
                // f"{x! s}"
                // t"{x! s}"
                // f"{x! z}"
                if self.prev_token_end != conversion_flag_range.start() {
                    self.add_error(
                        ParseErrorType::from_interpolated_string_error(
                            InterpolatedStringErrorType::ConversionFlagNotImmediatelyAfterExclamation,
                            string_kind,
                        ),
                        TextRange::new(self.prev_token_end, conversion_flag_range.start()),
                    );
                }
                let name = self.bump_identifier();
                match self.storage.name(name).as_str() {
                    "s" => ConversionFlag::Str,
                    "r" => ConversionFlag::Repr,
                    "a" => ConversionFlag::Ascii,
                    _ => {
                        // test_err f_string_invalid_conversion_flag_name_tok
                        // f"{x!z}"

                        // test_err t_string_invalid_conversion_flag_name_tok
                        // # parse_options: {"target-version": "3.14"}
                        // t"{x!z}"
                        self.add_error(
                            ParseErrorType::from_interpolated_string_error(
                                InterpolatedStringErrorType::InvalidConversionFlag,
                                string_kind,
                            ),
                            conversion_flag_range,
                        );
                        ConversionFlag::None
                    }
                }
            } else {
                // test_err f_string_invalid_conversion_flag_other_tok
                // f"{x!123}"
                // f"{x!'a'}"

                // test_err t_string_invalid_conversion_flag_other_tok
                // # parse_options: {"target-version": "3.14"}
                // t"{x!123}"
                // t"{x!'a'}"
                self.add_error(
                    ParseErrorType::from_interpolated_string_error(
                        InterpolatedStringErrorType::InvalidConversionFlag,
                        string_kind,
                    ),
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
            let elements = self.with_recursion(|parser| {
                parser.parse_interpolated_string_elements(
                    flags,
                    InterpolatedStringElementsKind::FormatSpec(string_kind),
                    string_kind,
                )
            });
            Some(
                build::InterpolatedStringFormatSpec {
                    range: self.node_range(spec_start),
                    elements: elements.into_list(&mut self.storage),
                }
                .parsed(&mut self.storage),
            )
        } else {
            None
        };

        self.tokens
            .re_lex_string_token_in_interpolation_element(string_kind);

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

            // test_err t_string_unclosed_lbrace
            // # parse_options: {"target-version": "3.14"}
            // t"{"
            // t"{foo!r"
            // t"{foo="
            // t"{"
            // t"""{"""

            // The lexer does emit `FStringEnd` for the following test cases:

            // test_err f_string_unclosed_lbrace_in_format_spec
            // f"hello {x:"
            // f"hello {x:.3f"

            // test_err t_string_unclosed_lbrace_in_format_spec
            // # parse_options: {"target-version": "3.14"}
            // t"hello {x:"
            // t"hello {x:.3f"
            self.add_error(
                ParseErrorType::from_interpolated_string_error(
                    InterpolatedStringErrorType::UnclosedLbrace,
                    string_kind,
                ),
                self.current_token_range(),
            );
        }

        // test_ok pep701_f_string_py312
        // # parse_options: {"target-version": "3.12"}
        // f'Magic wand: { bag['wand'] }'     # nested quotes
        // f"{'\n'.join(a)}"                  # escape sequence
        // f'''A complex trick: {
        //     bag['bag']                     # comment
        // }'''
        // f"{f"{f"{f"{f"{f"{1+1}"}"}"}"}"}"  # arbitrary nesting
        // f"{f'''{"nested"} inner'''} outer" # nested (triple) quotes
        // f"{
        //     1
        // }"
        // f"test {a \
        //     } more"                        # line continuation

        // test_ok pep750_t_string_py314
        // # parse_options: {"target-version": "3.14"}
        // t'Magic wand: { bag['wand'] }'     # nested quotes
        // t"{'\n'.join(a)}"                  # escape sequence
        // t'''A complex trick: {
        //     bag['bag']                     # comment
        // }'''
        // t"{t"{t"{t"{t"{t"{1+1}"}"}"}"}"}"  # arbitrary nesting
        // t"{t'''{"nested"} inner'''} outer" # nested (triple) quotes
        // t"test {a \
        //     } more"                        # line continuation

        // test_ok pep701_f_string_py311
        // # parse_options: {"target-version": "3.11"}
        // f"outer {'# not a comment'}"
        // f'outer {x:{"# not a comment"} }'
        // f"""{f'''{f'{"# not a comment"}'}'''}"""
        // f"""{f'''# before expression {f'# aro{f"#{1+1}#"}und #'}'''} # after expression"""
        // f"""{
        //     1
        // }"""
        // f"escape outside of \t {expr}\n"
        // f"test\"abcd"
        // f"{1:\x64}"  # escapes are valid in the format spec
        // f"{1:\"d\"}"  # this also means that escaped outer quotes are valid

        // test_err pep701_f_string_py311
        // # parse_options: {"target-version": "3.11"}
        // f'Magic wand: { bag['wand'] }'     # nested quotes
        // f"{'\n'.join(a)}"                  # escape sequence
        // f'''A complex trick: {
        //     bag['bag']                     # comment
        // }'''
        // f"{f"{f"{f"{f"{f"{1+1}"}"}"}"}"}"  # arbitrary nesting
        // f"{f'''{"nested"} inner'''} outer" # nested (triple) quotes
        // f"{
        //     1
        // }"
        // f"test {a \
        //     } more"                        # line continuation
        // f"""{f"""{x}"""}"""                # mark the whole triple quote
        // f"{'\n'.join(['\t', '\v', '\r'])}"  # multiple escape sequences, multiple errors

        // test_err pep701_nested_interpolation_py311
        // # parse_options: {"target-version": "3.11"}
        // # nested interpolations also need to be checked
        // f'{1: abcd "{'aa'}" }'
        // f'{1: abcd "{"\n"}" }'

        // test_err nested_quote_in_format_spec_py312
        // # parse_options: {"target-version": "3.12"}
        // f"{1:""}"  # this is a ParseError on all versions

        // test_ok non_nested_quote_in_format_spec_py311
        // # parse_options: {"target-version": "3.11"}
        // f"{1:''}"  # but this is okay on all versions
        let range = self.node_range(start);

        if !self.options.target_version.supports_pep_701()
            && matches!(string_kind, InterpolatedStringKind::FString)
        {
            // We need to check the whole expression range, including any leading or trailing
            // debug text, but exclude the format spec, where escapes and escaped, reused quotes
            // are allowed.
            let range = format_spec
                .as_ref()
                .map(|format_spec| TextRange::new(range.start(), format_spec.start()))
                .unwrap_or(range);

            let quote_bytes = flags.quote_str().as_bytes();
            let quote_len = flags.quote_len();
            let mut has_backslash_or_comment = false;

            for slash_position in memchr::memchr_iter(b'\\', self.source[range].as_bytes()) {
                has_backslash_or_comment = true;
                let slash_position = TextSize::try_from(slash_position).unwrap();
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::Backslash),
                    TextRange::at(range.start() + slash_position, '\\'.text_len()),
                );
            }

            if let Some(quote_position) =
                memchr::memmem::find(self.source[range].as_bytes(), quote_bytes)
            {
                let quote_position = TextSize::try_from(quote_position).unwrap();
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::NestedQuote),
                    TextRange::at(range.start() + quote_position, quote_len),
                );
            }

            has_backslash_or_comment |= self.check_fstring_comments(range);

            // Before Python 3.12, replacement fields could only span physical lines when the
            // outer f-string was triple-quoted.
            if !flags.is_triple_quoted()
                && !has_backslash_or_comment
                && memchr::memchr2(b'\n', b'\r', self.source[range].as_bytes()).is_some()
            {
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::LineBreak),
                    TextRange::at(range.start(), '{'.text_len()),
                );
            }
        }

        build::InterpolatedElement {
            expression: value.expr.id,
            debug_text,
            conversion,
            format_spec: format_spec.map(|node| node.id),
            range,
        }
        .parsed(&mut self.storage)
    }

    /// Parses a list or a list comprehension expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#list-displays>
    fn parse_list_like_expression(&mut self) -> id::Expr {
        let start = self.node_start();

        self.bump(TokenKind::Lsqb);

        // Nice error message when having a unclosed open bracket `[`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("Missing closing bracket `]`".to_string()),
                self.current_token_range(),
            );
        }

        // Return an empty `ListExpr` when finding a `]` right after the `[`
        if self.eat(TokenKind::Rsqb) {
            return id::Expr::from(
                build::ExprList {
                    elts: flat::ListId::default(),
                    ctx: ExprContext::Load,
                    range: self.node_range(start),
                }
                .parsed(&mut self.storage),
            );
        }

        // Parse the first element with a more general rule and limit it later.
        let first_element = self.parse_named_expression_or_higher(
            ExpressionContext::starred_bitwise_or().with_for_excluded(),
        );

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                // Parenthesized starred expression isn't allowed either but that is
                // handled by the `parse_parenthesized_expression` method.

                // test_ok starred_list_comp_py315
                // # parse_options: {"target-version": "3.15"}
                // [*x for x in y]
                // [*factor.dims for factor in bases]

                // test_err starred_list_comp_py314
                // # parse_options: {"target-version": "3.14"}
                // [*x for x in y]
                if first_element.is_unparenthesized_starred_expr() {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                            ComprehensionUnpackingKind::IterableInList,
                        ),
                        first_element.range(),
                    );
                }

                id::Expr::from(self.parse_list_comprehension_expression(first_element.expr, start))
            }
            _ => id::Expr::from(self.parse_list_expression(first_element.expr, start)),
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
    fn parse_set_or_dict_like_expression(&mut self) -> id::Expr {
        // test_ok pep_798_unpacking_comprehensions_py315
        // # parse_options: {"target-version": "3.15"}
        // [*x for x in y]
        // {*x for x in y}
        // {**x for x in y}
        // (*x for x in y)
        // f(*x for x in y)
        // [*x async for x in y]
        // {*x async for x in y}
        // {**x async for x in y}
        // (*x async for x in y)

        // test_err pep_798_unpacking_comprehensions_py314
        // # parse_options: {"target-version": "3.14"}
        // [*x for x in y]
        // {*x for x in y}
        // {**x for x in y}
        // (*x for x in y)
        // f(*x for x in y)

        // test_err pep_798_invalid_dict_unpacking_comprehensions_py315
        // # parse_options: {"target-version": "3.15"}
        // {*k: v for k, v in items}
        // {k: *v for k, v in items}
        // {**k: v for k, v in items}
        // {k: **v for k, v in items}

        let start = self.node_start();
        self.bump(TokenKind::Lbrace);

        // Nice error message when having a unclosed open brace `{`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("Missing closing brace `}`".to_string()),
                self.current_token_range(),
            );
        }

        // Return an empty `DictExpr` when finding a `}` right after the `{`
        if self.eat(TokenKind::Rbrace) {
            return id::Expr::from(
                build::ExprDict {
                    items: flat::ListId::default(),
                    range: self.node_range(start),
                }
                .parsed(&mut self.storage),
            );
        }

        let after_brace = self.node_start();

        if self.eat(TokenKind::DoubleStar) {
            // Handle dictionary unpacking. Here, the grammar is `'**' bitwise_or`
            // which requires limiting the expression.
            let value = self.parse_expression_with_bitwise_or_precedence();
            let unpack_range = TextRange::new(after_brace, value.range().end());

            if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                        ComprehensionUnpackingKind::DictInDict,
                    ),
                    unpack_range,
                );

                return id::Expr::from(
                    self.parse_dictionary_comprehension_expression(None, value.expr, start),
                );
            }

            if self.at(TokenKind::Colon) {
                self.add_error(ParseErrorType::InvalidStarredExpressionUsage, unpack_range);

                self.bump(TokenKind::Colon);
                let dict_value = self.parse_conditional_expression_or_higher();

                if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                    return id::Expr::from(self.parse_dictionary_comprehension_expression(
                        Some(value.expr),
                        dict_value.expr,
                        start,
                    ));
                }

                return id::Expr::from(self.parse_dictionary_expression(
                    Some(value.expr),
                    dict_value.expr,
                    start,
                ));
            }

            return id::Expr::from(self.parse_dictionary_expression(None, value.expr, start));
        }

        // For dictionary expressions, the key uses the `expression` rule while for
        // set expressions, the element uses the `star_expression` rule. So, use the
        // one that is more general and limit it later.
        let key_or_element = self.parse_named_expression_or_higher(
            ExpressionContext::starred_bitwise_or().with_for_excluded(),
        );

        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                if key_or_element.is_unparenthesized_starred_expr() {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                            ComprehensionUnpackingKind::IterableInSet,
                        ),
                        key_or_element.range(),
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

                id::Expr::from(self.parse_set_comprehension_expression(key_or_element.expr, start))
            }
            TokenKind::Colon => {
                // Now, we know that it's either a dictionary expression or a dictionary comprehension.
                // In either case, the key is limited to an `expression`.
                if !key_or_element.is_parenthesized {
                    match key_or_element.expr.view(&self.storage).classify() {
                        flat::ExprKind::Starred(_) => self.add_error(
                            ParseErrorType::InvalidStarredExpressionUsage,
                            key_or_element.expr,
                        ),
                        flat::ExprKind::Named(_) => self.add_error(
                            ParseErrorType::UnparenthesizedNamedExpression,
                            key_or_element,
                        ),
                        _ => {}
                    }
                }

                self.bump(TokenKind::Colon);
                let value = if self.at(TokenKind::DoubleStar) {
                    let unpack_start = self.node_start();
                    self.bump(TokenKind::DoubleStar);
                    let value = self.parse_expression_with_bitwise_or_precedence();
                    self.add_error(
                        ParseErrorType::InvalidStarredExpressionUsage,
                        TextRange::new(unpack_start, value.range().end()),
                    );
                    value
                } else {
                    self.parse_conditional_expression_or_higher()
                };

                if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                    id::Expr::from(self.parse_dictionary_comprehension_expression(
                        Some(key_or_element.expr),
                        value.expr,
                        start,
                    ))
                } else {
                    id::Expr::from(self.parse_dictionary_expression(
                        Some(key_or_element.expr),
                        value.expr,
                        start,
                    ))
                }
            }
            _ => id::Expr::from(self.parse_set_expression(key_or_element, start)),
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
                ParseErrorType::OtherError("Missing closing parenthesis `)`".to_string()),
                range,
            );
        }

        // Return an empty `TupleExpr` when finding a `)` right after the `(`
        if self.eat(TokenKind::Rpar) {
            return id::Expr::from(
                build::ExprTuple {
                    elts: flat::ListId::default(),
                    ctx: ExprContext::Load,
                    range: self.node_range(start),

                    parenthesized: true,
                }
                .parsed(&mut self.storage),
            )
            .into();
        }

        // Use the more general rule of the three to parse the first element
        // and limit it later.
        let mut parsed_expr = self.parse_named_expression_or_higher(
            ExpressionContext::yield_or_starred_bitwise_or().with_for_excluded(),
        );

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
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                            ComprehensionUnpackingKind::IterableInGenerator,
                        ),
                        parsed_expr.range(),
                    );
                }

                let generator = id::Expr::from(self.parse_generator_expression(
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
                    self.add_error(ParseErrorType::InvalidStarredExpressionUsage, parsed_expr);
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
        first_element: id::Expr,
        start: TextSize,
        parenthesized: Parenthesized,
        mut parse_func: impl FnMut(&mut Parser<'src>) -> ParsedExpr,
    ) -> id::ExprTuple {
        // TODO(dhruvmanila): Can we remove `parse_func` and use `parenthesized` to
        // determine the parsing function?

        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let elts_snapshot = self.expr_scratch.snapshot();
        self.expr_scratch.push(first_element.id);

        self.parse_comma_separated_list(RecoveryContextKind::TupleElements(parenthesized), |p| {
            let element = parse_func(p).expr;
            p.expr_scratch.push(element.id);
        });

        if parenthesized.is_yes() {
            self.expect(TokenKind::Rpar);
        }

        build::ExprTuple {
            elts: self
                .expr_scratch
                .take_range(elts_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
            ctx: ExprContext::Load,
            range: self.node_range(start),

            parenthesized: parenthesized.is_yes(),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a list expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#list-displays>
    fn parse_list_expression(&mut self, first_element: id::Expr, start: TextSize) -> id::ExprList {
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let elts_snapshot = self.expr_scratch.snapshot();
        self.expr_scratch.push(first_element.id);

        self.parse_comma_separated_list(RecoveryContextKind::ListElements, |parser| {
            let element = parser
                .parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
                .expr;
            parser.expr_scratch.push(element.id);
        });

        self.expect(TokenKind::Rsqb);

        build::ExprList {
            elts: self
                .expr_scratch
                .take_range(elts_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a set expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#set-displays>
    fn parse_set_expression(&mut self, first_element: ParsedExpr, start: TextSize) -> id::ExprSet {
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

        let elts_snapshot = self.expr_scratch.snapshot();
        self.expr_scratch.push(first_element.expr.id);

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

            parser.expr_scratch.push(parsed_expr.expr.id);
        });

        self.expect(TokenKind::Rbrace);

        build::ExprSet {
            range: self.node_range(start),

            elts: self
                .expr_scratch
                .take_range(elts_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a dictionary expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#dictionary-displays>
    fn parse_dictionary_expression(
        &mut self,
        key: Option<id::Expr>,
        value: id::Expr,
        start: TextSize,
    ) -> id::ExprDict {
        if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
        }

        let mut items = vec![id::DictItem { key, value }];

        self.parse_comma_separated_list(RecoveryContextKind::DictElements, |parser| {
            if parser.eat(TokenKind::DoubleStar) {
                // Handle dictionary unpacking. Here, the grammar is `'**' bitwise_or`
                // which requires limiting the expression.
                items.push(id::DictItem {
                    key: None,
                    value: parser.parse_expression_with_bitwise_or_precedence().expr,
                });
            } else {
                let key = parser.parse_conditional_expression_or_higher().expr;
                parser.expect(TokenKind::Colon);

                items.push(id::DictItem {
                    key: Some(key),
                    value: parser.parse_conditional_expression_or_higher().expr,
                });
            }
        });

        self.expect(TokenKind::Rbrace);

        items.shrink_to_fit();

        build::ExprDict {
            range: self.node_range(start),

            items: self.storage.list(items).into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a list of comprehension generators.
    ///
    /// These are the `for` and `async for` clauses in a comprehension, optionally
    /// followed by `if` clauses.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#grammar-token-python-grammar-comp_for>
    fn parse_generators(&mut self) -> ThinVec<id::Comprehension> {
        const GENERATOR_SET: TokenSet = TokenSet::new([TokenKind::For, TokenKind::Async]);

        let mut generators = ThinVec::with_capacity(1);
        let mut progress = ParserProgress::default();

        while self.at_ts(GENERATOR_SET) {
            progress.assert_progressing(self);
            generators.push(self.parse_comprehension());
        }

        generators.shrink_to_fit();

        generators
    }

    /// Parses a comprehension.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `async` or `for` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_comprehension(&mut self) -> id::Comprehension {
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

        let target =
            self.parse_expression_list(ExpressionContext::starred_conditional().with_in_excluded());

        self.storage
            .set_expr_context(target.expr.id, ExprContext::Store);
        self.validate_assignment_target(&target.expr);

        self.expect(TokenKind::In);
        let iter = self.parse_simple_expression(ExpressionContext::default());

        let ifs_snapshot = self.expr_scratch.snapshot();
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::If) {
            progress.assert_progressing(self);

            let parsed_expr = self.parse_simple_expression(ExpressionContext::default());

            self.expr_scratch.push(parsed_expr.expr.id);
        }

        build::Comprehension {
            range: self.node_range(start),

            target: target.expr.id,
            iter: iter.expr.id,
            ifs: self
                .expr_scratch
                .take_range(ifs_snapshot, &mut self.storage)
                .into_list(&mut self.storage),
            is_async,
        }
        .parsed(&mut self.storage)
    }

    /// Parses a generator expression.
    ///
    /// The given `start` offset is the start of either the opening parenthesis if the generator is
    /// parenthesized or the first token of the expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#generator-expressions>
    fn parse_generator_expression(
        &mut self,
        element: id::Expr,
        start: TextSize,
        parenthesized: Parenthesized,
    ) -> id::ExprGenerator {
        let generators = self.parse_generators();

        if parenthesized.is_yes() {
            self.expect(TokenKind::Rpar);
        }

        build::ExprGenerator {
            elt: element.id,
            generators: generators.into_list(&mut self.storage),
            range: self.node_range(start),

            parenthesized: parenthesized.is_yes(),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a list comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_list_comprehension_expression(
        &mut self,
        element: id::Expr,
        start: TextSize,
    ) -> id::ExprListComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rsqb);

        build::ExprListComp {
            elt: element.id,
            generators: generators.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a dictionary comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_dictionary_comprehension_expression(
        &mut self,
        key: Option<id::Expr>,
        value: id::Expr,
        start: TextSize,
    ) -> id::ExprDictComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rbrace);

        build::ExprDictComp {
            key: key.map(|node| node.id),
            value: value.id,
            generators: generators.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a set comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_set_comprehension_expression(
        &mut self,
        element: id::Expr,
        start: TextSize,
    ) -> id::ExprSetComp {
        let generators = self.parse_generators();

        self.expect(TokenKind::Rbrace);

        build::ExprSetComp {
            elt: element.id,
            generators: generators.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
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
    fn parse_starred_expression(&mut self, context: ExpressionContext) -> id::ExprStarred {
        let start = self.node_start();
        self.bump(TokenKind::Star);

        let parsed_expr = match context.starred_expression_precedence() {
            StarredExpressionPrecedence::Conditional => self
                .parse_conditional_expression_or_higher_impl(
                    // test_err starred_starred_expression
                    // print(*
                    // *[])
                    // print(* *[])
                    context.disallow_starred_expressions(),
                ),
            StarredExpressionPrecedence::BitwiseOr => {
                self.parse_expression_with_bitwise_or_precedence()
            }
        };

        build::ExprStarred {
            value: parsed_expr.expr.id,
            ctx: ExprContext::Load,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `await` expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `await` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#await-expression>
    fn parse_await_expression(&mut self) -> id::ExprAwait {
        let start = self.node_start();
        self.bump(TokenKind::Await);

        let parsed_expr = self.parse_binary_expression_or_higher(
            OperatorPrecedence::Await,
            ExpressionContext::default(),
        );

        build::ExprAwait {
            value: parsed_expr.expr.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `yield` expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `yield` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_expression(&mut self) -> id::Expr {
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

            parsed_expr.expr
        });

        id::Expr::from(
            build::ExprYield {
                value: value.map(|node| node.id),
                range: self.node_range(start),
            }
            .parsed(&mut self.storage),
        )
    }

    /// Parses a `yield from` expression.
    ///
    /// This method should not be used directly. Use [`Parser::parse_yield_expression`]
    /// even when parsing a `yield from` expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_from_expression(&mut self, start: TextSize) -> id::Expr {
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

        match expr.view(&self.storage).classify() {
            flat::ExprKind::Tuple(tuple) if !tuple.parenthesized() => {
                self.add_error(ParseErrorType::UnparenthesizedTupleExpression, expr);
            }
            _ => {}
        }

        id::Expr::from(
            build::ExprYieldFrom {
                value: expr.id,
                range: self.node_range(start),
            }
            .parsed(&mut self.storage),
        )
    }

    /// Parses a named expression (`:=`).
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `:=` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#assignment-expressions>
    fn parse_named_expression(&mut self, target: id::Expr, start: TextSize) -> id::ExprNamed {
        self.bump(TokenKind::ColonEqual);

        if !target.is_name_expr() {
            self.add_error(ParseErrorType::InvalidNamedAssignmentTarget, target.range());
        }
        self.storage.set_expr_context(target.id, ExprContext::Store);

        let value = self.parse_conditional_expression_or_higher();

        let range = self.node_range(start);

        // test_err walrus_py37
        // # parse_options: { "target-version": "3.7" }
        // (x := 1)

        // test_ok walrus_py38
        // # parse_options: { "target-version": "3.8" }
        // (x := 1)

        self.add_unsupported_syntax_error(UnsupportedSyntaxErrorKind::Walrus, range);

        let [target, value] = [target, value.expr];
        build::ExprNamed {
            target: target.id,
            value: value.id,
            range,
        }
        .parsed(&mut self.storage)
    }

    /// Parses a lambda expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `lambda` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#lambda>
    fn parse_lambda_expr(&mut self) -> id::ExprLambda {
        let start = self.node_start();
        self.bump(TokenKind::Lambda);

        let parameters = if self.at(TokenKind::Colon) {
            // test_ok lambda_with_no_parameters
            // lambda: 1
            None
        } else {
            Some(self.parse_parameters(FunctionKind::Lambda))
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

        // Lambda bodies recurse through the conditional layer without entering the binary parser.
        let body = self.with_recursion(Self::parse_conditional_expression_or_higher);

        build::ExprLambda {
            body: body.expr.id,
            parameters: parameters.map(|node| node.id),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `if` expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `if` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#conditional-expressions>
    fn parse_if_expression(&mut self, body: id::Expr, start: TextSize) -> id::ExprIf {
        self.bump(TokenKind::If);

        let test = self.parse_simple_expression(ExpressionContext::default());

        self.expect(TokenKind::Else);

        // The binary-expression guard has already returned before parsing the `else` branch.
        let orelse = self.with_recursion(Self::parse_conditional_expression_or_higher);

        let [body, test, orelse] = [body, test.expr, orelse.expr];
        build::ExprIf {
            body: body.id,
            test: test.id,
            orelse: orelse.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an IPython escape command at the expression level.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `IpyEscapeCommand` token.
    /// If the escape command kind is not `%` or `!`.
    fn parse_ipython_escape_command_expression(&mut self) -> id::ExprIpyEscapeCommand {
        let start = self.node_start();

        let (value, kind) = self.bump_ipython_escape_command(IpyEscapeContext::Assignment);

        if !matches!(kind, IpyEscapeKind::Magic | IpyEscapeKind::Shell) {
            // This should never occur as the lexer won't allow it.
            unreachable!("IPython escape command expression is only allowed for % and !");
        }

        let command = build::ExprIpyEscapeCommand {
            range: self.node_range(start),

            kind,
            value,
        }
        .parsed(&mut self.storage);

        if self.options.mode != Mode::Ipython {
            self.add_error(ParseErrorType::UnexpectedIpythonEscapeCommand, command);
        }

        command
    }

    /// Performs the following validations on the arguments:
    /// - Generator expressions are parenthesized when required by the argument context.
    fn validate_arguments(
        &mut self,
        arguments: &id::Arguments,
        has_trailing_comma: bool,
        context: ArgumentsContext,
    ) {
        let arguments = arguments.view(&self.storage);
        let generator_must_be_parenthesized = match context {
            ArgumentsContext::Call => {
                has_trailing_comma || arguments.args().len() + arguments.keywords().len() > 1
            }
            // CPython rejects an unparenthesized generator expression as a class base even though
            // this restriction isn't specified in the class definition grammar.
            ArgumentsContext::ClassDefinition => true,
        };

        if generator_must_be_parenthesized {
            let args = arguments.args().id();
            for index in 0..args.len() {
                if let Some(generator) = self
                    .storage
                    .get_list(args)
                    .get(index)
                    .and_then(ast::flat::View::cast::<ast::ExprGenerator>)
                    && !generator.parenthesized()
                {
                    let range = generator.range();
                    // test_ok args_unparenthesized_generator
                    // zip((x for x in range(10)), (y for y in range(10)))
                    // sum(x for x in range(10))
                    // sum((x for x in range(10)),)

                    // test_err args_unparenthesized_generator
                    // sum(x for x in range(10), 5)
                    // total(1, 2, x for x in range(5), 6)
                    // sum(x for x in range(10),)
                    self.add_error(ParseErrorType::UnparenthesizedGeneratorExpression, range);
                }
            }
        }
    }
}

/// Identifies the syntactic context for an argument list.
///
/// Unlike calls, class definitions require a sole generator expression to be parenthesized:
///
/// ```python
/// f(x for x in xs)
/// class C((x for x in xs)): ...
/// ```
#[derive(Debug, Copy, Clone)]
pub(super) enum ArgumentsContext {
    Call,
    ClassDefinition,
}

#[derive(Debug, Copy, Clone)]
pub(super) struct ParsedExpr {
    pub(super) expr: id::Expr,
    pub(super) is_parenthesized: bool,
}

impl ParsedExpr {
    #[inline]
    pub(super) const fn is_unparenthesized_starred_expr(&self) -> bool {
        !self.is_parenthesized && self.expr.is_starred_expr()
    }

    #[inline]
    const fn is_unparenthesized_named_expr(&self) -> bool {
        !self.is_parenthesized && self.expr.is_named_expr()
    }
}

impl From<id::Expr> for ParsedExpr {
    #[inline]
    fn from(expr: id::Expr) -> Self {
        ParsedExpr {
            expr,
            is_parenthesized: false,
        }
    }
}

impl Deref for ParsedExpr {
    type Target = id::Expr;

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
    /// Attempts to convert the token into the corresponding binary-like operator. `next` is
    /// required to distinguish `is not` and `not in` from their one-token alternatives.
    /// Returns [None] if it's not a binary-like operator.
    fn try_from_tokens(current: TokenKind, next: Option<TokenKind>) -> Option<BinaryLikeOperator> {
        if let Some(bool_op) = current.as_bool_operator() {
            Some(BinaryLikeOperator::Boolean(bool_op))
        } else if let Some(bin_op) = current.as_binary_operator() {
            Some(BinaryLikeOperator::Binary(bin_op))
        } else {
            helpers::token_kind_to_cmp_op(current, next).map(BinaryLikeOperator::Comparison)
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

        /// This flag is set when the `for` keyword, or `async` starting `async for`, should be
        /// excluded from an expression.
        const EXCLUDE_FOR = 1 << 4;
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

    fn disallow_starred_expressions(self) -> Self {
        let flags = self.0 & !ExpressionContextFlags::ALLOW_STARRED_EXPRESSION;
        ExpressionContext(flags)
    }

    fn disallow_yield_expressions(self) -> Self {
        ExpressionContext(self.0 & !ExpressionContextFlags::ALLOW_YIELD_EXPRESSION)
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

    /// Returns a new [`ExpressionContext`] which excludes `for` from an expression.
    fn with_for_excluded(self) -> Self {
        ExpressionContext(self.0 | ExpressionContextFlags::EXCLUDE_FOR)
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

    /// Returns `true` if `for` should be excluded from the expression.
    const fn is_for_excluded(self) -> bool {
        self.0.contains(ExpressionContextFlags::EXCLUDE_FOR)
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

#[derive(Debug)]
struct InterpolatedStringData {
    elements: flat::ListId<flat::Handle<ast::InterpolatedStringElement>>,
    range: TextRange,
    flags: AnyStringFlags,
}

impl InterpolatedStringData {
    fn into_fstring(self, builder: &mut flat::Builder) -> id::FString {
        build::FString {
            range: self.range,
            elements: self.elements,
            flags: self.flags.into(),
        }
        .parsed(builder)
    }
    fn into_tstring(self, builder: &mut flat::Builder) -> id::TString {
        build::TString {
            range: self.range,
            elements: self.elements,
            flags: self.flags.into(),
        }
        .parsed(builder)
    }
}

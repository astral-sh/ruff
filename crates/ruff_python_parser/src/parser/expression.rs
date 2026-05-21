use std::ops::Deref;

use bitflags::bitflags;
use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_python_ast::name::Name;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    self as ast, AnyStringFlags, AtomicNodeIndex, BoolOp, CmpOp, ConversionFlag, Expr, ExprContext,
    FString, InterpolatedStringElement, InterpolatedStringElements, IpyEscapeKind, Number,
    Operator, OperatorPrecedence, StringFlags, TString, UnaryOp,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::error::{
    ComprehensionUnpackingKind, FStringKind, StarTupleKind, UnparenthesizedNamedExprKind,
};
use crate::parser::progress::ParserProgress;
use crate::parser::{FunctionKind, Parser, helpers};
use crate::string::{
    InterpolatedStringKind, StringType, parse_interpolated_string_literal_element,
    parse_string_literal,
};
use crate::token::TokenValue;
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
    TokenKind::TStringStart,
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

    fn parse_named_expression_or_higher_unrolling_nested_trailers(
        &mut self,
        context: ExpressionContext,
    ) -> ParsedExpr {
        if !self.at(TokenKind::Name) {
            return self.parse_named_expression_or_higher(context);
        }

        let start = self.node_start();
        let lhs = self.parse_atom();
        let lhs = match self.current_token_kind() {
            TokenKind::Lpar => {
                Expr::Call(self.parse_call_expression_unrolling_nested_calls(lhs.expr, start))
                    .into()
            }
            TokenKind::Lsqb => Expr::Subscript(
                self.parse_subscript_expression_unrolling_nested_subscripts(lhs.expr, start),
            )
            .into(),
            _ => lhs,
        };

        self.parse_named_expression_or_higher_from_lhs(lhs, start, context)
    }

    fn parse_conditional_expression_or_higher_unrolling_nested_trailers(&mut self) -> ParsedExpr {
        self.parse_conditional_expression_or_higher_unrolling_nested_trailers_with_context(
            ExpressionContext::default(),
        )
    }

    fn parse_conditional_expression_or_higher_unrolling_nested_trailers_with_context(
        &mut self,
        context: ExpressionContext,
    ) -> ParsedExpr {
        if !self.at(TokenKind::Name) {
            return self.parse_conditional_expression_or_higher_impl(context);
        }

        let start = self.node_start();
        let lhs = self.parse_atom();
        let lhs = match self.current_token_kind() {
            TokenKind::Lpar => {
                Expr::Call(self.parse_call_expression_unrolling_nested_calls(lhs.expr, start))
                    .into()
            }
            TokenKind::Lsqb => Expr::Subscript(
                self.parse_subscript_expression_unrolling_nested_subscripts(lhs.expr, start),
            )
            .into(),
            _ => lhs,
        };

        self.parse_conditional_expression_or_higher_from_lhs(lhs, start, context)
    }

    fn parse_named_expression_or_higher_from_lhs(
        &mut self,
        lhs: ParsedExpr,
        start: TextSize,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let lhs = ParsedExpr {
            expr: self.parse_postfix_expression(lhs.expr, start),
            is_parenthesized: lhs.is_parenthesized,
        };
        self.parse_named_expression_or_higher_from_simple_lhs(lhs, start, context)
    }

    fn parse_conditional_expression_or_higher_from_lhs(
        &mut self,
        lhs: ParsedExpr,
        start: TextSize,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let lhs = ParsedExpr {
            expr: self.parse_postfix_expression(lhs.expr, start),
            is_parenthesized: lhs.is_parenthesized,
        };
        let parsed_expr = self.parse_binary_expression_or_higher_recursive(
            lhs,
            OperatorPrecedence::None,
            context,
            start,
        );

        self.parse_conditional_expression_or_higher_from_simple_expression(parsed_expr, start)
    }

    fn parse_named_expression_or_higher_from_simple_lhs(
        &mut self,
        lhs: ParsedExpr,
        start: TextSize,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let parsed_expr = self.parse_binary_expression_or_higher_recursive(
            lhs,
            OperatorPrecedence::None,
            context,
            start,
        );
        let parsed_expr =
            self.parse_conditional_expression_or_higher_from_simple_expression(parsed_expr, start);

        self.parse_named_expression_or_higher_from_expression(parsed_expr, start)
    }

    fn parse_named_expression_or_higher_from_expression(
        &mut self,
        parsed_expr: ParsedExpr,
        start: TextSize,
    ) -> ParsedExpr {
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

    fn parse_conditional_expression_or_higher_from_simple_expression(
        &mut self,
        parsed_expr: ParsedExpr,
        start: TextSize,
    ) -> ParsedExpr {
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

    fn parse_binary_expression_or_higher_from_lhs(
        &mut self,
        lhs: ParsedExpr,
        left_precedence: OperatorPrecedence,
        context: ExpressionContext,
        start: TextSize,
    ) -> ParsedExpr {
        let lhs = ParsedExpr {
            expr: self.parse_postfix_expression(lhs.expr, start),
            is_parenthesized: lhs.is_parenthesized,
        };
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
                BinaryLikeOperator::Boolean(bool_op) => {
                    Expr::BoolOp(self.parse_boolean_expression(left.expr, start, bool_op, context))
                }
                BinaryLikeOperator::Comparison(cmp_op) => Expr::Compare(
                    self.parse_comparison_expression(left.expr, start, cmp_op, context),
                ),
                BinaryLikeOperator::Binary(bin_op) => {
                    if bin_op == Operator::Pow {
                        self.parse_nested_power_expression(left.expr, start, context)
                    } else {
                        self.bump(TokenKind::from(bin_op));

                        let right = self.parse_binary_expression_or_higher(new_precedence, context);

                        Expr::BinOp(ast::ExprBinOp {
                            left: Box::new(left.expr),
                            op: bin_op,
                            right: Box::new(right.expr),
                            range: self.node_range(start),
                            node_index: AtomicNodeIndex::NONE,
                        })
                    }
                }
            };
        }

        left
    }

    fn parse_nested_power_expression(
        &mut self,
        lhs: Expr,
        start: TextSize,
        context: ExpressionContext,
    ) -> Expr {
        let mut expressions = vec![PendingPowerExpression { left: lhs, start }];

        let mut right = loop {
            self.bump(TokenKind::DoubleStar);

            let right_start = self.node_start();
            let right = self.parse_lhs_expression(OperatorPrecedence::Exponent, context);

            if !self.at(TokenKind::DoubleStar) {
                break right.expr;
            }

            expressions.push(PendingPowerExpression {
                left: right.expr,
                start: right_start,
            });
        };

        while let Some(expression) = expressions.pop() {
            right = Expr::BinOp(ast::ExprBinOp {
                left: Box::new(expression.left),
                op: Operator::Pow,
                right: Box::new(right),
                range: self.node_range(expression.start),
                node_index: AtomicNodeIndex::NONE,
            });
        }

        right
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
        if !Self::token_starts_recursive_lhs(token) {
            return self.parse_lhs_expression_inner(left_precedence, context, token);
        }

        if let Some(result) = self.with_recursion(|parser| {
            parser.parse_lhs_expression_inner(left_precedence, context, token)
        }) {
            result
        } else {
            self.report_recursion_limit_exceeded(self.current_token_range());
            self.recursion_recovery_expr()
        }
    }

    /// Returns whether parsing an expression that starts with `token` can
    /// immediately recurse through another expression parse.
    #[inline]
    fn token_starts_recursive_lhs(token: TokenKind) -> bool {
        token.as_unary_operator().is_some()
            || matches!(
                token,
                TokenKind::Star
                    | TokenKind::Await
                    | TokenKind::Lambda
                    | TokenKind::Yield
                    | TokenKind::FStringStart
                    | TokenKind::TStringStart
                    | TokenKind::Lpar
                    | TokenKind::Lsqb
                    | TokenKind::Lbrace
            )
    }

    /// The standard expression-recovery node returned when the recursion
    /// limit is exceeded: an empty `Name` with the `Invalid` context.
    fn recursion_recovery_expr(&mut self) -> ParsedExpr {
        ParsedExpr {
            expr: Expr::Name(ast::ExprName {
                range: self.missing_node_range(),
                id: Name::empty(),
                ctx: ExprContext::Invalid,
                node_index: AtomicNodeIndex::NONE,
            }),
            is_parenthesized: false,
        }
    }

    fn parse_lhs_expression_inner(
        &mut self,
        left_precedence: OperatorPrecedence,
        context: ExpressionContext,
        token: TokenKind,
    ) -> ParsedExpr {
        let start = self.node_start();

        if let Some(unary_op) = token.as_unary_operator() {
            let expr = if self.peek().as_unary_operator().is_some() {
                self.parse_nested_unary_expression(context)
            } else {
                self.parse_unary_expression(unary_op, context)
            };

            self.validate_unary_expression(&expr, left_precedence);

            return Expr::UnaryOp(expr).into();
        }

        match token {
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
                let await_expr = if self.peek() == TokenKind::Await {
                    self.parse_nested_await_expression()
                } else {
                    self.parse_await_expression()
                };

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

    fn parse_nested_unary_expression(&mut self, context: ExpressionContext) -> ast::ExprUnaryOp {
        let mut expressions = Vec::new();

        while let Some(op) = self.current_token_kind().as_unary_operator() {
            let start = self.node_start();
            self.bump(TokenKind::from(op));

            expressions.push(PendingUnaryExpression {
                op,
                start,
                operand_start: self.node_start(),
            });
        }

        let innermost = expressions
            .pop()
            .expect("nested unary parsing always includes the outer unary expression");
        let operand =
            self.parse_binary_expression_or_higher(OperatorPrecedence::from(innermost.op), context);
        let mut expression = self.unary_expression(innermost.op, operand.expr, innermost.start);

        while let Some(outer) = expressions.pop() {
            self.validate_unary_expression(&expression, OperatorPrecedence::from(outer.op));

            let operand = self.parse_binary_expression_or_higher_recursive(
                Expr::UnaryOp(expression).into(),
                OperatorPrecedence::from(outer.op),
                context,
                outer.operand_start,
            );
            expression = self.unary_expression(outer.op, operand.expr, outer.start);
        }

        expression
    }

    fn validate_unary_expression(
        &mut self,
        expression: &ast::ExprUnaryOp,
        left_precedence: OperatorPrecedence,
    ) {
        let op = expression.op;

        if matches!(op, UnaryOp::Not) {
            if left_precedence > OperatorPrecedence::Not {
                self.add_error(
                    ParseErrorType::OtherError(
                        "Boolean 'not' expression cannot be used here".to_string(),
                    ),
                    expression,
                );
            }
        } else if left_precedence > OperatorPrecedence::PosNegBitNot
            // > The power operator `**` binds less tightly than an arithmetic
            // > or bitwise unary operator on its right, that is, 2**-1 is 0.5.
            //
            // Reference: https://docs.python.org/3/reference/expressions.html#id21
            && left_precedence != OperatorPrecedence::Exponent
        {
            self.add_error(
                ParseErrorType::OtherError(format!("Unary '{op}' expression cannot be used here")),
                expression,
            );
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
        self.validate_expression_with_bitwise_or_precedence(parsed_expr)
    }

    fn parse_expression_with_bitwise_or_precedence_from_lhs(
        &mut self,
        lhs: ParsedExpr,
        start: TextSize,
    ) -> ParsedExpr {
        let parsed_expr = self.parse_conditional_expression_or_higher_from_lhs(
            lhs,
            start,
            ExpressionContext::default(),
        );
        self.validate_expression_with_bitwise_or_precedence(parsed_expr)
    }

    fn validate_expression_with_bitwise_or_precedence(
        &mut self,
        parsed_expr: ParsedExpr,
    ) -> ParsedExpr {
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
            node_index: AtomicNodeIndex::NONE,
        }
    }

    pub(super) fn parse_missing_name(&mut self) -> ast::ExprName {
        let identifier = self.parse_missing_identifier();

        ast::ExprName {
            range: identifier.range,
            id: identifier.id,
            ctx: ExprContext::Invalid,
            node_index: AtomicNodeIndex::NONE,
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
            return ast::Identifier {
                id: name,
                range,
                node_index: AtomicNodeIndex::NONE,
            };
        }

        if self.current_token_kind().is_soft_keyword() {
            let id = Name::new(self.src_text(range));
            self.bump_soft_keyword_as_name();
            return ast::Identifier {
                id,
                range,
                node_index: AtomicNodeIndex::NONE,
            };
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
            ast::Identifier {
                id,
                range,
                node_index: AtomicNodeIndex::NONE,
            }
        } else {
            self.parse_missing_identifier()
        }
    }

    fn parse_missing_identifier(&mut self) -> ast::Identifier {
        self.add_error(
            ParseErrorType::OtherError("Expected an identifier".into()),
            self.current_token_range(),
        );

        ast::Identifier {
            id: Name::empty(),
            range: self.missing_node_range(),
            node_index: AtomicNodeIndex::NONE,
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
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::Complex => {
                let TokenValue::Complex { real, imag } = self.bump_value(TokenKind::Complex) else {
                    unreachable!()
                };
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Complex { real, imag },
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::Int => {
                let TokenValue::Int(value) = self.bump_value(TokenKind::Int) else {
                    unreachable!()
                };
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Int(value),
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::True => {
                self.bump(TokenKind::True);
                Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                    value: true,
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::False => {
                self.bump(TokenKind::False);
                Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                    value: false,
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::None => {
                self.bump(TokenKind::None);
                Expr::NoneLiteral(ast::ExprNoneLiteral {
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::Ellipsis => {
                self.bump(TokenKind::Ellipsis);
                Expr::EllipsisLiteral(ast::ExprEllipsisLiteral {
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::Name => Expr::Name(self.parse_name()),
            TokenKind::IpyEscapeCommand => {
                Expr::IpyEscapeCommand(self.parse_ipython_escape_command_expression())
            }
            TokenKind::String | TokenKind::FStringStart | TokenKind::TStringStart => {
                self.parse_strings()
            }
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
                        node_index: AtomicNodeIndex::NONE,
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
                TokenKind::Lpar => {
                    if self.tokens.nesting() > self.max_nesting_depth {
                        self.report_recursion_limit_exceeded(self.current_token_range());
                        break lhs;
                    }
                    Expr::Call(self.parse_call_expression(lhs, start))
                }
                TokenKind::Lsqb => {
                    if self.tokens.nesting() > self.max_nesting_depth {
                        self.report_recursion_limit_exceeded(self.current_token_range());
                        break lhs;
                    }
                    Expr::Subscript(self.parse_subscript_expression(lhs, start))
                }
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
        let arguments_start = self.node_start();
        self.bump(TokenKind::Lpar);

        self.parse_call_expression_after_lpar(func, start, arguments_start)
    }

    fn parse_call_expression_unrolling_nested_calls(
        &mut self,
        func: Expr,
        start: TextSize,
    ) -> ast::ExprCall {
        let arguments_start = self.node_start();
        self.bump(TokenKind::Lpar);

        if let Some(nested) = self.try_parse_nested_call_argument() {
            return self.parse_nested_call_expression(func, start, arguments_start, nested);
        }

        self.parse_call_expression_after_lpar(func, start, arguments_start)
    }

    fn parse_call_expression_after_lpar(
        &mut self,
        func: Expr,
        start: TextSize,
        arguments_start: TextSize,
    ) -> ast::ExprCall {
        let arguments = self.parse_arguments_after_lpar(arguments_start);

        ast::ExprCall {
            func: Box::new(func),
            arguments,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
        }
    }

    fn parse_nested_call_expression(
        &mut self,
        outer_func: Expr,
        outer_start: TextSize,
        outer_arguments_start: TextSize,
        nested: NestedCallArgument,
    ) -> ast::ExprCall {
        let mut calls = vec![PendingCall {
            func: outer_func,
            start: outer_start,
            arguments_start: outer_arguments_start,
            argument: nested.argument,
            state: nested.state,
        }];
        let mut func = nested.func;
        let mut start = nested.start;

        let mut expr = loop {
            let arguments_start = self.node_start();
            self.bump(TokenKind::Lpar);

            if let Some(nested) = self.try_parse_nested_call_argument() {
                calls.push(PendingCall {
                    func,
                    start,
                    arguments_start,
                    argument: nested.argument,
                    state: nested.state,
                });
                func = nested.func;
                start = nested.start;
                continue;
            }

            let arguments = self.parse_arguments_after_lpar(arguments_start);
            break Expr::Call(ast::ExprCall {
                func: Box::new(func),
                arguments,
                range: self.node_range(start),
                node_index: AtomicNodeIndex::NONE,
            });
        };

        while let Some(call) = calls.pop() {
            let value_start = call.argument.value_start();
            let arguments = match call.argument {
                PendingCallArgument::Positional { argument_start } => {
                    let parsed_expr = self.parse_named_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::starred_conditional(),
                    );
                    self.parse_arguments_after_positional(
                        call.arguments_start,
                        call.state,
                        argument_start,
                        parsed_expr,
                    )
                }
                PendingCallArgument::Keyword {
                    argument_start,
                    arg,
                    ..
                } => {
                    let parsed_expr = self.parse_conditional_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::default(),
                    );
                    self.parse_arguments_after_keyword_value(
                        call.arguments_start,
                        call.state,
                        argument_start,
                        arg,
                        parsed_expr.expr,
                    )
                }
                PendingCallArgument::KeywordUnpacking { argument_start, .. } => {
                    let parsed_expr = self.parse_conditional_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::default(),
                    );
                    self.parse_arguments_after_keyword_unpacking_value(
                        call.arguments_start,
                        call.state,
                        argument_start,
                        parsed_expr.expr,
                    )
                }
                PendingCallArgument::Starred { argument_start, .. } => {
                    let parsed_expr = self.parse_conditional_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::starred_conditional().disallow_starred_expressions(),
                    );
                    self.parse_arguments_after_positional(
                        call.arguments_start,
                        call.state,
                        argument_start,
                        Expr::Starred(ast::ExprStarred {
                            value: Box::new(parsed_expr.expr),
                            ctx: ExprContext::Load,
                            range: self.node_range(argument_start),
                            node_index: AtomicNodeIndex::NONE,
                        })
                        .into(),
                    )
                }
            };

            expr = Expr::Call(ast::ExprCall {
                func: Box::new(call.func),
                arguments,
                range: self.node_range(call.start),
                node_index: AtomicNodeIndex::NONE,
            });
        }

        let Expr::Call(call) = expr else {
            unreachable!("nested call parsing always builds a call expression");
        };
        call
    }

    fn try_parse_nested_call_argument(&mut self) -> Option<NestedCallArgument> {
        let arguments_checkpoint = self.checkpoint();
        let mut state = ArgumentParsingState::default();

        loop {
            if self.at(TokenKind::Star) && self.peek() == TokenKind::Name {
                let argument_checkpoint = self.checkpoint();
                let argument_start = self.node_start();
                self.bump(TokenKind::Star);

                if self.at(TokenKind::Name) && self.peek() == TokenKind::Lpar {
                    let value_start = self.node_start();
                    let func = self.parse_atom().expr;
                    return Some(NestedCallArgument {
                        func,
                        start: value_start,
                        argument: PendingCallArgument::Starred {
                            argument_start,
                            value_start,
                        },
                        state,
                    });
                }

                self.rewind(argument_checkpoint);
            }

            if self.at(TokenKind::DoubleStar) && self.peek() == TokenKind::Name {
                let argument_checkpoint = self.checkpoint();
                let argument_start = self.node_start();
                self.bump(TokenKind::DoubleStar);

                if self.at(TokenKind::Name) && self.peek() == TokenKind::Lpar {
                    let value_start = self.node_start();
                    let func = self.parse_atom().expr;
                    return Some(NestedCallArgument {
                        func,
                        start: value_start,
                        argument: PendingCallArgument::KeywordUnpacking {
                            argument_start,
                            value_start,
                        },
                        state,
                    });
                }

                self.rewind(argument_checkpoint);
            }

            if self.at(TokenKind::Name) && self.peek() == TokenKind::Lpar {
                let argument_start = self.node_start();
                let func = self.parse_atom().expr;
                return Some(NestedCallArgument {
                    func,
                    start: argument_start,
                    argument: PendingCallArgument::Positional { argument_start },
                    state,
                });
            }

            let argument_checkpoint = self.checkpoint();
            let argument_start = self.node_start();
            let start = self.node_start();
            let parsed_expr = self.parse_named_expression_or_higher_unrolling_nested_trailers(
                ExpressionContext::starred_conditional(),
            );
            let arg_range = self.node_range(start);
            if self.eat(TokenKind::Equal)
                && self.at(TokenKind::Name)
                && self.peek() == TokenKind::Lpar
            {
                let arg = self.parse_keyword_argument_name(parsed_expr, arg_range);
                let value_start = self.node_start();
                let func = self.parse_atom().expr;
                return Some(NestedCallArgument {
                    func,
                    start: value_start,
                    argument: PendingCallArgument::Keyword {
                        argument_start,
                        arg,
                        value_start,
                    },
                    state,
                });
            }
            self.rewind(argument_checkpoint);

            self.parse_argument(&mut state);

            if !self.eat(TokenKind::Comma) || self.at(TokenKind::Rpar) {
                self.rewind(arguments_checkpoint);
                return None;
            }
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

        self.parse_arguments_after_lpar(start)
    }

    fn parse_arguments_after_lpar(&mut self, start: TextSize) -> ast::Arguments {
        let mut args = vec![];
        let mut keywords = vec![];
        let mut seen_keyword_argument = false; // foo = 1
        let mut seen_keyword_unpacking = false; // **foo

        let has_trailing_comma =
            self.parse_comma_separated_list(RecoveryContextKind::Arguments, |parser| {
                let argument_start = parser.node_start();
                if parser.eat(TokenKind::DoubleStar) {
                    let value =
                        parser.parse_conditional_expression_or_higher_unrolling_nested_trailers();

                    keywords.push(ast::Keyword {
                        arg: None,
                        value: value.expr,
                        range: parser.node_range(argument_start),
                        node_index: AtomicNodeIndex::NONE,
                    });

                    seen_keyword_unpacking = true;
                } else {
                    let start = parser.node_start();
                    let mut parsed_expr = parser
                        .parse_named_expression_or_higher_unrolling_nested_trailers(
                            ExpressionContext::starred_conditional(),
                        );

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

                            parsed_expr = Expr::Generator(parser.parse_generator_expression(
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
                                node_index: AtomicNodeIndex::NONE,
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
                                node_index: AtomicNodeIndex::NONE,
                            }
                        };

                        let value = parser
                            .parse_conditional_expression_or_higher_unrolling_nested_trailers();

                        keywords.push(ast::Keyword {
                            arg: Some(arg),
                            value: value.expr,
                            range: parser.node_range(argument_start),
                            node_index: AtomicNodeIndex::NONE,
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
            node_index: AtomicNodeIndex::NONE,
            args: args.into_boxed_slice(),
            keywords: keywords.into_boxed_slice(),
        };

        self.validate_arguments(&arguments, has_trailing_comma);

        arguments
    }

    fn parse_arguments_after_positional(
        &mut self,
        start: TextSize,
        mut state: ArgumentParsingState,
        argument_start: TextSize,
        parsed_expr: ParsedExpr,
    ) -> ast::Arguments {
        self.parse_argument_from_expression(
            &mut state,
            argument_start,
            argument_start,
            parsed_expr,
        );

        self.parse_arguments_after_state(start, state)
    }

    fn parse_arguments_after_keyword_value(
        &mut self,
        start: TextSize,
        mut state: ArgumentParsingState,
        argument_start: TextSize,
        arg: ast::Identifier,
        value: Expr,
    ) -> ast::Arguments {
        self.parse_keyword_argument_from_value(&mut state, argument_start, arg, value);

        self.parse_arguments_after_state(start, state)
    }

    fn parse_arguments_after_keyword_unpacking_value(
        &mut self,
        start: TextSize,
        mut state: ArgumentParsingState,
        argument_start: TextSize,
        value: Expr,
    ) -> ast::Arguments {
        state.keywords.push(ast::Keyword {
            arg: None,
            value,
            range: self.node_range(argument_start),
            node_index: AtomicNodeIndex::NONE,
        });
        state.seen_keyword_unpacking = true;

        self.parse_arguments_after_state(start, state)
    }

    fn parse_arguments_after_state(
        &mut self,
        start: TextSize,
        mut state: ArgumentParsingState,
    ) -> ast::Arguments {
        let has_trailing_comma = if self.eat(TokenKind::Comma) {
            if self.at(TokenKind::Rpar) {
                true
            } else {
                self.parse_comma_separated_list(RecoveryContextKind::Arguments, |parser| {
                    parser.parse_argument(&mut state);
                })
            }
        } else if self.at(TokenKind::Rpar) {
            false
        } else {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(RecoveryContextKind::Arguments, |parser| {
                parser.parse_argument(&mut state);
            })
        };

        self.finish_arguments(start, state, has_trailing_comma)
    }

    fn parse_argument(&mut self, state: &mut ArgumentParsingState) {
        let argument_start = self.node_start();
        if self.eat(TokenKind::DoubleStar) {
            let value = self.parse_conditional_expression_or_higher_unrolling_nested_trailers();

            state.keywords.push(ast::Keyword {
                arg: None,
                value: value.expr,
                range: self.node_range(argument_start),
                node_index: AtomicNodeIndex::NONE,
            });

            state.seen_keyword_unpacking = true;
        } else {
            let start = self.node_start();
            let parsed_expr = self.parse_named_expression_or_higher_unrolling_nested_trailers(
                ExpressionContext::starred_conditional(),
            );

            self.parse_argument_from_expression(state, argument_start, start, parsed_expr);
        }
    }

    fn parse_argument_from_expression(
        &mut self,
        state: &mut ArgumentParsingState,
        argument_start: TextSize,
        start: TextSize,
        mut parsed_expr: ParsedExpr,
    ) {
        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                if parsed_expr.is_unparenthesized_starred_expr() {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                            ComprehensionUnpackingKind::IterableInGenerator,
                        ),
                        parsed_expr.range(),
                    );
                }

                parsed_expr = Expr::Generator(self.parse_generator_expression(
                    parsed_expr.expr,
                    start,
                    Parenthesized::No,
                ))
                .into();
            }
            _ => {
                if state.seen_keyword_unpacking && parsed_expr.is_unparenthesized_starred_expr() {
                    self.add_error(ParseErrorType::InvalidArgumentUnpackingOrder, &parsed_expr);
                }
            }
        }

        let arg_range = self.node_range(start);
        if self.eat(TokenKind::Equal) {
            let arg = self.parse_keyword_argument_name(parsed_expr, arg_range);

            let value = self.parse_conditional_expression_or_higher_unrolling_nested_trailers();
            self.parse_keyword_argument_from_value(state, argument_start, arg, value.expr);
        } else {
            if !parsed_expr.is_unparenthesized_starred_expr() {
                if state.seen_keyword_unpacking {
                    self.add_error(
                        ParseErrorType::PositionalAfterKeywordUnpacking,
                        &parsed_expr,
                    );
                } else if state.seen_keyword_argument {
                    self.add_error(ParseErrorType::PositionalAfterKeywordArgument, &parsed_expr);
                }
            }
            state.args.push(parsed_expr.expr);
        }
    }

    fn parse_keyword_argument_name(
        &mut self,
        parsed_expr: ParsedExpr,
        arg_range: TextRange,
    ) -> ast::Identifier {
        if let ParsedExpr {
            expr: Expr::Name(ident_expr),
            is_parenthesized,
        } = parsed_expr
        {
            if is_parenthesized {
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::ParenthesizedKeywordArgumentName,
                    arg_range,
                );
            }

            ast::Identifier {
                id: ident_expr.id,
                range: ident_expr.range,
                node_index: AtomicNodeIndex::NONE,
            }
        } else {
            // TODO(dhruvmanila): Parser shouldn't drop the `parsed_expr` if it's
            // not a name expression. We could add the expression into `args` but
            // that means the error is a missing comma instead.
            self.add_error(
                ParseErrorType::OtherError("Expected a parameter name".to_string()),
                &parsed_expr,
            );
            ast::Identifier {
                id: Name::empty(),
                range: parsed_expr.range(),
                node_index: AtomicNodeIndex::NONE,
            }
        }
    }

    fn parse_keyword_argument_from_value(
        &self,
        state: &mut ArgumentParsingState,
        argument_start: TextSize,
        arg: ast::Identifier,
        value: Expr,
    ) {
        state.seen_keyword_argument = true;
        state.keywords.push(ast::Keyword {
            arg: Some(arg),
            value,
            range: self.node_range(argument_start),
            node_index: AtomicNodeIndex::NONE,
        });
    }

    fn finish_arguments(
        &mut self,
        start: TextSize,
        state: ArgumentParsingState,
        has_trailing_comma: bool,
    ) -> ast::Arguments {
        let ArgumentParsingState { args, keywords, .. } = state;

        self.expect(TokenKind::Rpar);

        let arguments = ast::Arguments {
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            args: args.into_boxed_slice(),
            keywords: keywords.into_boxed_slice(),
        };

        self.validate_arguments(&arguments, has_trailing_comma);

        arguments
    }

    /// Parses a subscript expression.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#subscriptions>
    fn parse_subscript_expression(&mut self, value: Expr, start: TextSize) -> ast::ExprSubscript {
        self.bump(TokenKind::Lsqb);

        let slice_start = self.node_start();
        self.parse_subscript_expression_after_lsqb(value, start, slice_start)
    }

    fn parse_subscript_expression_unrolling_nested_subscripts(
        &mut self,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprSubscript {
        self.bump(TokenKind::Lsqb);

        let slice_start = self.node_start();
        if let Some(nested) = self.try_parse_nested_subscript_slice() {
            return self.parse_nested_subscript_expression(
                PendingSubscript {
                    value,
                    start,
                    slice_start,
                    slices: nested.slices,
                    nested_slice: nested.nested_slice,
                },
                nested.value,
                nested.start,
            );
        }

        self.parse_subscript_expression_after_lsqb(value, start, slice_start)
    }

    fn try_parse_nested_subscript_slice(&mut self) -> Option<NestedSubscriptSlice> {
        let checkpoint = self.checkpoint();
        let mut slices = vec![];

        loop {
            if self.at(TokenKind::Colon) {
                let slice_checkpoint = self.checkpoint();
                self.bump(TokenKind::Colon);

                if self.at(TokenKind::Name) && self.peek() == TokenKind::Lsqb {
                    let value_start = self.node_start();
                    let value = self.parse_atom().expr;
                    return Some(NestedSubscriptSlice {
                        value,
                        start: value_start,
                        slices,
                        nested_slice: PendingSubscriptSlice::Upper { value_start },
                    });
                }

                if self.eat(TokenKind::Colon)
                    && self.at(TokenKind::Name)
                    && self.peek() == TokenKind::Lsqb
                {
                    let value_start = self.node_start();
                    let value = self.parse_atom().expr;
                    return Some(NestedSubscriptSlice {
                        value,
                        start: value_start,
                        slices,
                        nested_slice: PendingSubscriptSlice::Step { value_start },
                    });
                }

                self.rewind(slice_checkpoint);
            }

            if self.at(TokenKind::Star) && self.peek() == TokenKind::Name {
                let slice_checkpoint = self.checkpoint();
                let starred_start = self.node_start();
                self.bump(TokenKind::Star);

                if self.at(TokenKind::Name) && self.peek() == TokenKind::Lsqb {
                    let start = self.node_start();
                    let value = self.parse_atom().expr;
                    return Some(NestedSubscriptSlice {
                        value,
                        start,
                        slices,
                        nested_slice: PendingSubscriptSlice::Starred {
                            starred_start,
                            value_start: start,
                        },
                    });
                }

                self.rewind(slice_checkpoint);
            }

            if self.at(TokenKind::Name) && self.peek() == TokenKind::Lsqb {
                let start = self.node_start();
                let value = self.parse_atom().expr;
                return Some(NestedSubscriptSlice {
                    value,
                    start,
                    slices,
                    nested_slice: PendingSubscriptSlice::Direct,
                });
            }

            slices.push(self.parse_slice());

            if !self.eat(TokenKind::Comma) || self.at(TokenKind::Rsqb) {
                self.rewind(checkpoint);
                return None;
            }
        }
    }

    fn parse_nested_subscript_expression(
        &mut self,
        outer: PendingSubscript,
        mut value: Expr,
        mut start: TextSize,
    ) -> ast::ExprSubscript {
        let mut subscripts = vec![outer];

        let mut expr = loop {
            self.bump(TokenKind::Lsqb);
            let slice_start = self.node_start();

            let Some(nested) = self.try_parse_nested_subscript_slice() else {
                break Expr::Subscript(self.parse_subscript_expression_after_lsqb(
                    value,
                    start,
                    slice_start,
                ));
            };

            subscripts.push(PendingSubscript {
                value,
                start,
                slice_start,
                slices: nested.slices,
                nested_slice: nested.nested_slice,
            });

            value = nested.value;
            start = nested.start;
        };

        while let Some(subscript) = subscripts.pop() {
            let slice = match subscript.nested_slice {
                PendingSubscriptSlice::Direct => {
                    let lower = self.parse_named_expression_or_higher_from_lhs(
                        expr.into(),
                        subscript.slice_start,
                        ExpressionContext::starred_conditional(),
                    );
                    self.parse_slice_from_lower(subscript.slice_start, lower)
                }
                PendingSubscriptSlice::Starred {
                    starred_start,
                    value_start,
                } => {
                    let value = self.parse_conditional_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::starred_conditional().disallow_starred_expressions(),
                    );
                    let lower = Expr::Starred(ast::ExprStarred {
                        value: Box::new(value.expr),
                        ctx: ExprContext::Load,
                        range: self.node_range(starred_start),
                        node_index: AtomicNodeIndex::NONE,
                    })
                    .into();
                    self.parse_slice_from_lower(subscript.slice_start, lower)
                }
                PendingSubscriptSlice::Upper { value_start } => {
                    let upper = self.parse_conditional_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::default(),
                    );
                    self.finish_slice_after_upper(subscript.slice_start, None, Some(upper.expr))
                }
                PendingSubscriptSlice::Step { value_start } => {
                    let step = self.parse_conditional_expression_or_higher_from_lhs(
                        expr.into(),
                        value_start,
                        ExpressionContext::default(),
                    );
                    self.finish_slice_after_step(subscript.slice_start, None, None, Some(step.expr))
                }
            };
            expr = Expr::Subscript(if subscript.slices.is_empty() {
                self.finish_subscript_expression(
                    subscript.value,
                    subscript.start,
                    subscript.slice_start,
                    slice,
                )
            } else {
                self.finish_subscript_expression_after_slice(
                    subscript.value,
                    subscript.start,
                    subscript.slice_start,
                    subscript.slices,
                    slice,
                )
            });
        }

        let Expr::Subscript(subscript) = expr else {
            unreachable!("nested subscript parsing always builds a subscript expression");
        };
        subscript
    }

    fn parse_subscript_expression_after_lsqb(
        &mut self,
        mut value: Expr,
        start: TextSize,
        slice_start: TextSize,
    ) -> ast::ExprSubscript {
        // To prevent the `value` context from being `Del` within a `del` statement,
        // we set the context as `Load` here.
        helpers::set_expr_ctx(&mut value, ExprContext::Load);

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
                    node_index: AtomicNodeIndex::NONE,
                })),
                ctx: ExprContext::Load,
                range: self.node_range(start),
                node_index: AtomicNodeIndex::NONE,
            };
        }

        let slice = self.parse_slice();
        self.finish_subscript_expression(value, start, slice_start, slice)
    }

    fn finish_subscript_expression(
        &mut self,
        value: Expr,
        start: TextSize,
        slice_start: TextSize,
        mut slice: Expr,
    ) -> ast::ExprSubscript {
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
                node_index: AtomicNodeIndex::NONE,
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
                node_index: AtomicNodeIndex::NONE,
            });
        }

        self.finish_subscript_expression_with_slice(value, start, slice)
    }

    fn finish_subscript_expression_after_slice(
        &mut self,
        value: Expr,
        start: TextSize,
        slice_start: TextSize,
        mut slices: Vec<Expr>,
        slice: Expr,
    ) -> ast::ExprSubscript {
        slices.push(slice);

        if self.eat(TokenKind::Comma) {
            if !self.at(TokenKind::Rsqb) {
                self.parse_comma_separated_list(RecoveryContextKind::Slices, |parser| {
                    slices.push(parser.parse_slice());
                });
            }
        } else if !self.at(TokenKind::Rsqb) {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(RecoveryContextKind::Slices, |parser| {
                slices.push(parser.parse_slice());
            });
        }

        let slice = Expr::Tuple(ast::ExprTuple {
            elts: slices,
            ctx: ExprContext::Load,
            range: self.node_range(slice_start),
            parenthesized: false,
            node_index: AtomicNodeIndex::NONE,
        });

        self.finish_subscript_expression_with_slice(value, start, slice)
    }

    fn finish_subscript_expression_with_slice(
        &mut self,
        value: Expr,
        start: TextSize,
        slice: Expr,
    ) -> ast::ExprSubscript {
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
            node_index: AtomicNodeIndex::NONE,
        }
    }

    /// Parses a slice expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#slicings>
    fn parse_slice(&mut self) -> Expr {
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

        if self.at_expr() {
            let lower = self.parse_named_expression_or_higher_unrolling_nested_trailers(
                ExpressionContext::starred_conditional(),
            );
            self.parse_slice_from_lower(start, lower)
        } else {
            self.finish_slice(start, None)
        }
    }

    fn parse_slice_from_lower(&mut self, start: TextSize, lower: ParsedExpr) -> Expr {
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

        self.finish_slice(start, Some(lower.expr))
    }

    fn finish_slice(&mut self, start: TextSize, lower: Option<Expr>) -> Expr {
        const UPPER_END_SET: TokenSet =
            TokenSet::new([TokenKind::Comma, TokenKind::Colon, TokenKind::Rsqb])
                .union(NEWLINE_EOF_SET);

        self.expect(TokenKind::Colon);

        let upper = if self.at_ts(UPPER_END_SET) {
            None
        } else {
            Some(
                self.parse_conditional_expression_or_higher_unrolling_nested_trailers()
                    .expr,
            )
        };

        self.finish_slice_after_upper(start, lower, upper)
    }

    fn finish_slice_after_upper(
        &mut self,
        start: TextSize,
        lower: Option<Expr>,
        upper: Option<Expr>,
    ) -> Expr {
        const STEP_END_SET: TokenSet =
            TokenSet::new([TokenKind::Comma, TokenKind::Rsqb]).union(NEWLINE_EOF_SET);

        let lower = lower.map(Box::new);
        let upper = upper.map(Box::new);
        let step = if self.eat(TokenKind::Colon) {
            if self.at_ts(STEP_END_SET) {
                None
            } else {
                Some(
                    self.parse_conditional_expression_or_higher_unrolling_nested_trailers()
                        .expr,
                )
            }
        } else {
            None
        };

        self.finish_slice_after_step(start, lower, upper, step)
    }

    fn finish_slice_after_step(
        &self,
        start: TextSize,
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Expr>,
    ) -> Expr {
        Expr::Slice(ast::ExprSlice {
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            lower,
            upper,
            step: step.map(Box::new),
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

        self.unary_expression(op, operand.expr, start)
    }

    fn unary_expression(&self, op: UnaryOp, operand: Expr, start: TextSize) -> ast::ExprUnaryOp {
        ast::ExprUnaryOp {
            op,
            operand: Box::new(operand),
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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
            node_index: AtomicNodeIndex::NONE,
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
            node_index: AtomicNodeIndex::NONE,
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

            let next_next_token =
                matches!(next_token, TokenKind::Is | TokenKind::Not).then(|| self.peek());
            let Some(next_op) = helpers::token_kind_to_cmp_op(next_token, next_next_token) else {
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
            node_index: AtomicNodeIndex::NONE,
        }
    }

    /// Parses all kinds of strings and implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `String`, `FStringStart`, or `TStringStart` token.
    ///
    /// See: <https://docs.python.org/3/reference/grammar.html> (Search "strings:")
    pub(super) fn parse_strings(&mut self) -> Expr {
        const STRING_START_SET: TokenSet = TokenSet::new([
            TokenKind::String,
            TokenKind::FStringStart,
            TokenKind::TStringStart,
        ]);

        let start = self.node_start();
        let mut strings = vec![];

        let mut progress = ParserProgress::default();

        while self.at_ts(STRING_START_SET) {
            progress.assert_progressing(self);

            if self.at(TokenKind::String) {
                strings.push(self.parse_string_or_byte_literal());
            } else if self.at(TokenKind::FStringStart) {
                strings.push(StringType::FString(
                    self.parse_interpolated_string(InterpolatedStringKind::FString)
                        .into(),
                ));
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
                        .into(),
                );
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::TemplateStrings,
                    string_type.range(),
                );
                strings.push(string_type);
            }
        }

        let range = self.node_range(start);

        match strings.len() {
            // This is not possible as the function was called by matching against a
            // `String`, `FStringStart`, or `TStringStart` token.
            0 => unreachable!("Expected to parse at least one string"),
            // We need a owned value, hence the `pop` here.
            1 => match strings.pop().unwrap() {
                StringType::Str(string) => Expr::StringLiteral(ast::ExprStringLiteral {
                    value: ast::StringLiteralValue::single(string),
                    range,
                    node_index: AtomicNodeIndex::NONE,
                }),
                StringType::Bytes(bytes) => Expr::BytesLiteral(ast::ExprBytesLiteral {
                    value: ast::BytesLiteralValue::single(bytes),
                    range,
                    node_index: AtomicNodeIndex::NONE,
                }),
                StringType::FString(fstring) => Expr::FString(ast::ExprFString {
                    value: ast::FStringValue::single(fstring),
                    range,
                    node_index: AtomicNodeIndex::NONE,
                }),
                StringType::TString(tstring) => Expr::TString(ast::ExprTString {
                    value: ast::TStringValue::single(tstring),
                    range,
                    node_index: AtomicNodeIndex::NONE,
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
                return Expr::from(ast::ExprBytesLiteral {
                    value: ast::BytesLiteralValue::concatenated(values),
                    range,
                    node_index: AtomicNodeIndex::NONE,
                });
            }
        }

        if has_tstring {
            if tstring_count < strings.len() {
                self.add_error(
                    ParseErrorType::OtherError(
                        "cannot mix t-string literals with string or bytes literals".to_string(),
                    ),
                    range,
                );
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
                return Expr::from(ast::ExprTString {
                    value: ast::TStringValue::concatenated(values),
                    range,
                    node_index: AtomicNodeIndex::NONE,
                });
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
                    _ => ast::StringLiteral::invalid(string.range()),
                });
            }
            return Expr::from(ast::ExprStringLiteral {
                value: ast::StringLiteralValue::concatenated(values),
                range,
                node_index: AtomicNodeIndex::NONE,
            });
        }

        let mut parts = Vec::with_capacity(strings.len());
        for string in strings {
            match string {
                StringType::FString(fstring) => parts.push(ast::FStringPart::FString(fstring)),
                StringType::Str(string) => parts.push(ast::FStringPart::Literal(string)),
                // Bytes and Template strings are invalid at this point
                // and stored as invalid string literal parts in the
                // f-string
                StringType::TString(tstring) => parts.push(ast::FStringPart::Literal(
                    ast::StringLiteral::invalid(tstring.range()),
                )),
                StringType::Bytes(bytes) => parts.push(ast::FStringPart::Literal(
                    ast::StringLiteral::invalid(bytes.range()),
                )),
            }
        }

        Expr::from(ast::ExprFString {
            value: ast::FStringValue::concatenated(parts),
            range,
            node_index: AtomicNodeIndex::NONE,
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
                        node_index: AtomicNodeIndex::NONE,
                    })
                } else {
                    // test_err invalid_string_literal
                    // 'hello \N{INVALID} world'
                    // """hello \N{INVALID} world"""
                    StringType::Str(ast::StringLiteral {
                        value: "".into(),
                        range,
                        flags: ast::StringLiteralFlags::from(flags).with_invalid(),
                        node_index: AtomicNodeIndex::NONE,
                    })
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
    ) -> ast::InterpolatedStringElements {
        let mut elements = vec![];
        let middle_token_kind = string_kind.middle_token();

        self.parse_list(
            RecoveryContextKind::InterpolatedStringElements(elements_kind),
            |parser| {
                let element = match parser.current_token_kind() {
                    TokenKind::Lbrace => ast::InterpolatedStringElement::from(
                        parser.parse_interpolated_element(flags, string_kind),
                    ),
                    tok if tok == middle_token_kind => {
                        let range = parser.current_token_range();
                        let TokenValue::InterpolatedStringMiddle(value) =
                            parser.bump_value(middle_token_kind)
                        else {
                            unreachable!()
                        };
                        InterpolatedStringElement::Literal(
                            parse_interpolated_string_literal_element(value, flags, range)
                                .unwrap_or_else(|lex_error| {
                                    // test_err invalid_fstring_literal_element
                                    // f'hello \N{INVALID} world'
                                    // f"""hello \N{INVALID} world"""
                                    let location = lex_error.location();
                                    parser.add_error(
                                        ParseErrorType::Lexical(lex_error.into_error()),
                                        location,
                                    );
                                    ast::InterpolatedStringLiteralElement {
                                        value: "".into(),
                                        range,
                                        node_index: AtomicNodeIndex::NONE,
                                    }
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

        ast::InterpolatedStringElements::from(elements)
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
    ) -> ast::InterpolatedElement {
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
            // Ensure that the `r` is lexed as a `r` name token instead of a raw string
            // in `f{abc!r"` (note the missing `}`).
            self.tokens.re_lex_raw_string_in_format_spec();

            let conversion_flag_range = self.current_token_range();
            if self.at(TokenKind::Name) {
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
            let elements = if let Some(elements) = self.with_recursion(|parser| {
                parser.parse_interpolated_string_elements(
                    flags,
                    InterpolatedStringElementsKind::FormatSpec(string_kind),
                    string_kind,
                )
            }) {
                elements
            } else {
                self.report_recursion_limit_exceeded(self.current_token_range());
                ast::InterpolatedStringElements::from(vec![])
            };
            Some(Box::new(ast::InterpolatedStringFormatSpec {
                range: self.node_range(spec_start),
                elements,
                node_index: AtomicNodeIndex::NONE,
            }))
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

        ast::InterpolatedElement {
            expression: Box::new(value.expr),
            debug_text,
            conversion,
            format_spec,
            range,
            node_index: AtomicNodeIndex::NONE,
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

        self.report_unclosed_bracket();

        // Return an empty `ListExpr` when finding a `]` right after the `[`
        if self.eat(TokenKind::Rsqb) {
            return self.empty_list(start);
        }

        if let Some(pending) = self.try_parse_nested_list_like_expression(start) {
            return self.parse_nested_list_like_expression(pending);
        }

        // Parse the first element with a more general rule and limit it later.
        let first_element =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        self.finish_list_like_expression(first_element, start)
    }

    fn try_parse_nested_list_like_expression(
        &mut self,
        start: TextSize,
    ) -> Option<PendingListLikeExpression> {
        let checkpoint = self.checkpoint();
        let mut elts = vec![];

        loop {
            if self.at(TokenKind::Lsqb) {
                return Some(PendingListLikeExpression {
                    start,
                    elts,
                    nested_element: PendingListElement::Direct,
                });
            }

            if self.at(TokenKind::Star) && self.peek() == TokenKind::Lsqb {
                let starred_start = self.node_start();
                self.bump(TokenKind::Star);
                return Some(PendingListLikeExpression {
                    start,
                    elts,
                    nested_element: PendingListElement::Starred {
                        starred_start,
                        value_start: self.node_start(),
                    },
                });
            }

            elts.push(
                self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
                    .expr,
            );

            if !self.eat(TokenKind::Comma) || self.at_sequence_end() {
                self.rewind(checkpoint);
                return None;
            }
        }
    }

    fn parse_nested_list_like_expression(&mut self, outer: PendingListLikeExpression) -> Expr {
        let mut pending = vec![outer];

        let mut first_element = loop {
            let start = self.node_start();
            self.bump(TokenKind::Lsqb);

            self.report_unclosed_bracket();

            if self.eat(TokenKind::Rsqb) {
                break self.parse_named_expression_or_higher_from_lhs(
                    self.empty_list(start).into(),
                    start,
                    ExpressionContext::starred_bitwise_or(),
                );
            }

            if let Some(nested) = self.try_parse_nested_list_like_expression(start) {
                pending.push(nested);
                continue;
            }

            pending.push(PendingListLikeExpression {
                start,
                elts: vec![],
                nested_element: PendingListElement::Direct,
            });
            break self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());
        };

        while let Some(outer) = pending.pop() {
            let nested_element = match outer.nested_element {
                PendingListElement::Direct => first_element,
                PendingListElement::Starred {
                    starred_start,
                    value_start,
                } => {
                    let value = self.parse_expression_with_bitwise_or_precedence_from_lhs(
                        first_element,
                        value_start,
                    );
                    Expr::Starred(ast::ExprStarred {
                        value: Box::new(value.expr),
                        ctx: ExprContext::Load,
                        range: self.node_range(starred_start),
                        node_index: AtomicNodeIndex::NONE,
                    })
                    .into()
                }
            };

            let expr = if outer.elts.is_empty() {
                self.finish_list_like_expression(nested_element, outer.start)
            } else {
                Expr::List(self.parse_list_expression_after_element(
                    outer.elts,
                    nested_element.expr,
                    outer.start,
                ))
            };

            if pending.is_empty() {
                return expr;
            }

            first_element = self.parse_named_expression_or_higher_from_lhs(
                expr.into(),
                outer.start,
                ExpressionContext::starred_bitwise_or(),
            );
        }

        unreachable!("nested list parsing always includes the outer list");
    }

    fn finish_list_like_expression(&mut self, first_element: ParsedExpr, start: TextSize) -> Expr {
        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                self.validate_list_comprehension_element(&first_element);
                Expr::ListComp(self.parse_list_comprehension_expression(first_element.expr, start))
            }
            _ => Expr::List(self.parse_list_expression(first_element.expr, start)),
        }
    }

    fn validate_list_comprehension_element(&mut self, element: &ParsedExpr) {
        // Parenthesized starred expression isn't allowed either but that is
        // handled by the `parse_parenthesized_expression` method.

        // test_ok starred_list_comp_py315
        // # parse_options: {"target-version": "3.15"}
        // [*x for x in y]
        // [*factor.dims for factor in bases]

        // test_err starred_list_comp_py314
        // # parse_options: {"target-version": "3.14"}
        // [*x for x in y]
        if element.is_unparenthesized_starred_expr() {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                    ComprehensionUnpackingKind::IterableInList,
                ),
                element.range(),
            );
        }
    }

    fn report_unclosed_bracket(&mut self) {
        // Nice error message when having a unclosed open bracket `[`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("missing closing bracket `]`".to_string()),
                self.current_token_range(),
            );
        }
    }

    fn empty_list(&self, start: TextSize) -> Expr {
        Expr::List(ast::ExprList {
            elts: vec![],
            ctx: ExprContext::Load,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
        })
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

        self.report_unclosed_brace();

        // Return an empty `DictExpr` when finding a `}` right after the `{`
        if self.eat(TokenKind::Rbrace) {
            return self.empty_dict(start);
        }

        let after_brace = self.node_start();

        if let Some(pending) = self.try_parse_nested_set_or_dict_like_expression(start) {
            return self.parse_nested_set_or_dict_like_expression(pending);
        }

        if self.at(TokenKind::DoubleStar) {
            return self.parse_dict_unpacking_after_lbrace(start, after_brace);
        }

        // For dictionary expressions, the key uses the `expression` rule while for
        // set expressions, the element uses the `star_expression` rule. So, use the
        // one that is more general and limit it later.
        let key_or_element =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        self.finish_set_or_dict_like_expression(key_or_element, start)
    }

    fn try_parse_nested_set_or_dict_like_expression(
        &mut self,
        start: TextSize,
    ) -> Option<PendingSetOrDictLikeExpression> {
        let checkpoint = self.checkpoint();
        let mut elts = vec![];
        let mut items: Option<Vec<ast::DictItem>> = None;

        loop {
            if self.at(TokenKind::Lbrace) {
                return Some(match items {
                    Some(items) => PendingSetOrDictLikeExpression::DictKey { start, items },
                    None => PendingSetOrDictLikeExpression::Set { start, elts },
                });
            }

            if self.at(TokenKind::Star) && self.peek() == TokenKind::Lbrace {
                if items.is_some() {
                    self.rewind(checkpoint);
                    return None;
                }

                let starred_start = self.node_start();
                self.bump(TokenKind::Star);
                return Some(PendingSetOrDictLikeExpression::StarredSet {
                    start,
                    elts,
                    starred_start,
                    value_start: self.node_start(),
                });
            }

            if self.eat(TokenKind::DoubleStar) {
                if !elts.is_empty() {
                    self.rewind(checkpoint);
                    return None;
                }

                if self.at(TokenKind::Lbrace) {
                    return Some(PendingSetOrDictLikeExpression::DictUnpackingValue {
                        start,
                        items: items.unwrap_or_default(),
                    });
                }

                items.get_or_insert_default().push(ast::DictItem {
                    key: None,
                    value: self.parse_expression_with_bitwise_or_precedence().expr,
                });
            } else {
                let key_or_element =
                    self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

                if self.eat(TokenKind::Colon) {
                    if !elts.is_empty() {
                        self.rewind(checkpoint);
                        return None;
                    }

                    self.validate_dictionary_key(&key_or_element);

                    if self.at(TokenKind::Lbrace) {
                        return Some(PendingSetOrDictLikeExpression::DictValue {
                            start,
                            items: items.unwrap_or_default(),
                            key: key_or_element.expr,
                        });
                    }

                    items.get_or_insert_default().push(ast::DictItem {
                        key: Some(key_or_element.expr),
                        value: self.parse_conditional_expression_or_higher().expr,
                    });
                } else {
                    if items.is_some()
                        || matches!(
                            self.current_token_kind(),
                            TokenKind::Async | TokenKind::Colon | TokenKind::For
                        )
                    {
                        self.rewind(checkpoint);
                        return None;
                    }

                    elts.push(key_or_element);
                }
            }

            if !self.eat(TokenKind::Comma) || self.at_sequence_end() {
                self.rewind(checkpoint);
                return None;
            }
        }
    }

    fn parse_nested_set_or_dict_like_expression(
        &mut self,
        outer: PendingSetOrDictLikeExpression,
    ) -> Expr {
        let mut pending = vec![outer];

        let mut key_or_element = loop {
            let start = self.node_start();
            self.bump(TokenKind::Lbrace);

            self.report_unclosed_brace();

            if self.eat(TokenKind::Rbrace) {
                break self.parse_named_expression_or_higher_from_lhs(
                    self.empty_dict(start).into(),
                    start,
                    ExpressionContext::starred_bitwise_or(),
                );
            }

            if let Some(nested) = self.try_parse_nested_set_or_dict_like_expression(start) {
                pending.push(nested);
                continue;
            }

            if self.at(TokenKind::DoubleStar) {
                let dict = self.parse_dict_unpacking_after_lbrace(start, self.node_start());
                break self.parse_named_expression_or_higher_from_lhs(
                    dict.into(),
                    start,
                    ExpressionContext::starred_bitwise_or(),
                );
            }

            pending.push(PendingSetOrDictLikeExpression::Set {
                start,
                elts: vec![],
            });
            break self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());
        };

        while let Some(outer) = pending.pop() {
            let (expr, start) = match outer {
                PendingSetOrDictLikeExpression::Set { start, elts } if elts.is_empty() => (
                    self.finish_set_or_dict_like_expression(key_or_element, start),
                    start,
                ),
                PendingSetOrDictLikeExpression::Set { start, elts } => (
                    Expr::Set(self.parse_set_expression_after_element(elts, key_or_element, start)),
                    start,
                ),
                PendingSetOrDictLikeExpression::StarredSet {
                    start,
                    elts,
                    starred_start,
                    value_start,
                } => {
                    let value = self.parse_expression_with_bitwise_or_precedence_from_lhs(
                        key_or_element,
                        value_start,
                    );
                    let element = Expr::Starred(ast::ExprStarred {
                        value: Box::new(value.expr),
                        ctx: ExprContext::Load,
                        range: self.node_range(starred_start),
                        node_index: AtomicNodeIndex::NONE,
                    })
                    .into();

                    if elts.is_empty() {
                        (
                            self.finish_set_or_dict_like_expression(element, start),
                            start,
                        )
                    } else {
                        (
                            Expr::Set(
                                self.parse_set_expression_after_element(elts, element, start),
                            ),
                            start,
                        )
                    }
                }
                PendingSetOrDictLikeExpression::DictValue { start, items, key } => (
                    Expr::Dict(self.parse_dictionary_expression_after_item(
                        items,
                        Some(key),
                        key_or_element.expr,
                        start,
                    )),
                    start,
                ),
                PendingSetOrDictLikeExpression::DictUnpackingValue { start, items } => (
                    Expr::Dict(self.parse_dictionary_expression_after_item(
                        items,
                        None,
                        key_or_element.expr,
                        start,
                    )),
                    start,
                ),
                PendingSetOrDictLikeExpression::DictKey { start, items } => {
                    self.validate_dictionary_key(&key_or_element);
                    self.expect(TokenKind::Colon);
                    let value = self.parse_conditional_expression_or_higher().expr;

                    (
                        Expr::Dict(self.parse_dictionary_expression_after_item(
                            items,
                            Some(key_or_element.expr),
                            value,
                            start,
                        )),
                        start,
                    )
                }
            };

            if pending.is_empty() {
                return expr;
            }

            key_or_element = self.parse_named_expression_or_higher_from_lhs(
                expr.into(),
                start,
                ExpressionContext::starred_bitwise_or(),
            );
        }

        unreachable!("nested set parsing always includes the outer set");
    }

    fn parse_dict_unpacking_after_lbrace(
        &mut self,
        start: TextSize,
        unpack_start: TextSize,
    ) -> Expr {
        self.bump(TokenKind::DoubleStar);

        // Handle dictionary unpacking. Here, the grammar is `'**' bitwise_or`
        // which requires limiting the expression.
        let value = self.parse_expression_with_bitwise_or_precedence();
        let unpack_range = TextRange::new(unpack_start, value.range().end());

        if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                    ComprehensionUnpackingKind::DictInDict,
                ),
                unpack_range,
            );

            return Expr::DictComp(
                self.parse_dictionary_comprehension_expression(None, value.expr, start),
            );
        }

        if self.at(TokenKind::Colon) {
            self.add_error(ParseErrorType::InvalidStarredExpressionUsage, unpack_range);

            self.bump(TokenKind::Colon);
            let dict_value = self.parse_conditional_expression_or_higher();

            if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                return Expr::DictComp(self.parse_dictionary_comprehension_expression(
                    Some(value.expr),
                    dict_value.expr,
                    start,
                ));
            }

            return Expr::Dict(self.parse_dictionary_expression(
                Some(value.expr),
                dict_value.expr,
                start,
            ));
        }

        Expr::Dict(self.parse_dictionary_expression(None, value.expr, start))
    }

    fn finish_set_or_dict_like_expression(
        &mut self,
        key_or_element: ParsedExpr,
        start: TextSize,
    ) -> Expr {
        match self.current_token_kind() {
            TokenKind::Async | TokenKind::For => {
                self.validate_set_comprehension_element(&key_or_element);
                Expr::SetComp(self.parse_set_comprehension_expression(key_or_element.expr, start))
            }
            TokenKind::Colon => {
                // Now, we know that it's either a dictionary expression or a dictionary comprehension.
                // In either case, the key is limited to an `expression`.
                self.validate_dictionary_key(&key_or_element);

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
                    Expr::DictComp(self.parse_dictionary_comprehension_expression(
                        Some(key_or_element.expr),
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

    fn validate_set_comprehension_element(&mut self, element: &ParsedExpr) {
        if element.is_unparenthesized_starred_expr() {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                    ComprehensionUnpackingKind::IterableInSet,
                ),
                element.range(),
            );
        } else if element.is_unparenthesized_named_expr() {
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
                element.range(),
            );
        }
    }

    fn validate_dictionary_key(&mut self, key: &ParsedExpr) {
        if !key.is_parenthesized {
            match &key.expr {
                Expr::Starred(_) => {
                    self.add_error(ParseErrorType::InvalidStarredExpressionUsage, &key.expr)
                }
                Expr::Named(_) => {
                    self.add_error(ParseErrorType::UnparenthesizedNamedExpression, key)
                }
                _ => {}
            }
        }
    }

    fn report_unclosed_brace(&mut self) {
        // Nice error message when having a unclosed open brace `{`
        if self.at_ts(NEWLINE_EOF_SET) {
            self.add_error(
                ParseErrorType::OtherError("missing closing brace `}`".to_string()),
                self.current_token_range(),
            );
        }
    }

    fn empty_dict(&self, start: TextSize) -> Expr {
        Expr::Dict(ast::ExprDict {
            items: vec![],
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
        })
    }

    /// Parses an expression in parentheses, a tuple expression, or a generator expression.
    ///
    /// Matches the `(tuple | group | genexp)` rule in the [Python grammar].
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_parenthesized_expression(&mut self) -> ParsedExpr {
        let start = self.node_start();
        self.bump(TokenKind::Lpar);

        self.report_unclosed_parenthesis();

        // Return an empty `TupleExpr` when finding a `)` right after the `(`
        if self.eat(TokenKind::Rpar) {
            return self.empty_parenthesized_tuple(start);
        }

        if self.at(TokenKind::Lpar) {
            return self.parse_nested_parenthesized_expression(PendingParenthesizedExpression {
                start,
                elts: vec![],
                nested_element: PendingParenthesizedElement::Direct,
            });
        }

        if self.at(TokenKind::Star) && self.peek() == TokenKind::Lpar {
            let starred_start = self.node_start();
            self.bump(TokenKind::Star);
            return self.parse_nested_parenthesized_expression(PendingParenthesizedExpression {
                start,
                elts: vec![],
                nested_element: PendingParenthesizedElement::Starred {
                    starred_start,
                    value_start: self.node_start(),
                },
            });
        }

        let context = ExpressionContext::yield_or_starred_bitwise_or();

        if !self.at(TokenKind::Lambda) {
            let expression_start = self.node_start();
            let lhs = self.parse_lhs_expression(OperatorPrecedence::None, context);

            if self.parenthesized_binary_operator().is_some() {
                return self.parse_nested_parenthesized_binary_expression(
                    lhs,
                    expression_start,
                    start,
                    context,
                );
            }

            let parsed_expr = self.parse_named_expression_or_higher_from_simple_lhs(
                lhs,
                expression_start,
                context,
            );
            let parsed_expr = if self.at(TokenKind::Comma) {
                match self.try_parse_nested_parenthesized_tuple_expression(start, parsed_expr) {
                    Ok(pending) => return self.parse_nested_parenthesized_expression(pending),
                    Err(parsed_expr) => parsed_expr,
                }
            } else {
                parsed_expr
            };
            return self.finish_parenthesized_expression(parsed_expr, start);
        }

        // Use the more general rule of the three to parse the first element
        // and limit it later.
        let parsed_expr = self.parse_named_expression_or_higher(context);

        self.finish_parenthesized_expression(parsed_expr, start)
    }

    fn try_parse_nested_parenthesized_tuple_expression(
        &mut self,
        start: TextSize,
        first_element: ParsedExpr,
    ) -> Result<PendingParenthesizedExpression, ParsedExpr> {
        let checkpoint = self.checkpoint();
        let is_parenthesized = first_element.is_parenthesized;
        let mut elts = vec![first_element.expr];

        loop {
            if !self.eat(TokenKind::Comma) || self.at_sequence_end() {
                self.rewind(checkpoint);
                return Err(ParsedExpr {
                    expr: elts.into_iter().next().expect("first element is present"),
                    is_parenthesized,
                });
            }

            if self.at(TokenKind::Lpar) {
                return Ok(PendingParenthesizedExpression {
                    start,
                    elts,
                    nested_element: PendingParenthesizedElement::Direct,
                });
            }

            if self.at(TokenKind::Star) && self.peek() == TokenKind::Lpar {
                let starred_start = self.node_start();
                self.bump(TokenKind::Star);
                return Ok(PendingParenthesizedExpression {
                    start,
                    elts,
                    nested_element: PendingParenthesizedElement::Starred {
                        starred_start,
                        value_start: self.node_start(),
                    },
                });
            }

            elts.push(
                self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
                    .expr,
            );
        }
    }

    fn parse_nested_parenthesized_expression(
        &mut self,
        outer: PendingParenthesizedExpression,
    ) -> ParsedExpr {
        let mut pending = vec![outer];

        let mut parsed_expr = loop {
            let start = self.node_start();
            self.bump(TokenKind::Lpar);

            self.report_unclosed_parenthesis();

            if self.eat(TokenKind::Rpar) {
                let empty_tuple = self.empty_parenthesized_tuple(start);
                break self.parse_named_expression_or_higher_from_lhs(
                    empty_tuple,
                    start,
                    ExpressionContext::yield_or_starred_bitwise_or(),
                );
            }

            if self.at(TokenKind::Lpar) {
                pending.push(PendingParenthesizedExpression {
                    start,
                    elts: vec![],
                    nested_element: PendingParenthesizedElement::Direct,
                });
                continue;
            }

            if self.at(TokenKind::Star) && self.peek() == TokenKind::Lpar {
                let starred_start = self.node_start();
                self.bump(TokenKind::Star);
                pending.push(PendingParenthesizedExpression {
                    start,
                    elts: vec![],
                    nested_element: PendingParenthesizedElement::Starred {
                        starred_start,
                        value_start: self.node_start(),
                    },
                });
                continue;
            }

            let parsed_expr = self
                .parse_named_expression_or_higher(ExpressionContext::yield_or_starred_bitwise_or());
            if self.at(TokenKind::Comma) {
                match self.try_parse_nested_parenthesized_tuple_expression(start, parsed_expr) {
                    Ok(nested) => {
                        pending.push(nested);
                        continue;
                    }
                    Err(parsed_expr) => {
                        pending.push(PendingParenthesizedExpression {
                            start,
                            elts: vec![],
                            nested_element: PendingParenthesizedElement::Direct,
                        });
                        break parsed_expr;
                    }
                }
            }

            pending.push(PendingParenthesizedExpression {
                start,
                elts: vec![],
                nested_element: PendingParenthesizedElement::Direct,
            });
            break parsed_expr;
        };

        while let Some(outer) = pending.pop() {
            let nested_element = match outer.nested_element {
                PendingParenthesizedElement::Direct => parsed_expr,
                PendingParenthesizedElement::Starred {
                    starred_start,
                    value_start,
                } => {
                    let value = self.parse_expression_with_bitwise_or_precedence_from_lhs(
                        parsed_expr,
                        value_start,
                    );
                    Expr::Starred(ast::ExprStarred {
                        value: Box::new(value.expr),
                        ctx: ExprContext::Load,
                        range: self.node_range(starred_start),
                        node_index: AtomicNodeIndex::NONE,
                    })
                    .into()
                }
            };

            parsed_expr = if outer.elts.is_empty() {
                self.finish_parenthesized_expression(nested_element, outer.start)
            } else {
                Expr::Tuple(self.finish_parenthesized_tuple_expression_after_element(
                    outer.elts,
                    nested_element.expr,
                    outer.start,
                ))
                .into()
            };

            if !pending.is_empty() {
                parsed_expr = self.parse_named_expression_or_higher_from_lhs(
                    parsed_expr,
                    outer.start,
                    ExpressionContext::yield_or_starred_bitwise_or(),
                );
            }
        }

        parsed_expr
    }

    fn parenthesized_binary_operator(&mut self) -> Option<Operator> {
        let operator = self.current_token_kind().as_binary_operator()?;

        (self.peek() == TokenKind::Lpar).then_some(operator)
    }

    fn parse_nested_parenthesized_binary_expression(
        &mut self,
        mut lhs: ParsedExpr,
        mut expression_start: TextSize,
        mut parenthesis_start: TextSize,
        context: ExpressionContext,
    ) -> ParsedExpr {
        let mut expressions = Vec::new();

        let mut parsed_expr = loop {
            let operator = self
                .parenthesized_binary_operator()
                .expect("nested parenthesized binary parsing starts at a binary operator");
            self.bump(TokenKind::from(operator));

            let right_start = self.node_start();
            self.bump(TokenKind::Lpar);
            self.report_unclosed_parenthesis();

            expressions.push(PendingParenthesizedBinaryExpression {
                left: lhs.expr,
                op: operator,
                expression_start,
                parenthesis_start,
                right_start,
            });

            if self.eat(TokenKind::Rpar) {
                break self.empty_parenthesized_tuple(right_start);
            }

            if self.at(TokenKind::Lpar) {
                break self.parse_nested_parenthesized_expression(PendingParenthesizedExpression {
                    start: right_start,
                    elts: vec![],
                    nested_element: PendingParenthesizedElement::Direct,
                });
            }

            expression_start = self.node_start();
            parenthesis_start = right_start;

            if self.at(TokenKind::Lambda) {
                let parsed_expr = self.parse_named_expression_or_higher(context);
                break self.finish_parenthesized_expression(parsed_expr, parenthesis_start);
            }

            lhs = self.parse_lhs_expression(OperatorPrecedence::None, context);

            if self.parenthesized_binary_operator().is_none() {
                let parsed_expr = self.parse_named_expression_or_higher_from_simple_lhs(
                    lhs,
                    expression_start,
                    context,
                );
                break self.finish_parenthesized_expression(parsed_expr, parenthesis_start);
            }
        };

        while let Some(expression) = expressions.pop() {
            let right = self.parse_binary_expression_or_higher_from_lhs(
                parsed_expr,
                OperatorPrecedence::from(expression.op),
                context,
                expression.right_start,
            );

            parsed_expr = Expr::BinOp(ast::ExprBinOp {
                left: Box::new(expression.left),
                op: expression.op,
                right: Box::new(right.expr),
                range: self.node_range(expression.expression_start),
                node_index: AtomicNodeIndex::NONE,
            })
            .into();
            parsed_expr = self.parse_named_expression_or_higher_from_simple_lhs(
                parsed_expr,
                expression.expression_start,
                context,
            );
            parsed_expr =
                self.finish_parenthesized_expression(parsed_expr, expression.parenthesis_start);
        }

        parsed_expr
    }

    fn report_unclosed_parenthesis(&mut self) {
        // Nice error message when having a unclosed open parenthesis `(`
        if self.at_ts(NEWLINE_EOF_SET) {
            let range = self.current_token_range();
            self.add_error(
                ParseErrorType::OtherError("missing closing parenthesis `)`".to_string()),
                range,
            );
        }
    }

    fn empty_parenthesized_tuple(&self, start: TextSize) -> ParsedExpr {
        Expr::Tuple(ast::ExprTuple {
            elts: vec![],
            ctx: ExprContext::Load,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            parenthesized: true,
        })
        .into()
    }

    fn finish_parenthesized_expression(
        &mut self,
        mut parsed_expr: ParsedExpr,
        start: TextSize,
    ) -> ParsedExpr {
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
                self.validate_generator_expression_element(&parsed_expr);
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

    fn validate_generator_expression_element(&mut self, element: &ParsedExpr) {
        if element.is_unparenthesized_starred_expr() {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::UnpackingInComprehension(
                    ComprehensionUnpackingKind::IterableInGenerator,
                ),
                element.range(),
            );
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
            node_index: AtomicNodeIndex::NONE,
            parenthesized: parenthesized.is_yes(),
        }
    }

    fn finish_parenthesized_tuple_expression_after_element(
        &mut self,
        mut elts: Vec<Expr>,
        element: Expr,
        start: TextSize,
    ) -> ast::ExprTuple {
        elts.push(element);

        if self.eat(TokenKind::Comma) {
            if !self.at(TokenKind::Rpar) {
                self.parse_comma_separated_list(
                    RecoveryContextKind::TupleElements(Parenthesized::Yes),
                    |parser| {
                        elts.push(
                            parser
                                .parse_named_expression_or_higher(
                                    ExpressionContext::starred_bitwise_or(),
                                )
                                .expr,
                        );
                    },
                );
            }
        } else if !self.at(TokenKind::Rpar) {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(
                RecoveryContextKind::TupleElements(Parenthesized::Yes),
                |parser| {
                    elts.push(
                        parser
                            .parse_named_expression_or_higher(
                                ExpressionContext::starred_bitwise_or(),
                            )
                            .expr,
                    );
                },
            );
        }

        self.expect(TokenKind::Rpar);

        ast::ExprTuple {
            elts,
            ctx: ExprContext::Load,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            parenthesized: true,
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
            node_index: AtomicNodeIndex::NONE,
        }
    }

    fn parse_list_expression_after_element(
        &mut self,
        mut elts: Vec<Expr>,
        element: Expr,
        start: TextSize,
    ) -> ast::ExprList {
        elts.push(element);

        if self.eat(TokenKind::Comma) {
            if !self.at_sequence_end() {
                self.parse_comma_separated_list(RecoveryContextKind::ListElements, |parser| {
                    elts.push(
                        parser
                            .parse_named_expression_or_higher(
                                ExpressionContext::starred_bitwise_or(),
                            )
                            .expr,
                    );
                });
            }
        } else if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(RecoveryContextKind::ListElements, |parser| {
                elts.push(
                    parser
                        .parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
                        .expr,
                );
            });
        }

        self.expect(TokenKind::Rsqb);

        ast::ExprList {
            elts,
            ctx: ExprContext::Load,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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

        self.validate_set_expression_element(&first_element);

        let mut elts = vec![first_element.expr];

        self.parse_comma_separated_list(RecoveryContextKind::SetElements, |parser| {
            let parsed_expr =
                parser.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

            parser.validate_set_expression_element(&parsed_expr);

            elts.push(parsed_expr.expr);
        });

        self.expect(TokenKind::Rbrace);

        ast::ExprSet {
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            elts,
        }
    }

    fn parse_set_expression_after_element(
        &mut self,
        mut elements: Vec<ParsedExpr>,
        element: ParsedExpr,
        start: TextSize,
    ) -> ast::ExprSet {
        elements.push(element);

        let mut elts = Vec::with_capacity(elements.len());
        for element in elements {
            self.validate_set_expression_element(&element);
            elts.push(element.expr);
        }

        if self.eat(TokenKind::Comma) {
            if !self.at_sequence_end() {
                self.parse_comma_separated_list(RecoveryContextKind::SetElements, |parser| {
                    let parsed_expr = parser
                        .parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

                    parser.validate_set_expression_element(&parsed_expr);

                    elts.push(parsed_expr.expr);
                });
            }
        } else if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(RecoveryContextKind::SetElements, |parser| {
                let parsed_expr = parser
                    .parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

                parser.validate_set_expression_element(&parsed_expr);

                elts.push(parsed_expr.expr);
            });
        }

        self.expect(TokenKind::Rbrace);

        ast::ExprSet {
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            elts,
        }
    }

    fn validate_set_expression_element(&mut self, element: &ParsedExpr) {
        if element.is_unparenthesized_named_expr() {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                    UnparenthesizedNamedExprKind::SetLiteral,
                ),
                element.range(),
            );
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
        self.parse_dictionary_expression_after_item(vec![], key, value, start)
    }

    fn parse_dictionary_expression_after_item(
        &mut self,
        mut items: Vec<ast::DictItem>,
        key: Option<Expr>,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprDict {
        items.push(ast::DictItem { key, value });

        if self.eat(TokenKind::Comma) {
            if !self.at_sequence_end() {
                self.parse_dictionary_expression_items(&mut items);
            }
        } else if !self.at_sequence_end() {
            self.expect(TokenKind::Comma);
            self.parse_dictionary_expression_items(&mut items);
        }

        self.expect(TokenKind::Rbrace);

        ast::ExprDict {
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            items,
        }
    }

    fn parse_dictionary_expression_items(&mut self, items: &mut Vec<ast::DictItem>) {
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
        let pending = self.parse_comprehension_header();
        let iter = self.parse_comprehension_iter();
        self.finish_comprehension(pending, iter)
    }

    fn parse_comprehension_header(&mut self) -> PendingComprehension {
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

        PendingComprehension {
            start,
            target: target.expr,
            is_async,
        }
    }

    fn parse_comprehension_iter(&mut self) -> Expr {
        if matches!(
            self.current_token_kind(),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
        ) && let Some(iter) = self.try_parse_nested_comprehension_iter()
        {
            return iter;
        }

        self.parse_simple_expression(ExpressionContext::default())
            .expr
    }

    fn try_parse_nested_comprehension_iter(&mut self) -> Option<Expr> {
        let mut pending = vec![];

        let mut iter = loop {
            let checkpoint = self.checkpoint();
            let next = match self.current_token_kind() {
                TokenKind::Lpar => self.try_parse_generator_expression_iter(),
                TokenKind::Lsqb => self.try_parse_list_comprehension_iter(),
                TokenKind::Lbrace => self.try_parse_set_or_dict_comprehension_iter(),
                _ => None,
            };

            let Some(next) = next else {
                self.rewind(checkpoint);

                if pending.is_empty() {
                    return None;
                }

                break self
                    .parse_simple_expression(ExpressionContext::default())
                    .expr;
            };

            pending.push(next);

            if !matches!(
                self.current_token_kind(),
                TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
            ) {
                break self
                    .parse_simple_expression(ExpressionContext::default())
                    .expr;
            }
        };

        while let Some(outer) = pending.pop() {
            iter = outer.finish(self, iter);
        }

        Some(iter)
    }

    fn try_parse_generator_expression_iter(&mut self) -> Option<PendingComprehensionIter> {
        let start = self.node_start();

        self.bump(TokenKind::Lpar);
        self.report_unclosed_parenthesis();

        if self.eat(TokenKind::Rpar) {
            return None;
        }

        let element =
            self.parse_named_expression_or_higher(ExpressionContext::yield_or_starred_bitwise_or());

        if !matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
            return None;
        }

        self.validate_generator_expression_element(&element);
        Some(PendingComprehensionIter::Generator {
            start,
            element: element.expr,
            comprehension: self.parse_comprehension_header(),
        })
    }

    fn try_parse_list_comprehension_iter(&mut self) -> Option<PendingComprehensionIter> {
        let start = self.node_start();

        self.bump(TokenKind::Lsqb);
        self.report_unclosed_bracket();

        if self.eat(TokenKind::Rsqb) {
            return None;
        }

        let element =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        if !matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
            return None;
        }

        self.validate_list_comprehension_element(&element);
        Some(PendingComprehensionIter::List {
            start,
            element: element.expr,
            comprehension: self.parse_comprehension_header(),
        })
    }

    fn try_parse_set_or_dict_comprehension_iter(&mut self) -> Option<PendingComprehensionIter> {
        let start = self.node_start();

        self.bump(TokenKind::Lbrace);
        self.report_unclosed_brace();

        if self.eat(TokenKind::Rbrace) {
            return None;
        }

        let element =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
            self.validate_set_comprehension_element(&element);
            return Some(PendingComprehensionIter::Set {
                start,
                element: element.expr,
                comprehension: self.parse_comprehension_header(),
            });
        }

        if !self.eat(TokenKind::Colon) {
            return None;
        }

        self.validate_dictionary_key(&element);
        let value = self.parse_conditional_expression_or_higher();

        if !matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
            return None;
        }

        Some(PendingComprehensionIter::Dict {
            start,
            key: element.expr,
            value: value.expr,
            comprehension: self.parse_comprehension_header(),
        })
    }

    fn finish_comprehension(
        &mut self,
        pending: PendingComprehension,
        iter: Expr,
    ) -> ast::Comprehension {
        let ifs = self.parse_comprehension_ifs();
        self.comprehension(pending, iter, ifs)
    }

    fn parse_comprehension_ifs(&mut self) -> Vec<Expr> {
        let mut ifs = vec![];
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::If) {
            progress.assert_progressing(self);
            ifs.push(self.parse_comprehension_if());
        }

        ifs
    }

    fn parse_comprehension_if(&mut self) -> Expr {
        if matches!(
            self.current_token_kind(),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
        ) && let Some(expr) = self.try_parse_nested_comprehension_if()
        {
            return expr;
        }

        self.parse_simple_expression(ExpressionContext::default())
            .expr
    }

    fn try_parse_nested_comprehension_if(&mut self) -> Option<Expr> {
        let checkpoint = self.checkpoint();
        let mut frame = match self.try_parse_comprehension_if_frame() {
            Some(frame) => frame,
            None => {
                self.rewind(checkpoint);
                return None;
            }
        };
        let mut pending = vec![];

        'frames: loop {
            let mut progress = ParserProgress::default();

            while self.eat(TokenKind::If) {
                progress.assert_progressing(self);

                if matches!(
                    self.current_token_kind(),
                    TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
                ) {
                    let checkpoint = self.checkpoint();

                    if let Some(nested) = self.try_parse_comprehension_if_frame() {
                        pending.push(frame);
                        frame = nested;
                        continue 'frames;
                    }

                    self.rewind(checkpoint);
                }

                frame.ifs.push(
                    self.parse_simple_expression(ExpressionContext::default())
                        .expr,
                );
            }

            let expr = frame.finish(self);

            let Some(mut outer) = pending.pop() else {
                return Some(expr);
            };

            outer.ifs.push(expr);
            frame = outer;
        }
    }

    fn try_parse_comprehension_if_frame(&mut self) -> Option<PendingComprehensionIf> {
        let expression = match self.current_token_kind() {
            TokenKind::Lpar => self.try_parse_generator_expression_iter(),
            TokenKind::Lsqb => self.try_parse_list_comprehension_iter(),
            TokenKind::Lbrace => self.try_parse_set_or_dict_comprehension_iter(),
            _ => None,
        };

        Some(PendingComprehensionIf {
            expression: expression?,
            iter: self.parse_comprehension_iter(),
            ifs: vec![],
        })
    }

    fn comprehension(
        &self,
        pending: PendingComprehension,
        iter: Expr,
        ifs: Vec<Expr>,
    ) -> ast::Comprehension {
        let PendingComprehension {
            start,
            target,
            is_async,
        } = pending;

        ast::Comprehension {
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
            target,
            iter,
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
        self.generator_expression(element, generators, start, parenthesized)
    }

    fn generator_expression(
        &mut self,
        element: Expr,
        generators: Vec<ast::Comprehension>,
        start: TextSize,
        parenthesized: Parenthesized,
    ) -> ast::ExprGenerator {
        if parenthesized.is_yes() {
            self.expect(TokenKind::Rpar);
        }

        ast::ExprGenerator {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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
        self.list_comprehension_expression(element, generators, start)
    }

    fn list_comprehension_expression(
        &mut self,
        element: Expr,
        generators: Vec<ast::Comprehension>,
        start: TextSize,
    ) -> ast::ExprListComp {
        self.expect(TokenKind::Rsqb);

        ast::ExprListComp {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
        }
    }

    /// Parses a dictionary comprehension expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries>
    fn parse_dictionary_comprehension_expression(
        &mut self,
        key: Option<Expr>,
        value: Expr,
        start: TextSize,
    ) -> ast::ExprDictComp {
        let generators = self.parse_generators();
        self.dictionary_comprehension_expression(key, value, generators, start)
    }

    fn dictionary_comprehension_expression(
        &mut self,
        key: Option<Expr>,
        value: Expr,
        generators: Vec<ast::Comprehension>,
        start: TextSize,
    ) -> ast::ExprDictComp {
        self.expect(TokenKind::Rbrace);

        ast::ExprDictComp {
            key: key.map(Box::new),
            value: Box::new(value),
            generators,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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
        self.set_comprehension_expression(element, generators, start)
    }

    fn set_comprehension_expression(
        &mut self,
        element: Expr,
        generators: Vec<ast::Comprehension>,
        start: TextSize,
    ) -> ast::ExprSetComp {
        self.expect(TokenKind::Rbrace);

        ast::ExprSetComp {
            elt: Box::new(element),
            generators,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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
            StarredExpressionPrecedence::Conditional => self
                .parse_conditional_expression_or_higher_unrolling_nested_trailers_with_context(
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

        ast::ExprStarred {
            value: Box::new(parsed_expr.expr),
            ctx: ExprContext::Load,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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

        self.await_expression(parsed_expr.expr, start)
    }

    fn parse_nested_await_expression(&mut self) -> ast::ExprAwait {
        let mut expressions = vec![];

        while self.at(TokenKind::Await) {
            let start = self.node_start();
            self.bump(TokenKind::Await);

            expressions.push(PendingAwaitExpression {
                start,
                operand_start: self.node_start(),
            });
        }

        let innermost = expressions
            .pop()
            .expect("nested await parsing always includes the outer await expression");
        let operand = self.parse_binary_expression_or_higher(
            OperatorPrecedence::Await,
            ExpressionContext::default(),
        );
        let mut expression = self.await_expression(operand.expr, innermost.start);

        while let Some(outer) = expressions.pop() {
            self.add_error(
                ParseErrorType::OtherError("Await expression cannot be used here".to_string()),
                &expression,
            );

            let operand = self.parse_binary_expression_or_higher_recursive(
                Expr::Await(expression).into(),
                OperatorPrecedence::Await,
                ExpressionContext::default(),
                outer.operand_start,
            );
            expression = self.await_expression(operand.expr, outer.start);
        }

        expression
    }

    fn await_expression(&self, value: Expr, start: TextSize) -> ast::ExprAwait {
        ast::ExprAwait {
            value: Box::new(value),
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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

        if self.at(TokenKind::Lpar) && self.peek() == TokenKind::Yield {
            return self.parse_nested_parenthesized_yield_expression(start);
        }

        self.finish_yield_expression(start)
    }

    fn parse_nested_parenthesized_yield_expression(&mut self, mut yield_start: TextSize) -> Expr {
        let mut pending = vec![];

        let mut expr = loop {
            let parenthesis_start = self.node_start();
            self.bump(TokenKind::Lpar);
            self.report_unclosed_parenthesis();

            let nested_yield_start = self.node_start();
            self.bump(TokenKind::Yield);

            pending.push(PendingParenthesizedYieldExpression {
                yield_start,
                parenthesis_start,
            });

            if self.at(TokenKind::Lpar) && self.peek() == TokenKind::Yield {
                yield_start = nested_yield_start;
                continue;
            }

            break self.finish_yield_expression(nested_yield_start);
        };

        while let Some(outer) = pending.pop() {
            let value = self.finish_parenthesized_expression(expr.into(), outer.parenthesis_start);
            expr = Expr::Yield(ast::ExprYield {
                value: Some(Box::new(value.expr)),
                range: self.node_range(outer.yield_start),
                node_index: AtomicNodeIndex::NONE,
            });
        }

        expr
    }

    fn finish_yield_expression(&mut self, start: TextSize) -> Expr {
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
            node_index: AtomicNodeIndex::NONE,
        })
    }

    /// Parses a `yield from` expression.
    ///
    /// This method should not be used directly. Use [`Parser::parse_yield_expression`]
    /// even when parsing a `yield from` expression.
    ///
    /// See: <https://docs.python.org/3/reference/expressions.html#yield-expressions>
    fn parse_yield_from_expression(&mut self, start: TextSize) -> Expr {
        if let Some(expr) = self.try_parse_nested_parenthesized_yield_from_expression(start) {
            return expr;
        }

        self.finish_yield_from_expression(start)
    }

    fn try_parse_nested_parenthesized_yield_from_expression(
        &mut self,
        mut yield_start: TextSize,
    ) -> Option<Expr> {
        if !self.at(TokenKind::Lpar) || self.peek() != TokenKind::Yield {
            return None;
        }

        let checkpoint = self.checkpoint();
        let mut pending = vec![];

        let mut expr = loop {
            let parenthesis_start = self.node_start();
            self.bump(TokenKind::Lpar);
            self.report_unclosed_parenthesis();

            let nested_yield_start = self.node_start();
            self.bump(TokenKind::Yield);

            if !self.eat(TokenKind::From) {
                self.rewind(checkpoint);
                return None;
            }

            pending.push(PendingParenthesizedYieldExpression {
                yield_start,
                parenthesis_start,
            });

            if self.at(TokenKind::Lpar) && self.peek() == TokenKind::Yield {
                yield_start = nested_yield_start;
                continue;
            }

            break self.finish_yield_from_expression(nested_yield_start);
        };

        while let Some(outer) = pending.pop() {
            let value = self.finish_parenthesized_expression(expr.into(), outer.parenthesis_start);
            expr = Expr::YieldFrom(ast::ExprYieldFrom {
                value: Box::new(value.expr),
                range: self.node_range(outer.yield_start),
                node_index: AtomicNodeIndex::NONE,
            });
        }

        Some(expr)
    }

    fn finish_yield_from_expression(&mut self, start: TextSize) -> Expr {
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
            node_index: AtomicNodeIndex::NONE,
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

        self.prepare_named_expression_target(&mut target);

        let target = match self.try_parse_nested_parenthesized_named_expression(target, start) {
            Ok(named) => return named,
            Err(target) => target,
        };

        let value = self.parse_conditional_expression_or_higher();
        self.named_expression(target, value.expr, start)
    }

    fn try_parse_nested_parenthesized_named_expression(
        &mut self,
        mut target: Expr,
        mut start: TextSize,
    ) -> Result<ast::ExprNamed, Expr> {
        if !self.at(TokenKind::Lpar) || self.peek() != TokenKind::Name {
            return Err(target);
        }

        let mut pending = vec![];

        let mut named = loop {
            let checkpoint = self.checkpoint();
            let parenthesis_start = self.node_start();
            self.bump(TokenKind::Lpar);
            self.report_unclosed_parenthesis();

            let nested_start = self.node_start();
            let mut nested_target = self.parse_atom().expr;

            if !self.eat(TokenKind::ColonEqual) {
                self.rewind(checkpoint);

                if pending.is_empty() {
                    return Err(target);
                }

                let value = self.parse_conditional_expression_or_higher();
                break self.named_expression(target, value.expr, start);
            }

            self.prepare_named_expression_target(&mut nested_target);
            pending.push(PendingParenthesizedNamedExpression {
                target,
                start,
                parenthesis_start,
            });

            target = nested_target;
            start = nested_start;

            if !self.at(TokenKind::Lpar) || self.peek() != TokenKind::Name {
                let value = self.parse_conditional_expression_or_higher();
                break self.named_expression(target, value.expr, start);
            }
        };

        while let Some(outer) = pending.pop() {
            let value = self.finish_parenthesized_expression(
                Expr::Named(named).into(),
                outer.parenthesis_start,
            );
            named = self.named_expression(outer.target, value.expr, outer.start);
        }

        Ok(named)
    }

    fn prepare_named_expression_target(&mut self, target: &mut Expr) {
        if !target.is_name_expr() {
            self.add_error(ParseErrorType::InvalidNamedAssignmentTarget, target.range());
        }
        helpers::set_expr_ctx(target, ExprContext::Store);
    }

    fn named_expression(&mut self, target: Expr, value: Expr, start: TextSize) -> ast::ExprNamed {
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
            value: Box::new(value),
            range,
            node_index: AtomicNodeIndex::NONE,
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
        let pending = self.parse_lambda_header();

        if self.at(TokenKind::Lambda) {
            return self.parse_nested_lambda_expr(pending);
        }

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

        // `lambda: lambda: lambda: ...` recurses through the lambda body at
        // the conditional layer, bypassing the `parse_lhs_expression` guard.
        let body =
            if let Some(body) = self.with_recursion(Self::parse_conditional_expression_or_higher) {
                body
            } else {
                self.report_recursion_limit_exceeded(self.current_token_range());
                self.recursion_recovery_expr()
            };

        self.lambda_expression(pending, body.expr)
    }

    fn parse_lambda_header(&mut self) -> PendingLambdaExpression {
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

        PendingLambdaExpression { start, parameters }
    }

    fn parse_nested_lambda_expr(&mut self, outer: PendingLambdaExpression) -> ast::ExprLambda {
        let mut pending = vec![outer];

        while self.at(TokenKind::Lambda) {
            pending.push(self.parse_lambda_header());
        }

        let body = self.parse_conditional_expression_or_higher();
        let innermost = pending
            .pop()
            .expect("nested lambda parsing includes the outer lambda");
        let mut lambda = self.lambda_expression(innermost, body.expr);

        while let Some(outer) = pending.pop() {
            lambda = self.lambda_expression(outer, Expr::Lambda(lambda));
        }

        lambda
    }

    fn lambda_expression(&self, pending: PendingLambdaExpression, body: Expr) -> ast::ExprLambda {
        let PendingLambdaExpression { start, parameters } = pending;
        ast::ExprLambda {
            body: Box::new(body),
            parameters,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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
        let pending = self.parse_if_expression_header(body, start);

        if self.at(TokenKind::Lambda) {
            let orelse = self.parse_conditional_expression_or_higher();
            return self.if_expression(pending, orelse.expr);
        }

        let orelse_start = self.node_start();
        let orelse = self.parse_simple_expression(ExpressionContext::default());

        if self.at(TokenKind::If) {
            self.parse_nested_if_expression(pending, orelse.expr, orelse_start)
        } else {
            self.if_expression(pending, orelse.expr)
        }
    }

    fn parse_if_expression_header(&mut self, body: Expr, start: TextSize) -> PendingIfExpression {
        self.bump(TokenKind::If);

        let test = self.parse_simple_expression(ExpressionContext::default());

        self.expect(TokenKind::Else);

        PendingIfExpression {
            body,
            test: test.expr,
            start,
        }
    }

    fn parse_nested_if_expression(
        &mut self,
        outer: PendingIfExpression,
        mut body: Expr,
        mut start: TextSize,
    ) -> ast::ExprIf {
        let mut pending = vec![outer];

        let mut orelse = loop {
            pending.push(self.parse_if_expression_header(body, start));

            if self.at(TokenKind::Lambda) {
                break self.parse_conditional_expression_or_higher().expr;
            }

            start = self.node_start();
            let parsed_orelse = self.parse_simple_expression(ExpressionContext::default());

            if self.at(TokenKind::If) {
                body = parsed_orelse.expr;
                continue;
            }

            break parsed_orelse.expr;
        };

        let innermost = pending
            .pop()
            .expect("nested if parsing includes the outer if expression");
        let mut if_expr = self.if_expression(innermost, orelse);

        while let Some(outer) = pending.pop() {
            orelse = Expr::If(if_expr);
            if_expr = self.if_expression(outer, orelse);
        }

        if_expr
    }

    fn if_expression(&self, pending: PendingIfExpression, orelse: Expr) -> ast::ExprIf {
        let PendingIfExpression { body, test, start } = pending;
        ast::ExprIf {
            body: Box::new(body),
            test: Box::new(test),
            orelse: Box::new(orelse),
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
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
            node_index: AtomicNodeIndex::NONE,
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
    /// 2. If there are more than one argument (positional or keyword) or a single argument with a
    ///    trailing comma, all generator expressions present should be parenthesized.
    fn validate_arguments(&mut self, arguments: &ast::Arguments, has_trailing_comma: bool) {
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

        if has_trailing_comma || arguments.len() > 1 {
            for arg in &*arguments.args {
                if let Some(ast::ExprGenerator {
                    range,
                    parenthesized: false,
                    ..
                }) = arg.as_generator_expr()
                {
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

struct PendingCall {
    func: Expr,
    start: TextSize,
    arguments_start: TextSize,
    argument: PendingCallArgument,
    state: ArgumentParsingState,
}

struct NestedCallArgument {
    func: Expr,
    start: TextSize,
    argument: PendingCallArgument,
    state: ArgumentParsingState,
}

enum PendingCallArgument {
    Positional {
        argument_start: TextSize,
    },
    Keyword {
        argument_start: TextSize,
        arg: ast::Identifier,
        value_start: TextSize,
    },
    KeywordUnpacking {
        argument_start: TextSize,
        value_start: TextSize,
    },
    Starred {
        argument_start: TextSize,
        value_start: TextSize,
    },
}

impl PendingCallArgument {
    const fn value_start(&self) -> TextSize {
        match self {
            PendingCallArgument::Positional { argument_start } => *argument_start,
            PendingCallArgument::Keyword { value_start, .. }
            | PendingCallArgument::KeywordUnpacking { value_start, .. }
            | PendingCallArgument::Starred { value_start, .. } => *value_start,
        }
    }
}

struct PendingListLikeExpression {
    start: TextSize,
    elts: Vec<Expr>,
    nested_element: PendingListElement,
}

enum PendingListElement {
    Direct,
    Starred {
        starred_start: TextSize,
        value_start: TextSize,
    },
}

enum PendingSetOrDictLikeExpression {
    Set {
        start: TextSize,
        elts: Vec<ParsedExpr>,
    },
    StarredSet {
        start: TextSize,
        elts: Vec<ParsedExpr>,
        starred_start: TextSize,
        value_start: TextSize,
    },
    DictValue {
        start: TextSize,
        items: Vec<ast::DictItem>,
        key: Expr,
    },
    DictUnpackingValue {
        start: TextSize,
        items: Vec<ast::DictItem>,
    },
    DictKey {
        start: TextSize,
        items: Vec<ast::DictItem>,
    },
}

#[derive(Default)]
struct ArgumentParsingState {
    args: Vec<Expr>,
    keywords: Vec<ast::Keyword>,
    seen_keyword_argument: bool,
    seen_keyword_unpacking: bool,
}

struct PendingParenthesizedExpression {
    start: TextSize,
    elts: Vec<Expr>,
    nested_element: PendingParenthesizedElement,
}

enum PendingParenthesizedElement {
    Direct,
    Starred {
        starred_start: TextSize,
        value_start: TextSize,
    },
}

struct PendingParenthesizedBinaryExpression {
    left: Expr,
    op: Operator,
    expression_start: TextSize,
    parenthesis_start: TextSize,
    right_start: TextSize,
}

struct PendingSubscript {
    value: Expr,
    start: TextSize,
    slice_start: TextSize,
    slices: Vec<Expr>,
    nested_slice: PendingSubscriptSlice,
}

struct NestedSubscriptSlice {
    value: Expr,
    start: TextSize,
    slices: Vec<Expr>,
    nested_slice: PendingSubscriptSlice,
}

enum PendingSubscriptSlice {
    Direct,
    Starred {
        starred_start: TextSize,
        value_start: TextSize,
    },
    Upper {
        value_start: TextSize,
    },
    Step {
        value_start: TextSize,
    },
}

struct PendingUnaryExpression {
    op: UnaryOp,
    start: TextSize,
    operand_start: TextSize,
}

struct PendingPowerExpression {
    left: Expr,
    start: TextSize,
}

struct PendingAwaitExpression {
    start: TextSize,
    operand_start: TextSize,
}

struct PendingParenthesizedYieldExpression {
    yield_start: TextSize,
    parenthesis_start: TextSize,
}

struct PendingParenthesizedNamedExpression {
    target: Expr,
    start: TextSize,
    parenthesis_start: TextSize,
}

struct PendingComprehension {
    start: TextSize,
    target: Expr,
    is_async: bool,
}

struct PendingComprehensionIf {
    expression: PendingComprehensionIter,
    iter: Expr,
    ifs: Vec<Expr>,
}

struct PendingLambdaExpression {
    start: TextSize,
    parameters: Option<Box<ast::Parameters>>,
}

struct PendingIfExpression {
    body: Expr,
    test: Expr,
    start: TextSize,
}

enum PendingComprehensionIter {
    List {
        start: TextSize,
        element: Expr,
        comprehension: PendingComprehension,
    },
    Set {
        start: TextSize,
        element: Expr,
        comprehension: PendingComprehension,
    },
    Dict {
        start: TextSize,
        key: Expr,
        value: Expr,
        comprehension: PendingComprehension,
    },
    Generator {
        start: TextSize,
        element: Expr,
        comprehension: PendingComprehension,
    },
}

impl PendingComprehensionIter {
    fn finish(self, parser: &mut Parser, iter: Expr) -> Expr {
        let ifs = parser.parse_comprehension_ifs();
        self.finish_with_ifs(parser, iter, ifs)
    }

    fn finish_with_ifs(self, parser: &mut Parser, iter: Expr, ifs: Vec<Expr>) -> Expr {
        match self {
            PendingComprehensionIter::List {
                start,
                element,
                comprehension,
            } => {
                let first = parser.comprehension(comprehension, iter, ifs);
                let mut generators = vec![first];
                generators.extend(parser.parse_generators());
                Expr::ListComp(parser.list_comprehension_expression(element, generators, start))
            }
            PendingComprehensionIter::Set {
                start,
                element,
                comprehension,
            } => {
                let first = parser.comprehension(comprehension, iter, ifs);
                let mut generators = vec![first];
                generators.extend(parser.parse_generators());
                Expr::SetComp(parser.set_comprehension_expression(element, generators, start))
            }
            PendingComprehensionIter::Dict {
                start,
                key,
                value,
                comprehension,
            } => {
                let first = parser.comprehension(comprehension, iter, ifs);
                let mut generators = vec![first];
                generators.extend(parser.parse_generators());
                Expr::DictComp(parser.dictionary_comprehension_expression(
                    Some(key),
                    value,
                    generators,
                    start,
                ))
            }
            PendingComprehensionIter::Generator {
                start,
                element,
                comprehension,
            } => {
                let first = parser.comprehension(comprehension, iter, ifs);
                let mut generators = vec![first];
                generators.extend(parser.parse_generators());
                Expr::Generator(parser.generator_expression(
                    element,
                    generators,
                    start,
                    Parenthesized::Yes,
                ))
            }
        }
    }
}

impl PendingComprehensionIf {
    fn finish(self, parser: &mut Parser) -> Expr {
        self.expression.finish_with_ifs(parser, self.iter, self.ifs)
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

    pub(super) fn disallow_starred_expressions(self) -> Self {
        let flags = self.0 & !ExpressionContextFlags::ALLOW_STARRED_EXPRESSION;
        ExpressionContext(flags)
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

#[derive(Debug)]
struct InterpolatedStringData {
    elements: InterpolatedStringElements,
    range: TextRange,
    flags: AnyStringFlags,
}

impl From<InterpolatedStringData> for FString {
    fn from(value: InterpolatedStringData) -> Self {
        Self {
            elements: value.elements,
            range: value.range,
            flags: value.flags.into(),
            node_index: AtomicNodeIndex::NONE,
        }
    }
}

impl From<InterpolatedStringData> for TString {
    fn from(value: InterpolatedStringData) -> Self {
        Self {
            elements: value.elements,
            range: value.range,
            flags: value.flags.into(),
            node_index: AtomicNodeIndex::NONE,
        }
    }
}

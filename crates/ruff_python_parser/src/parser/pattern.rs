use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr, ExprContext, Number, Operator, Pattern, Singleton};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::progress::ParserProgress;
use crate::parser::{recovery, Parser, RecoveryContextKind, SequenceMatchPatternParentheses};
use crate::token::{TokenKind, TokenValue};
use crate::token_set::TokenSet;
use crate::ParseErrorType;

use super::expression::ExpressionContext;

/// The set of tokens that can start a literal pattern.
const LITERAL_PATTERN_START_SET: TokenSet = TokenSet::new([
    TokenKind::None,
    TokenKind::True,
    TokenKind::False,
    TokenKind::String,
    TokenKind::Int,
    TokenKind::Float,
    TokenKind::Complex,
    TokenKind::Minus, // Unary minus
]);

/// The set of tokens that can start a pattern.
const PATTERN_START_SET: TokenSet = TokenSet::new([
    // Star pattern
    TokenKind::Star,
    // Capture pattern
    // Wildcard pattern ('_' is a name token)
    // Value pattern (name or attribute)
    // Class pattern
    TokenKind::Name,
    // Group pattern
    TokenKind::Lpar,
    // Sequence pattern
    TokenKind::Lsqb,
    // Mapping pattern
    TokenKind::Lbrace,
])
.union(LITERAL_PATTERN_START_SET);

/// The set of tokens that can start a mapping pattern.
const MAPPING_PATTERN_START_SET: TokenSet = TokenSet::new([
    // Double star pattern
    TokenKind::DoubleStar,
    // Value pattern
    TokenKind::Name,
])
.union(LITERAL_PATTERN_START_SET);

impl<'src> Parser<'src> {
    /// Returns `true` if the current token is a valid start of a pattern.
    pub(super) fn at_pattern_start(&self) -> bool {
        self.at_ts(PATTERN_START_SET) || self.at_soft_keyword()
    }

    /// Returns `true` if the current token is a valid start of a mapping pattern.
    pub(super) fn at_mapping_pattern_start(&self) -> bool {
        self.at_ts(MAPPING_PATTERN_START_SET) || self.at_soft_keyword()
    }

    /// Entry point to start parsing a pattern.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-patterns>
    pub(super) fn parse_match_patterns(&mut self) -> Pattern {
        let start = self.node_start();

        // We don't yet know if it's a sequence pattern or a single pattern, so
        // we need to allow star pattern here.
        let pattern = self.parse_match_pattern(AllowStarPattern::Yes);

        if self.at(TokenKind::Comma) {
            Pattern::MatchSequence(self.parse_sequence_match_pattern(pattern, start, None))
        } else {
            // We know it's not a sequence pattern now, so check for star pattern usage.
            if pattern.is_match_star() {
                self.add_error(ParseErrorType::InvalidStarPatternUsage, &pattern);
            }
            pattern
        }
    }

    /// Parses an `or_pattern` or an `as_pattern`.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-pattern>
    fn parse_match_pattern(&mut self, allow_star_pattern: AllowStarPattern) -> Pattern {
        let start = self.node_start();

        // We don't yet know if it's an or pattern or an as pattern, so use whatever
        // was passed in.
        let mut lhs = self.parse_match_pattern_lhs(allow_star_pattern);

        // Or pattern
        if self.at(TokenKind::Vbar) {
            // We know it's an `or` pattern now, so check for star pattern usage.
            if lhs.is_match_star() {
                self.add_error(ParseErrorType::InvalidStarPatternUsage, &lhs);
            }

            let mut patterns = vec![lhs];
            let mut progress = ParserProgress::default();

            while self.eat(TokenKind::Vbar) {
                progress.assert_progressing(self);
                let pattern = self.parse_match_pattern_lhs(AllowStarPattern::No);
                patterns.push(pattern);
            }

            lhs = Pattern::MatchOr(ast::PatternMatchOr {
                range: self.node_range(start),
                patterns,
            });
        }

        // As pattern
        if self.eat(TokenKind::As) {
            // We know it's an `as` pattern now, so check for star pattern usage.
            if lhs.is_match_star() {
                self.add_error(ParseErrorType::InvalidStarPatternUsage, &lhs);
            }

            let ident = self.parse_identifier();
            lhs = Pattern::MatchAs(ast::PatternMatchAs {
                range: self.node_range(start),
                name: Some(ident),
                pattern: Some(Box::new(lhs)),
            });
        }

        lhs
    }

    /// Parses a pattern.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-closed_pattern>
    fn parse_match_pattern_lhs(&mut self, allow_star_pattern: AllowStarPattern) -> Pattern {
        let start = self.node_start();

        let mut lhs = match self.current_token_kind() {
            TokenKind::Lbrace => Pattern::MatchMapping(self.parse_match_pattern_mapping()),
            TokenKind::Star => {
                let star_pattern = self.parse_match_pattern_star();
                if allow_star_pattern.is_no() {
                    self.add_error(ParseErrorType::InvalidStarPatternUsage, &star_pattern);
                }
                Pattern::MatchStar(star_pattern)
            }
            TokenKind::Lpar | TokenKind::Lsqb => self.parse_parenthesized_or_sequence_pattern(),
            _ => self.parse_match_pattern_literal(),
        };

        if self.at(TokenKind::Lpar) {
            lhs = Pattern::MatchClass(self.parse_match_pattern_class(lhs, start));
        }

        if matches!(
            self.current_token_kind(),
            TokenKind::Plus | TokenKind::Minus
        ) {
            lhs = Pattern::MatchValue(self.parse_complex_literal_pattern(lhs, start));
        }

        lhs
    }

    /// Parses a mapping pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#mapping-patterns>
    fn parse_match_pattern_mapping(&mut self) -> ast::PatternMatchMapping {
        let start = self.node_start();
        self.bump(TokenKind::Lbrace);

        let mut keys = vec![];
        let mut patterns = vec![];
        let mut rest = None;

        self.parse_comma_separated_list(RecoveryContextKind::MatchPatternMapping, |parser| {
            let mapping_item_start = parser.node_start();

            if parser.eat(TokenKind::DoubleStar) {
                let identifier = parser.parse_identifier();
                if rest.is_some() {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "Only one double star pattern is allowed".to_string(),
                        ),
                        parser.node_range(mapping_item_start),
                    );
                }
                // TODO(dhruvmanila): It's not possible to retain multiple double starred
                // patterns because of the way the mapping node is represented in the grammar.
                // The last value will always win. Update the AST representation.
                // See: https://github.com/astral-sh/ruff/pull/10477#discussion_r1535143536
                rest = Some(identifier);
            } else {
                let key = match parser.parse_match_pattern_lhs(AllowStarPattern::No) {
                    Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => *value,
                    Pattern::MatchSingleton(ast::PatternMatchSingleton { value, range }) => {
                        match value {
                            Singleton::None => Expr::NoneLiteral(ast::ExprNoneLiteral { range }),
                            Singleton::True => {
                                Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, range })
                            }
                            Singleton::False => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                                value: false,
                                range,
                            }),
                        }
                    }
                    pattern => {
                        parser.add_error(
                            ParseErrorType::OtherError("Invalid mapping pattern key".to_string()),
                            &pattern,
                        );
                        recovery::pattern_to_expr(pattern)
                    }
                };
                keys.push(key);

                parser.expect(TokenKind::Colon);

                patterns.push(parser.parse_match_pattern(AllowStarPattern::No));

                if rest.is_some() {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "Pattern cannot follow a double star pattern".to_string(),
                        ),
                        parser.node_range(mapping_item_start),
                    );
                }
            }
        });

        self.expect(TokenKind::Rbrace);

        ast::PatternMatchMapping {
            range: self.node_range(start),
            keys,
            patterns,
            rest,
        }
    }

    /// Parses a star pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `*` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-star_pattern>
    fn parse_match_pattern_star(&mut self) -> ast::PatternMatchStar {
        let start = self.node_start();
        self.bump(TokenKind::Star);

        let ident = self.parse_identifier();

        ast::PatternMatchStar {
            range: self.node_range(start),
            name: if ident.is_valid() && ident.id == "_" {
                None
            } else {
                Some(ident)
            },
        }
    }

    /// Parses a parenthesized pattern or a sequence pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `(` or `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#sequence-patterns>
    fn parse_parenthesized_or_sequence_pattern(&mut self) -> Pattern {
        let start = self.node_start();
        let parentheses = if self.eat(TokenKind::Lpar) {
            SequenceMatchPatternParentheses::Tuple
        } else {
            self.bump(TokenKind::Lsqb);
            SequenceMatchPatternParentheses::List
        };

        if matches!(
            self.current_token_kind(),
            TokenKind::Newline | TokenKind::Colon
        ) {
            // TODO(dhruvmanila): This recovery isn't possible currently because
            // of the soft keyword transformer. If there's a missing closing
            // parenthesis, it'll consider `case` a name token instead.
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "Missing '{closing}'",
                    closing = if parentheses.is_list() { "]" } else { ")" }
                )),
                self.current_token_range(),
            );
        }

        if self.eat(parentheses.closing_kind()) {
            return Pattern::MatchSequence(ast::PatternMatchSequence {
                patterns: vec![],
                range: self.node_range(start),
            });
        }

        let mut pattern = self.parse_match_pattern(AllowStarPattern::Yes);

        if parentheses.is_list() || self.at(TokenKind::Comma) {
            pattern = Pattern::MatchSequence(self.parse_sequence_match_pattern(
                pattern,
                start,
                Some(parentheses),
            ));
        } else {
            self.expect(parentheses.closing_kind());
        }

        pattern
    }

    /// Parses the rest of a sequence pattern, given the first element.
    ///
    /// If the `parentheses` is `None`, it is an [open sequence pattern].
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#sequence-patterns>
    ///
    /// [open sequence pattern]: https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-open_sequence_pattern
    fn parse_sequence_match_pattern(
        &mut self,
        first_element: Pattern,
        start: TextSize,
        parentheses: Option<SequenceMatchPatternParentheses>,
    ) -> ast::PatternMatchSequence {
        if parentheses.is_some_and(|parentheses| {
            self.at(parentheses.closing_kind()) || self.peek() == parentheses.closing_kind()
        }) {
            // The comma is optional if it is a single-element sequence
            self.eat(TokenKind::Comma);
        } else {
            self.expect(TokenKind::Comma);
        }

        let mut patterns = vec![first_element];

        self.parse_comma_separated_list(
            RecoveryContextKind::SequenceMatchPattern(parentheses),
            |parser| patterns.push(parser.parse_match_pattern(AllowStarPattern::Yes)),
        );

        if let Some(parentheses) = parentheses {
            self.expect(parentheses.closing_kind());
        }

        ast::PatternMatchSequence {
            range: self.node_range(start),
            patterns,
        }
    }

    /// Parses a literal pattern.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-literal_pattern>
    fn parse_match_pattern_literal(&mut self) -> Pattern {
        let start = self.node_start();
        match self.current_token_kind() {
            TokenKind::None => {
                self.bump(TokenKind::None);
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::None,
                    range: self.node_range(start),
                })
            }
            TokenKind::True => {
                self.bump(TokenKind::True);
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::True,
                    range: self.node_range(start),
                })
            }
            TokenKind::False => {
                self.bump(TokenKind::False);
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::False,
                    range: self.node_range(start),
                })
            }
            TokenKind::String | TokenKind::FStringStart => {
                let str = self.parse_strings();

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(str),
                    range: self.node_range(start),
                })
            }
            TokenKind::Complex => {
                let TokenValue::Complex { real, imag } = self.bump_value(TokenKind::Complex) else {
                    unreachable!()
                };
                let range = self.node_range(start);

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: Number::Complex { real, imag },
                        range,
                    })),
                    range,
                })
            }
            TokenKind::Int => {
                let TokenValue::Int(value) = self.bump_value(TokenKind::Int) else {
                    unreachable!()
                };
                let range = self.node_range(start);

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: Number::Int(value),
                        range,
                    })),
                    range,
                })
            }
            TokenKind::Float => {
                let TokenValue::Float(value) = self.bump_value(TokenKind::Float) else {
                    unreachable!()
                };
                let range = self.node_range(start);

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: Number::Float(value),
                        range,
                    })),
                    range,
                })
            }
            kind => {
                // The `+` is only for better error recovery.
                if let Some(unary_arithmetic_op) = kind.as_unary_arithmetic_operator() {
                    if matches!(
                        self.peek(),
                        TokenKind::Int | TokenKind::Float | TokenKind::Complex
                    ) {
                        let unary_expr = self.parse_unary_expression(
                            unary_arithmetic_op,
                            ExpressionContext::default(),
                        );

                        if unary_expr.op.is_u_add() {
                            self.add_error(
                                ParseErrorType::OtherError(
                                    "Unary '+' is not allowed as a literal pattern".to_string(),
                                ),
                                &unary_expr,
                            );
                        }

                        return Pattern::MatchValue(ast::PatternMatchValue {
                            value: Box::new(Expr::UnaryOp(unary_expr)),
                            range: self.node_range(start),
                        });
                    }
                }

                if self.at_name_or_keyword() {
                    if self.peek() == TokenKind::Dot {
                        // test_ok match_attr_pattern_soft_keyword
                        // match foo:
                        //     case match.bar: ...
                        //     case case.bar: ...
                        //     case type.bar: ...
                        //     case match.case.type.bar.type.case.match: ...
                        let id = Expr::Name(self.parse_name());

                        let attribute = self.parse_attr_expr_for_match_pattern(id, start);

                        Pattern::MatchValue(ast::PatternMatchValue {
                            value: Box::new(attribute),
                            range: self.node_range(start),
                        })
                    } else {
                        // test_ok match_as_pattern_soft_keyword
                        // match foo:
                        //     case case: ...
                        //     case match: ...
                        //     case type: ...
                        let ident = self.parse_identifier();

                        // test_ok match_as_pattern
                        // match foo:
                        //     case foo_bar: ...
                        //     case _: ...
                        Pattern::MatchAs(ast::PatternMatchAs {
                            range: ident.range,
                            pattern: None,
                            name: if &ident == "_" { None } else { Some(ident) },
                        })
                    }
                } else {
                    // Upon encountering an unexpected token, return a `Pattern::MatchValue` containing
                    // an empty `Expr::Name`.
                    self.add_error(
                        ParseErrorType::OtherError("Expected a pattern".to_string()),
                        self.current_token_range(),
                    );
                    let invalid_node = Expr::Name(ast::ExprName {
                        range: self.missing_node_range(),
                        id: Name::empty(),
                        ctx: ExprContext::Invalid,
                    });
                    Pattern::MatchValue(ast::PatternMatchValue {
                        range: invalid_node.range(),
                        value: Box::new(invalid_node),
                    })
                }
            }
        }
    }

    /// Parses a complex literal pattern, given the `lhs` pattern and the `start`
    /// position of the pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `+` or `-` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#literal-patterns>
    fn parse_complex_literal_pattern(
        &mut self,
        lhs: Pattern,
        start: TextSize,
    ) -> ast::PatternMatchValue {
        let operator = if self.eat(TokenKind::Plus) {
            Operator::Add
        } else {
            self.bump(TokenKind::Minus);
            Operator::Sub
        };

        let lhs_value = if let Pattern::MatchValue(lhs) = lhs {
            if !is_real_number(&lhs.value) {
                self.add_error(ParseErrorType::ExpectedRealNumber, &lhs);
            }
            lhs.value
        } else {
            self.add_error(ParseErrorType::ExpectedRealNumber, &lhs);
            Box::new(recovery::pattern_to_expr(lhs))
        };

        let rhs_pattern = self.parse_match_pattern_lhs(AllowStarPattern::No);
        let rhs_value = if let Pattern::MatchValue(rhs) = rhs_pattern {
            if !is_complex_number(&rhs.value) {
                self.add_error(ParseErrorType::ExpectedImaginaryNumber, &rhs);
            }
            rhs.value
        } else {
            self.add_error(ParseErrorType::ExpectedImaginaryNumber, &rhs_pattern);
            Box::new(recovery::pattern_to_expr(rhs_pattern))
        };

        let range = self.node_range(start);

        ast::PatternMatchValue {
            value: Box::new(Expr::BinOp(ast::ExprBinOp {
                left: lhs_value,
                op: operator,
                right: rhs_value,
                range,
            })),
            range,
        }
    }

    /// Parses an attribute expression until the current token is not a `.`.
    fn parse_attr_expr_for_match_pattern(&mut self, mut lhs: Expr, start: TextSize) -> Expr {
        while self.current_token_kind() == TokenKind::Dot {
            lhs = Expr::Attribute(self.parse_attribute_expression(lhs, start));
        }

        lhs
    }

    /// Parses the [pattern arguments] in a class pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `(` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#class-patterns>
    ///
    /// [pattern arguments]: https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-pattern_arguments
    fn parse_match_pattern_class(
        &mut self,
        cls: Pattern,
        start: TextSize,
    ) -> ast::PatternMatchClass {
        let arguments_start = self.node_start();

        let cls = match cls {
            Pattern::MatchAs(ast::PatternMatchAs {
                pattern: None,
                name: Some(ident),
                ..
            }) => {
                if ident.is_valid() {
                    Box::new(Expr::Name(ast::ExprName {
                        range: ident.range(),
                        id: ident.id,
                        ctx: ExprContext::Load,
                    }))
                } else {
                    Box::new(Expr::Name(ast::ExprName {
                        range: ident.range(),
                        id: Name::empty(),
                        ctx: ExprContext::Invalid,
                    }))
                }
            }
            Pattern::MatchValue(ast::PatternMatchValue { value, .. })
                if matches!(&*value, Expr::Attribute(_)) =>
            {
                value
            }
            pattern => {
                self.add_error(
                    ParseErrorType::OtherError("Invalid value for a class pattern".to_string()),
                    &pattern,
                );
                Box::new(recovery::pattern_to_expr(pattern))
            }
        };

        self.bump(TokenKind::Lpar);

        let mut patterns = vec![];
        let mut keywords = vec![];
        let mut has_seen_pattern = false;
        let mut has_seen_keyword_pattern = false;

        self.parse_comma_separated_list(
            RecoveryContextKind::MatchPatternClassArguments,
            |parser| {
                let pattern_start = parser.node_start();
                let pattern = parser.parse_match_pattern(AllowStarPattern::No);

                if parser.eat(TokenKind::Equal) {
                    has_seen_pattern = false;
                    has_seen_keyword_pattern = true;

                    let key = if let Pattern::MatchAs(ast::PatternMatchAs {
                        pattern: None,
                        name: Some(name),
                        ..
                    }) = pattern
                    {
                        name
                    } else {
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Expected an identifier for the keyword pattern".to_string(),
                            ),
                            &pattern,
                        );
                        ast::Identifier {
                            id: Name::empty(),
                            range: parser.missing_node_range(),
                        }
                    };

                    let value_pattern = parser.parse_match_pattern(AllowStarPattern::No);

                    keywords.push(ast::PatternKeyword {
                        attr: key,
                        pattern: value_pattern,
                        range: parser.node_range(pattern_start),
                    });
                } else {
                    has_seen_pattern = true;
                    patterns.push(pattern);
                }

                if has_seen_keyword_pattern && has_seen_pattern {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "Positional patterns cannot follow keyword patterns".to_string(),
                        ),
                        parser.node_range(pattern_start),
                    );
                }
            },
        );

        self.expect(TokenKind::Rpar);

        ast::PatternMatchClass {
            cls,
            arguments: ast::PatternArguments {
                patterns,
                keywords,
                range: self.node_range(arguments_start),
            },
            range: self.node_range(start),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AllowStarPattern {
    Yes,
    No,
}

impl AllowStarPattern {
    const fn is_no(self) -> bool {
        matches!(self, AllowStarPattern::No)
    }
}

/// Returns `true` if the given expression is a real number literal or a unary
/// addition or subtraction of a real number literal.
const fn is_real_number(expr: &Expr) -> bool {
    match expr {
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(_) | ast::Number::Float(_),
            ..
        }) => true,
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::UAdd | ast::UnaryOp::USub,
            operand,
            ..
        }) => is_real_number(operand),
        _ => false,
    }
}

/// Returns `true` if the given expression is a complex number literal.
const fn is_complex_number(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Complex { .. },
            ..
        })
    )
}

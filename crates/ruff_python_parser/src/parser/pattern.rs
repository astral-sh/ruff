use ruff_python_ast::name::Name;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    self as ast, AtomicNodeIndex, Expr, ExprContext, Number, Operator, Pattern, Singleton,
};
use ruff_text_size::{Ranged, TextSize};

use crate::ParseErrorType;
use crate::parser::progress::ParserProgress;
use crate::parser::{Parser, RecoveryContextKind, SequenceMatchPatternParentheses, recovery};
use crate::token::TokenValue;
use crate::token_set::TokenSet;

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

impl Parser<'_> {
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
        if let Some(result) =
            self.with_recursion(|parser| parser.parse_match_pattern_inner(allow_star_pattern))
        {
            result
        } else {
            let range = self.missing_node_range();
            self.report_recursion_limit_exceeded(self.current_token_range());
            let invalid_node = Expr::Name(ast::ExprName {
                range,
                id: Name::empty(),
                ctx: ExprContext::Invalid,
                node_index: AtomicNodeIndex::NONE,
            });
            Pattern::MatchValue(ast::PatternMatchValue {
                range: invalid_node.range(),
                value: Box::new(invalid_node),
                node_index: AtomicNodeIndex::NONE,
            })
        }
    }

    fn parse_match_pattern_inner(&mut self, allow_star_pattern: AllowStarPattern) -> Pattern {
        let start = self.node_start();

        // We don't yet know if it's an or pattern or an as pattern, so use whatever
        // was passed in.
        let lhs = self.parse_match_pattern_lhs(allow_star_pattern);

        self.parse_match_pattern_from_lhs(lhs, start)
    }

    fn parse_match_pattern_from_lhs(&mut self, mut lhs: Pattern, start: TextSize) -> Pattern {
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
                node_index: AtomicNodeIndex::NONE,
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
                node_index: AtomicNodeIndex::NONE,
            });
        }

        lhs
    }

    /// Parses a pattern.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-closed_pattern>
    fn parse_match_pattern_lhs(&mut self, allow_star_pattern: AllowStarPattern) -> Pattern {
        let start = self.node_start();

        let lhs = match self.current_token_kind() {
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

        self.finish_match_pattern_lhs(lhs, start)
    }

    fn finish_match_pattern_lhs(&mut self, mut lhs: Pattern, start: TextSize) -> Pattern {
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

        if let Some(pending) = self.try_parse_nested_match_pattern_mapping_value(start) {
            return self.parse_nested_match_pattern_mapping(pending);
        }

        self.parse_match_pattern_mapping_after_lbrace(start)
    }

    fn parse_match_pattern_mapping_after_lbrace(
        &mut self,
        start: TextSize,
    ) -> ast::PatternMatchMapping {
        let mut state = MatchPatternMappingParsingState::default();

        self.parse_comma_separated_list(RecoveryContextKind::MatchPatternMapping, |parser| {
            parser.parse_match_pattern_mapping_item(&mut state);
        });

        self.expect(TokenKind::Rbrace);

        self.finish_match_pattern_mapping(start, state)
    }

    fn parse_match_pattern_mapping_item(&mut self, state: &mut MatchPatternMappingParsingState) {
        let mapping_item_start = self.node_start();

        if self.eat(TokenKind::DoubleStar) {
            let identifier = self.parse_identifier();
            if state.rest.is_some() {
                self.add_error(
                    ParseErrorType::OtherError(
                        "Only one double star pattern is allowed".to_string(),
                    ),
                    self.node_range(mapping_item_start),
                );
            }
            // TODO(dhruvmanila): It's not possible to retain multiple double starred
            // patterns because of the way the mapping node is represented in the grammar.
            // The last value will always win. Update the AST representation.
            // See: https://github.com/astral-sh/ruff/pull/10477#discussion_r1535143536
            state.rest = Some(identifier);
        } else {
            let key = self.parse_match_pattern_mapping_key();

            self.expect(TokenKind::Colon);

            let pattern = self.parse_match_pattern(AllowStarPattern::No);

            self.parse_match_pattern_mapping_item_from_value(
                state,
                mapping_item_start,
                key,
                pattern,
            );
        }
    }

    fn parse_match_pattern_mapping_item_from_value(
        &mut self,
        state: &mut MatchPatternMappingParsingState,
        mapping_item_start: TextSize,
        key: Expr,
        pattern: Pattern,
    ) {
        state.keys.push(key);
        state.patterns.push(pattern);

        if state.rest.is_some() {
            self.add_error(
                ParseErrorType::OtherError(
                    "Pattern cannot follow a double star pattern".to_string(),
                ),
                self.node_range(mapping_item_start),
            );
        }
    }

    fn parse_match_pattern_mapping_key(&mut self) -> Expr {
        match self.parse_match_pattern_lhs(AllowStarPattern::No) {
            Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => *value,
            Pattern::MatchSingleton(ast::PatternMatchSingleton {
                value,
                range,
                node_index,
            }) => match value {
                Singleton::None => Expr::NoneLiteral(ast::ExprNoneLiteral { range, node_index }),
                Singleton::True => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                    value: true,
                    range,
                    node_index,
                }),
                Singleton::False => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                    value: false,
                    range,
                    node_index,
                }),
            },
            pattern => {
                self.add_error(
                    ParseErrorType::OtherError("Invalid mapping pattern key".to_string()),
                    &pattern,
                );
                recovery::pattern_to_expr(pattern)
            }
        }
    }

    fn try_parse_nested_match_pattern_mapping_value(
        &mut self,
        start: TextSize,
    ) -> Option<PendingMatchPatternMapping> {
        if !self.at_mapping_pattern_start() || self.at(TokenKind::DoubleStar) {
            return None;
        }

        let checkpoint = self.checkpoint();
        let mapping_item_start = self.node_start();
        let key = self.parse_match_pattern_mapping_key();

        self.expect(TokenKind::Colon);

        if self.at(TokenKind::Lbrace) {
            Some(PendingMatchPatternMapping {
                start,
                mapping_item_start,
                key,
                value_start: self.node_start(),
            })
        } else {
            self.rewind(checkpoint);
            None
        }
    }

    fn parse_nested_match_pattern_mapping(
        &mut self,
        outer: PendingMatchPatternMapping,
    ) -> ast::PatternMatchMapping {
        let mut pending = vec![outer];

        let mut mapping = loop {
            let start = self.node_start();
            self.bump(TokenKind::Lbrace);

            if let Some(pending_mapping) = self.try_parse_nested_match_pattern_mapping_value(start)
            {
                pending.push(pending_mapping);
                continue;
            }

            break self.parse_match_pattern_mapping_after_lbrace(start);
        };

        while let Some(pending) = pending.pop() {
            let lhs =
                self.finish_match_pattern_lhs(Pattern::MatchMapping(mapping), pending.value_start);
            let pattern = self.parse_match_pattern_from_lhs(lhs, pending.value_start);
            mapping = self.parse_match_pattern_mapping_after_first(
                pending.start,
                pending.mapping_item_start,
                pending.key,
                pattern,
            );
        }

        mapping
    }

    fn parse_match_pattern_mapping_after_first(
        &mut self,
        start: TextSize,
        mapping_item_start: TextSize,
        key: Expr,
        pattern: Pattern,
    ) -> ast::PatternMatchMapping {
        let mut state = MatchPatternMappingParsingState::default();
        self.parse_match_pattern_mapping_item_from_value(
            &mut state,
            mapping_item_start,
            key,
            pattern,
        );

        if self.eat(TokenKind::Comma) {
            if !self.at(TokenKind::Rbrace) {
                self.parse_comma_separated_list(
                    RecoveryContextKind::MatchPatternMapping,
                    |parser| parser.parse_match_pattern_mapping_item(&mut state),
                );
            }
        } else if !self.at(TokenKind::Rbrace) {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(RecoveryContextKind::MatchPatternMapping, |parser| {
                parser.parse_match_pattern_mapping_item(&mut state);
            });
        }

        self.expect(TokenKind::Rbrace);

        self.finish_match_pattern_mapping(start, state)
    }

    fn finish_match_pattern_mapping(
        &self,
        start: TextSize,
        state: MatchPatternMappingParsingState,
    ) -> ast::PatternMatchMapping {
        ast::PatternMatchMapping {
            range: self.node_range(start),
            keys: state.keys,
            patterns: state.patterns,
            rest: state.rest,
            node_index: AtomicNodeIndex::NONE,
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
            node_index: AtomicNodeIndex::NONE,
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
        let parentheses = self.parse_sequence_match_pattern_parentheses();

        self.report_unclosed_sequence_match_pattern(parentheses);

        if self.eat(parentheses.closing_kind()) {
            return self.empty_sequence_match_pattern(start);
        }

        if matches!(self.current_token_kind(), TokenKind::Lpar | TokenKind::Lsqb) {
            return self.parse_nested_parenthesized_or_sequence_pattern(start, parentheses);
        }

        let pattern = self.parse_match_pattern(AllowStarPattern::Yes);

        self.finish_parenthesized_or_sequence_pattern(pattern, start, parentheses)
    }

    fn parse_nested_parenthesized_or_sequence_pattern(
        &mut self,
        outer_start: TextSize,
        outer_parentheses: SequenceMatchPatternParentheses,
    ) -> Pattern {
        let mut pending = vec![(outer_start, outer_parentheses)];

        let mut pattern = loop {
            let start = self.node_start();
            let parentheses = self.parse_sequence_match_pattern_parentheses();

            self.report_unclosed_sequence_match_pattern(parentheses);

            if self.eat(parentheses.closing_kind()) {
                let lhs =
                    self.finish_match_pattern_lhs(self.empty_sequence_match_pattern(start), start);
                break self.parse_match_pattern_from_lhs(lhs, start);
            }

            pending.push((start, parentheses));

            if !matches!(self.current_token_kind(), TokenKind::Lpar | TokenKind::Lsqb) {
                break self.parse_match_pattern(AllowStarPattern::Yes);
            }
        };

        while let Some((start, parentheses)) = pending.pop() {
            pattern = self.finish_parenthesized_or_sequence_pattern(pattern, start, parentheses);

            if !pending.is_empty() {
                let lhs = self.finish_match_pattern_lhs(pattern, start);
                pattern = self.parse_match_pattern_from_lhs(lhs, start);
            }
        }

        pattern
    }

    fn parse_sequence_match_pattern_parentheses(&mut self) -> SequenceMatchPatternParentheses {
        if self.eat(TokenKind::Lpar) {
            SequenceMatchPatternParentheses::Tuple
        } else {
            self.bump(TokenKind::Lsqb);
            SequenceMatchPatternParentheses::List
        }
    }

    fn report_unclosed_sequence_match_pattern(
        &mut self,
        parentheses: SequenceMatchPatternParentheses,
    ) {
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
    }

    fn empty_sequence_match_pattern(&self, start: TextSize) -> Pattern {
        Pattern::MatchSequence(ast::PatternMatchSequence {
            patterns: vec![],
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
        })
    }

    fn finish_parenthesized_or_sequence_pattern(
        &mut self,
        pattern: Pattern,
        start: TextSize,
        parentheses: SequenceMatchPatternParentheses,
    ) -> Pattern {
        if parentheses.is_list() || self.at(TokenKind::Comma) {
            Pattern::MatchSequence(self.parse_sequence_match_pattern(
                pattern,
                start,
                Some(parentheses),
            ))
        } else {
            self.expect(parentheses.closing_kind());
            pattern
        }
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
            node_index: AtomicNodeIndex::NONE,
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
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::True => {
                self.bump(TokenKind::True);
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::True,
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::False => {
                self.bump(TokenKind::False);
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::False,
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
                })
            }
            TokenKind::String | TokenKind::FStringStart | TokenKind::TStringStart => {
                let str = self.parse_strings();

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(str),
                    range: self.node_range(start),
                    node_index: AtomicNodeIndex::NONE,
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
                        node_index: AtomicNodeIndex::NONE,
                    })),
                    range,
                    node_index: AtomicNodeIndex::NONE,
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
                        node_index: AtomicNodeIndex::NONE,
                    })),
                    range,
                    node_index: AtomicNodeIndex::NONE,
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
                        node_index: AtomicNodeIndex::NONE,
                    })),
                    range,
                    node_index: AtomicNodeIndex::NONE,
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
                            node_index: AtomicNodeIndex::NONE,
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
                            node_index: AtomicNodeIndex::NONE,
                        })
                    } else {
                        // test_ok match_as_pattern_soft_keyword
                        // match foo:
                        //     case case: ...
                        // match foo:
                        //     case match: ...
                        // match foo:
                        //     case type: ...
                        let ident = self.parse_identifier();

                        // test_ok match_as_pattern
                        // match foo:
                        //     case foo_bar: ...
                        // match foo:
                        //     case _: ...
                        Pattern::MatchAs(ast::PatternMatchAs {
                            range: ident.range,
                            pattern: None,
                            name: if &ident == "_" { None } else { Some(ident) },
                            node_index: AtomicNodeIndex::NONE,
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
                        node_index: AtomicNodeIndex::NONE,
                    });
                    Pattern::MatchValue(ast::PatternMatchValue {
                        range: invalid_node.range(),
                        value: Box::new(invalid_node),
                        node_index: AtomicNodeIndex::NONE,
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
                node_index: AtomicNodeIndex::NONE,
            })),
            range,
            node_index: AtomicNodeIndex::NONE,
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

        let cls = self.parse_match_pattern_class_expr(cls);

        self.bump(TokenKind::Lpar);

        if let Some(nested) = self.try_parse_nested_match_pattern_class_argument() {
            return self.parse_nested_match_pattern_class(cls, start, arguments_start, nested);
        }

        let arguments = self.parse_match_pattern_class_arguments(arguments_start);

        ast::PatternMatchClass {
            cls,
            arguments,
            range: self.node_range(start),
            node_index: AtomicNodeIndex::NONE,
        }
    }

    fn parse_match_pattern_class_expr(&mut self, cls: Pattern) -> Box<Expr> {
        match cls {
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
                        node_index: AtomicNodeIndex::NONE,
                    }))
                } else {
                    Box::new(Expr::Name(ast::ExprName {
                        range: ident.range(),
                        id: Name::empty(),
                        ctx: ExprContext::Invalid,
                        node_index: AtomicNodeIndex::NONE,
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
        }
    }

    fn try_parse_nested_match_pattern_class_head(&mut self) -> Option<(Pattern, TextSize)> {
        if !self.at_name_or_keyword() {
            return None;
        }

        let checkpoint = self.checkpoint();
        let start = self.node_start();
        let cls = self.parse_match_pattern_literal();

        if self.at(TokenKind::Lpar) {
            Some((cls, start))
        } else {
            self.rewind(checkpoint);
            None
        }
    }

    fn try_parse_nested_match_pattern_class_argument(
        &mut self,
    ) -> Option<NestedMatchPatternClassArgument> {
        if !self.at_name_or_keyword() {
            return None;
        }

        let checkpoint = self.checkpoint();
        let pattern_start = self.node_start();
        let pattern = self.parse_match_pattern_literal();

        if self.at(TokenKind::Lpar) {
            return Some(NestedMatchPatternClassArgument {
                cls: pattern,
                cls_start: pattern_start,
                pending_argument: PendingMatchPatternClassArgument::Positional { pattern_start },
            });
        }

        if self.eat(TokenKind::Equal)
            && let Some((cls, cls_start)) = self.try_parse_nested_match_pattern_class_head()
        {
            let attr = self.parse_match_pattern_class_keyword_attr(pattern);
            return Some(NestedMatchPatternClassArgument {
                cls,
                cls_start,
                pending_argument: PendingMatchPatternClassArgument::Keyword {
                    pattern_start,
                    attr,
                    value_start: cls_start,
                },
            });
        }

        self.rewind(checkpoint);
        None
    }

    fn parse_nested_match_pattern_class(
        &mut self,
        outer_cls: Box<Expr>,
        outer_start: TextSize,
        outer_arguments_start: TextSize,
        nested: NestedMatchPatternClassArgument,
    ) -> ast::PatternMatchClass {
        let mut pending = vec![PendingMatchPatternClass {
            cls: outer_cls,
            start: outer_start,
            arguments_start: outer_arguments_start,
            first_argument: nested.pending_argument,
        }];
        let mut cls = nested.cls;
        let mut start = nested.cls_start;

        let mut class = loop {
            let arguments_start = self.node_start();
            let parsed_cls = self.parse_match_pattern_class_expr(cls);

            self.bump(TokenKind::Lpar);

            if let Some(nested) = self.try_parse_nested_match_pattern_class_argument() {
                pending.push(PendingMatchPatternClass {
                    cls: parsed_cls,
                    start,
                    arguments_start,
                    first_argument: nested.pending_argument,
                });
                cls = nested.cls;
                start = nested.cls_start;
                continue;
            }

            let arguments = self.parse_match_pattern_class_arguments(arguments_start);

            break ast::PatternMatchClass {
                cls: parsed_cls,
                arguments,
                range: self.node_range(start),
                node_index: AtomicNodeIndex::NONE,
            };
        };

        while let Some(pending) = pending.pop() {
            let value_start = pending.first_argument.value_start();
            let lhs = self.finish_match_pattern_lhs(Pattern::MatchClass(class), value_start);
            let pattern = self.parse_match_pattern_from_lhs(lhs, value_start);
            let arguments = match pending.first_argument {
                PendingMatchPatternClassArgument::Positional { pattern_start } => self
                    .parse_match_pattern_class_arguments_after_first(
                        pending.arguments_start,
                        pattern_start,
                        pattern,
                    ),
                PendingMatchPatternClassArgument::Keyword {
                    pattern_start,
                    attr,
                    ..
                } => self.parse_match_pattern_class_arguments_after_first_keyword(
                    pending.arguments_start,
                    pattern_start,
                    attr,
                    pattern,
                ),
            };

            class = ast::PatternMatchClass {
                cls: pending.cls,
                arguments,
                range: self.node_range(pending.start),
                node_index: AtomicNodeIndex::NONE,
            };
        }

        class
    }

    fn parse_match_pattern_class_arguments(
        &mut self,
        arguments_start: TextSize,
    ) -> ast::PatternArguments {
        let mut state = MatchPatternClassArgumentParsingState::default();

        self.parse_comma_separated_list(
            RecoveryContextKind::MatchPatternClassArguments,
            |parser| parser.parse_match_pattern_class_argument(&mut state),
        );

        self.expect(TokenKind::Rpar);

        self.finish_match_pattern_class_arguments(arguments_start, state)
    }

    fn parse_match_pattern_class_arguments_after_first(
        &mut self,
        arguments_start: TextSize,
        first_pattern_start: TextSize,
        first_pattern: Pattern,
    ) -> ast::PatternArguments {
        let mut state = MatchPatternClassArgumentParsingState::default();
        self.parse_match_pattern_class_argument_from_pattern(
            &mut state,
            first_pattern_start,
            first_pattern,
        );

        self.parse_match_pattern_class_arguments_after_first_state(arguments_start, state)
    }

    fn parse_match_pattern_class_arguments_after_first_keyword(
        &mut self,
        arguments_start: TextSize,
        pattern_start: TextSize,
        attr: ast::Identifier,
        pattern: Pattern,
    ) -> ast::PatternArguments {
        let mut state = MatchPatternClassArgumentParsingState::default();
        self.parse_match_pattern_class_keyword_argument_from_value(
            &mut state,
            pattern_start,
            attr,
            pattern,
        );

        self.parse_match_pattern_class_arguments_after_first_state(arguments_start, state)
    }

    fn parse_match_pattern_class_arguments_after_first_state(
        &mut self,
        arguments_start: TextSize,
        mut state: MatchPatternClassArgumentParsingState,
    ) -> ast::PatternArguments {
        if self.eat(TokenKind::Comma) {
            if !self.at(TokenKind::Rpar) {
                self.parse_comma_separated_list(
                    RecoveryContextKind::MatchPatternClassArguments,
                    |parser| parser.parse_match_pattern_class_argument(&mut state),
                );
            }
        } else if !self.at(TokenKind::Rpar) {
            self.expect(TokenKind::Comma);
            self.parse_comma_separated_list(
                RecoveryContextKind::MatchPatternClassArguments,
                |parser| parser.parse_match_pattern_class_argument(&mut state),
            );
        }

        self.expect(TokenKind::Rpar);

        self.finish_match_pattern_class_arguments(arguments_start, state)
    }

    fn parse_match_pattern_class_argument(
        &mut self,
        state: &mut MatchPatternClassArgumentParsingState,
    ) {
        let pattern_start = self.node_start();
        let pattern = self.parse_match_pattern(AllowStarPattern::No);

        self.parse_match_pattern_class_argument_from_pattern(state, pattern_start, pattern);
    }

    fn parse_match_pattern_class_argument_from_pattern(
        &mut self,
        state: &mut MatchPatternClassArgumentParsingState,
        pattern_start: TextSize,
        pattern: Pattern,
    ) {
        if self.eat(TokenKind::Equal) {
            let attr = self.parse_match_pattern_class_keyword_attr(pattern);
            let value_pattern = self.parse_match_pattern(AllowStarPattern::No);
            self.parse_match_pattern_class_keyword_argument_from_value(
                state,
                pattern_start,
                attr,
                value_pattern,
            );
        } else {
            state.has_seen_pattern = true;
            state.patterns.push(pattern);
        }

        if state.has_seen_keyword_pattern && state.has_seen_pattern {
            self.add_error(
                ParseErrorType::OtherError(
                    "Positional patterns cannot follow keyword patterns".to_string(),
                ),
                self.node_range(pattern_start),
            );
        }
    }

    fn parse_match_pattern_class_keyword_attr(&mut self, pattern: Pattern) -> ast::Identifier {
        if let Pattern::MatchAs(ast::PatternMatchAs {
            pattern: None,
            name: Some(name),
            ..
        }) = pattern
        {
            name
        } else {
            self.add_error(
                ParseErrorType::OtherError(
                    "Expected an identifier for the keyword pattern".to_string(),
                ),
                &pattern,
            );
            ast::Identifier {
                id: Name::empty(),
                range: self.missing_node_range(),
                node_index: AtomicNodeIndex::NONE,
            }
        }
    }

    fn parse_match_pattern_class_keyword_argument_from_value(
        &self,
        state: &mut MatchPatternClassArgumentParsingState,
        pattern_start: TextSize,
        attr: ast::Identifier,
        pattern: Pattern,
    ) {
        state.has_seen_pattern = false;
        state.has_seen_keyword_pattern = true;
        state.keywords.push(ast::PatternKeyword {
            attr,
            pattern,
            range: self.node_range(pattern_start),
            node_index: AtomicNodeIndex::NONE,
        });
    }

    fn finish_match_pattern_class_arguments(
        &self,
        arguments_start: TextSize,
        state: MatchPatternClassArgumentParsingState,
    ) -> ast::PatternArguments {
        ast::PatternArguments {
            patterns: state.patterns,
            keywords: state.keywords,
            range: self.node_range(arguments_start),
            node_index: AtomicNodeIndex::NONE,
        }
    }
}

struct PendingMatchPatternMapping {
    start: TextSize,
    mapping_item_start: TextSize,
    key: Expr,
    value_start: TextSize,
}

#[derive(Default)]
struct MatchPatternMappingParsingState {
    keys: Vec<Expr>,
    patterns: Vec<Pattern>,
    rest: Option<ast::Identifier>,
}

struct PendingMatchPatternClass {
    cls: Box<Expr>,
    start: TextSize,
    arguments_start: TextSize,
    first_argument: PendingMatchPatternClassArgument,
}

struct NestedMatchPatternClassArgument {
    cls: Pattern,
    cls_start: TextSize,
    pending_argument: PendingMatchPatternClassArgument,
}

enum PendingMatchPatternClassArgument {
    Positional {
        pattern_start: TextSize,
    },
    Keyword {
        pattern_start: TextSize,
        attr: ast::Identifier,
        value_start: TextSize,
    },
}

impl PendingMatchPatternClassArgument {
    const fn value_start(&self) -> TextSize {
        match self {
            PendingMatchPatternClassArgument::Positional { pattern_start } => *pattern_start,
            PendingMatchPatternClassArgument::Keyword { value_start, .. } => *value_start,
        }
    }
}

#[derive(Default)]
struct MatchPatternClassArgumentParsingState {
    patterns: Vec<Pattern>,
    keywords: Vec<ast::PatternKeyword>,
    has_seen_pattern: bool,
    has_seen_keyword_pattern: bool,
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

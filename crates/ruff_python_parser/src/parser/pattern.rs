use crate::parser::syntax::{self as id, Construct, IntoList};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    self as ast, AstNode, ExprContext, ExprKind, ListId, Number, Operator, PatternKind, Singleton,
    build,
};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::progress::ParserProgress;
use crate::parser::{Parser, RecoveryContextKind, SequenceMatchPatternParentheses};
use crate::token_set::TokenSet;
use crate::{ParseErrorType, UnsupportedSyntaxErrorKind};

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
    TokenKind::Plus,  // Unary plus
]);

/// The set of tokens that can start a pattern.
const PATTERN_START_SET: TokenSet = TokenSet::new([
    // Star pattern
    TokenKind::Star,
    // Capture pattern
    // Wildcard pattern ('_' is an identifier token)
    // Value pattern (name or attribute)
    // Class pattern
    TokenKind::Identifier,
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
    TokenKind::Identifier,
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
    pub(super) fn parse_match_patterns(&mut self) -> id::Pattern {
        let start = self.node_start();

        // We don't yet know if it's a sequence pattern or a single pattern, so
        // we need to allow star pattern here.
        let pattern = self.parse_match_pattern(AllowStarPattern::Yes);

        if self.at(TokenKind::Comma) {
            id::Pattern::from(self.parse_sequence_match_pattern(pattern, start, None))
        } else {
            // We know it's not a sequence pattern now, so check for star pattern usage.
            if pattern.kind == ast::NodeKind::PatternMatchStar {
                self.add_error(ParseErrorType::InvalidStarPatternUsage, pattern);
            }
            pattern
        }
    }

    /// Parses an `or_pattern` or an `as_pattern`.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-pattern>
    fn parse_match_pattern(&mut self, allow_star_pattern: AllowStarPattern) -> id::Pattern {
        let start = self.node_start();

        // We don't yet know if it's an or pattern or an as pattern, so use whatever
        // was passed in.
        let mut lhs = self.parse_match_pattern_lhs(allow_star_pattern);

        // Or pattern
        if self.at(TokenKind::Vbar) {
            // We know it's an `or` pattern now, so check for star pattern usage.
            if lhs.kind == ast::NodeKind::PatternMatchStar {
                self.add_error(ParseErrorType::InvalidStarPatternUsage, lhs);
            }

            let mut patterns = vec![lhs];
            let mut progress = ParserProgress::default();

            while self.eat(TokenKind::Vbar) {
                progress.assert_progressing(self);
                let pattern = self.parse_match_pattern_lhs(AllowStarPattern::No);
                patterns.push(pattern);
            }

            lhs = id::Pattern::from(
                build::PatternMatchOr {
                    range: self.node_range(start),
                    patterns: patterns.into_list(&mut self.storage),
                }
                .parsed(&mut self.storage),
            );
        }

        // As pattern
        if self.eat(TokenKind::As) {
            // We know it's an `as` pattern now, so check for star pattern usage.
            if lhs.kind == ast::NodeKind::PatternMatchStar {
                self.add_error(ParseErrorType::InvalidStarPatternUsage, lhs);
            }

            let ident = self.parse_match_pattern_target();
            lhs = id::Pattern::from(
                build::PatternMatchAs {
                    range: self.node_range(start),
                    name: Some(ident).map(|node| node.id),
                    pattern: Some(lhs).map(|node| node.id),
                }
                .parsed(&mut self.storage),
            );
        }

        lhs
    }

    /// Parses a pattern.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-closed_pattern>
    fn parse_match_pattern_lhs(&mut self, allow_star_pattern: AllowStarPattern) -> id::Pattern {
        self.with_recursion(|parser| {
            let start = parser.node_start();

            let mut lhs = match parser.current_token_kind() {
                TokenKind::Lbrace => id::Pattern::from(parser.parse_match_pattern_mapping()),
                TokenKind::Star => {
                    let star_pattern = parser.parse_match_pattern_star();
                    if allow_star_pattern.is_no() {
                        parser.add_error(ParseErrorType::InvalidStarPatternUsage, star_pattern);
                    }
                    id::Pattern::from(star_pattern)
                }
                TokenKind::Lpar | TokenKind::Lsqb => {
                    parser.parse_parenthesized_or_sequence_pattern()
                }
                _ => parser.parse_match_pattern_literal(),
            };

            if parser.at(TokenKind::Lpar) {
                lhs = id::Pattern::from(parser.parse_match_pattern_class(lhs, start));
            }

            if matches!(
                parser.current_token_kind(),
                TokenKind::Plus | TokenKind::Minus
            ) {
                lhs = id::Pattern::from(parser.parse_complex_literal_pattern(lhs, start));
            }

            lhs
        })
    }

    /// Parses a mapping pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `{` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#mapping-patterns>
    fn parse_match_pattern_mapping(&mut self) -> id::PatternMatchMapping {
        let start = self.node_start();
        self.bump(TokenKind::Lbrace);

        let mut items = Vec::new();
        let mut has_rest = false;

        self.parse_comma_separated_list(RecoveryContextKind::MatchPatternMapping, |parser| {
            let mapping_item_start = parser.node_start();

            if parser.eat(TokenKind::DoubleStar) {
                let identifier = parser.parse_match_pattern_target();
                if has_rest {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "Only one double star pattern is allowed".to_string(),
                        ),
                        parser.node_range(mapping_item_start),
                    );
                }
                has_rest = true;
                items.push(ast::MappingPatternItem::Rest(identifier.id));
            } else {
                let pattern = parser.parse_match_pattern_lhs(AllowStarPattern::No);
                let range = pattern.range();
                let key = match pattern.view(&parser.storage).classify() {
                    PatternKind::MatchValue(node) => id::Node::from_view(node.value()),
                    PatternKind::MatchSingleton(node) => match node.value() {
                        Singleton::None => build::ExprNoneLiteral { range }
                            .parsed(&mut parser.storage)
                            .into(),
                        value => build::ExprBooleanLiteral {
                            range,
                            value: value == Singleton::True,
                        }
                        .parsed(&mut parser.storage)
                        .into(),
                    },
                    _ => {
                        parser.add_error(
                            ParseErrorType::OtherError("Invalid mapping pattern key".to_string()),
                            pattern,
                        );
                        parser.pattern_to_expr(pattern.id)
                    }
                };
                parser.expect(TokenKind::Colon);

                let pattern = parser.parse_match_pattern(AllowStarPattern::No);
                items.push(ast::MappingPatternItem::Entry {
                    key: key.id,
                    pattern: pattern.id,
                });

                if has_rest {
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

        build::PatternMatchMapping {
            range: self.node_range(start),
            items: self.storage.mapping_pattern_items(items),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a star pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `*` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-star_pattern>
    fn parse_match_pattern_star(&mut self) -> id::PatternMatchStar {
        let start = self.node_start();
        self.bump(TokenKind::Star);

        let ident = self.parse_identifier();

        build::PatternMatchStar {
            range: self.node_range(start),
            name: if ident.view(&self.storage).id() == "_" {
                None
            } else {
                Some(ident)
            }
            .map(|node| node.id),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a binding target in an `as` or mapping pattern.
    fn parse_match_pattern_target(&mut self) -> id::Identifier {
        // test_err invalid_match_pattern_target
        // match value:
        //     case 1 as _: ...
        //     case {**_}: ...
        // after = 1
        let identifier = self.parse_identifier();
        if identifier.view(&self.storage).id() == "_" {
            self.add_error(ParseErrorType::InvalidMatchPatternTarget, identifier);
        }
        identifier
    }

    /// Parses a parenthesized pattern or a sequence pattern.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `(` or `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#sequence-patterns>
    fn parse_parenthesized_or_sequence_pattern(&mut self) -> id::Pattern {
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
            // parenthesis, it'll consider `case` an identifier token instead.
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "Missing '{closing}'",
                    closing = if parentheses.is_list() { "]" } else { ")" }
                )),
                self.current_token_range(),
            );
        }

        if self.eat(parentheses.closing_kind()) {
            return id::Pattern::from(
                build::PatternMatchSequence {
                    patterns: ListId::default(),
                    range: self.node_range(start),
                }
                .parsed(&mut self.storage),
            );
        }

        let mut pattern = self.parse_match_pattern(AllowStarPattern::Yes);

        if parentheses.is_list() || self.at(TokenKind::Comma) {
            pattern = id::Pattern::from(self.parse_sequence_match_pattern(
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
        first_element: id::Pattern,
        start: TextSize,
        parentheses: Option<SequenceMatchPatternParentheses>,
    ) -> id::PatternMatchSequence {
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

        build::PatternMatchSequence {
            range: self.node_range(start),
            patterns: patterns.into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a literal pattern.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-literal_pattern>
    fn parse_match_pattern_literal(&mut self) -> id::Pattern {
        let start = self.node_start();
        match self.current_token_kind() {
            TokenKind::None => {
                self.bump(TokenKind::None);
                id::Pattern::from(
                    build::PatternMatchSingleton {
                        value: Singleton::None,
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::True => {
                self.bump(TokenKind::True);
                id::Pattern::from(
                    build::PatternMatchSingleton {
                        value: Singleton::True,
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::False => {
                self.bump(TokenKind::False);
                id::Pattern::from(
                    build::PatternMatchSingleton {
                        value: Singleton::False,
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::String | TokenKind::FStringStart | TokenKind::TStringStart => {
                let str = self.parse_strings();

                id::Pattern::from(
                    build::PatternMatchValue {
                        value: str.id,
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Complex => {
                let (real, imag) = self.bump_complex();
                let range = self.node_range(start);

                id::Pattern::from(
                    build::PatternMatchValue {
                        value: id::Expr::from(
                            build::ExprNumberLiteral {
                                value: Number::Complex { real, imag },
                                range,
                            }
                            .parsed(&mut self.storage),
                        )
                        .id,
                        range,
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Int => {
                let value = self.bump_int();
                let range = self.node_range(start);

                id::Pattern::from(
                    build::PatternMatchValue {
                        value: id::Expr::from(
                            build::ExprNumberLiteral {
                                value: Number::Int(value),
                                range,
                            }
                            .parsed(&mut self.storage),
                        )
                        .id,
                        range,
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Float => {
                let value = self.bump_float();
                let range = self.node_range(start);

                id::Pattern::from(
                    build::PatternMatchValue {
                        value: id::Expr::from(
                            build::ExprNumberLiteral {
                                value: Number::Float(value),
                                range,
                            }
                            .parsed(&mut self.storage),
                        )
                        .id,
                        range,
                    }
                    .parsed(&mut self.storage),
                )
            }
            kind => {
                if let Some(unary_arithmetic_op) = kind.as_unary_arithmetic_operator() {
                    if matches!(
                        self.peek(),
                        TokenKind::Int | TokenKind::Float | TokenKind::Complex
                    ) {
                        let unary_expr = self.parse_unary_expression(
                            unary_arithmetic_op,
                            ExpressionContext::default(),
                        );

                        // test_err signed_pattern_non_literal_operand
                        // # parse_options: {"target-version": "3.15"}
                        // match value:
                        //     case -1**2: ...
                        //     case -1 .real: ...
                        //     case -1[0]: ...
                        //     case -1(): ...
                        //     case {+1**2: _}: ...

                        // Parse the full operand for error recovery, but only numeric literals
                        // are valid after a sign in a literal pattern.
                        if unary_expr.view(&self.storage).operand().node_kind()
                            != ast::NodeKind::ExprNumberLiteral
                        {
                            self.add_error(
                                ParseErrorType::OtherError(
                                    "Expected a numeric literal after unary operator".to_string(),
                                ),
                                unary_expr.view(&self.storage).operand().range(),
                            );
                        }

                        // test_ok unary_plus_py315
                        // # parse_options: {"target-version": "3.15"}
                        // match foo:
                        //     case +1: ...
                        //     # this is also now valid inside more complicated patterns
                        //     case {+1: 2}: ...

                        // test_err unary_plus_py314
                        // # parse_options: {"target-version": "3.14"}
                        // match foo:
                        //     case +1: ...
                        //     case {+1: 2}: ...
                        if unary_expr.view(&self.storage).op().is_u_add() {
                            self.add_unsupported_syntax_error(
                                UnsupportedSyntaxErrorKind::UnaryPlusMatchPattern,
                                unary_expr.range,
                            );
                        }

                        return id::Pattern::from(
                            build::PatternMatchValue {
                                value: id::Expr::from(unary_expr).id,
                                range: self.node_range(start),
                            }
                            .parsed(&mut self.storage),
                        );
                    }
                }

                if self.at_identifier_or_keyword() {
                    if self.peek() == TokenKind::Dot {
                        // test_ok match_attr_pattern_soft_keyword
                        // match foo:
                        //     case match.bar: ...
                        //     case case.bar: ...
                        //     case type.bar: ...
                        //     case match.case.type.bar.type.case.match: ...
                        let id = id::Expr::from(self.parse_name(ExpressionContext::default()));

                        let attribute = self.parse_attr_expr_for_match_pattern(id, start);

                        id::Pattern::from(
                            build::PatternMatchValue {
                                value: attribute.id,
                                range: self.node_range(start),
                            }
                            .parsed(&mut self.storage),
                        )
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
                        id::Pattern::from(
                            build::PatternMatchAs {
                                range: ident.range,
                                pattern: None,
                                name: if ident.view(&self.storage).id() == "_" {
                                    None
                                } else {
                                    Some(ident)
                                }
                                .map(|node| node.id),
                            }
                            .parsed(&mut self.storage),
                        )
                    }
                } else {
                    // Upon encountering an unexpected token, return a `Pattern::MatchValue` containing
                    // an empty `Expr::Name`.
                    self.add_error(
                        ParseErrorType::OtherError("Expected a pattern".to_string()),
                        self.current_token_range(),
                    );
                    let invalid_node = id::Expr::from(
                        build::ExprName {
                            range: self.missing_node_range(),
                            id: self.storage.intern_name(""),
                            ctx: ExprContext::Invalid,
                        }
                        .parsed(&mut self.storage),
                    );
                    id::Pattern::from(
                        build::PatternMatchValue {
                            range: invalid_node.range(),
                            value: invalid_node.id,
                        }
                        .parsed(&mut self.storage),
                    )
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
        lhs: id::Pattern,
        start: TextSize,
    ) -> id::PatternMatchValue {
        let operator = if self.eat(TokenKind::Plus) {
            Operator::Add
        } else {
            self.bump(TokenKind::Minus);
            Operator::Sub
        };

        let lhs_value =
            if let Some(matched) = lhs.view(&self.storage).cast::<ast::PatternMatchValue>() {
                let value = id::Node::from_view(matched.value());
                if !is_real_number(matched.value()) {
                    self.add_error(ParseErrorType::ExpectedRealNumber, lhs);
                }
                value
            } else {
                self.add_error(ParseErrorType::ExpectedRealNumber, lhs);
                self.pattern_to_expr(lhs.id)
            };

        let rhs_pattern = self.parse_match_pattern_lhs(AllowStarPattern::No);
        let rhs_value = if let Some(matched) = rhs_pattern
            .view(&self.storage)
            .cast::<ast::PatternMatchValue>()
        {
            let value = id::Node::from_view(matched.value());
            if !is_complex_number(matched.value()) {
                self.add_error(ParseErrorType::ExpectedImaginaryNumber, rhs_pattern);
            }
            value
        } else {
            self.add_error(ParseErrorType::ExpectedImaginaryNumber, rhs_pattern);
            self.pattern_to_expr(rhs_pattern.id)
        };

        let range = self.node_range(start);

        build::PatternMatchValue {
            value: id::Expr::from(
                build::ExprBinOp {
                    left: lhs_value.id,
                    op: operator,
                    right: rhs_value.id,
                    range,
                }
                .parsed(&mut self.storage),
            )
            .id,
            range,
        }
        .parsed(&mut self.storage)
    }

    /// Parses an attribute expression until the current token is not a `.`.
    fn parse_attr_expr_for_match_pattern(
        &mut self,
        mut lhs: id::Expr,
        start: TextSize,
    ) -> id::Expr {
        while self.current_token_kind() == TokenKind::Dot {
            lhs = id::Expr::from(self.parse_attribute_expression(
                lhs,
                start,
                ExpressionContext::default(),
            ));
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
        cls: id::Pattern,
        start: TextSize,
    ) -> id::PatternMatchClass {
        let arguments_start = self.node_start();

        let cls = match cls.view(&self.storage).classify() {
            PatternKind::MatchAs(node)
                if node.pattern().is_none()
                    && let Some(ident) = node.name() =>
            {
                let ctx = if ident.id().is_empty() {
                    ExprContext::Invalid
                } else {
                    ExprContext::Load
                };
                build::ExprName {
                    range: ident.range(),
                    id: ident.name_id(),
                    ctx,
                }
                .parsed(&mut self.storage)
                .into()
            }
            PatternKind::MatchValue(node)
                if node.value().node_kind() == ast::NodeKind::ExprAttribute =>
            {
                id::Node::from_view(node.value())
            }
            _ => {
                self.add_error(
                    ParseErrorType::OtherError("Invalid value for a class pattern".to_string()),
                    cls,
                );
                self.pattern_to_expr(cls.id)
            }
        };

        self.bump(TokenKind::Lpar);

        let mut patterns = Vec::<id::Pattern>::new();
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

                    let key = if let Some(node) =
                        pattern.view(&parser.storage).cast::<ast::PatternMatchAs>()
                        && node.pattern().is_none()
                        && let Some(name) = node.name()
                    {
                        id::Node::from_view(name)
                    } else {
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Expected an identifier for the keyword pattern".to_string(),
                            ),
                            pattern,
                        );
                        build::Identifier {
                            id: parser.storage.intern_name(""),
                            range: parser.missing_node_range(),
                        }
                        .parsed(&mut parser.storage)
                    };

                    let value_pattern = parser.parse_match_pattern(AllowStarPattern::No);

                    keywords.push(
                        build::PatternKeyword {
                            attr: key.id,
                            pattern: value_pattern.id,
                            range: parser.node_range(pattern_start),
                        }
                        .parsed(&mut parser.storage),
                    );
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

        build::PatternMatchClass {
            cls: cls.id,
            arguments: build::PatternArguments {
                patterns: patterns.into_list(&mut self.storage),
                keywords: keywords.into_list(&mut self.storage),
                range: self.node_range(arguments_start),
            }
            .parsed(&mut self.storage)
            .id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
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
fn is_real_number(expr: ast::Expr<'_>) -> bool {
    match expr.classify() {
        ExprKind::NumberLiteral(value) => {
            matches!(value.value(), Number::Int(_) | Number::Float(_))
        }
        ExprKind::UnaryOp(unary)
            if matches!(unary.op(), ast::UnaryOp::UAdd | ast::UnaryOp::USub) =>
        {
            is_real_number(unary.operand())
        }
        _ => false,
    }
}

/// Returns `true` if the given expression is a complex number literal.
fn is_complex_number(expr: ast::Expr<'_>) -> bool {
    expr.cast::<ast::ExprNumberLiteral>()
        .is_some_and(|value| matches!(value.value(), Number::Complex { .. }))
}

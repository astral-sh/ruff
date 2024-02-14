use ruff_python_ast::{
    self as ast, Expr, ExprContext, Number, Operator, Pattern, Singleton, UnaryOp,
};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::progress::ParserProgress;
use crate::parser::{Parser, SequenceMatchPatternParentheses};
use crate::token_set::TokenSet;
use crate::{ParseErrorType, Tok, TokenKind};

const END_EXPR_SET: TokenSet = TokenSet::new([
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
    pub(super) fn parse_match_patterns(&mut self) -> Pattern {
        let start = self.node_start();
        let pattern = self.parse_match_pattern();

        if self.at(TokenKind::Comma) {
            Pattern::MatchSequence(self.parse_sequence_match_pattern(pattern, start, None))
        } else {
            pattern
        }
    }

    fn parse_match_pattern(&mut self) -> Pattern {
        let start = self.node_start();
        let mut lhs = self.parse_match_pattern_lhs();

        // Or pattern
        if self.at(TokenKind::Vbar) {
            let mut patterns = vec![lhs];
            let mut progress = ParserProgress::default();

            while self.eat(TokenKind::Vbar) {
                progress.assert_progressing(self);
                let pattern = self.parse_match_pattern_lhs();
                patterns.push(pattern);
            }

            lhs = Pattern::MatchOr(ast::PatternMatchOr {
                range: self.node_range(start),
                patterns,
            });
        }

        // As pattern
        if self.eat(TokenKind::As) {
            let ident = self.parse_identifier();
            lhs = Pattern::MatchAs(ast::PatternMatchAs {
                range: self.node_range(start),
                name: Some(ident),
                pattern: Some(Box::new(lhs)),
            });
        }

        lhs
    }

    fn parse_match_pattern_lhs(&mut self) -> Pattern {
        let start = self.node_start();
        let mut lhs = match self.current_kind() {
            TokenKind::Lbrace => Pattern::MatchMapping(self.parse_match_pattern_mapping()),
            TokenKind::Star => Pattern::MatchStar(self.parse_match_pattern_star()),
            TokenKind::Lpar | TokenKind::Lsqb => self.parse_delimited_match_pattern(),
            _ => self.parse_match_pattern_literal(),
        };

        if self.at(TokenKind::Lpar) {
            lhs = Pattern::MatchClass(self.parse_match_pattern_class(lhs, start));
        }

        if self.at(TokenKind::Plus) || self.at(TokenKind::Minus) {
            let (operator_token, _) = self.next_token();
            let operator = if matches!(operator_token, Tok::Plus) {
                Operator::Add
            } else {
                Operator::Sub
            };

            let lhs_value = if let Pattern::MatchValue(lhs) = lhs {
                if !lhs.value.is_literal_expr() && !matches!(lhs.value.as_ref(), Expr::UnaryOp(_)) {
                    self.add_error(
                        ParseErrorType::OtherError(format!(
                            "invalid `{}` expression for match pattern",
                            self.src_text(lhs.range)
                        )),
                        lhs.range,
                    );
                }
                lhs.value
            } else {
                self.add_error(
                    ParseErrorType::OtherError("invalid lhs pattern".to_string()),
                    &lhs,
                );

                #[allow(deprecated)]
                Box::new(Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(lhs.range()).into(),
                    range: lhs.range(),
                }))
            };

            let rhs_pattern = self.parse_match_pattern_lhs();
            let rhs_value = if let Pattern::MatchValue(rhs) = rhs_pattern {
                if !rhs.value.is_literal_expr() {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "invalid expression for match pattern".to_string(),
                        ),
                        &rhs,
                    );
                }
                rhs.value
            } else {
                self.add_error(
                    ParseErrorType::OtherError("invalid rhs pattern".to_string()),
                    rhs_pattern.range(),
                );

                #[allow(deprecated)]
                Box::new(Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(rhs_pattern.range()).into(),
                    range: rhs_pattern.range(),
                }))
            };

            if matches!(
                rhs_value.as_ref(),
                Expr::UnaryOp(ast::ExprUnaryOp {
                    op: UnaryOp::USub,
                    ..
                })
            ) {
                self.add_error(
                    ParseErrorType::OtherError(
                        "`-` not allowed in rhs of match pattern".to_string(),
                    ),
                    rhs_value.range(),
                );
            }

            let range = self.node_range(start);

            return Pattern::MatchValue(ast::PatternMatchValue {
                value: Box::new(Expr::BinOp(ast::ExprBinOp {
                    left: lhs_value,
                    op: operator,
                    right: rhs_value,
                    range,
                })),
                range,
            });
        }

        lhs
    }

    fn parse_match_pattern_mapping(&mut self) -> ast::PatternMatchMapping {
        let start = self.node_start();
        let mut keys = vec![];
        let mut patterns = vec![];
        let mut rest = None;

        #[allow(deprecated)]
        self.parse_delimited(
            true,
            TokenKind::Lbrace,
            TokenKind::Comma,
            TokenKind::Rbrace,
            |parser| {
                if parser.eat(TokenKind::DoubleStar) {
                    rest = Some(parser.parse_identifier());
                } else {
                    let key = match parser.parse_match_pattern_lhs() {
                        Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => *value,
                        Pattern::MatchSingleton(ast::PatternMatchSingleton { value, range }) => {
                            match value {
                                Singleton::None => {
                                    Expr::NoneLiteral(ast::ExprNoneLiteral { range })
                                }
                                Singleton::True => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                                    value: true,
                                    range,
                                }),
                                Singleton::False => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                                    value: false,
                                    range,
                                }),
                            }
                        }
                        pattern => {
                            parser.add_error(
                                ParseErrorType::OtherError(format!(
                                    "invalid mapping pattern key `{}`",
                                    parser.src_text(&pattern)
                                )),
                                &pattern,
                            );
                            Expr::Invalid(ast::ExprInvalid {
                                value: parser.src_text(&pattern).into(),
                                range: pattern.range(),
                            })
                        }
                    };
                    keys.push(key);

                    parser.expect(TokenKind::Colon);

                    patterns.push(parser.parse_match_pattern());
                }
            },
        );

        ast::PatternMatchMapping {
            range: self.node_range(start),
            keys,
            patterns,
            rest,
        }
    }

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

    fn parse_delimited_match_pattern(&mut self) -> Pattern {
        let start = self.node_start();
        let parentheses = if self.eat(TokenKind::Lpar) {
            SequenceMatchPatternParentheses::Tuple
        } else {
            self.bump(TokenKind::Lsqb);
            SequenceMatchPatternParentheses::List
        };

        if matches!(self.current_kind(), TokenKind::Newline | TokenKind::Colon) {
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "missing `{closing}`",
                    closing = if parentheses.is_list() { "]" } else { ")" }
                )),
                self.current_range(),
            );
        }

        if self.eat(parentheses.closing_kind()) {
            return Pattern::MatchSequence(ast::PatternMatchSequence {
                patterns: vec![],
                range: self.node_range(start),
            });
        }

        let mut pattern = self.parse_match_pattern();

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

    fn parse_sequence_match_pattern(
        &mut self,
        first_elt: Pattern,
        start: TextSize,
        parentheses: Option<SequenceMatchPatternParentheses>,
    ) -> ast::PatternMatchSequence {
        let ending = parentheses.map_or(
            TokenKind::Colon,
            SequenceMatchPatternParentheses::closing_kind,
        );

        if parentheses.is_some_and(|parentheses| {
            self.at(parentheses.closing_kind()) || self.peek_nth(1) == parentheses.closing_kind()
        }) {
            // The comma is optional if it is a single-element sequence
            self.eat(TokenKind::Comma);
        } else {
            self.expect(TokenKind::Comma);
        }

        let mut patterns = vec![first_elt];

        #[allow(deprecated)]
        self.parse_separated(true, TokenKind::Comma, [ending], |parser| {
            patterns.push(parser.parse_match_pattern());
        });

        if let Some(parentheses) = parentheses {
            self.expect(parentheses.closing_kind());
        }

        let range = self.node_range(start);
        ast::PatternMatchSequence { range, patterns }
    }

    fn parse_match_pattern_literal(&mut self) -> Pattern {
        let start = self.node_start();
        match self.current_kind() {
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
            TokenKind::String => {
                let str = self.parse_string_expression();

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(str),
                    range: self.node_range(start),
                })
            }
            TokenKind::Complex => {
                let (Tok::Complex { real, imag }, _) = self.bump(TokenKind::Complex) else {
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
                let (Tok::Int { value }, _) = self.bump(TokenKind::Int) else {
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
                let (Tok::Float { value }, _) = self.bump(TokenKind::Float) else {
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
            TokenKind::Name if self.peek_nth(1) == TokenKind::Dot => {
                let (Tok::Name { name }, _) = self.bump(TokenKind::Name) else {
                    unreachable!()
                };
                let id = Expr::Name(ast::ExprName {
                    id: name.to_string(),
                    ctx: ExprContext::Load,
                    range: self.node_range(start),
                });

                let attribute = self.parse_attr_expr_for_match_pattern(id, start);

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(attribute),
                    range: self.node_range(start),
                })
            }
            TokenKind::Name => {
                let (Tok::Name { name }, _) = self.bump(TokenKind::Name) else {
                    unreachable!()
                };
                let range = self.node_range(start);

                Pattern::MatchAs(ast::PatternMatchAs {
                    range,
                    pattern: None,
                    name: if name.contains("_") {
                        None
                    } else {
                        Some(ast::Identifier {
                            id: name.to_string(),
                            range,
                        })
                    },
                })
            }
            TokenKind::Minus
                if matches!(
                    self.peek_nth(1),
                    TokenKind::Int | TokenKind::Float | TokenKind::Complex
                ) =>
            {
                let parsed_expr = self.parse_lhs_expression();

                let range = self.node_range(start);
                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(parsed_expr.expr),
                    range,
                })
            }
            kind => {
                // Upon encountering an unexpected token, return a `Pattern::MatchValue` containing
                // an empty `Expr::Name`.
                let invalid_node = if kind.is_keyword() {
                    Expr::Name(self.parse_name())
                } else {
                    self.add_error(
                        ParseErrorType::OtherError("Expression expected.".to_string()),
                        self.current_range(),
                    );
                    Expr::Name(ast::ExprName {
                        range: self.missing_node_range(),
                        id: String::new(),
                        ctx: ExprContext::Load,
                    })
                };

                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(invalid_node),
                    range: self.missing_node_range(),
                })
            }
        }
    }

    fn parse_attr_expr_for_match_pattern(&mut self, mut lhs: Expr, start: TextSize) -> Expr {
        while self.current_kind() == TokenKind::Dot {
            lhs = Expr::Attribute(self.parse_attribute_expression(lhs, start));
        }

        lhs
    }

    fn parse_match_pattern_class(
        &mut self,
        cls: Pattern,
        start: TextSize,
    ) -> ast::PatternMatchClass {
        let mut patterns = vec![];
        let mut keywords = vec![];
        let mut has_seen_pattern = false;
        let mut has_seen_keyword_pattern = false;

        let arguments_start = self.node_start();
        #[allow(deprecated)]
        self.parse_delimited(
            true,
            TokenKind::Lpar,
            TokenKind::Comma,
            TokenKind::Rpar,
            |parser| {
                let pattern_start = parser.node_start();
                let pattern = parser.parse_match_pattern();

                if parser.eat(TokenKind::Equal) {
                    has_seen_pattern = false;
                    has_seen_keyword_pattern = true;

                    if let Pattern::MatchAs(ast::PatternMatchAs {
                        name: Some(attr), ..
                    }) = pattern
                    {
                        let pattern = parser.parse_match_pattern();

                        keywords.push(ast::PatternKeyword {
                            attr,
                            pattern,
                            range: parser.node_range(pattern_start),
                        });
                    } else {
                        #[allow(deprecated)]
                        parser.skip_until(END_EXPR_SET);
                        parser.add_error(
                            ParseErrorType::OtherError("`not valid keyword pattern".to_string()),
                            parser.node_range(pattern_start),
                        );
                    }
                } else {
                    has_seen_pattern = true;
                    patterns.push(pattern);
                }

                if has_seen_keyword_pattern && has_seen_pattern {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "pattern not allowed after keyword pattern".to_string(),
                        ),
                        parser.node_range(pattern_start),
                    );
                }
            },
        );

        let arguments_range = self.node_range(arguments_start);

        let cls = match cls {
            Pattern::MatchAs(ast::PatternMatchAs {
                name: Some(ident), ..
            }) => {
                if ident.is_valid() {
                    Box::new(Expr::Name(ast::ExprName {
                        range: ident.range(),
                        id: ident.id,
                        ctx: ExprContext::Load,
                    }))
                } else {
                    #[allow(deprecated)]
                    Box::new(Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(&ident).into(),
                        range: ident.range(),
                    }))
                }
            }
            Pattern::MatchValue(ast::PatternMatchValue { value, range: _ })
                if matches!(value.as_ref(), Expr::Attribute(_)) =>
            {
                value
            }
            pattern => {
                self.add_error(
                    ParseErrorType::OtherError("invalid pattern match class".to_string()),
                    &pattern,
                );
                // FIXME(micha): Including the entire range is not ideal because it also includes trivia.
                #[allow(deprecated)]
                Box::new(Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(pattern.range()).into(),
                    range: pattern.range(),
                }))
            }
        };

        ast::PatternMatchClass {
            cls,
            arguments: ast::PatternArguments {
                patterns,
                keywords,
                range: arguments_range,
            },
            range: self.node_range(start),
        }
    }
}

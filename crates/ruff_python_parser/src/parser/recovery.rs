use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    self as ast, AstNode, AstNodeId, BuildId, ExprContext, ListId, Operator, PatternKind,
    Singleton, build,
};
use ruff_text_size::{Ranged, TextLen, TextRange};

use super::Parser;
use super::syntax::{self as id, Construct};

impl Parser<'_> {
    /// Converts an invalid pattern to the equivalent expression for error recovery.
    ///
    /// Singletons become literals, sequences become lists, mappings become dictionaries,
    /// and class patterns become calls. A starred capture becomes a starred name. An
    /// or-pattern becomes a left-associative chain of bitwise-or expressions.
    ///
    /// Value patterns, mapping keys, and class heads already contain expressions. Reuse their
    /// handles, since a class head can itself contain an expression from an earlier recovery.
    /// Copying it into the same arena would retain its literal data once per recovery.
    pub(super) fn pattern_to_expr(&mut self, pattern: BuildId<ast::kind::Pattern>) -> id::Expr {
        self.with_recursion(|parser| parser.pattern_to_expr_inner(pattern))
    }

    fn pattern_to_expr_inner(&mut self, pattern: BuildId<ast::kind::Pattern>) -> id::Expr {
        let pattern = self.storage.get(pattern);
        let range = pattern.range();
        match pattern.classify() {
            PatternKind::MatchValue(node) => id::Node::from_view(node.value()),
            PatternKind::MatchSingleton(node) => match node.value() {
                Singleton::None => build::ExprNoneLiteral { range }
                    .parsed(&mut self.storage)
                    .into(),
                value => build::ExprBooleanLiteral {
                    range,
                    value: value == Singleton::True,
                }
                .parsed(&mut self.storage)
                .into(),
            },
            PatternKind::MatchSequence(node) => {
                // Both `case [x, y]` and `case (x, y)` are sequence patterns.
                let elts = self.patterns_to_exprs(node.patterns().id());
                build::ExprList {
                    range,
                    elts: self.storage.list(elts),
                    ctx: ExprContext::Store,
                }
                .parsed(&mut self.storage)
                .into()
            }
            PatternKind::MatchMapping(node) => {
                let entries: Vec<_> = node
                    .items()
                    .into_iter()
                    .map(|item| match item {
                        ast::MappingPatternItem::Entry { key, pattern } => {
                            ast::MappingPatternItem::Entry {
                                key: key.build_id(),
                                pattern: pattern.build_id(),
                            }
                        }
                        ast::MappingPatternItem::Rest(name) => {
                            ast::MappingPatternItem::Rest(capture_name(name))
                        }
                    })
                    .collect();
                let items: Vec<_> = entries
                    .into_iter()
                    .map(|item| {
                        let (key, value, range) = match item {
                            ast::MappingPatternItem::Entry { key, pattern } => {
                                let value = self.pattern_to_expr(pattern);
                                let range = self.storage.get(key).range().cover(value.range());
                                (Some(key), value.id, range)
                            }
                            ast::MappingPatternItem::Rest(name) => {
                                // The pattern stores the identifier. The dictionary entry also
                                // covers its `**`, including any trivia before the identifier.
                                let range = self
                                    .tokens
                                    .previous_non_trivia(name.range.start())
                                    .filter(|token| token.kind() == TokenKind::DoubleStar)
                                    .map_or(name.range, |stars| stars.range().cover(name.range));
                                let name = name.parsed(&mut self.storage);
                                (None, name.id.into(), range)
                            }
                        };
                        build::DictItem { key, value, range }
                            .parsed(&mut self.storage)
                            .id
                    })
                    .collect();
                build::ExprDict {
                    range,
                    items: self.storage.list(items),
                }
                .parsed(&mut self.storage)
                .into()
            }
            PatternKind::MatchClass(node) => {
                let func = node.cls().build_id();
                let arguments = node.arguments();
                let arguments_range = arguments.range();
                debug_assert_eq!(range.end(), arguments_range.end());
                let patterns = arguments.patterns().id();
                let keyword_patterns = arguments.keywords().id();
                let args = self.patterns_to_exprs(patterns);
                let mut keywords = Vec::with_capacity(keyword_patterns.len());
                for index in 0..keyword_patterns.len() {
                    let keyword = self.storage.get_list(keyword_patterns).at(index);
                    let range = keyword.range();
                    let arg = keyword.attr().build_id();
                    let value = self.pattern_to_expr(keyword.pattern().build_id());
                    keywords.push(
                        build::Keyword {
                            range,
                            arg: Some(arg),
                            value: value.id,
                        }
                        .parsed(&mut self.storage),
                    );
                }
                let arguments = build::Arguments {
                    range: arguments_range,
                    args: self.storage.list(args),
                    keywords: self.storage.list(keywords),
                }
                .parsed(&mut self.storage);
                build::ExprCall {
                    range,
                    func,
                    arguments: arguments.id,
                }
                .parsed(&mut self.storage)
                .into()
            }
            PatternKind::MatchStar(node) => {
                let name = match node.name() {
                    Some(name) => capture_name(name),
                    None => build::ExprName {
                        range: TextRange::new(range.end() - "_".text_len(), range.end()),
                        id: self.storage.intern_name("_"),
                        ctx: ExprContext::Store,
                    },
                };
                let value = name.parsed(&mut self.storage);
                build::ExprStarred {
                    range,
                    value: value.id.into(),
                    ctx: ExprContext::Store,
                }
                .parsed(&mut self.storage)
                .into()
            }
            PatternKind::MatchAs(node) => {
                let name = match (node.pattern(), node.name()) {
                    // There is no expression that represents both sides of `x as y`.
                    (Some(_), Some(_)) => build::ExprName {
                        range,
                        id: self.storage.intern_name(""),
                        ctx: ExprContext::Invalid,
                    },
                    (Some(pattern), None) => return self.pattern_to_expr(pattern.build_id()),
                    (None, Some(name)) => capture_name(name),
                    (None, None) => build::ExprName {
                        range,
                        id: self.storage.intern_name("_"),
                        ctx: ExprContext::Store,
                    },
                };
                name.parsed(&mut self.storage).into()
            }
            PatternKind::MatchOr(node) => {
                // The parser creates an or-pattern only after seeing a second alternative.
                let patterns = node.patterns();
                let first = patterns.at(0);
                let start = first.start();
                let patterns = patterns.id();
                let mut left = self.pattern_to_expr(first.build_id());
                for index in 1..patterns.len() {
                    let pattern = self.storage.get_list(patterns).at(index);
                    let range = TextRange::new(start, pattern.end());
                    let right = self.pattern_to_expr(pattern.build_id());
                    left = build::ExprBinOp {
                        range,
                        left: left.id,
                        op: Operator::BitOr,
                        right: right.id,
                    }
                    .parsed(&mut self.storage)
                    .into();
                }
                left
            }
        }
    }

    fn patterns_to_exprs(
        &mut self,
        patterns: ListId<AstNodeId<ast::kind::Pattern>>,
    ) -> Vec<id::Expr> {
        (0..patterns.len())
            .map(|index| {
                let pattern = self.storage.get_list(patterns).at(index).build_id();
                self.pattern_to_expr(pattern)
            })
            .collect()
    }
}

fn capture_name(name: ast::Identifier<'_>) -> build::ExprName {
    build::ExprName {
        range: name.range(),
        id: name.name_id(),
        ctx: ExprContext::Store,
    }
}

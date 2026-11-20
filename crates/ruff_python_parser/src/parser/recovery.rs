use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    self as ast, AstNode, AstNodeId, BuildId, Emit, ExprContext, ListId, Operator, PatternKind,
    Singleton, build,
};
use ruff_text_size::{Ranged, TextLen, TextRange};

use super::Parser;
use super::syntax as id;

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
            PatternKind::MatchValue(node) => node.value().build_id(),
            PatternKind::MatchSingleton(node) => match node.value() {
                Singleton::None => build::ExprNoneLiteral { range }
                    .emit(&mut self.storage)
                    .into(),
                value => build::ExprBooleanLiteral {
                    range,
                    value: value == Singleton::True,
                }
                .emit(&mut self.storage)
                .into(),
            },
            PatternKind::MatchSequence(node) => {
                // Both `case [x, y]` and `case (x, y)` are sequence patterns.
                let elts = self.patterns_to_exprs(node.patterns().id());
                build::ExprList {
                    range,
                    elts,
                    ctx: ExprContext::Store,
                }
                .emit(&mut self.storage)
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
                let items = self.dict_item_scratch.snapshot();
                for item in entries {
                    let (key, value, range) = match item {
                        ast::MappingPatternItem::Entry { key, pattern } => {
                            let value = self.pattern_to_expr(pattern);
                            let range = self
                                .storage
                                .get(key)
                                .range()
                                .cover(self.storage.range(value));
                            (Some(key), value, range)
                        }
                        ast::MappingPatternItem::Rest(name) => {
                            // The pattern stores the identifier. The dictionary entry also
                            // covers its `**`, including any trivia before the identifier.
                            let range = self
                                .tokens
                                .previous_non_trivia(name.range.start())
                                .filter(|token| token.kind() == TokenKind::DoubleStar)
                                .map_or(name.range, |stars| stars.range().cover(name.range));
                            let name = name.emit(&mut self.storage);
                            (None, name.into(), range)
                        }
                    };
                    let item = self.storage.add(build::DictItem { key, value, range });
                    self.dict_item_scratch.push(item);
                }
                build::ExprDict {
                    range,
                    items: self.dict_item_scratch.take_range(items, &mut self.storage),
                }
                .emit(&mut self.storage)
                .into()
            }
            PatternKind::MatchClass(node) => {
                let func = node.cls().build_id();
                let arguments = node.arguments();
                let arguments_range = arguments.range();
                debug_assert_eq!(range.end(), arguments_range.end());
                let items = arguments.items().id();
                let converted = self.argument_scratch.snapshot();
                for index in 0..items.len() {
                    let argument = match self.storage.list_item(items, index) {
                        ast::PatternOrKeyword::Pattern(pattern) => {
                            ast::ArgumentId::Positional(self.pattern_to_expr(pattern.build_id()))
                        }
                        ast::PatternOrKeyword::Keyword(keyword) => {
                            let range = keyword.range();
                            let arg = keyword.attr().build_id();
                            let value = self.pattern_to_expr(keyword.pattern().build_id());
                            let keyword = build::Keyword {
                                range,
                                arg: Some(arg),
                                value,
                            }
                            .emit(&mut self.storage);
                            ast::ArgumentId::Keyword(keyword)
                        }
                    };
                    self.argument_scratch.push(argument);
                }
                let arguments = build::Arguments {
                    range: arguments_range,
                    items: self
                        .argument_scratch
                        .take_range(converted, &mut self.storage),
                }
                .emit(&mut self.storage);
                build::ExprCall {
                    range,
                    func,
                    arguments,
                }
                .emit(&mut self.storage)
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
                let value = name.emit(&mut self.storage);
                build::ExprStarred {
                    range,
                    value: value.into(),
                    ctx: ExprContext::Store,
                }
                .emit(&mut self.storage)
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
                name.emit(&mut self.storage).into()
            }
            PatternKind::MatchOr(node) => {
                // The parser creates an or-pattern only after seeing a second alternative.
                let patterns = node.patterns();
                let first = patterns.at(0);
                let start = first.start();
                let patterns = patterns.id();
                let mut left = self.pattern_to_expr(first.build_id());
                for index in 1..patterns.len() {
                    let pattern = self.storage.list_item(patterns, index);
                    let range = TextRange::new(start, pattern.end());
                    let right = self.pattern_to_expr(pattern.build_id());
                    left = build::ExprBinOp {
                        range,
                        left,
                        op: Operator::BitOr,
                        right,
                    }
                    .emit(&mut self.storage)
                    .into();
                }
                left
            }
        }
    }

    fn patterns_to_exprs(
        &mut self,
        patterns: ListId<AstNodeId<ast::kind::Pattern>>,
    ) -> ListId<AstNodeId<ast::kind::Expr>> {
        let snapshot = self.expr_scratch.snapshot();
        for index in 0..patterns.len() {
            let pattern = self.storage.list_item(patterns, index).build_id();
            let expr = self.pattern_to_expr(pattern);
            self.expr_scratch.push(expr);
        }
        self.expr_scratch.take_range(snapshot, &mut self.storage)
    }
}

fn capture_name(name: ast::Identifier<'_>) -> build::ExprName {
    build::ExprName {
        range: name.range(),
        id: name.name_id(),
        ctx: ExprContext::Store,
    }
}

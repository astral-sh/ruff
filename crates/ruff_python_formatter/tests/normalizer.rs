use regex::Regex;
use std::sync::LazyLock;

use ruff_allocator::{Allocator, Box as ArenaBox};
use ruff_python_ast::{
    self as ast, BytesLiteralFlags, Expr, FStringFlags, FStringPart, InterpolatedStringElement,
    InterpolatedStringLiteralElement, Stmt, StringFlags,
};
use ruff_python_ast::{AtomicNodeIndex, visitor::transformer::Transformer};
use ruff_python_ast::{StringLiteralFlags, visitor::transformer};
use ruff_text_size::{Ranged, TextRange};

/// A struct to normalize AST nodes for the purpose of comparing formatted representations for
/// semantic equivalence.
///
/// Vis-à-vis comparing ASTs, comparing these normalized representations does the following:
/// - Ignores non-abstraction information that we've encoded into the AST, e.g., the difference
///   between `class C: ...` and `class C(): ...`, which is part of our AST but not `CPython`'s.
/// - Normalize strings. The formatter can re-indent docstrings, so we need to compare string
///   contents ignoring whitespace. (Black does the same.)
/// - The formatter can also reformat code snippets when they're Python code, which can of
///   course change the string in arbitrary ways. Black itself does not reformat code snippets,
///   so we carve our own path here by stripping everything that looks like code snippets from
///   string literals.
/// - Ignores nested tuples in deletions. (Black does the same.)
pub(crate) struct Normalizer<'ast> {
    allocator: &'ast Allocator,
}

impl<'ast> Normalizer<'ast> {
    #[allow(dead_code)]
    pub(crate) const fn new(allocator: &'ast Allocator) -> Self {
        Self { allocator }
    }

    /// Transform an AST module into a normalized representation.
    #[allow(dead_code)]
    pub(crate) fn visit_module(&self, module: &mut ast::Mod<'ast>) {
        match module {
            ast::Mod::Module(module) => {
                self.visit_body(&mut module.body);
            }
            ast::Mod::Expression(expression) => {
                let mut body = (*expression.body).clone();
                self.visit_expr(&mut body);
                expression.body = ArenaBox::new_in(body, self.allocator);
            }
        }
    }
}

impl<'ast> Transformer<'ast> for Normalizer<'ast> {
    fn allocator(&self) -> &'ast Allocator {
        self.allocator
    }

    fn visit_stmt(&self, stmt: &mut Stmt<'ast>) {
        if let Stmt::Delete(delete) = stmt {
            // Treat `del a, b` and `del (a, b)` equivalently.
            if let [Expr::Tuple(tuple)] = delete.targets.as_slice() {
                delete.targets = tuple.elts;
            }
        }

        transformer::walk_stmt(self, stmt);
    }

    fn visit_expr(&self, expr: &mut Expr<'ast>) {
        // Ruff supports joining implicitly concatenated strings. The code below implements this
        // at an AST level by joining the string literals in the AST if they can be joined (it doesn't mean that
        // they'll be joined in the formatted output but they could).
        // Comparable expression handles some of this by comparing the concatenated string
        // but not joining here doesn't play nicely with other string normalizations done in the
        // Normalizer.
        match expr {
            Expr::StringLiteral(string) if string.value.is_implicit_concatenated() => {
                let can_join = string.value.iter().all(|literal| {
                    !literal.flags.is_triple_quoted() && !literal.flags.prefix().is_raw()
                });

                if can_join {
                    string.value = ast::StringLiteralValue::single(ast::StringLiteral {
                        value: ArenaBox::from_str_in(string.value.to_str(), self.allocator),
                        range: string.range,
                        flags: StringLiteralFlags::empty(),
                        node_index: AtomicNodeIndex::NONE,
                    });
                }
            }

            Expr::BytesLiteral(bytes) if bytes.value.is_implicit_concatenated() => {
                let can_join = bytes.value.iter().all(|literal| {
                    !literal.flags.is_triple_quoted() && !literal.flags.prefix().is_raw()
                });

                if can_join {
                    bytes.value = ast::BytesLiteralValue::single(ast::BytesLiteral {
                        value: ArenaBox::from_slice_copy_in(
                            &bytes.value.bytes().collect::<Vec<_>>(),
                            self.allocator,
                        ),
                        range: bytes.range,
                        flags: BytesLiteralFlags::empty(),
                        node_index: AtomicNodeIndex::NONE,
                    });
                }
            }

            Expr::FString(fstring) if fstring.value.is_implicit_concatenated() => {
                let can_join = fstring.value.iter().all(|part| match part {
                    FStringPart::Literal(literal) => {
                        !literal.flags.is_triple_quoted() && !literal.flags.prefix().is_raw()
                    }
                    FStringPart::FString(string) => {
                        !string.flags.is_triple_quoted() && !string.flags.prefix().is_raw()
                    }
                });

                if can_join {
                    struct Collector<'ast> {
                        allocator: &'ast Allocator,
                        elements: Vec<InterpolatedStringElement<'ast>>,
                    }

                    impl<'ast> Collector<'ast> {
                        // The logic for concatenating adjacent string literals
                        // occurs here, implicitly: when we encounter a sequence
                        // of string literals, the first gets pushed to the
                        // `elements` vector, while subsequent strings
                        // are concatenated onto this top string.
                        fn push_literal(&mut self, literal: &str, range: TextRange) {
                            if let Some(InterpolatedStringElement::Literal(existing_literal)) =
                                self.elements.last_mut()
                            {
                                let mut value = existing_literal.value.to_string();
                                value.push_str(literal);
                                existing_literal.value =
                                    ArenaBox::from_str_in(&value, self.allocator);
                                existing_literal.range =
                                    TextRange::new(existing_literal.start(), range.end());
                            } else {
                                self.elements.push(InterpolatedStringElement::Literal(
                                    InterpolatedStringLiteralElement {
                                        range,
                                        value: ArenaBox::from_str_in(literal, self.allocator),
                                        node_index: AtomicNodeIndex::NONE,
                                    },
                                ));
                            }
                        }

                        fn push_expression(&mut self, expression: ast::InterpolatedElement<'ast>) {
                            self.elements
                                .push(InterpolatedStringElement::Interpolation(expression));
                        }
                    }

                    let mut collector = Collector {
                        allocator: self.allocator,
                        elements: Vec::new(),
                    };

                    for part in &fstring.value {
                        match part {
                            ast::FStringPart::Literal(string_literal) => {
                                collector.push_literal(&string_literal.value, string_literal.range);
                            }
                            ast::FStringPart::FString(fstring) => {
                                for element in &fstring.elements {
                                    match element {
                                        ast::InterpolatedStringElement::Literal(literal) => {
                                            collector.push_literal(&literal.value, literal.range);
                                        }
                                        ast::InterpolatedStringElement::Interpolation(
                                            expression,
                                        ) => {
                                            collector.push_expression(expression.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    fstring.value = ast::FStringValue::single(ast::FString {
                        elements: ast::InterpolatedStringElements::from_vec_in(
                            collector.elements,
                            self.allocator,
                        ),
                        range: fstring.range,
                        flags: FStringFlags::empty(),
                        node_index: AtomicNodeIndex::NONE,
                    });
                }
            }

            _ => {}
        }
        transformer::walk_expr(self, expr);
    }

    fn visit_interpolated_string_element(
        &self,
        interpolated_string_element: &mut InterpolatedStringElement<'ast>,
    ) {
        let InterpolatedStringElement::Interpolation(interpolation) = interpolated_string_element
        else {
            return;
        };

        let Some(debug) = &mut interpolation.debug_text else {
            return;
        };

        // The formatter normalizes newlines in the text around a debug expression.
        let leading = debug.leading().replace("\r\n", "\n").replace('\r', "\n");
        let expression = debug.expression().to_string();
        let trailing = debug.trailing().replace("\r\n", "\n").replace('\r', "\n");
        *debug = ast::DebugText::new_in(&leading, &expression, &trailing, self.allocator);
    }

    fn visit_string_literal(&self, string_literal: &mut ast::StringLiteral<'ast>) {
        static STRIP_DOC_TESTS: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(
                r"(?mx)
                    (
                        # strip doctest PS1 prompt lines
                        ^\s*>>>\s.*(\n|$)
                        |
                        # strip doctest PS2 prompt lines
                        # Also handles the case of an empty ... line.
                        ^\s*\.\.\.((\n|$)|\s.*(\n|$))
                    )+
                ",
            )
            .unwrap()
        });
        static STRIP_RST_BLOCKS: LazyLock<Regex> = LazyLock::new(|| {
            // This is kind of unfortunate, but it's pretty tricky (likely
            // impossible) to detect a reStructuredText block with a simple
            // regex. So we just look for the start of a block and remove
            // everything after it. Talk about a hammer.
            Regex::new(r"::(?s:.*)").unwrap()
        });
        static STRIP_MARKDOWN_BLOCKS: LazyLock<Regex> = LazyLock::new(|| {
            // This covers more than valid Markdown blocks, but that's OK.
            Regex::new(r"(```|~~~)\p{any}*(```|~~~|$)").unwrap()
        });

        // Start by (1) stripping everything that looks like a code
        // snippet, since code snippets may be completely reformatted if
        // they are Python code.
        let value = STRIP_DOC_TESTS
            .replace_all(
                &string_literal.value,
                "<DOCTEST-CODE-SNIPPET: Removed by normalizer>\n",
            )
            .into_owned();
        string_literal.value = ArenaBox::from_str_in(&value, self.allocator);
        let value = STRIP_RST_BLOCKS
            .replace_all(
                &string_literal.value,
                "<RSTBLOCK-CODE-SNIPPET: Removed by normalizer>\n",
            )
            .into_owned();
        string_literal.value = ArenaBox::from_str_in(&value, self.allocator);
        let value = STRIP_MARKDOWN_BLOCKS
            .replace_all(
                &string_literal.value,
                "<MARKDOWN-CODE-SNIPPET: Removed by normalizer>\n",
            )
            .into_owned();
        string_literal.value = ArenaBox::from_str_in(&value, self.allocator);
        // Normalize a string by (2) stripping any leading and trailing space from each
        // line, and (3) removing any blank lines from the start and end of the string.
        let value = string_literal
            .value
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();
        string_literal.value = ArenaBox::from_str_in(&value, self.allocator);
    }
}

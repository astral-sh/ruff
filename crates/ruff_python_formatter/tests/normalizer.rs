use regex::Regex;
use std::sync::LazyLock;

use ruff_python_ast::{
    self as ast, BuildId, Builder, BytesLiteralFlags, Expr, ExprKind, FStringFlags, FStringPartId,
    FStringPartView, InterpolatedStringElement, InterpolatedStringElementKind, Rewriter, Stmt,
    StmtKind, StringFlags, StringLiteralFlags, Tree, build,
};
use ruff_text_size::{Ranged, TextRange};

/// Normalizes the input and output of the formatter for a semantic-equivalence check.
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
#[allow(
    dead_code,
    reason = "Cargo also builds this helper as a standalone test binary"
)]
pub(crate) fn normalize_module(module: ast::Mod<'_>) -> Tree<ast::kind::Mod> {
    let mut builder = Builder::default();
    let root = builder.rewrite_node(module, &mut Normalizer);
    builder.finish_tree(root)
}

struct Normalizer;

impl Rewriter for Normalizer {
    fn replace_stmt(
        &mut self,
        builder: &mut Builder,
        stmt: Stmt<'_>,
    ) -> Option<BuildId<ast::kind::Stmt>> {
        let StmtKind::Delete(delete) = stmt.classify() else {
            return None;
        };
        // Treat `del a, b` and `del (a, b)` equivalently.
        let [target] = delete.targets().as_array()?;
        let tuple = target.as_tuple_expr()?;
        let targets =
            builder.map_list(tuple.elts(), |builder, elt| builder.rewrite_node(elt, self));
        Some(
            builder
                .add(build::StmtDelete {
                    range: delete.range(),
                    targets,
                })
                .into(),
        )
    }

    fn replace_expr(
        &mut self,
        builder: &mut Builder,
        expr: Expr<'_>,
    ) -> Option<BuildId<ast::kind::Expr>> {
        // Normalize joined literals before comparing. Formatting can join adjacent
        // parts, and docstring normalization depends on seeing their combined text.
        match expr.classify() {
            ExprKind::StringLiteral(string)
                if string.value().is_implicit_concatenated()
                    && string.value().iter().all(|part| {
                        !part.flags().is_triple_quoted() && !part.flags().prefix().is_raw()
                    }) =>
            {
                let range = string.range();
                let part = normalized_string(
                    builder,
                    string.value().to_str(),
                    range,
                    StringLiteralFlags::empty(),
                );
                let value = builder.list([part]);
                Some(
                    builder
                        .add(build::ExprStringLiteral { range, value })
                        .into(),
                )
            }
            ExprKind::BytesLiteral(bytes)
                if bytes.value().is_implicit_concatenated()
                    && bytes.value().iter().all(|part| {
                        !part.flags().is_triple_quoted() && !part.flags().prefix().is_raw()
                    }) =>
            {
                let range = bytes.range();
                let joined: Vec<_> = bytes.value().bytes().collect();
                let part = builder.add(build::BytesLiteral {
                    range,
                    value: &joined,
                    flags: BytesLiteralFlags::empty(),
                });
                let value = builder.list([part]);
                Some(builder.add(build::ExprBytesLiteral { range, value }).into())
            }
            ExprKind::FString(string)
                if string.value().is_implicit_concatenated()
                    && string.value().iter().all(|part| match part {
                        FStringPartView::Literal(literal) => {
                            !literal.flags().is_triple_quoted()
                                && !literal.flags().prefix().is_raw()
                        }
                        FStringPartView::FString(string) => {
                            !string.flags().is_triple_quoted() && !string.flags().prefix().is_raw()
                        }
                    }) =>
            {
                Some(self.join_fstring(builder, string))
            }
            _ => None,
        }
    }

    fn replace_interpolated_string_element(
        &mut self,
        builder: &mut Builder,
        element: InterpolatedStringElement<'_>,
    ) -> Option<BuildId<ast::kind::InterpolatedStringElement>> {
        let InterpolatedStringElementKind::Interpolation(interpolation) = element.classify() else {
            return None;
        };
        let Some(debug) = interpolation.debug_text() else {
            return Some(builder.copy_node(element));
        };
        // The formatter normalizes newlines in the text around a debug expression.
        let leading = debug.leading().replace("\r\n", "\n").replace('\r', "\n");
        let trailing = debug.trailing().replace("\r\n", "\n").replace('\r', "\n");
        let expression = builder.copy_node(interpolation.expression());
        let format_spec = interpolation
            .format_spec()
            .map(|spec| builder.copy_node(spec));
        Some(
            builder
                .add(build::InterpolatedElement {
                    range: interpolation.range(),
                    expression,
                    conversion: interpolation.conversion(),
                    format_spec,
                    debug_text: Some(ast::DebugText::new(&leading, debug.expression(), &trailing)),
                })
                .into(),
        )
    }

    fn replace_string_literal(
        &mut self,
        builder: &mut Builder,
        literal: ast::StringLiteral<'_>,
    ) -> Option<BuildId<ast::kind::StringLiteral>> {
        Some(normalized_string(
            builder,
            literal.value(),
            literal.range(),
            literal.flags(),
        ))
    }
}

impl Normalizer {
    fn join_fstring(
        &mut self,
        builder: &mut Builder,
        string: ast::ExprFString<'_>,
    ) -> BuildId<ast::kind::Expr> {
        enum Part<'a> {
            Text(String, TextRange),
            Interpolation(InterpolatedStringElement<'a>),
        }
        fn push_text(parts: &mut Vec<Part<'_>>, text: &str, range: TextRange) {
            if let Some(Part::Text(previous, previous_range)) = parts.last_mut() {
                previous.push_str(text);
                *previous_range = TextRange::new(previous_range.start(), range.end());
            } else {
                parts.push(Part::Text(text.to_owned(), range));
            }
        }

        let mut parts = Vec::new();
        for part in string.value() {
            match part {
                FStringPartView::Literal(literal) => {
                    push_text(&mut parts, literal.value(), literal.range());
                }
                FStringPartView::FString(fstring) => {
                    for element in fstring.elements() {
                        match element.classify() {
                            InterpolatedStringElementKind::Literal(literal) => {
                                push_text(&mut parts, literal.value(), literal.range());
                            }
                            InterpolatedStringElementKind::Interpolation(_) => {
                                parts.push(Part::Interpolation(element));
                            }
                        }
                    }
                }
            }
        }
        let elements = builder.map_list(parts, |builder, part| match part {
            Part::Text(value, range) => builder
                .add(build::InterpolatedStringLiteralElement {
                    range,
                    value: &value,
                })
                .into(),
            Part::Interpolation(element) => builder.rewrite_node(element, self),
        });
        let range = string.range();
        let part = builder.add(build::FString {
            range,
            elements,
            flags: FStringFlags::empty(),
        });
        let value = builder.list([FStringPartId::FString(part)]);
        builder.add(build::ExprFString { range, value }).into()
    }
}

fn normalized_string(
    builder: &mut Builder,
    text: &str,
    range: TextRange,
    flags: StringLiteralFlags,
) -> BuildId<ast::kind::StringLiteral> {
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
    let text = STRIP_DOC_TESTS.replace_all(text, "<DOCTEST-CODE-SNIPPET: Removed by normalizer>\n");
    let text =
        STRIP_RST_BLOCKS.replace_all(&text, "<RSTBLOCK-CODE-SNIPPET: Removed by normalizer>\n");
    let text = STRIP_MARKDOWN_BLOCKS
        .replace_all(&text, "<MARKDOWN-CODE-SNIPPET: Removed by normalizer>\n");
    // Strip surrounding whitespace from each line and blank lines from the ends.
    let lines = text.lines().map(str::trim).collect::<Vec<_>>().join("\n");
    builder.add(build::StringLiteral {
        range,
        flags,
        value: lines.trim(),
    })
}

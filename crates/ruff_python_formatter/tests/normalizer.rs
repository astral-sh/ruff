use {
    itertools::Either::{Left, Right},
    once_cell::sync::Lazy,
    regex::Regex,
};

use ruff_python_ast::visitor::transformer;
use ruff_python_ast::visitor::transformer::Transformer;
use ruff_python_ast::{self as ast, Expr, Stmt};

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
pub(crate) struct Normalizer;

impl Normalizer {
    /// Transform an AST module into a normalized representation.
    #[allow(dead_code)]
    pub(crate) fn visit_module(&self, module: &mut ast::Mod) {
        match module {
            ast::Mod::Module(module) => {
                self.visit_body(&mut module.body);
            }
            ast::Mod::Expression(expression) => {
                self.visit_expr(&mut expression.body);
            }
        }
    }
}

impl Transformer for Normalizer {
    fn visit_stmt(&self, stmt: &mut Stmt) {
        if let Stmt::Delete(delete) = stmt {
            // Treat `del a, b` and `del (a, b)` equivalently.
            delete.targets = delete
                .targets
                .clone()
                .into_iter()
                .flat_map(|target| {
                    if let Expr::Tuple(tuple) = target {
                        Left(tuple.elts.into_iter())
                    } else {
                        Right(std::iter::once(target))
                    }
                })
                .collect();
        }

        transformer::walk_stmt(self, stmt);
    }

    fn visit_string_literal(&self, string_literal: &mut ast::StringLiteral) {
        static STRIP_DOC_TESTS: Lazy<Regex> = Lazy::new(|| {
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
        static STRIP_RST_BLOCKS: Lazy<Regex> = Lazy::new(|| {
            // This is kind of unfortunate, but it's pretty tricky (likely
            // impossible) to detect a reStructuredText block with a simple
            // regex. So we just look for the start of a block and remove
            // everything after it. Talk about a hammer.
            Regex::new(r"::(?s:.*)").unwrap()
        });
        static STRIP_MARKDOWN_BLOCKS: Lazy<Regex> = Lazy::new(|| {
            // This covers more than valid Markdown blocks, but that's OK.
            Regex::new(r"(```|~~~)\p{any}*(```|~~~|$)").unwrap()
        });

        // Start by (1) stripping everything that looks like a code
        // snippet, since code snippets may be completely reformatted if
        // they are Python code.
        string_literal.value = STRIP_DOC_TESTS
            .replace_all(
                &string_literal.value,
                "<DOCTEST-CODE-SNIPPET: Removed by normalizer>\n",
            )
            .into_owned()
            .into_boxed_str();
        string_literal.value = STRIP_RST_BLOCKS
            .replace_all(
                &string_literal.value,
                "<RSTBLOCK-CODE-SNIPPET: Removed by normalizer>\n",
            )
            .into_owned()
            .into_boxed_str();
        string_literal.value = STRIP_MARKDOWN_BLOCKS
            .replace_all(
                &string_literal.value,
                "<MARKDOWN-CODE-SNIPPET: Removed by normalizer>\n",
            )
            .into_owned()
            .into_boxed_str();
        // Normalize a string by (2) stripping any leading and trailing space from each
        // line, and (3) removing any blank lines from the start and end of the string.
        string_literal.value = string_literal
            .value
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
            .into_boxed_str();
    }
}

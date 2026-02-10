use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, ExprStringLiteral};
use ruff_python_ast::{Expr, Operator};
use ruff_python_parser::semantic_errors::SemanticSyntaxContext;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability};

/// ## What it does
/// Checks for the presence of string literals in `X | Y`-style union types.
///
/// ## Why is this bad?
/// [PEP 604] introduced a new syntax for union type annotations based on the
/// `|` operator.
///
/// While Python's type annotations can typically be wrapped in strings to
/// avoid runtime evaluation, the use of a string member within an `X | Y`-style
/// union type will cause a runtime error.
///
/// Instead, remove the quotes, wrap the _entire_ union in quotes, or use
/// `from __future__ import annotations` to disable runtime evaluation of
/// annotations entirely.
///
/// ## Example
/// ```python
/// var: "Foo" | None
///
///
/// class Foo: ...
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// var: Foo | None
///
///
/// class Foo: ...
/// ```
///
/// Or, extend the quotes to include the entire union:
/// ```python
/// var: "Foo | None"
///
///
/// class Foo: ...
/// ```
///
/// ## References
/// - [PEP 563 - Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [PEP 604 – Allow writing union types as `X | Y`](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0")]
pub(crate) struct RuntimeStringUnion {
    strategy: Strategy,
}

impl Violation for RuntimeStringUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid string member in `X | Y`-style union type".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        match self.strategy {
            Strategy::FutureImport { has_import } => {
                if has_import {
                    Some("Remove quotes from string literals".to_string())
                } else {
                    Some("Add `from __future__ import annotations` and remove quotes from string literals".to_string())
                }
            }
            Strategy::QuoteUnion => Some("Quote the entire union expression".to_string()),
        }
    }
}

/// TC010
pub(crate) fn runtime_string_union(checker: &Checker, expr: &Expr) {
    if !checker.semantic().in_type_definition() {
        return;
    }

    if !checker.semantic().execution_context().is_runtime() {
        return;
    }

    // Search for strings within the binary operator.
    let mut strings = Vec::new();
    let mut has_bytes = false;
    traverse_op(expr, &mut strings, &mut has_bytes);

    let strategy = if checker.settings().flake8_type_checking.quote_annotations {
        Strategy::QuoteUnion
    } else {
        Strategy::FutureImport {
            has_import: checker.future_annotations_or_stub(),
        }
    };

    let fix = if has_bytes {
        None
    } else if checker.settings().flake8_type_checking.quote_annotations {
        quote_union(checker, &strings, expr)
    } else {
        unquote_and_add_future_import(checker, &strings)
    };

    if !strings.is_empty() {
        let mut diagnostic =
            checker.report_diagnostic(RuntimeStringUnion { strategy }, expr.range());

        if !has_bytes && let Some(fix) = fix {
            diagnostic.set_fix(fix);
        }
    }
}

/// Collect all string members in possibly-nested binary `|` expressions.
fn traverse_op<'a>(expr: &'a Expr, strings: &mut Vec<&'a Expr>, has_bytes: &mut bool) {
    match expr {
        Expr::StringLiteral(_) => {
            strings.push(expr);
        }
        Expr::BytesLiteral(_) => {
            *has_bytes = true;
            strings.push(expr);
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            right,
            op: Operator::BitOr,
            ..
        }) => {
            traverse_op(left, strings, has_bytes);
            traverse_op(right, strings, has_bytes);
        }
        _ => {}
    }
}

fn unquote_and_add_future_import(checker: &Checker, strings: &[&Expr]) -> Option<Fix> {
    let mut edits = vec![];
    for string_expr in strings {
        if let Expr::StringLiteral(ExprStringLiteral { value, .. }) = string_expr {
            edits.push(Edit::range_replacement(
                value.to_str().to_string(),
                string_expr.range(),
            ));
        }
    }
    if !checker.future_annotations_or_stub() {
        let future_import = checker.importer().add_future_import();
        edits.push(future_import);
    }
    if edits.is_empty() {
        return None;
    }
    let mut edits_iter = edits.into_iter();
    let first = edits_iter.next().expect("Empty edits");
    Some(Fix::unsafe_edits(first, edits_iter))
}

fn quote_union(checker: &Checker, strings: &[&Expr], union_expr: &Expr) -> Option<Fix> {
    let mut union_text = checker.locator().slice(union_expr.range()).to_string();
    let mut unquoted: Vec<_> = strings
        .iter()
        .filter_map(|string_expr| {
            if let Expr::StringLiteral(ExprStringLiteral { value, .. }) = string_expr {
                let range = string_expr.range();
                let start = (range.start() - union_expr.start()).to_usize();
                let end = (range.end() - union_expr.start()).to_usize();
                Some((start, end, value.to_str()))
            } else {
                None
            }
        })
        .collect();
    unquoted.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end, value) in unquoted {
        union_text.replace_range(start..end, value);
    }
    let quoted_union = format!("\"{union_text}\"");
    Some(Fix::safe_edit(Edit::range_replacement(
        quoted_union,
        union_expr.range(),
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    FutureImport { has_import: bool },
    QuoteUnion,
}

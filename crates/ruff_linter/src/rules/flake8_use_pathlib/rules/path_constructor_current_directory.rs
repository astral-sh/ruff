use std::ops::Range;

use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{AstNode, Expr, ExprBinOp, ExprCall, ExprStringLiteral, Operator};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for `pathlib.Path` objects that are initialized with the current
/// directory.
///
/// ## Why is this bad?
/// The `Path()` constructor defaults to the current directory, so passing it
/// in explicitly (as `"."`) is unnecessary.
///
/// ## Example
/// ```python
/// from pathlib import Path
///
/// _ = Path(".")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// _ = Path()
/// ```
///
/// ## References
/// - [Python documentation: `Path`](https://docs.python.org/3/library/pathlib.html#pathlib.Path)
#[derive(ViolationMetadata)]
pub(crate) struct PathConstructorCurrentDirectory;

impl AlwaysFixableViolation for PathConstructorCurrentDirectory {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not pass the current directory explicitly to `Path`".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove the current directory argument".to_string()
    }
}

/// PTH201
pub(crate) fn path_constructor_current_directory(checker: &mut Checker, call: &ExprCall) {
    let (semantic, locator, source, comment_ranges) = (
        checker.semantic(),
        checker.locator(),
        checker.source(),
        checker.comment_ranges(),
    );

    let applicability = |range| {
        if comment_ranges.intersects(range) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        }
    };

    let (func, arguments) = (&call.func, &call.arguments);

    if !semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["pathlib", "Path" | "PurePath"])
        })
    {
        return;
    }

    if !arguments.keywords.is_empty() {
        return;
    }

    let [Expr::StringLiteral(ExprStringLiteral {
        value,
        range: argument_range,
    })] = &*arguments.args
    else {
        return;
    };

    if !matches!(value.to_str(), "" | ".") {
        return;
    }

    let fix = match parent_and_next_path_fragment_range(checker) {
        Some((parent_range, next_fragment_range)) => {
            let next_fragment_expr = locator.slice(next_fragment_range);
            let call_expr = locator.slice(call.range);

            let relative_argument_range: Range<usize> = {
                let range = argument_range - call.start();
                range.start().into()..range.end().into()
            };

            let mut new_call_expr = call_expr.to_string();
            new_call_expr.replace_range(relative_argument_range, next_fragment_expr);

            let edit = Edit::range_replacement(new_call_expr, parent_range);

            Fix::applicable_edit(edit, applicability(parent_range))
        }
        None => {
            let Ok(edit) =
                remove_argument(argument_range, arguments, Parentheses::Preserve, source)
            else {
                unreachable!("Cannot remove argument");
            };

            Fix::applicable_edit(edit, applicability(call.range))
        }
    };

    let diagnostic = Diagnostic::new(PathConstructorCurrentDirectory, *argument_range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn parent_and_next_path_fragment_range(checker: &Checker) -> Option<(TextRange, TextRange)> {
    let (semantic, comment_ranges, source) = (
        checker.semantic(),
        checker.comment_ranges(),
        checker.source(),
    );

    let parent = semantic.current_expression_parent()?;

    let Expr::BinOp(parent @ ExprBinOp { op, right, .. }) = parent else {
        return None;
    };

    let original_range = right.range();

    match op {
        Operator::Div => {
            let parenthesized_range = parenthesized_range(
                right.into(),
                parent.as_any_node_ref(),
                comment_ranges,
                source,
            );

            Some((parent.range, parenthesized_range.unwrap_or(original_range)))
        }
        _ => None,
    }
}

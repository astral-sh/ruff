use std::ops::Range;

use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{Expr, ExprBinOp, ExprCall, Operator};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::CommentRanges;
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
pub(crate) fn path_constructor_current_directory(checker: &Checker, call: &ExprCall) {
    let applicability = |range| {
        if checker.comment_ranges().intersects(range) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        }
    };

    let (func, arguments) = (&call.func, &call.arguments);

    if !checker
        .semantic()
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

    let [Expr::StringLiteral(arg)] = &*arguments.args else {
        return;
    };

    if !matches!(arg.value.to_str(), "" | ".") {
        return;
    }

    let mut diagnostic = Diagnostic::new(PathConstructorCurrentDirectory, arg.range());

    match parent_and_next_path_fragment_range(
        checker.semantic(),
        checker.comment_ranges(),
        checker.source(),
    ) {
        Some((parent_range, next_fragment_range)) => {
            let next_fragment_expr = checker.locator().slice(next_fragment_range);
            let call_expr = checker.locator().slice(call.range());

            let relative_argument_range: Range<usize> = {
                let range = arg.range() - call.start();
                range.start().into()..range.end().into()
            };

            let mut new_call_expr = call_expr.to_string();
            new_call_expr.replace_range(relative_argument_range, next_fragment_expr);

            let edit = Edit::range_replacement(new_call_expr, parent_range);

            diagnostic.set_fix(Fix::applicable_edit(edit, applicability(parent_range)));
        }
        None => diagnostic.try_set_fix(|| {
            let edit = remove_argument(arg, arguments, Parentheses::Preserve, checker.source())?;
            Ok(Fix::applicable_edit(edit, applicability(call.range())))
        }),
    }

    checker.report_diagnostic(diagnostic);
}

fn parent_and_next_path_fragment_range(
    semantic: &SemanticModel,
    comment_ranges: &CommentRanges,
    source: &str,
) -> Option<(TextRange, TextRange)> {
    let parent = semantic.current_expression_parent()?;

    let Expr::BinOp(parent @ ExprBinOp { op, right, .. }) = parent else {
        return None;
    };

    let range = right.range();

    if !matches!(op, Operator::Div) {
        return None;
    }

    Some((
        parent.range(),
        parenthesized_range(right.into(), parent.into(), comment_ranges, source).unwrap_or(range),
    ))
}

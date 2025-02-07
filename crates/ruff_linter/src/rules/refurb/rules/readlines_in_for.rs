use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Comprehension, Expr, StmtFor};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::analyze::typing::is_io_base_expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `readlines()` when iterating over a file line-by-line.
///
/// ## Why is this bad?
/// Rather than iterating over all lines in a file by calling `readlines()`,
/// it's more convenient and performant to iterate over the file object
/// directly.
///
/// ## Example
/// ```python
/// with open("file.txt") as fp:
///     for line in fp.readlines():
///         ...
/// ```
///
/// Use instead:
/// ```python
/// with open("file.txt") as fp:
///     for line in fp:
///         ...
/// ```
///
/// ## References
/// - [Python documentation: `io.IOBase.readlines`](https://docs.python.org/3/library/io.html#io.IOBase.readlines)
#[derive(ViolationMetadata)]
pub(crate) struct ReadlinesInFor;

impl AlwaysFixableViolation for ReadlinesInFor {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Instead of calling `readlines()`, iterate over file object directly".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `readlines()`".into()
    }
}

/// FURB129
pub(crate) fn readlines_in_for(checker: &Checker, for_stmt: &StmtFor) {
    readlines_in_iter(checker, for_stmt.iter.as_ref());
}

/// FURB129
pub(crate) fn readlines_in_comprehension(checker: &Checker, comprehension: &Comprehension) {
    readlines_in_iter(checker, &comprehension.iter);
}

fn readlines_in_iter(checker: &Checker, iter_expr: &Expr) {
    let Expr::Call(expr_call) = iter_expr else {
        return;
    };

    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return;
    };

    if expr_attr.attr.as_str() != "readlines" || !expr_call.arguments.is_empty() {
        return;
    }

    // Determine whether `fp` in `fp.readlines()` was bound to a file object.
    if let Expr::Name(name) = expr_attr.value.as_ref() {
        if !checker
            .semantic()
            .resolve_name(name)
            .map(|id| checker.semantic().binding(id))
            .is_some_and(|binding| typing::is_io_base(binding, checker.semantic()))
        {
            return;
        }
    } else {
        if !is_io_base_expr(expr_attr.value.as_ref(), checker.semantic()) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(ReadlinesInFor, expr_call.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(
        expr_call.range().add_start(expr_attr.value.range().len()),
    )));
    checker.report_diagnostic(diagnostic);
}

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Comprehension, Expr, StmtFor};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `readlines()` when iterating a file object line-by-line.
///
/// ## Why is this bad?
/// Instead of iterating through `list[str]` which is returned from `readlines()`, use the iteration
/// through a file object which is a more convenient and performant way.
///
/// ## Example
/// ```python
/// with open("file.txt") as f:
///     for line in f.readlines():
///         ...
/// ```
///
/// Use instead:
/// ```python
/// with open("file.txt") as f:
///     for line in f:
///         ...
/// ```
///
/// ## References
/// - [Python documentation: `io.IOBase.readlines`](https://docs.python.org/3/library/io.html#io.IOBase.readlines)
/// - [Python documentation: methods of file objects](https://docs.python.org/3/tutorial/inputoutput.html#methods-of-file-objects)
///
#[violation]
pub(crate) struct ReadlinesInFor;

impl AlwaysFixableViolation for ReadlinesInFor {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `readlines()` in loop")
    }

    fn fix_title(&self) -> String {
        "Remove `readlines()`".into()
    }
}

/// FURB129
pub(crate) fn readlines_in_for(checker: &mut Checker, for_stmt: &StmtFor) {
    readlines_in_iter(checker, for_stmt.iter.as_ref());
}

/// FURB129
pub(crate) fn readlines_in_comprehension(checker: &mut Checker, comprehension: &Comprehension) {
    readlines_in_iter(checker, &comprehension.iter);
}

fn readlines_in_iter(checker: &mut Checker, iter_expr: &Expr) {
    let Expr::Call(expr_call) = iter_expr else {
        return;
    };

    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return;
    };

    if expr_attr.attr.as_str() == "readlines" && expr_call.arguments.is_empty() {
        let mut diagnostic = Diagnostic::new(ReadlinesInFor, expr_call.range());
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(
            expr_call.range().add_start(expr_attr.value.range().len()),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

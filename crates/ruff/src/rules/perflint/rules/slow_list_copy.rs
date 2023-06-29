use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for cases where a new list is made as a copy of an existing one by appending elements
/// in a for-loop
///
/// ## Why is this bad?
/// It is more performant to use `list()` or `list.copy()` to copy a list
///
/// ## Example
/// ```python
/// original = range(10_000)
/// filtered = []
/// for i in original:
///     filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = range(10_000)
/// filtered = list(original)
/// ```
#[violation]
pub struct SlowListCopy;

impl Violation for SlowListCopy {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `list()` or `list.copy()` to create a copy of a list")
    }
}

/// PERF402
pub(crate) fn slow_list_copy(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }

    let stmt = &body[0];

    let Stmt::Expr(ast::StmtExpr { value, .. })= stmt else {
        return
    };

    let Expr::Call(ast::ExprCall { func, range, .. }) = value.as_ref() else {
        return
    };

    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return
    };

    let attr = attr.as_str();

    if attr == "append" || attr == "insert" {
        checker
            .diagnostics
            .push(Diagnostic::new(SlowListCopy, *range));
    }
}

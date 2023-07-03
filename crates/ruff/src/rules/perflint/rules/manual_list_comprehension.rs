use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a list comprehension.
///
/// ## Why is this bad?
/// When creating a filtered list from an existing list using a for-loop,
/// prefer a list comprehension. List comprehensions are more readable and
/// more performant.
///
/// Using the below as an example, the list comprehension is ~10% faster on
/// Python 3.11, and ~25% faster on Python 3.10.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// original = list(range(10000))
/// filtered = []
/// for i in original:
///     if i % 2:
///         filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = list(range(10000))
/// filtered = [x for x in original if x % 2]
/// ```
#[violation]
pub struct ManualListComprehension;

impl Violation for ManualListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a list comprehension to create a new filtered list")
    }
}

/// PERF401
pub(crate) fn manual_list_comprehension(checker: &mut Checker, body: &[Stmt]) {
    let [stmt] = body else {
        return;
    };

    let Stmt::If(ast::StmtIf { body, .. }) = stmt else {
        return;
    };

    for stmt in body {
        let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
            continue;
        };

        let Expr::Call(ast::ExprCall { func, range, .. }) = value.as_ref() else {
            continue;
        };

        let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
            continue;
        };

        if attr.as_str() == "append" {
            checker
                .diagnostics
                .push(Diagnostic::new(ManualListComprehension, *range));
        }
    }
}

use rustpython_parser::ast;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::Stmt;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for cases where new lists are made by appending elements (with a filter) in a for-loop
///
/// ## Why is this bad?
/// List comprehensions are 25% more efficient at creating new lists, with or without an
/// if-statement. So these should be used when creating new lists
///
/// ## Example
/// ```python
/// original = range(10_000)
/// filtered = []
/// for i in original:
///     if i % 2:
///         filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = range(10_000)
/// filtered = [x for x in original if x % 2]
/// ```
#[violation]
pub struct UseListComprehension;

impl Violation for UseListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a list comprehension to create a new filtered list")
    }
}

/// PERF401
pub(crate) fn use_list_comprehension(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return
    }

    let stmt = &body[0];

    let Stmt::If(ast::StmtIf { body: if_body, .. }) = stmt else {
        return
    };

    for if_stmt in if_body {
        let Stmt::Expr(ast::StmtExpr { value, .. })= if_stmt else {
            continue
        };

        let Expr::Call(ast::ExprCall { func, range, .. }) = value.as_ref() else {
            continue
        };

        let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
            continue
        };

        let attr = attr.as_str();

        if attr == "append" {
            checker
                .diagnostics
                .push(Diagnostic::new(UseListComprehension, *range));
        }
    }
}

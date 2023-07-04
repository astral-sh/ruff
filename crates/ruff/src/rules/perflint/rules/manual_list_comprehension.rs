use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a list comprehension.
///
/// ## Why is this bad?
/// When creating a transformed list from an existing list using a for-loop,
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
///
/// If you're appending to an existing list, use the `extend` method instead:
/// ```python
/// original = list(range(10000))
/// filtered.extend(x for x in original if x % 2)
/// ```
#[violation]
pub struct ManualListComprehension;

impl Violation for ManualListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a list comprehension to create a transformed list")
    }
}

/// PERF401
pub(crate) fn manual_list_comprehension(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    let (stmt, conditional) = match body {
        // ```python
        // for x in y:
        //     if z:
        //         filtered.append(x)
        // ```
        [Stmt::If(ast::StmtIf { body, orelse, .. })] => {
            if !orelse.is_empty() {
                return;
            }
            let [stmt] = body.as_slice() else {
                return;
            };
            (stmt, true)
        }
        // ```python
        // for x in y:
        //     filtered.append(f(x))
        // ```
        [stmt] => (stmt, false),
        _ => return,
    };

    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return;
    };

    let Expr::Call(ast::ExprCall {
        func,
        range,
        args,
        keywords,
    }) = value.as_ref()
    else {
        return;
    };

    if !keywords.is_empty() {
        return;
    }

    let [arg] = args.as_slice() else {
        return;
    };

    // Ignore direct list copies (e.g., `for x in y: filtered.append(x)`).
    if !conditional {
        if arg.as_name_expr().map_or(false, |arg| {
            target
                .as_name_expr()
                .map_or(false, |target| arg.id == target.id)
        }) {
            return;
        }
    }

    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return;
    };

    if attr.as_str() == "append" {
        checker
            .diagnostics
            .push(Diagnostic::new(ManualListComprehension, *range));
    }
}

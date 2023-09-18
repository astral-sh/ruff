use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_semantic::analyze::typing::is_list;
use ruff_python_semantic::Binding;

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
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    let (stmt, if_test) = match body {
        // ```python
        // for x in y:
        //     if z:
        //         filtered.append(x)
        // ```
        [Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            test,
            ..
        })] => {
            if !elif_else_clauses.is_empty() {
                return;
            }
            let [stmt] = body.as_slice() else {
                return;
            };
            (stmt, Some(test))
        }
        // ```python
        // for x in y:
        //     filtered.append(f(x))
        // ```
        [stmt] => (stmt, None),
        _ => return,
    };

    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return;
    };

    let Expr::Call(ast::ExprCall {
        func,
        arguments:
            Arguments {
                args,
                keywords,
                range: _,
            },
        range,
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

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    if attr.as_str() != "append" {
        return;
    }

    // Ignore direct list copies (e.g., `for x in y: filtered.append(x)`).
    if if_test.is_none() {
        if arg.as_name_expr().is_some_and(|arg| arg.id == *id) {
            return;
        }
    }

    // Avoid, e.g., `for x in y: filtered[x].append(x * x)`.
    if any_over_expr(value, &|expr| {
        expr.as_name_expr().is_some_and(|expr| expr.id == *id)
    }) {
        return;
    }

    // Avoid, e.g., `for x in y: filtered.append(filtered[-1] * 2)`.
    if any_over_expr(arg, &|expr| {
        ComparableExpr::from(expr) == ComparableExpr::from(value)
    }) {
        return;
    }

    // Avoid non-list values.
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };
    let bindings: Vec<&Binding> = checker
        .semantic()
        .current_scope()
        .get_all(id)
        .map(|binding_id| checker.semantic().binding(binding_id))
        .collect();

    let [binding] = bindings.as_slice() else {
        return;
    };

    if !is_list(binding, checker.semantic()) {
        return;
    }

    // Avoid if the value is used in the conditional test, e.g.,
    //
    // ```python
    // for x in y:
    //    if x in filtered:
    //        filtered.append(x)
    // ```
    //
    // Converting this to a list comprehension would raise a `NameError` as
    // `filtered` is not defined yet:
    //
    // ```python
    // filtered = [x for x in y if x in filtered]
    // ```
    if let Some(value_name) = value.as_name_expr() {
        if if_test.is_some_and(|test| {
            any_over_expr(test, &|expr| {
                expr.as_name_expr()
                    .is_some_and(|expr| expr.id == value_name.id)
            })
        }) {
            return;
        }
    }

    checker
        .diagnostics
        .push(Diagnostic::new(ManualListComprehension, *range));
}

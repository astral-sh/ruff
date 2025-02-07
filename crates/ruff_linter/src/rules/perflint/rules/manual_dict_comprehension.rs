use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::typing::is_dict;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a dictionary comprehension.
///
/// ## Why is this bad?
/// When creating or extending a dictionary in a for-loop, prefer a dictionary
/// comprehension. Comprehensions are more readable and more performant.
///
/// For example, when comparing `{x: x for x in list(range(1000))}` to the `for`
/// loop version, the comprehension is ~10% faster on Python 3.11.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result = {}
/// for x, y in pairs:
///     if y % 2:
///         result[x] = y
/// ```
///
/// Use instead:
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result = {x: y for x, y in pairs if y % 2}
/// ```
///
/// If you're appending to an existing dictionary, use the `update` method instead:
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result.update({x: y for x, y in pairs if y % 2})
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ManualDictComprehension;

impl Violation for ManualDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use a dictionary comprehension instead of a for-loop".to_string()
    }
}

/// PERF403
pub(crate) fn manual_dict_comprehension(checker: &Checker, target: &Expr, body: &[Stmt]) {
    let (stmt, if_test) = match body {
        // ```python
        // for idx, name in enumerate(names):
        //     if idx % 2 == 0:
        //         result[name] = idx
        // ```
        [Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            test,
            ..
        })] => {
            // TODO(charlie): If there's an `else` clause, verify that the `else` has the
            // same structure.
            if !elif_else_clauses.is_empty() {
                return;
            }
            let [stmt] = body.as_slice() else {
                return;
            };
            (stmt, Some(test))
        }
        // ```python
        // for idx, name in enumerate(names):
        //     result[name] = idx
        // ```
        [stmt] => (stmt, None),
        _ => return,
    };

    let Stmt::Assign(ast::StmtAssign {
        targets,
        value,
        range,
    }) = stmt
    else {
        return;
    };

    let [Expr::Subscript(ast::ExprSubscript {
        value: subscript_value,
        slice,
        ..
    })] = targets.as_slice()
    else {
        return;
    };

    match target {
        Expr::Tuple(tuple) => {
            if !tuple
                .iter()
                .any(|element| ComparableExpr::from(slice) == ComparableExpr::from(element))
            {
                return;
            }
            if !tuple
                .iter()
                .any(|element| ComparableExpr::from(value) == ComparableExpr::from(element))
            {
                return;
            }
        }
        Expr::Name(_) => {
            if ComparableExpr::from(slice) != ComparableExpr::from(target) {
                return;
            }
            if ComparableExpr::from(value) != ComparableExpr::from(target) {
                return;
            }
        }
        _ => return,
    }

    // Exclude non-dictionary value.
    let Expr::Name(name) = &**subscript_value else {
        return;
    };
    let Some(binding) = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_dict(binding, checker.semantic()) {
        return;
    }

    // Avoid if the value is used in the conditional test, e.g.,
    //
    // ```python
    // for x in y:
    //    if x in filtered:
    //        filtered[x] = y
    // ```
    //
    // Converting this to a dictionary comprehension would raise a `NameError` as
    // `filtered` is not defined yet:
    //
    // ```python
    // filtered = {x: y for x in y if x in filtered}
    // ```
    if if_test.is_some_and(|test| {
        any_over_expr(test, &|expr| {
            ComparableExpr::from(expr) == ComparableExpr::from(name)
        })
    }) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(ManualDictComprehension, *range));
}

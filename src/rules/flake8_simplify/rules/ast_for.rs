use rustpython_ast::{
    Comprehension, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind, Unaryop,
};

use crate::ast::helpers::{create_expr, create_stmt};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::source_code::{Generator, Stylist};
use crate::violations;

struct Loop<'a> {
    return_value: bool,
    next_return_value: bool,
    test: &'a Expr,
    target: &'a Expr,
    iter: &'a Expr,
}

/// Extract the returned boolean values a `StmtKind::For` with an `else` body.
fn return_values_for_else(stmt: &Stmt) -> Option<Loop> {
    let StmtKind::For {
        body,
        target,
        iter,
        orelse,
        ..
    } = &stmt.node else {
        return None;
    };

    // The loop itself should contain a single `if` statement, with an `else`
    // containing a single `return True` or `return False`.
    if body.len() != 1 {
        return None;
    }
    if orelse.len() != 1 {
        return None;
    }
    let StmtKind::If {
        body: nested_body,
        test: nested_test,
        orelse: nested_orelse,
    } = &body[0].node else {
        return None;
    };
    if nested_body.len() != 1 {
        return None;
    }
    if !nested_orelse.is_empty() {
        return None;
    }
    let StmtKind::Return { value } = &nested_body[0].node else {
        return None;
    };
    let Some(value) = value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(value), .. } = &value.node else {
        return None;
    };

    // The `else` block has to contain a single `return True` or `return False`.
    let StmtKind::Return { value: next_value } = &orelse[0].node else {
        return None;
    };
    let Some(next_value) = next_value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(next_value), .. } = &next_value.node else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        next_return_value: *next_value,
        test: nested_test,
        target,
        iter,
    })
}

/// Extract the returned boolean values from subsequent `StmtKind::For` and
/// `StmtKind::Return` statements, or `None`.
fn return_values_for_siblings<'a>(stmt: &'a Stmt, sibling: &'a Stmt) -> Option<Loop<'a>> {
    let StmtKind::For {
        body,
        target,
        iter,
        orelse,
        ..
    } = &stmt.node else {
        return None;
    };

    // The loop itself should contain a single `if` statement, with a single `return
    // True` or `return False`.
    if body.len() != 1 {
        return None;
    }
    if !orelse.is_empty() {
        return None;
    }
    let StmtKind::If {
        body: nested_body,
        test: nested_test,
        orelse: nested_orelse,
    } = &body[0].node else {
        return None;
    };
    if nested_body.len() != 1 {
        return None;
    }
    if !nested_orelse.is_empty() {
        return None;
    }
    let StmtKind::Return { value } = &nested_body[0].node else {
        return None;
    };
    let Some(value) = value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(value), .. } = &value.node else {
        return None;
    };

    // The next statement has to be a `return True` or `return False`.
    let StmtKind::Return { value: next_value } = &sibling.node else {
        return None;
    };
    let Some(next_value) = next_value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(next_value), .. } = &next_value.node else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        next_return_value: *next_value,
        test: nested_test,
        target,
        iter,
    })
}

/// Generate a return statement for an `any` or `all` builtin comprehension.
fn return_stmt(id: &str, test: &Expr, target: &Expr, iter: &Expr, stylist: &Stylist) -> String {
    let mut generator: Generator = stylist.into();
    generator.unparse_stmt(&create_stmt(StmtKind::Return {
        value: Some(Box::new(create_expr(ExprKind::Call {
            func: Box::new(create_expr(ExprKind::Name {
                id: id.to_string(),
                ctx: ExprContext::Load,
            })),
            args: vec![create_expr(ExprKind::GeneratorExp {
                elt: Box::new(test.clone()),
                generators: vec![Comprehension {
                    target: target.clone(),
                    iter: iter.clone(),
                    ifs: vec![],
                    is_async: 0,
                }],
            })],
            keywords: vec![],
        }))),
    }));
    generator.generate()
}

/// SIM110, SIM111
pub fn convert_for_loop_to_any_all(checker: &mut Checker, stmt: &Stmt, sibling: Option<&Stmt>) {
    if let Some(loop_info) = match sibling {
        // Ex) `for` loop with an `else: return True` or `else: return False`.
        None => return_values_for_else(stmt),
        // Ex) `for` loop followed by `return True` or `return False`
        Some(sibling) => return_values_for_siblings(stmt, sibling),
    } {
        if loop_info.return_value && !loop_info.next_return_value {
            if checker.settings.rules.enabled(&Rule::ConvertLoopToAny) {
                let contents = return_stmt(
                    "any",
                    loop_info.test,
                    loop_info.target,
                    loop_info.iter,
                    checker.stylist,
                );

                // Don't flag if the resulting expression would exceed the maximum line length.
                if stmt.location.column() + contents.len() > checker.settings.line_length {
                    return;
                }

                let mut diagnostic = Diagnostic::new(
                    violations::ConvertLoopToAny(contents.clone()),
                    Range::from_located(stmt),
                );
                if checker.patch(&Rule::ConvertLoopToAny) {
                    diagnostic.amend(Fix::replacement(
                        contents,
                        stmt.location,
                        match sibling {
                            None => stmt.end_location.unwrap(),
                            Some(sibling) => sibling.end_location.unwrap(),
                        },
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        if !loop_info.return_value && loop_info.next_return_value {
            if checker.settings.rules.enabled(&Rule::ConvertLoopToAll) {
                // Invert the condition.
                let test = {
                    if let ExprKind::UnaryOp {
                        op: Unaryop::Not,
                        operand,
                    } = &loop_info.test.node
                    {
                        *operand.clone()
                    } else {
                        create_expr(ExprKind::UnaryOp {
                            op: Unaryop::Not,
                            operand: Box::new(loop_info.test.clone()),
                        })
                    }
                };
                let contents = return_stmt(
                    "all",
                    &test,
                    loop_info.target,
                    loop_info.iter,
                    checker.stylist,
                );

                // Don't flag if the resulting expression would exceed the maximum line length.
                if stmt.location.column() + contents.len() > checker.settings.line_length {
                    return;
                }

                let mut diagnostic = Diagnostic::new(
                    violations::ConvertLoopToAll(contents.clone()),
                    Range::from_located(stmt),
                );
                if checker.patch(&Rule::ConvertLoopToAll) {
                    diagnostic.amend(Fix::replacement(
                        contents,
                        stmt.location,
                        match sibling {
                            None => stmt.end_location.unwrap(),
                            Some(sibling) => sibling.end_location.unwrap(),
                        },
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

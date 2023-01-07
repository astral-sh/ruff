use rustpython_ast::{
    Comprehension, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind, Unaryop,
};

use crate::ast::helpers::{create_expr, create_stmt};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticCode};
use crate::source_code_generator::SourceCodeGenerator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::violations;

struct Loop<'a> {
    return_value: bool,
    next_return_value: bool,
    test: &'a Expr,
    target: &'a Expr,
    iter: &'a Expr,
}

/// Extract the returned boolean values from subsequent `StmtKind::If` and
/// `StmtKind::Return` statements, or `None`.
fn return_values<'a>(stmt: &'a Stmt, sibling: &'a Stmt) -> Option<Loop<'a>> {
    let StmtKind::For {
        body,
        target,
        iter,
        ..
    } = &stmt.node else {
        return None;
    };

    // The loop itself should contain a single `if` statement, with a single `return
    // True` or `return False`.
    if body.len() != 1 {
        return None;
    }
    let StmtKind::If {
        body: nested_body,
        test: nested_test,
        ..
    } = &body[0].node else {
        return None;
    };
    if nested_body.len() != 1 {
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
fn return_stmt(
    id: &str,
    test: &Expr,
    target: &Expr,
    iter: &Expr,
    stylist: &SourceCodeStyleDetector,
) -> String {
    let mut generator: SourceCodeGenerator = stylist.into();
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
pub fn convert_loop_to_any_all(checker: &mut Checker, stmt: &Stmt, sibling: &Stmt) {
    if let Some(loop_info) = return_values(stmt, sibling) {
        if loop_info.return_value && !loop_info.next_return_value {
            if checker.settings.enabled.contains(&DiagnosticCode::SIM110) {
                let content = return_stmt(
                    "any",
                    loop_info.test,
                    loop_info.target,
                    loop_info.iter,
                    checker.style,
                );
                let mut check = Diagnostic::new(
                    violations::ConvertLoopToAny(content.clone()),
                    Range::from_located(stmt),
                );
                if checker.patch(&DiagnosticCode::SIM110) {
                    check.amend(Fix::replacement(
                        content,
                        stmt.location,
                        sibling.end_location.unwrap(),
                    ));
                }
                checker.checks.push(check);
            }
        }

        if !loop_info.return_value && loop_info.next_return_value {
            if checker.settings.enabled.contains(&DiagnosticCode::SIM111) {
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
                let content = return_stmt(
                    "all",
                    &test,
                    loop_info.target,
                    loop_info.iter,
                    checker.style,
                );
                let mut check = Diagnostic::new(
                    violations::ConvertLoopToAll(content.clone()),
                    Range::from_located(stmt),
                );
                if checker.patch(&DiagnosticCode::SIM111) {
                    check.amend(Fix::replacement(
                        content,
                        stmt.location,
                        sibling.end_location.unwrap(),
                    ));
                }
                checker.checks.push(check);
            }
        }
    }
}

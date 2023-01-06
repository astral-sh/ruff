use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;
use crate::source_code_style::SourceCodeStyleDetector;

/// Generate source code from an `Expr`.
fn to_source(expr: &Expr, stylist: &SourceCodeStyleDetector) -> String {
    let mut generator: SourceCodeGenerator = stylist.into();
    generator.unparse_expr(expr, 0);
    generator.generate()
}

fn is_main_check(expr: &Expr) -> bool {
    if let ExprKind::Compare {
        left, comparators, ..
    } = &expr.node
    {
        if let ExprKind::Name { id, .. } = &left.node {
            if id == "__name__" {
                if comparators.len() == 1 {
                    if let ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    } = &comparators[0].node
                    {
                        if value == "__main__" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// SIM102
pub fn nested_if_statements(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };

    // if a: <---
    //     if b: <---
    //         c
    let is_nested_if = {
        if orelse.is_empty() && body.len() == 1 {
            if let StmtKind::If { orelse, .. } = &body[0].node {
                orelse.is_empty()
            } else {
                false
            }
        } else {
            false
        }
    };

    if !is_nested_if {
        return;
    };

    if is_main_check(test) {
        return;
    }

    checker.add_check(Check::new(
        CheckKind::NestedIfStatements,
        Range::from_located(stmt),
    ));
}

/// SIM108
pub fn use_ternary_operator(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let StmtKind::Assign { targets: body_targets, value: body_value, .. } = &body[0].node else {
            return;
        };
    let StmtKind::Assign { targets: orelse_targets, value: orelse_value, .. } = &orelse[0].node else {
            return;
        };
    if body_targets.len() != 1 || orelse_targets.len() != 1 {
        return;
    }
    let ExprKind::Name { id: body_id, .. } = &body_targets[0].node else {
        return;
    };
    let ExprKind::Name { id: orelse_id, .. } = &orelse_targets[0].node else {
        return;
    };
    if body_id != orelse_id {
        return;
    }

    let target_var = &body_targets[0];

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        body: parent_body, ..
    }) = parent.map(|p| &p.node)
    {
        for s in parent_body {
            let StmtKind::Assign { targets: parent_targets, .. } = &s.node else {
                continue;
            };
            let Some(ExprKind::Name { id: parent_id, .. }) =
                parent_targets.get(0).map(|t| &t.node) else {
                continue;
            };
            if body_id == parent_id {
                return;
            }
        }
    }

    let assign = to_source(target_var, checker.style);
    let body = to_source(body_value, checker.style);
    let cond = to_source(test, checker.style);
    let orelse = to_source(orelse_value, checker.style);
    let new_code = format!("{assign} = {body} if {cond} else {orelse}");
    let mut check = Check::new(
        CheckKind::UseTernaryOperator(new_code.clone()),
        Range::from_located(stmt),
    );
    if checker.patch(&CheckCode::SIM108) {
        check.amend(Fix::replacement(
            new_code,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}

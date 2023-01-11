use rustpython_ast::{Cmpop, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::{
    contains_call_path, create_expr, create_stmt, unparse_expr, unparse_stmt,
};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;

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

    checker.diagnostics.push(Diagnostic::new(
        violations::NestedIfStatements,
        Range::from_located(stmt),
    ));
}

fn is_one_line_return_bool(stmts: &[Stmt]) -> bool {
    if stmts.len() != 1 {
        return false;
    }
    let StmtKind::Return { value } = &stmts[0].node else {
        return false;
    };
    let Some(ExprKind::Constant { value, .. }) = value.as_ref().map(|value| &value.node) else {
        return false;
    };
    matches!(value, Constant::Bool(_))
}

/// SIM103
pub fn return_bool_condition_directly(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    if !(is_one_line_return_bool(body) && is_one_line_return_bool(orelse)) {
        return;
    }
    let condition = unparse_expr(test, checker.style);
    let mut diagnostic = Diagnostic::new(
        violations::ReturnBoolConditionDirectly(condition),
        Range::from_located(stmt),
    );
    if checker.patch(&RuleCode::SIM103) {
        let return_stmt = create_stmt(StmtKind::Return {
            value: Some(test.clone()),
        });
        diagnostic.amend(Fix::replacement(
            unparse_stmt(&return_stmt, checker.style),
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn ternary(target_var: &Expr, body_value: &Expr, test: &Expr, orelse_value: &Expr) -> Stmt {
    create_stmt(StmtKind::Assign {
        targets: vec![target_var.clone()],
        value: Box::new(create_expr(ExprKind::IfExp {
            test: Box::new(test.clone()),
            body: Box::new(body_value.clone()),
            orelse: Box::new(orelse_value.clone()),
        })),
        type_comment: None,
    })
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

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if contains_call_path(
        test,
        "sys",
        "version_info",
        &checker.import_aliases,
        &checker.from_imports,
    ) {
        return;
    }

    // Avoid suggesting ternary for `if sys.platform.startswith("...")`-style
    // checks.
    if contains_call_path(
        test,
        "sys",
        "platform",
        &checker.import_aliases,
        &checker.from_imports,
    ) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        orelse: parent_orelse,
        ..
    }) = parent.map(|parent| &parent.node)
    {
        if parent_orelse.len() == 1 && stmt == &parent_orelse[0] {
            // TODO(charlie): These two cases have the same AST:
            //
            // if True:
            //     pass
            // elif a:
            //     b = 1
            // else:
            //     b = 2
            //
            // if True:
            //     pass
            // else:
            //     if a:
            //         b = 1
            //     else:
            //         b = 2
            //
            // We want to flag the latter, but not the former. Right now, we flag neither.
            return;
        }
    }

    let target_var = &body_targets[0];
    let ternary = ternary(target_var, body_value, test, orelse_value);
    let content = unparse_stmt(&ternary, checker.style);
    let mut diagnostic = Diagnostic::new(
        violations::UseTernaryOperator(content.clone()),
        Range::from_located(stmt),
    );
    if checker.patch(&RuleCode::SIM108) {
        diagnostic.amend(Fix::replacement(
            content,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn is_same_name_expr(expr1: &Expr, expr2: &Expr) -> bool {
    let ExprKind::Name { id: expr1_id, ..} = &expr1.node else {
        return false;
    };
    let ExprKind::Name { id: expr2_id, ..} = &expr2.node else {
        return false;
    };
    expr1_id.eq(expr2_id)
}

// SIM401
pub fn use_dict_get_with_default(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &Vec<Stmt>,
    orelse: &Vec<Stmt>,
) {
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let StmtKind::Assign { targets: body_lhs, value: body_rhs, ..} = &body[0].node else {
        return;
    };
    if body_lhs.len() != 1 {
        return;
    };
    let StmtKind::Assign { targets: orelse_lhs, value: orelse_rhs, .. } = &orelse[0].node else {
        return;
    };
    if orelse_lhs.len() != 1 {
        return;
    };
    let  ExprKind::Compare { left: test_lhs, ops , comparators: test_rhs } = &test.node else {
        return;
    };
    if test_rhs.len() != 1 {
        return;
    }

    let (expected_lhs, expected_rhs, default_lhs, default_rhs) = match ops[..] {
        [Cmpop::In] => (&body_lhs[0], body_rhs, &orelse_lhs[0], orelse_rhs),
        [Cmpop::NotIn] => (&orelse_lhs[0], orelse_rhs, &body_lhs[0], body_rhs),
        _ => {
            return;
        }
    };
    let test_rhs = &test_rhs[0];

    let ExprKind::Subscript { value: subscript_var, slice, .. }  =  &expected_rhs.node else {
        return;
    };

    // check: dict-key, target-variable, dict-name are same
    if !is_same_name_expr(slice, test_lhs)
        || !is_same_name_expr(expected_lhs, default_lhs)
        || !is_same_name_expr(test_rhs, subscript_var)
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        violations::VerboseDictGetWithDefault(
            unparse_expr(expected_lhs, checker.style),
            unparse_expr(subscript_var, checker.style),
            unparse_expr(test_lhs, checker.style),
            unparse_expr(default_lhs, checker.style),
        ),
        Range::from_located(stmt),
    );
    if checker.patch(&RuleCode::SIM401) {
        diagnostic.amend(Fix::replacement(
            unparse_stmt(
                &create_stmt(StmtKind::Assign {
                    targets: vec![create_expr(expected_lhs.node.clone())],
                    value: Box::new(create_expr(ExprKind::Call {
                        func: Box::new(create_expr(ExprKind::Attribute {
                            value: subscript_var.clone(),
                            attr: "get".to_string(),
                            ctx: ExprContext::Load,
                        })),
                        args: vec![
                            create_expr(test_lhs.node.clone()),
                            create_expr(default_rhs.node.clone()),
                        ],
                        keywords: vec![],
                    })),
                    type_comment: None,
                }),
                checker.style,
            ),
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

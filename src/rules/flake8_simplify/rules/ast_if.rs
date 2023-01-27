use log::error;
use rustpython_ast::{Cmpop, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use crate::ast::comparable::ComparableExpr;
use crate::ast::helpers::{
    contains_call_path, contains_effect, create_expr, create_stmt, first_colon_range,
    has_comments_in, unparse_expr, unparse_stmt,
};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::flake8_simplify::rules::fix_if;
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

/// Find the last nested if statement and return the test expression and the
/// first statement.
///
/// ```python
/// if xxx:
///     if yyy:
///      # ^^^ returns this expression
///         z = 1
///       # ^^^^^ and this statement
///         ...
/// ```
fn find_last_nested_if(body: &[Stmt]) -> Option<(&Expr, &Stmt)> {
    let [Stmt { node: StmtKind::If { test, body: inner_body, orelse }, ..}] = body else { return None };
    if !orelse.is_empty() {
        return None;
    }
    find_last_nested_if(inner_body).or_else(|| {
        Some((
            test,
            inner_body.last().expect("Expected body to be non-empty"),
        ))
    })
}

/// SIM102
pub fn nested_if_statements(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
    parent: Option<&Stmt>,
) {
    // If the parent could contain a nested if-statement, abort.
    if let Some(parent) = parent {
        if let StmtKind::If { body, orelse, .. } = &parent.node {
            if orelse.is_empty() && body.len() == 1 {
                return;
            }
        }
    }

    // If this if-statement has an else clause, or more than one child, abort.
    if !(orelse.is_empty() && body.len() == 1) {
        return;
    }

    if is_main_check(test) {
        return;
    }

    // Find the deepest nested if-statement, to inform the range.
    let Some((test, first_stmt)) = find_last_nested_if(body) else {
        return;
    };
    let colon = first_colon_range(
        Range::new(test.end_location.unwrap(), first_stmt.location),
        checker.locator,
    );
    let mut diagnostic = Diagnostic::new(
        violations::NestedIfStatements,
        colon.map_or_else(
            || Range::from_located(stmt),
            |colon| Range::new(stmt.location, colon.end_location),
        ),
    );
    if checker.patch(&Rule::NestedIfStatements) {
        // The fixer preserves comments in the nested body, but removes comments between
        // the outer and inner if statements.
        let nested_if = &body[0];
        if !has_comments_in(
            Range::new(stmt.location, nested_if.location),
            checker.locator,
        ) {
            match fix_if::fix_nested_if_statements(checker.locator, checker.stylist, stmt) {
                Ok(fix) => {
                    if fix
                        .content
                        .lines()
                        .all(|line| line.len() <= checker.settings.line_length)
                    {
                        diagnostic.amend(fix);
                    }
                }
                Err(err) => error!("Failed to fix nested if: {err}"),
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}

enum Bool {
    True,
    False,
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        if value {
            Bool::True
        } else {
            Bool::False
        }
    }
}

fn is_one_line_return_bool(stmts: &[Stmt]) -> Option<Bool> {
    if stmts.len() != 1 {
        return None;
    }
    let StmtKind::Return { value } = &stmts[0].node else {
        return None;
    };
    let Some(ExprKind::Constant { value, .. }) = value.as_ref().map(|value| &value.node) else {
        return None;
    };
    let Constant::Bool(value) = value else {
        return None;
    };
    Some((*value).into())
}

/// SIM103
pub fn return_bool_condition_directly(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    let (Some(if_return), Some(else_return)) = (is_one_line_return_bool(body), is_one_line_return_bool(orelse)) else {
        return;
    };
    let condition = unparse_expr(test, checker.stylist);
    let mut diagnostic = Diagnostic::new(
        violations::ReturnBoolConditionDirectly(condition),
        Range::from_located(stmt),
    );
    if checker.patch(&Rule::ReturnBoolConditionDirectly)
        && matches!(if_return, Bool::True)
        && matches!(else_return, Bool::False)
        && !has_comments_in(Range::from_located(stmt), checker.locator)
    {
        let return_stmt = create_stmt(StmtKind::Return {
            value: Some(Box::new(create_expr(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Name {
                    id: "bool".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![(**test).clone()],
                keywords: vec![],
            }))),
        });
        diagnostic.amend(Fix::replacement(
            unparse_stmt(&return_stmt, checker.stylist),
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
    if contains_call_path(checker, test, &["sys", "version_info"]) {
        return;
    }

    // Avoid suggesting ternary for `if sys.platform.startswith("...")`-style
    // checks.
    if contains_call_path(checker, test, &["sys", "platform"]) {
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
    let contents = unparse_stmt(&ternary, checker.stylist);

    // Don't flag if the resulting expression would exceed the maximum line length.
    if stmt.location.column() + contents.len() > checker.settings.line_length {
        return;
    }

    // Don't flag if the statement expression contains any comments.
    if has_comments_in(Range::from_located(stmt), checker.locator) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        violations::UseTernaryOperator(contents.clone()),
        Range::from_located(stmt),
    );
    if checker.patch(&Rule::UseTernaryOperator) {
        diagnostic.amend(Fix::replacement(
            contents,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn compare_expr(expr1: &ComparableExpr, expr2: &ComparableExpr) -> bool {
    expr1.eq(expr2)
}

/// SIM401
pub fn use_dict_get_with_default(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &Vec<Stmt>,
    orelse: &Vec<Stmt>,
    parent: Option<&Stmt>,
) {
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let StmtKind::Assign { targets: body_var, value: body_val, ..} = &body[0].node else {
        return;
    };
    if body_var.len() != 1 {
        return;
    };
    let StmtKind::Assign { targets: orelse_var, value: orelse_val, .. } = &orelse[0].node else {
        return;
    };
    if orelse_var.len() != 1 {
        return;
    };
    let ExprKind::Compare { left: test_key, ops , comparators: test_dict } = &test.node else {
        return;
    };
    if test_dict.len() != 1 {
        return;
    }
    let (expected_var, expected_val, default_var, default_val) = match ops[..] {
        [Cmpop::In] => (&body_var[0], body_val, &orelse_var[0], orelse_val),
        [Cmpop::NotIn] => (&orelse_var[0], orelse_val, &body_var[0], body_val),
        _ => {
            return;
        }
    };
    let test_dict = &test_dict[0];
    let ExprKind::Subscript { value: expected_subscript, slice: expected_slice, .. }  =  &expected_val.node else {
        return;
    };

    // Check that the dictionary key, target variables, and dictionary name are all
    // equivalent.
    if !compare_expr(&expected_slice.into(), &test_key.into())
        || !compare_expr(&expected_var.into(), &default_var.into())
        || !compare_expr(&test_dict.into(), &expected_subscript.into())
    {
        return;
    }

    // Check that the default value is not "complex".
    if contains_effect(checker, default_val) {
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

    let contents = unparse_stmt(
        &create_stmt(StmtKind::Assign {
            targets: vec![create_expr(expected_var.node.clone())],
            value: Box::new(create_expr(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Attribute {
                    value: expected_subscript.clone(),
                    attr: "get".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![
                    create_expr(test_key.node.clone()),
                    create_expr(default_val.node.clone()),
                ],
                keywords: vec![],
            })),
            type_comment: None,
        }),
        checker.stylist,
    );

    // Don't flag if the resulting expression would exceed the maximum line length.
    if stmt.location.column() + contents.len() > checker.settings.line_length {
        return;
    }

    // Don't flag if the statement expression contains any comments.
    if has_comments_in(Range::from_located(stmt), checker.locator) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        violations::DictGetWithDefault(contents.clone()),
        Range::from_located(stmt),
    );
    if checker.patch(&Rule::DictGetWithDefault) {
        diagnostic.amend(Fix::replacement(
            contents,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

use anyhow::Result;
use itertools::izip;
use log::error;
use rustc_hash::FxHashMap;
use rustpython_ast::{Arguments, Location, StmtKind};
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Stmt, Unaryop};

use crate::ast::helpers::{match_leading_content, match_trailing_content};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind, RejectedCmpop};
use crate::code_gen::SourceGenerator;

fn compare(left: &Expr, ops: &[Cmpop], comparators: &[Expr]) -> Option<String> {
    let cmp = Expr::new(
        Default::default(),
        Default::default(),
        ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: ops.to_vec(),
            comparators: comparators.to_vec(),
        },
    );
    let mut generator = SourceGenerator::new();
    if let Ok(()) = generator.unparse_expr(&cmp, 0) {
        if let Ok(content) = generator.generate() {
            return Some(content);
        }
    }
    None
}

/// E711, E712
pub fn literal_comparisons(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    check_none_comparisons: bool,
    check_true_false_comparisons: bool,
) {
    // Mapping from (bad operator index) to (replacement operator). As we iterate
    // through the list of operators, we apply "dummy" fixes for each error,
    // then replace the entire expression at the end with one "real" fix, to
    // avoid conflicts.
    let mut bad_ops: FxHashMap<usize, Cmpop> = FxHashMap::default();
    let mut checks: Vec<Check> = vec![];

    let op = ops.first().unwrap();
    let comparator = left;

    // Check `left`.
    if check_none_comparisons
        && matches!(
            comparator.node,
            ExprKind::Constant {
                value: Constant::None,
                kind: None
            }
        )
    {
        if matches!(op, Cmpop::Eq) {
            let mut check = Check::new(
                CheckKind::NoneComparison(RejectedCmpop::Eq),
                Range::from_located(comparator),
            );
            if checker.patch(check.kind.code()) {
                // Dummy replacement
                check.amend(Fix::dummy(expr.location));
                bad_ops.insert(0, Cmpop::Is);
            }
            checks.push(check);
        }
        if matches!(op, Cmpop::NotEq) {
            let mut check = Check::new(
                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                Range::from_located(comparator),
            );
            if checker.patch(check.kind.code()) {
                check.amend(Fix::dummy(expr.location));
                bad_ops.insert(0, Cmpop::IsNot);
            }
            checks.push(check);
        }
    }

    if check_true_false_comparisons {
        if let ExprKind::Constant {
            value: Constant::Bool(value),
            kind: None,
        } = comparator.node
        {
            if matches!(op, Cmpop::Eq) {
                let mut check = Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                    Range::from_located(comparator),
                );
                if checker.patch(check.kind.code()) {
                    check.amend(Fix::dummy(expr.location));
                    bad_ops.insert(0, Cmpop::Is);
                }
                checks.push(check);
            }
            if matches!(op, Cmpop::NotEq) {
                let mut check = Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                    Range::from_located(comparator),
                );
                if checker.patch(check.kind.code()) {
                    check.amend(Fix::dummy(expr.location));
                    bad_ops.insert(0, Cmpop::IsNot);
                }
                checks.push(check);
            }
        }
    }

    // Check each comparator in order.
    for (idx, (op, comparator)) in izip!(ops, comparators).enumerate() {
        if check_none_comparisons
            && matches!(
                comparator.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None
                }
            )
        {
            if matches!(op, Cmpop::Eq) {
                let mut check = Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::Eq),
                    Range::from_located(comparator),
                );
                if checker.patch(check.kind.code()) {
                    check.amend(Fix::dummy(expr.location));
                    bad_ops.insert(idx, Cmpop::Is);
                }
                checks.push(check);
            }
            if matches!(op, Cmpop::NotEq) {
                let mut check = Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::NotEq),
                    Range::from_located(comparator),
                );
                if checker.patch(check.kind.code()) {
                    check.amend(Fix::dummy(expr.location));
                    bad_ops.insert(idx, Cmpop::IsNot);
                }
                checks.push(check);
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = comparator.node
            {
                if matches!(op, Cmpop::Eq) {
                    let mut check = Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                        Range::from_located(comparator),
                    );
                    if checker.patch(check.kind.code()) {
                        check.amend(Fix::dummy(expr.location));
                        bad_ops.insert(idx, Cmpop::Is);
                    }
                    checks.push(check);
                }
                if matches!(op, Cmpop::NotEq) {
                    let mut check = Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                        Range::from_located(comparator),
                    );
                    if checker.patch(check.kind.code()) {
                        check.amend(Fix::dummy(expr.location));
                        bad_ops.insert(idx, Cmpop::IsNot);
                    }
                    checks.push(check);
                }
            }
        }
    }

    if !bad_ops.is_empty() {
        // Replace the entire comparison expression.
        let ops = ops
            .iter()
            .enumerate()
            .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(content) = compare(left, &ops, comparators) {
            if let Some(check) = checks.last_mut() {
                check.fix = Some(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
    }

    checker.add_checks(checks.into_iter());
}

/// E713, E714
pub fn not_tests(
    checker: &mut Checker,
    expr: &Expr,
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) {
    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare {
            left,
            ops,
            comparators,
            ..
        } = &operand.node
        {
            let should_fix = ops.len() == 1;
            for op in ops.iter() {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            let mut check =
                                Check::new(CheckKind::NotInTest, Range::from_located(operand));
                            if checker.patch(check.kind.code()) && should_fix {
                                if let Some(content) = compare(left, &[Cmpop::NotIn], comparators) {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                            }
                            checker.add_check(check);
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            let mut check =
                                Check::new(CheckKind::NotIsTest, Range::from_located(operand));
                            if checker.patch(check.kind.code()) && should_fix {
                                if let Some(content) = compare(left, &[Cmpop::IsNot], comparators) {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                            }
                            checker.add_check(check);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn function(name: &str, args: &Arguments, body: &Expr) -> Result<String> {
    let body = Stmt::new(
        Default::default(),
        Default::default(),
        StmtKind::Return {
            value: Some(Box::new(body.clone())),
        },
    );
    let func = Stmt::new(
        Default::default(),
        Default::default(),
        StmtKind::FunctionDef {
            name: name.to_string(),
            args: Box::new(args.clone()),
            body: vec![body],
            decorator_list: vec![],
            returns: None,
            type_comment: None,
        },
    );
    let mut generator = SourceGenerator::new();
    generator.unparse_stmt(&func)?;
    generator.generate().map_err(|e| e.into())
}

/// E731
pub fn do_not_assign_lambda(checker: &mut Checker, target: &Expr, value: &Expr, stmt: &Stmt) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            let mut check = Check::new(CheckKind::DoNotAssignLambda, Range::from_located(stmt));
            if checker.patch(check.kind.code()) {
                if !match_leading_content(stmt, checker.locator)
                    && !match_trailing_content(stmt, checker.locator)
                {
                    match function(id, args, body) {
                        Ok(content) => {
                            let indentation =
                                &leading_space(&checker.locator.slice_source_code_range(&Range {
                                    location: Location::new(stmt.location.row(), 0),
                                    end_location: Location::new(stmt.location.row() + 1, 0),
                                }));
                            let mut indented = String::new();
                            for (idx, line) in content.lines().enumerate() {
                                if idx == 0 {
                                    indented.push_str(line);
                                } else {
                                    indented.push('\n');
                                    indented.push_str(indentation);
                                    indented.push_str(line);
                                }
                            }
                            check.amend(Fix::replacement(
                                indented,
                                stmt.location,
                                stmt.end_location.unwrap(),
                            ));
                        }
                        Err(e) => error!("Failed to generate fix: {}", e),
                    }
                }
            }
            checker.add_check(check);
        }
    }
}

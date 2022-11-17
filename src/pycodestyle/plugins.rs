use itertools::izip;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind, RejectedCmpop};
use crate::code_gen::SourceGenerator;
use fnv::FnvHashMap;

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
    let op = ops.first().unwrap();
    let comparator = left;
    let mut checks: Vec<Check> = vec![];
    // Mapping of bad operator index -> good operator
    let mut bad_ops: FnvHashMap<usize, Cmpop> = FnvHashMap::default();

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
            if checker.patch() {
                // Dummy replacement
                check.amend(Fix::replacement(
                    "".to_string(),
                    expr.location,
                    expr.location,
                ));
                bad_ops.insert(0, Cmpop::Is);
            }
            checks.push(check);
        }
        if matches!(op, Cmpop::NotEq) {
            let mut check = Check::new(
                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                Range::from_located(comparator),
            );
            if checker.patch() {
                check.amend(Fix::replacement(
                    "".to_string(),
                    expr.location,
                    expr.location,
                ));
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
                if checker.patch() {
                    check.amend(Fix::replacement(
                        "".to_string(),
                        expr.location,
                        expr.location,
                    ));
                    bad_ops.insert(0, Cmpop::Is);
                }
                checks.push(check);
            }
            if matches!(op, Cmpop::NotEq) {
                let mut check = Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                    Range::from_located(comparator),
                );
                if checker.patch() {
                    check.amend(Fix::replacement(
                        "".to_string(),
                        expr.location,
                        expr.location,
                    ));
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
                if checker.patch() {
                    check.amend(Fix::replacement(
                        "".to_string(),
                        expr.location,
                        expr.location,
                    ));
                    bad_ops.insert(idx, Cmpop::Is);
                }
                checks.push(check);
            }
            if matches!(op, Cmpop::NotEq) {
                let mut check = Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::NotEq),
                    Range::from_located(comparator),
                );
                if checker.patch() {
                    check.amend(Fix::replacement(
                        "".to_string(),
                        expr.location,
                        expr.location,
                    ));
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
                    if checker.patch() {
                        check.amend(Fix::replacement(
                            "".to_string(),
                            expr.location,
                            expr.location,
                        ));
                        bad_ops.insert(idx, Cmpop::Is);
                    }
                    checks.push(check);
                }
                if matches!(op, Cmpop::NotEq) {
                    let mut check = Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                        Range::from_located(comparator),
                    );
                    if checker.patch() {
                        check.amend(Fix::replacement(
                            "".to_string(),
                            expr.location,
                            expr.location,
                        ));
                        bad_ops.insert(idx, Cmpop::IsNot);
                    }
                    checks.push(check);
                }
            }
        }
    }

    if !bad_ops.is_empty() {
        let ops = ops
            .iter()
            .enumerate()
            .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(new_compare) = compare(left, &ops, comparators) {
            if let Some(check) = checks.last_mut() {
                // Replace the entire compare expression
                check.fix = Some(Fix::replacement(
                    new_compare,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
    }

    for check in checks {
        checker.add_check(check);
    }
}

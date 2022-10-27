use itertools::izip;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Unaryop};

use crate::ast::types::{CheckLocator, Range};
use crate::checks::{Check, CheckKind, RejectedCmpop};

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// E741
pub fn ambiguous_variable_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousVariableName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E742
pub fn ambiguous_class_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousClassName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E743
pub fn ambiguous_function_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousFunctionName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E731
pub fn do_not_assign_lambda(value: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Lambda { .. } = &value.node {
        Some(Check::new(CheckKind::DoNotAssignLambda, location))
    } else {
        None
    }
}

/// E713, E714
pub fn not_tests(
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
    locator: &dyn CheckLocator,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare { ops, .. } = &operand.node {
            for op in ops {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            checks.push(Check::new(
                                CheckKind::NotInTest,
                                locator.locate_check(Range::from_located(operand)),
                            ));
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            checks.push(Check::new(
                                CheckKind::NotIsTest,
                                locator.locate_check(Range::from_located(operand)),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    checks
}

/// E711, E712
pub fn literal_comparisons(
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    check_none_comparisons: bool,
    check_true_false_comparisons: bool,
    locator: &dyn CheckLocator,
) -> Vec<Check> {
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
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::Eq),
                locator.locate_check(Range::from_located(comparator)),
            ));
        }
        if matches!(op, Cmpop::NotEq) {
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                locator.locate_check(Range::from_located(comparator)),
            ));
        }
    }

    if check_true_false_comparisons {
        if let ExprKind::Constant {
            value: Constant::Bool(value),
            kind: None,
        } = comparator.node
        {
            if matches!(op, Cmpop::Eq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                    locator.locate_check(Range::from_located(comparator)),
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                    locator.locate_check(Range::from_located(comparator)),
                ));
            }
        }
    }

    // Check each comparator in order.
    for (op, comparator) in izip!(ops, comparators) {
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
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::Eq),
                    locator.locate_check(Range::from_located(comparator)),
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::NotEq),
                    locator.locate_check(Range::from_located(comparator)),
                ));
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = comparator.node
            {
                if matches!(op, Cmpop::Eq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                        locator.locate_check(Range::from_located(comparator)),
                    ));
                }
                if matches!(op, Cmpop::NotEq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                        locator.locate_check(Range::from_located(comparator)),
                    ));
                }
            }
        }
    }

    checks
}

/// E721
pub fn type_comparison(ops: &[Cmpop], comparators: &[Expr], location: Range) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    for (op, right) in izip!(ops, comparators) {
        if matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            match &right.node {
                ExprKind::Call { func, args, .. } => {
                    if let ExprKind::Name { id, .. } = &func.node {
                        // Ex) type(False)
                        if id == "type" {
                            if let Some(arg) = args.first() {
                                // Allow comparison for types which are not obvious.
                                if !matches!(arg.node, ExprKind::Name { .. }) {
                                    checks.push(Check::new(CheckKind::TypeComparison, location));
                                }
                            }
                        }
                    }
                }
                ExprKind::Attribute { value, .. } => {
                    if let ExprKind::Name { id, .. } = &value.node {
                        // Ex) types.IntType
                        if id == "types" {
                            checks.push(Check::new(CheckKind::TypeComparison, location));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    checks
}

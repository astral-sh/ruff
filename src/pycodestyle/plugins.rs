use itertools::izip;
use rustc_hash::FxHashMap;
use rustpython_ast::{Arguments, Location, StmtKind};
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Stmt, Unaryop};

use crate::ast::helpers;
use crate::ast::helpers::{
    create_expr, match_leading_content, match_trailing_content, unparse_expr,
};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::source_code_generator::SourceCodeGenerator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::violations;

pub fn compare(
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    stylist: &SourceCodeStyleDetector,
) -> String {
    unparse_expr(
        &create_expr(ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: ops.to_vec(),
            comparators: comparators.to_vec(),
        }),
        stylist,
    )
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
    let mut checks: Vec<Diagnostic> = vec![];

    let op = ops.first().unwrap();

    // Check `left`.
    let mut comparator = left;
    let next = &comparators[0];
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
            let check = Diagnostic::new(
                violations::NoneComparison(op.into()),
                Range::from_located(comparator),
            );
            if checker.patch(check.kind.code()) && !helpers::is_constant_non_singleton(next) {
                bad_ops.insert(0, Cmpop::Is);
            }
            checks.push(check);
        }
        if matches!(op, Cmpop::NotEq) {
            let check = Diagnostic::new(
                violations::NoneComparison(op.into()),
                Range::from_located(comparator),
            );
            if checker.patch(check.kind.code()) && !helpers::is_constant_non_singleton(next) {
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
                let check = Diagnostic::new(
                    violations::TrueFalseComparison(value, op.into()),
                    Range::from_located(comparator),
                );
                if checker.patch(check.kind.code()) && !helpers::is_constant_non_singleton(next) {
                    bad_ops.insert(0, Cmpop::Is);
                }
                checks.push(check);
            }
            if matches!(op, Cmpop::NotEq) {
                let check = Diagnostic::new(
                    violations::TrueFalseComparison(value, op.into()),
                    Range::from_located(comparator),
                );
                if checker.patch(check.kind.code()) && !helpers::is_constant_non_singleton(next) {
                    bad_ops.insert(0, Cmpop::IsNot);
                }
                checks.push(check);
            }
        }
    }

    // Check each comparator in order.
    for (idx, (op, next)) in izip!(ops, comparators).enumerate() {
        if check_none_comparisons
            && matches!(
                next.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None
                }
            )
        {
            if matches!(op, Cmpop::Eq) {
                let check = Diagnostic::new(
                    violations::NoneComparison(op.into()),
                    Range::from_located(next),
                );
                if checker.patch(check.kind.code())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::Is);
                }
                checks.push(check);
            }
            if matches!(op, Cmpop::NotEq) {
                let check = Diagnostic::new(
                    violations::NoneComparison(op.into()),
                    Range::from_located(next),
                );
                if checker.patch(check.kind.code())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::IsNot);
                }
                checks.push(check);
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = next.node
            {
                if matches!(op, Cmpop::Eq) {
                    let check = Diagnostic::new(
                        violations::TrueFalseComparison(value, op.into()),
                        Range::from_located(next),
                    );
                    if checker.patch(check.kind.code())
                        && !helpers::is_constant_non_singleton(comparator)
                    {
                        bad_ops.insert(idx, Cmpop::Is);
                    }
                    checks.push(check);
                }
                if matches!(op, Cmpop::NotEq) {
                    let check = Diagnostic::new(
                        violations::TrueFalseComparison(value, op.into()),
                        Range::from_located(next),
                    );
                    if checker.patch(check.kind.code())
                        && !helpers::is_constant_non_singleton(comparator)
                    {
                        bad_ops.insert(idx, Cmpop::IsNot);
                    }
                    checks.push(check);
                }
            }
        }

        comparator = next;
    }

    // TODO(charlie): Respect `noqa` directives. If one of the operators has a
    // `noqa`, but another doesn't, both will be removed here.
    if !bad_ops.is_empty() {
        // Replace the entire comparison expression.
        let ops = ops
            .iter()
            .enumerate()
            .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
            .cloned()
            .collect::<Vec<_>>();
        let content = compare(left, &ops, comparators, checker.style);
        for check in &mut checks {
            check.amend(Fix::replacement(
                content.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }

    checker.checks.extend(checks);
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
                            let mut check = Diagnostic::new(
                                violations::NotInTest,
                                Range::from_located(operand),
                            );
                            if checker.patch(check.kind.code()) && should_fix {
                                check.amend(Fix::replacement(
                                    compare(left, &[Cmpop::NotIn], comparators, checker.style),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                            checker.checks.push(check);
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            let mut check = Diagnostic::new(
                                violations::NotIsTest,
                                Range::from_located(operand),
                            );
                            if checker.patch(check.kind.code()) && should_fix {
                                check.amend(Fix::replacement(
                                    compare(left, &[Cmpop::IsNot], comparators, checker.style),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                            checker.checks.push(check);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn function(
    name: &str,
    args: &Arguments,
    body: &Expr,
    stylist: &SourceCodeStyleDetector,
) -> String {
    let body = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Return {
            value: Some(Box::new(body.clone())),
        },
    );
    let func = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::FunctionDef {
            name: name.to_string(),
            args: Box::new(args.clone()),
            body: vec![body],
            decorator_list: vec![],
            returns: None,
            type_comment: None,
        },
    );
    let mut generator: SourceCodeGenerator = stylist.into();
    generator.unparse_stmt(&func);
    generator.generate()
}

/// E731
pub fn do_not_assign_lambda(checker: &mut Checker, target: &Expr, value: &Expr, stmt: &Stmt) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            let mut check = Diagnostic::new(
                violations::DoNotAssignLambda(id.to_string()),
                Range::from_located(stmt),
            );
            if checker.patch(check.kind.code()) {
                if !match_leading_content(stmt, checker.locator)
                    && !match_trailing_content(stmt, checker.locator)
                {
                    let first_line = checker.locator.slice_source_code_range(&Range::new(
                        Location::new(stmt.location.row(), 0),
                        Location::new(stmt.location.row() + 1, 0),
                    ));
                    let indentation = &leading_space(&first_line);
                    let mut indented = String::new();
                    for (idx, line) in function(id, args, body, checker.style).lines().enumerate() {
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
            }
            checker.checks.push(check);
        }
    }
}

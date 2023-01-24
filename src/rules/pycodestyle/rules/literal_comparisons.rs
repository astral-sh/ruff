use itertools::izip;
use rustc_hash::FxHashMap;
use rustpython_ast::Constant;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::compare;
use crate::violations;

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
    let mut diagnostics: Vec<Diagnostic> = vec![];

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
            let diagnostic = Diagnostic::new(
                violations::NoneComparison(op.into()),
                Range::from_located(comparator),
            );
            if checker.patch(diagnostic.kind.rule()) && !helpers::is_constant_non_singleton(next) {
                bad_ops.insert(0, Cmpop::Is);
            }
            diagnostics.push(diagnostic);
        }
        if matches!(op, Cmpop::NotEq) {
            let diagnostic = Diagnostic::new(
                violations::NoneComparison(op.into()),
                Range::from_located(comparator),
            );
            if checker.patch(diagnostic.kind.rule()) && !helpers::is_constant_non_singleton(next) {
                bad_ops.insert(0, Cmpop::IsNot);
            }
            diagnostics.push(diagnostic);
        }
    }

    if check_true_false_comparisons {
        if let ExprKind::Constant {
            value: Constant::Bool(value),
            kind: None,
        } = comparator.node
        {
            if matches!(op, Cmpop::Eq) {
                let diagnostic = Diagnostic::new(
                    violations::TrueFalseComparison(value, op.into()),
                    Range::from_located(comparator),
                );
                if checker.patch(diagnostic.kind.rule())
                    && !helpers::is_constant_non_singleton(next)
                {
                    bad_ops.insert(0, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic = Diagnostic::new(
                    violations::TrueFalseComparison(value, op.into()),
                    Range::from_located(comparator),
                );
                if checker.patch(diagnostic.kind.rule())
                    && !helpers::is_constant_non_singleton(next)
                {
                    bad_ops.insert(0, Cmpop::IsNot);
                }
                diagnostics.push(diagnostic);
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
                let diagnostic = Diagnostic::new(
                    violations::NoneComparison(op.into()),
                    Range::from_located(next),
                );
                if checker.patch(diagnostic.kind.rule())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic = Diagnostic::new(
                    violations::NoneComparison(op.into()),
                    Range::from_located(next),
                );
                if checker.patch(diagnostic.kind.rule())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::IsNot);
                }
                diagnostics.push(diagnostic);
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = next.node
            {
                if matches!(op, Cmpop::Eq) {
                    let diagnostic = Diagnostic::new(
                        violations::TrueFalseComparison(value, op.into()),
                        Range::from_located(next),
                    );
                    if checker.patch(diagnostic.kind.rule())
                        && !helpers::is_constant_non_singleton(comparator)
                    {
                        bad_ops.insert(idx, Cmpop::Is);
                    }
                    diagnostics.push(diagnostic);
                }
                if matches!(op, Cmpop::NotEq) {
                    let diagnostic = Diagnostic::new(
                        violations::TrueFalseComparison(value, op.into()),
                        Range::from_located(next),
                    );
                    if checker.patch(diagnostic.kind.rule())
                        && !helpers::is_constant_non_singleton(comparator)
                    {
                        bad_ops.insert(idx, Cmpop::IsNot);
                    }
                    diagnostics.push(diagnostic);
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
        let content = compare(left, &ops, comparators, checker.stylist);
        for diagnostic in &mut diagnostics {
            diagnostic.amend(Fix::replacement(
                content.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }

    checker.diagnostics.extend(diagnostics);
}

use itertools::izip;
use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};
use serde::{Deserialize, Serialize};

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::compare;
use crate::violation::AlwaysAutofixableViolation;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqCmpop {
    Eq,
    NotEq,
}

impl From<&Cmpop> for EqCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Eq => EqCmpop::Eq,
            Cmpop::NotEq => EqCmpop::NotEq,
            _ => unreachable!("Expected Cmpop::Eq | Cmpop::NotEq"),
        }
    }
}

define_violation!(
    pub struct NoneComparison(pub EqCmpop);
);
impl AlwaysAutofixableViolation for NoneComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpop::Eq => format!("Comparison to `None` should be `cond is None`"),
            EqCmpop::NotEq => format!("Comparison to `None` should be `cond is not None`"),
        }
    }

    fn autofix_title(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpop::Eq => "Replace with `cond is None`".to_string(),
            EqCmpop::NotEq => "Replace with `cond is not None`".to_string(),
        }
    }
}

define_violation!(
    pub struct TrueFalseComparison(pub bool, pub EqCmpop);
);
impl AlwaysAutofixableViolation for TrueFalseComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpop::Eq) => format!("Comparison to `True` should be `cond is True`"),
            (true, EqCmpop::NotEq) => {
                format!("Comparison to `True` should be `cond is not True`")
            }
            (false, EqCmpop::Eq) => format!("Comparison to `False` should be `cond is False`"),
            (false, EqCmpop::NotEq) => {
                format!("Comparison to `False` should be `cond is not False`")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpop::Eq) => "Replace with `cond is True`".to_string(),
            (true, EqCmpop::NotEq) => "Replace with `cond is not True`".to_string(),
            (false, EqCmpop::Eq) => "Replace with `cond is False`".to_string(),
            (false, EqCmpop::NotEq) => "Replace with `cond is not False`".to_string(),
        }
    }
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
            let diagnostic =
                Diagnostic::new(NoneComparison(op.into()), Range::from_located(comparator));
            if checker.patch(diagnostic.kind.rule()) && !helpers::is_constant_non_singleton(next) {
                bad_ops.insert(0, Cmpop::Is);
            }
            diagnostics.push(diagnostic);
        }
        if matches!(op, Cmpop::NotEq) {
            let diagnostic =
                Diagnostic::new(NoneComparison(op.into()), Range::from_located(comparator));
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
                    TrueFalseComparison(value, op.into()),
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
                    TrueFalseComparison(value, op.into()),
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
                let diagnostic =
                    Diagnostic::new(NoneComparison(op.into()), Range::from_located(next));
                if checker.patch(diagnostic.kind.rule())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic =
                    Diagnostic::new(NoneComparison(op.into()), Range::from_located(next));
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
                        TrueFalseComparison(value, op.into()),
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
                        TrueFalseComparison(value, op.into()),
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

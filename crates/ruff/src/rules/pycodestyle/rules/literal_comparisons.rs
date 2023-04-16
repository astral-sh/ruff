use itertools::izip;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::compare;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum EqCmpop {
    Eq,
    NotEq,
}

impl From<&Cmpop> for EqCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Eq => EqCmpop::Eq,
            Cmpop::NotEq => EqCmpop::NotEq,
            _ => panic!("Expected Cmpop::Eq | Cmpop::NotEq"),
        }
    }
}

/// ## What it does
/// Checks for comparisons to `None` which are not using the `is` operator.
///
/// ## Why is this bad?
/// Per PEP 8, "Comparisons to singletons like None should always be done with
/// is or is not, never the equality operators."
///
/// ## Example
/// ```python
/// if arg != None:
/// if None == arg:
/// ```
///
/// Use instead:
/// ```python
/// if arg is not None:
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#programming-recommendations)
#[violation]
pub struct NoneComparison(pub EqCmpop);

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

/// ## What it does
/// Checks for comparisons to booleans which are not using the `is` operator.
///
/// ## Why is this bad?
/// Per PEP 8, "Comparisons to singletons like None should always be done with
/// is or is not, never the equality operators."
///
/// ## Example
/// ```python
/// if arg == True:
/// if False == arg:
/// ```
///
/// Use instead:
/// ```python
/// if arg is True:
/// if arg is False:
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#programming-recommendations)
#[violation]
pub struct TrueFalseComparison(pub bool, pub EqCmpop);

impl AlwaysAutofixableViolation for TrueFalseComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpop::Eq) => {
                format!("Comparison to `True` should be `cond is True` or `if cond:`")
            }
            (true, EqCmpop::NotEq) => {
                format!("Comparison to `True` should be `cond is not True` or `if not cond:`")
            }
            (false, EqCmpop::Eq) => {
                format!("Comparison to `False` should be `cond is False` or `if not cond:`")
            }
            (false, EqCmpop::NotEq) => {
                format!("Comparison to `False` should be `cond is not False` or `if cond:`")
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

    if !helpers::is_constant_non_singleton(next) {
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
                    Diagnostic::new(NoneComparison(op.into()), Range::from(comparator));
                if checker.patch(diagnostic.kind.rule()) {
                    bad_ops.insert(0, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic =
                    Diagnostic::new(NoneComparison(op.into()), Range::from(comparator));
                if checker.patch(diagnostic.kind.rule()) {
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
                        Range::from(comparator),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        bad_ops.insert(0, Cmpop::Is);
                    }
                    diagnostics.push(diagnostic);
                }
                if matches!(op, Cmpop::NotEq) {
                    let diagnostic = Diagnostic::new(
                        TrueFalseComparison(value, op.into()),
                        Range::from(comparator),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        bad_ops.insert(0, Cmpop::IsNot);
                    }
                    diagnostics.push(diagnostic);
                }
            }
        }
    }

    // Check each comparator in order.
    for (idx, (op, next)) in izip!(ops, comparators).enumerate() {
        if helpers::is_constant_non_singleton(comparator) {
            comparator = next;
            continue;
        }

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
                let diagnostic = Diagnostic::new(NoneComparison(op.into()), Range::from(next));
                if checker.patch(diagnostic.kind.rule()) {
                    bad_ops.insert(idx, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic = Diagnostic::new(NoneComparison(op.into()), Range::from(next));
                if checker.patch(diagnostic.kind.rule()) {
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
                    let diagnostic =
                        Diagnostic::new(TrueFalseComparison(value, op.into()), Range::from(next));
                    if checker.patch(diagnostic.kind.rule()) {
                        bad_ops.insert(idx, Cmpop::Is);
                    }
                    diagnostics.push(diagnostic);
                }
                if matches!(op, Cmpop::NotEq) {
                    let diagnostic =
                        Diagnostic::new(TrueFalseComparison(value, op.into()), Range::from(next));
                    if checker.patch(diagnostic.kind.rule()) {
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
            diagnostic.set_fix(Edit::replacement(
                content.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }

    checker.diagnostics.extend(diagnostics);
}

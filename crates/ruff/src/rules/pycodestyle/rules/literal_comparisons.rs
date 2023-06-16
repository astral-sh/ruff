use itertools::izip;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{self, Cmpop, Constant, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::is_const_none;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::compare;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum EqCmpop {
    Eq,
    NotEq,
}

impl EqCmpop {
    fn try_from(value: Cmpop) -> Option<EqCmpop> {
        match value {
            Cmpop::Eq => Some(EqCmpop::Eq),
            Cmpop::NotEq => Some(EqCmpop::NotEq),
            _ => None,
        }
    }
}

/// ## What it does
/// Checks for comparisons to `None` which are not using the `is` operator.
///
/// ## Why is this bad?
/// According to [PEP 8], "Comparisons to singletons like None should always be done with
/// is or is not, never the equality operators."
///
/// ## Example
/// ```python
/// if arg != None:
///     pass
/// if None == arg:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if arg is not None:
///     pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
#[violation]
pub struct NoneComparison(EqCmpop);

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
/// According to [PEP 8], "Comparisons to singletons like None should always be done with
/// is or is not, never the equality operators."
///
/// ## Example
/// ```python
/// if arg == True:
///     pass
/// if False == arg:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if arg is True:
///     pass
/// if arg is False:
///     pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
#[violation]
pub struct TrueFalseComparison(bool, EqCmpop);

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
pub(crate) fn literal_comparisons(
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
        if let Some(op) = EqCmpop::try_from(*op) {
            if check_none_comparisons && is_const_none(comparator) {
                match op {
                    EqCmpop::Eq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), comparator.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(0, Cmpop::Is);
                        }
                        diagnostics.push(diagnostic);
                    }
                    EqCmpop::NotEq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), comparator.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(0, Cmpop::IsNot);
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if check_true_false_comparisons {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Bool(value),
                    kind: None,
                    range: _,
                }) = comparator
                {
                    match op {
                        EqCmpop::Eq => {
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison(*value, op),
                                comparator.range(),
                            );
                            if checker.patch(diagnostic.kind.rule()) {
                                bad_ops.insert(0, Cmpop::Is);
                            }
                            diagnostics.push(diagnostic);
                        }
                        EqCmpop::NotEq => {
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison(*value, op),
                                comparator.range(),
                            );
                            if checker.patch(diagnostic.kind.rule()) {
                                bad_ops.insert(0, Cmpop::IsNot);
                            }
                            diagnostics.push(diagnostic);
                        }
                    }
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

        if let Some(op) = EqCmpop::try_from(*op) {
            if check_none_comparisons && is_const_none(next) {
                match op {
                    EqCmpop::Eq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), next.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(idx, Cmpop::Is);
                        }
                        diagnostics.push(diagnostic);
                    }
                    EqCmpop::NotEq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), next.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(idx, Cmpop::IsNot);
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if check_true_false_comparisons {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Bool(value),
                    kind: None,
                    range: _,
                }) = next
                {
                    match op {
                        EqCmpop::Eq => {
                            let diagnostic =
                                Diagnostic::new(TrueFalseComparison(*value, op), next.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                bad_ops.insert(idx, Cmpop::Is);
                            }
                            diagnostics.push(diagnostic);
                        }
                        EqCmpop::NotEq => {
                            let diagnostic =
                                Diagnostic::new(TrueFalseComparison(*value, op), next.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                bad_ops.insert(idx, Cmpop::IsNot);
                            }
                            diagnostics.push(diagnostic);
                        }
                    }
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
            .copied()
            .collect::<Vec<_>>();
        let content = compare(left, &ops, comparators, checker.generator());
        for diagnostic in &mut diagnostics {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                content.to_string(),
                expr.range(),
            )));
        }
    }

    checker.diagnostics.extend(diagnostics);
}

use rustc_hash::FxHashMap;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast, CmpOp, Constant, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::generate_comparison;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum EqCmpOp {
    Eq,
    NotEq,
}

impl EqCmpOp {
    fn try_from(value: CmpOp) -> Option<EqCmpOp> {
        match value {
            CmpOp::Eq => Some(EqCmpOp::Eq),
            CmpOp::NotEq => Some(EqCmpOp::NotEq),
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
pub struct NoneComparison(EqCmpOp);

impl AlwaysAutofixableViolation for NoneComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpOp::Eq => format!("Comparison to `None` should be `cond is None`"),
            EqCmpOp::NotEq => format!("Comparison to `None` should be `cond is not None`"),
        }
    }

    fn autofix_title(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpOp::Eq => "Replace with `cond is None`".to_string(),
            EqCmpOp::NotEq => "Replace with `cond is not None`".to_string(),
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
pub struct TrueFalseComparison(bool, EqCmpOp);

impl AlwaysAutofixableViolation for TrueFalseComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpOp::Eq) => {
                format!("Comparison to `True` should be `cond is True` or `if cond:`")
            }
            (true, EqCmpOp::NotEq) => {
                format!("Comparison to `True` should be `cond is not True` or `if not cond:`")
            }
            (false, EqCmpOp::Eq) => {
                format!("Comparison to `False` should be `cond is False` or `if not cond:`")
            }
            (false, EqCmpOp::NotEq) => {
                format!("Comparison to `False` should be `cond is not False` or `if cond:`")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpOp::Eq) => "Replace with `cond is True`".to_string(),
            (true, EqCmpOp::NotEq) => "Replace with `cond is not True`".to_string(),
            (false, EqCmpOp::Eq) => "Replace with `cond is False`".to_string(),
            (false, EqCmpOp::NotEq) => "Replace with `cond is not False`".to_string(),
        }
    }
}

/// E711, E712
pub(crate) fn literal_comparisons(checker: &mut Checker, compare: &ast::ExprCompare) {
    // Mapping from (bad operator index) to (replacement operator). As we iterate
    // through the list of operators, we apply "dummy" fixes for each error,
    // then replace the entire expression at the end with one "real" fix, to
    // avoid conflicts.
    let mut bad_ops: FxHashMap<usize, CmpOp> = FxHashMap::default();
    let mut diagnostics: Vec<Diagnostic> = vec![];

    // Check `left`.
    let mut comparator = compare.left.as_ref();
    let [op, ..] = compare.ops.as_slice() else {
        return;
    };
    let [next, ..] = compare.comparators.as_slice() else {
        return;
    };

    if !helpers::is_constant_non_singleton(next) {
        if let Some(op) = EqCmpOp::try_from(*op) {
            if checker.enabled(Rule::NoneComparison) && is_const_none(comparator) {
                match op {
                    EqCmpOp::Eq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), comparator.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(0, CmpOp::Is);
                        }
                        diagnostics.push(diagnostic);
                    }
                    EqCmpOp::NotEq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), comparator.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(0, CmpOp::IsNot);
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if checker.enabled(Rule::TrueFalseComparison) {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Bool(value),
                    range: _,
                }) = comparator
                {
                    match op {
                        EqCmpOp::Eq => {
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison(*value, op),
                                comparator.range(),
                            );
                            if checker.patch(diagnostic.kind.rule()) {
                                bad_ops.insert(0, CmpOp::Is);
                            }
                            diagnostics.push(diagnostic);
                        }
                        EqCmpOp::NotEq => {
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison(*value, op),
                                comparator.range(),
                            );
                            if checker.patch(diagnostic.kind.rule()) {
                                bad_ops.insert(0, CmpOp::IsNot);
                            }
                            diagnostics.push(diagnostic);
                        }
                    }
                }
            }
        }
    }

    // Check each comparator in order.
    for (index, (op, next)) in compare
        .ops
        .iter()
        .zip(compare.comparators.iter())
        .enumerate()
    {
        if helpers::is_constant_non_singleton(comparator) {
            comparator = next;
            continue;
        }

        let Some(op) = EqCmpOp::try_from(*op) else {
            continue;
        };

        if checker.enabled(Rule::NoneComparison) && is_const_none(next) {
            match op {
                EqCmpOp::Eq => {
                    let diagnostic = Diagnostic::new(NoneComparison(op), next.range());
                    if checker.patch(diagnostic.kind.rule()) {
                        bad_ops.insert(index, CmpOp::Is);
                    }
                    diagnostics.push(diagnostic);
                }
                EqCmpOp::NotEq => {
                    let diagnostic = Diagnostic::new(NoneComparison(op), next.range());
                    if checker.patch(diagnostic.kind.rule()) {
                        bad_ops.insert(index, CmpOp::IsNot);
                    }
                    diagnostics.push(diagnostic);
                }
            }
        }

        if checker.enabled(Rule::TrueFalseComparison) {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Bool(value),
                range: _,
            }) = next
            {
                match op {
                    EqCmpOp::Eq => {
                        let diagnostic =
                            Diagnostic::new(TrueFalseComparison(*value, op), next.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(index, CmpOp::Is);
                        }
                        diagnostics.push(diagnostic);
                    }
                    EqCmpOp::NotEq => {
                        let diagnostic =
                            Diagnostic::new(TrueFalseComparison(*value, op), next.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            bad_ops.insert(index, CmpOp::IsNot);
                        }
                        diagnostics.push(diagnostic);
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
        let ops = compare
            .ops
            .iter()
            .enumerate()
            .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
            .copied()
            .collect::<Vec<_>>();
        let content = generate_comparison(
            &compare.left,
            &ops,
            &compare.comparators,
            compare.into(),
            checker.locator(),
        );
        for diagnostic in &mut diagnostics {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                content.to_string(),
                compare.range(),
            )));
        }
    }

    checker.diagnostics.extend(diagnostics);
}

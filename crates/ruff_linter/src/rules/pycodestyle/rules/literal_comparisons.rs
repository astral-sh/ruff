use rustc_hash::FxHashMap;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::generate_comparison;
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::fix::snippet::SourceCodeSnippet;

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
/// `is` or `is not`, never the equality operators."
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
/// ## Fix safety
///
/// This rule's fix is marked as unsafe, as it may alter runtime behavior when
/// used with libraries that override the `==`/`__eq__` or `!=`/`__ne__` operators.
/// In these cases, `is`/`is not` may not be equivalent to `==`/`!=`. For more
/// information, see [this issue].
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
/// [this issue]: https://github.com/astral-sh/ruff/issues/4560
#[violation]
pub struct NoneComparison(EqCmpOp);

impl AlwaysFixableViolation for NoneComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpOp::Eq => format!("Comparison to `None` should be `cond is None`"),
            EqCmpOp::NotEq => format!("Comparison to `None` should be `cond is not None`"),
        }
    }

    fn fix_title(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpOp::Eq => "Replace with `cond is None`".to_string(),
            EqCmpOp::NotEq => "Replace with `cond is not None`".to_string(),
        }
    }
}

/// ## What it does
/// Checks for equality comparisons to boolean literals.
///
/// ## Why is this bad?
/// [PEP 8] recommends against using the equality operators `==` and `!=` to
/// compare values to `True` or `False`.
///
/// Instead, use `if cond:` or `if not cond:` to check for truth values.
///
/// If you intend to check if a value is the boolean literal `True` or `False`,
/// consider using `is` or `is not` to check for identity instead.
///
/// ## Example
/// ```python
/// if foo == True:
///     ...
///
/// if bar == False:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if foo:
///     ...
///
/// if not bar:
///     ...
/// ```
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe, as it may alter runtime behavior when
/// used with libraries that override the `==`/`__eq__` or `!=`/`__ne__` operators.
/// In these cases, `is`/`is not` may not be equivalent to `==`/`!=`. For more
/// information, see [this issue].
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
/// [this issue]: https://github.com/astral-sh/ruff/issues/4560
#[violation]
pub struct TrueFalseComparison {
    value: bool,
    op: EqCmpOp,
    cond: Option<SourceCodeSnippet>,
}

impl AlwaysFixableViolation for TrueFalseComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TrueFalseComparison { value, op, cond } = self;
        let Some(cond) = cond else {
            return "Avoid equality comparisons to `True` or `False`".to_string();
        };
        let cond = cond.truncated_display();
        match (value, op) {
            (true, EqCmpOp::Eq) => {
                format!("Avoid equality comparisons to `True`; use `if {cond}:` for truth checks")
            }
            (true, EqCmpOp::NotEq) => {
                format!(
                    "Avoid inequality comparisons to `True`; use `if not {cond}:` for false checks"
                )
            }
            (false, EqCmpOp::Eq) => {
                format!(
                    "Avoid equality comparisons to `False`; use `if not {cond}:` for false checks"
                )
            }
            (false, EqCmpOp::NotEq) => {
                format!(
                    "Avoid inequality comparisons to `False`; use `if {cond}:` for truth checks"
                )
            }
        }
    }

    fn fix_title(&self) -> String {
        let TrueFalseComparison { value, op, cond } = self;
        let Some(cond) = cond.as_ref().and_then(|cond| cond.full_display()) else {
            return "Replace comparison".to_string();
        };
        match (value, op) {
            (true, EqCmpOp::Eq) => format!("Replace with `{cond}`"),
            (true, EqCmpOp::NotEq) => format!("Replace with `not {cond}`"),
            (false, EqCmpOp::Eq) => format!("Replace with `not {cond}`"),
            (false, EqCmpOp::NotEq) => format!("Replace with `{cond}`"),
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
    let [op, ..] = &*compare.ops else {
        return;
    };
    let [next, ..] = &*compare.comparators else {
        return;
    };

    if !helpers::is_constant_non_singleton(next) {
        if let Some(op) = EqCmpOp::try_from(*op) {
            if checker.enabled(Rule::NoneComparison) && comparator.is_none_literal_expr() {
                match op {
                    EqCmpOp::Eq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), comparator.range());
                        bad_ops.insert(0, CmpOp::Is);
                        diagnostics.push(diagnostic);
                    }
                    EqCmpOp::NotEq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), comparator.range());
                        bad_ops.insert(0, CmpOp::IsNot);
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if checker.enabled(Rule::TrueFalseComparison) {
                if let Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) = comparator {
                    match op {
                        EqCmpOp::Eq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(checker.locator().slice(next)))
                            } else {
                                None
                            };
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(0, CmpOp::Is);
                            diagnostics.push(diagnostic);
                        }
                        EqCmpOp::NotEq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(checker.locator().slice(next)))
                            } else {
                                None
                            };
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(0, CmpOp::IsNot);
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

        if let Some(op) = EqCmpOp::try_from(*op) {
            if checker.enabled(Rule::NoneComparison) && next.is_none_literal_expr() {
                match op {
                    EqCmpOp::Eq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), next.range());
                        bad_ops.insert(index, CmpOp::Is);
                        diagnostics.push(diagnostic);
                    }
                    EqCmpOp::NotEq => {
                        let diagnostic = Diagnostic::new(NoneComparison(op), next.range());
                        bad_ops.insert(index, CmpOp::IsNot);
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if checker.enabled(Rule::TrueFalseComparison) {
                if let Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) = next {
                    match op {
                        EqCmpOp::Eq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(
                                    checker.locator().slice(comparator),
                                ))
                            } else {
                                None
                            };
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(index, CmpOp::Is);
                            diagnostics.push(diagnostic);
                        }
                        EqCmpOp::NotEq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(
                                    checker.locator().slice(comparator),
                                ))
                            } else {
                                None
                            };
                            let diagnostic = Diagnostic::new(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(index, CmpOp::IsNot);
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
            checker.comment_ranges(),
            checker.locator(),
        );
        for diagnostic in &mut diagnostics {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                content.to_string(),
                compare.range(),
            )));
        }
    }

    checker.diagnostics.extend(diagnostics);
}

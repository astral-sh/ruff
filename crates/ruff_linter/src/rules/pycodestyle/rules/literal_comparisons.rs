use ruff_python_ast::parenthesize::parenthesized_range;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{self, generate_comparison};
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
#[derive(ViolationMetadata)]
pub(crate) struct NoneComparison(EqCmpOp);

impl AlwaysFixableViolation for NoneComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpOp::Eq => "Comparison to `None` should be `cond is None`".to_string(),
            EqCmpOp::NotEq => "Comparison to `None` should be `cond is not None`".to_string(),
        }
    }

    fn fix_title(&self) -> String {
        let NoneComparison(op) = self;
        let title = match op {
            EqCmpOp::Eq => "Replace with `cond is None`",
            EqCmpOp::NotEq => "Replace with `cond is not None`",
        };
        title.to_string()
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
#[derive(ViolationMetadata)]
pub(crate) struct TrueFalseComparison {
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

fn is_redundant_boolean_comparison(op: CmpOp, comparator: &Expr) -> Option<bool> {
    let value = comparator.as_boolean_literal_expr()?.value;
    match op {
        CmpOp::Is | CmpOp::Eq => Some(value),
        CmpOp::IsNot | CmpOp::NotEq => Some(!value),
        _ => None,
    }
}

fn generate_redundant_comparison(
    compare: &ast::ExprCompare,
    comment_ranges: &ruff_python_trivia::CommentRanges,
    source: &str,
    comparator: &Expr,
    kind: bool,
    needs_wrap: bool,
) -> String {
    let comparator_range =
        parenthesized_range(comparator.into(), compare.into(), comment_ranges, source)
            .unwrap_or(comparator.range());

    let comparator_str = &source[comparator_range];

    let result = if kind {
        comparator_str.to_string()
    } else {
        format!("not {comparator_str}")
    };

    if needs_wrap {
        format!("({result})")
    } else {
        result
    }
}

/// E711, E712
pub(crate) fn literal_comparisons(checker: &Checker, compare: &ast::ExprCompare) {
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
    for (index, (op, next)) in compare.ops.iter().zip(&compare.comparators).enumerate() {
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
                            if let Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                                value: comparator_value,
                                ..
                            }) = comparator
                            {
                                if value == comparator_value {
                                    continue;
                                }
                            }

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
        let ops = compare
            .ops
            .iter()
            .enumerate()
            .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
            .copied()
            .collect::<Vec<_>>();

        let comment_ranges = checker.comment_ranges();
        let source = checker.source();

        let content = match (&*compare.ops, &*compare.comparators) {
            ([op], [comparator]) => {
                if let Some(kind) = is_redundant_boolean_comparison(*op, &compare.left) {
                    let needs_wrap = compare.left.range().start() != compare.range().start();
                    generate_redundant_comparison(
                        compare,
                        comment_ranges,
                        source,
                        comparator,
                        kind,
                        needs_wrap,
                    )
                } else if let Some(kind) = is_redundant_boolean_comparison(*op, comparator) {
                    let needs_wrap = comparator.range().end() != compare.range().end();
                    generate_redundant_comparison(
                        compare,
                        comment_ranges,
                        source,
                        &compare.left,
                        kind,
                        needs_wrap,
                    )
                } else {
                    generate_comparison(
                        &compare.left,
                        &ops,
                        &compare.comparators,
                        compare.into(),
                        comment_ranges,
                        source,
                    )
                }
            }
            _ => generate_comparison(
                &compare.left,
                &ops,
                &compare.comparators,
                compare.into(),
                comment_ranges,
                source,
            ),
        };

        for diagnostic in &mut diagnostics {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                content.to_string(),
                compare.range(),
            )));
        }
    }

    for diagnostic in diagnostics {
        checker.report_diagnostic(diagnostic);
    }
}

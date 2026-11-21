use ruff_python_ast::token::{Tokens, parenthesized_range};
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{self, generate_comparison};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::{Category, Rule};
use crate::fix::snippet::SourceCodeSnippet;
use crate::{AlwaysFixableViolation, Edit, Fix, FixAvailability, Violation};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum EqCmpOp {
    Eq,
    NotEq,
}

/// The kind of singleton involved in a flagged comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BadComparison {
    /// A comparison to `None` (E711), which can always be rewritten to `is`/`is not`.
    None,
    /// A comparison to `True` or `False` (E712), which must never be rewritten to
    /// `is`/`is not` (see <https://github.com/astral-sh/ruff/issues/4560>).
    Boolean,
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
#[violation_metadata(stable_since = "v0.0.28", category = Category::Pedantic)]
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
/// used with libraries that override the `==`/`__eq__` or `!=`/`__ne__` operators
/// (e.g., NumPy arrays, pandas objects, or SQLAlchemy columns), for which an
/// equality comparison is not equivalent to a truthiness check. For more
/// information, see [this issue].
///
/// This rule never rewrites a comparison to `True`/`False` into an `is`/`is not`
/// comparison, as that changes the meaning of the comparison for such types. As a
/// result, chained comparisons (e.g., `x == True != False`) are reported, but no
/// fix is offered for them.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
/// [this issue]: https://github.com/astral-sh/ruff/issues/4560
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.28", category = Category::Pedantic)]
pub(crate) struct TrueFalseComparison {
    value: bool,
    op: EqCmpOp,
    cond: Option<SourceCodeSnippet>,
}

impl Violation for TrueFalseComparison {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let TrueFalseComparison { value, op, cond } = self;
        let Some(cond) = cond else {
            return "Avoid equality comparisons to `True` or `False`".to_string();
        };
        let cond = cond.truncated_display();
        match (value, op) {
            (true, EqCmpOp::Eq) => {
                format!("Avoid equality comparisons to `True`; use `{cond}:` for truth checks")
            }
            (true, EqCmpOp::NotEq) => {
                format!(
                    "Avoid inequality comparisons to `True`; use `not {cond}:` for false checks"
                )
            }
            (false, EqCmpOp::Eq) => {
                format!("Avoid equality comparisons to `False`; use `not {cond}:` for false checks")
            }
            (false, EqCmpOp::NotEq) => {
                format!("Avoid inequality comparisons to `False`; use `{cond}:` for truth checks")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let TrueFalseComparison { value, op, cond } = self;
        // Chained comparisons (e.g., `x == True != False`) have no `cond` and are
        // not fixable: they must not be rewritten to `is`/`is not`.
        let cond = cond.as_ref().and_then(|cond| cond.full_display())?;
        Some(match (value, op) {
            (true, EqCmpOp::Eq) => format!("Replace with `{cond}`"),
            (true, EqCmpOp::NotEq) => format!("Replace with `not {cond}`"),
            (false, EqCmpOp::Eq) => format!("Replace with `not {cond}`"),
            (false, EqCmpOp::NotEq) => format!("Replace with `{cond}`"),
        })
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
    tokens: &Tokens,
    source: &str,
    comparator: &Expr,
    kind: bool,
    needs_wrap: bool,
) -> String {
    let comparator_range = parenthesized_range(comparator.into(), compare.into(), tokens)
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
    // The indices of operators that compare against `True`/`False` (E712). These
    // must never be rewritten to `is`/`is not`.
    let mut boolean_ops: FxHashSet<usize> = FxHashSet::default();
    let mut diagnostics = vec![];

    // Check `left`.
    let left = compare.first_operand();
    let comparators = compare.comparators();
    let mut comparator = left;
    let op = compare.first_operator();
    let next = compare.second_operand();

    if !helpers::is_constant_non_singleton(next) {
        if let Some(op) = EqCmpOp::try_from(op) {
            if checker.is_rule_enabled(Rule::NoneComparison) && comparator.is_none_literal_expr() {
                match op {
                    EqCmpOp::Eq => {
                        let diagnostic =
                            checker.report_diagnostic(NoneComparison(op), comparator.range());
                        bad_ops.insert(0, CmpOp::Is);
                        diagnostics.push((BadComparison::None, diagnostic));
                    }
                    EqCmpOp::NotEq => {
                        let diagnostic =
                            checker.report_diagnostic(NoneComparison(op), comparator.range());
                        bad_ops.insert(0, CmpOp::IsNot);
                        diagnostics.push((BadComparison::None, diagnostic));
                    }
                }
            }

            if checker.is_rule_enabled(Rule::TrueFalseComparison) {
                if let Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) = comparator {
                    match op {
                        EqCmpOp::Eq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(checker.locator().slice(next)))
                            } else {
                                None
                            };
                            let diagnostic = checker.report_diagnostic(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(0, CmpOp::Is);
                            boolean_ops.insert(0);
                            diagnostics.push((BadComparison::Boolean, diagnostic));
                        }
                        EqCmpOp::NotEq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(checker.locator().slice(next)))
                            } else {
                                None
                            };
                            let diagnostic = checker.report_diagnostic(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(0, CmpOp::IsNot);
                            boolean_ops.insert(0);
                            diagnostics.push((BadComparison::Boolean, diagnostic));
                        }
                    }
                }
            }
        }
    }

    // Check each comparator in order.
    for (index, (op, next)) in compare.ops.iter().zip(comparators).enumerate() {
        if helpers::is_constant_non_singleton(comparator) {
            comparator = next;
            continue;
        }

        if let Some(op) = EqCmpOp::try_from(*op) {
            if checker.is_rule_enabled(Rule::NoneComparison) && next.is_none_literal_expr() {
                match op {
                    EqCmpOp::Eq => {
                        let diagnostic =
                            checker.report_diagnostic(NoneComparison(op), next.range());
                        bad_ops.insert(index, CmpOp::Is);
                        diagnostics.push((BadComparison::None, diagnostic));
                    }
                    EqCmpOp::NotEq => {
                        let diagnostic =
                            checker.report_diagnostic(NoneComparison(op), next.range());
                        bad_ops.insert(index, CmpOp::IsNot);
                        diagnostics.push((BadComparison::None, diagnostic));
                    }
                }
            }

            if checker.is_rule_enabled(Rule::TrueFalseComparison) {
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
                            let diagnostic = checker.report_diagnostic(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(index, CmpOp::Is);
                            boolean_ops.insert(index);
                            diagnostics.push((BadComparison::Boolean, diagnostic));
                        }
                        EqCmpOp::NotEq => {
                            let cond = if compare.ops.len() == 1 {
                                Some(SourceCodeSnippet::from_str(
                                    checker.locator().slice(comparator),
                                ))
                            } else {
                                None
                            };
                            let diagnostic = checker.report_diagnostic(
                                TrueFalseComparison {
                                    value: *value,
                                    op,
                                    cond,
                                },
                                compare.range(),
                            );
                            bad_ops.insert(index, CmpOp::IsNot);
                            boolean_ops.insert(index);
                            diagnostics.push((BadComparison::Boolean, diagnostic));
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
        let tokens = checker.tokens();
        let source = checker.source();

        if let ([op], [comparator]) = (&*compare.ops, comparators) {
            // A single comparison: replace the entire expression with one fix.
            let ops = compare
                .ops
                .iter()
                .enumerate()
                .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
                .copied()
                .collect::<Vec<_>>();

            let content = if let Some(kind) = is_redundant_boolean_comparison(*op, left) {
                let needs_wrap = left.range().start() != compare.range().start();
                generate_redundant_comparison(compare, tokens, source, comparator, kind, needs_wrap)
            } else if let Some(kind) = is_redundant_boolean_comparison(*op, comparator) {
                let needs_wrap = comparator.range().end() != compare.range().end();
                generate_redundant_comparison(compare, tokens, source, left, kind, needs_wrap)
            } else {
                generate_comparison(left, &ops, comparators, compare.into(), tokens, source)
            };

            for (_, diagnostic) in &mut diagnostics {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    content.clone(),
                    compare.range(),
                )));
            }
        } else {
            // A chained comparison (e.g., `x == True != False`). Comparisons to
            // `True`/`False` (E712) must not be rewritten to `is`/`is not`, as that
            // changes the meaning of the comparison for types that overload `__eq__`
            // (e.g., NumPy arrays, pandas objects, or SQLAlchemy columns; see
            // https://github.com/astral-sh/ruff/issues/4560), and a chained comparison
            // cannot be rewritten as a truthiness check. Only comparisons to `None`
            // (E711) are fixed, by rewriting just those operators to `is`/`is not`.
            let none_ops = compare
                .ops
                .iter()
                .enumerate()
                .map(|(idx, op)| match bad_ops.get(&idx) {
                    Some(replacement) if !boolean_ops.contains(&idx) => replacement,
                    _ => op,
                })
                .copied()
                .collect::<Vec<_>>();

            for (kind, diagnostic) in &mut diagnostics {
                if *kind == BadComparison::None {
                    let content = generate_comparison(
                        left,
                        &none_ops,
                        comparators,
                        compare.into(),
                        tokens,
                        source,
                    );
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                        content,
                        compare.range(),
                    )));
                }
            }
        }
    }
}

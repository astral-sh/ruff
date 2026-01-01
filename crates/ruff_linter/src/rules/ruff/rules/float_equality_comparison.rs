use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::{
    SemanticModel,
    analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType},
};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for comparisons between floating-point values using `==` or `!=`.
///
/// ## Why is this bad?
/// Directly comparing floats can produce unreliable results due to the
/// inherent imprecision of floating-point arithmetic.
///
/// ## When to use `math.isclose()` vs `numpy.isclose()`
///
/// **Use `math.isclose()` for scalar values:**
/// - Comparing individual float numbers
/// - Working with regular Python variables (not arrays)
/// - When you need a single `True`/`False` result
///
/// **Use `numpy.isclose()` for array-like objects:**
/// - Comparing `pandas` Series, `numpy` arrays, or other vectorized objects
/// - When you need element-wise comparison of arrays
/// - Working in data science contexts with vectorized operations
///
/// ## Example
/// ```python
/// assert 0.1 + 0.2 == 0.3  # AssertionError
/// ```
/// Use instead:
/// ```python
/// import math
///
/// # Scalar comparison
/// assert math.isclose(0.1 + 0.2, 0.3, abs_tol=1e-9)
/// ```
/// ## References
/// - [Python documentation: `math.isclose`](https://docs.python.org/3/library/math.html#math.isclose)
/// - [NumPy documentation: `numpy.isclose`](https://numpy.org/doc/stable/reference/generated/numpy.isclose.html#numpy-isclose)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct FloatEqualityComparison<'a> {
    pub left: &'a str,
    pub right: &'a str,
    pub operand: &'a str,
}

impl Violation for FloatEqualityComparison<'_> {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FloatEqualityComparison {
            left,
            right,
            operand,
        } = self;
        format!(
            "Comparison `{left} {operand} {right}` should be replaced by `math.isclose()` or `numpy.isclose()`"
        )
    }
}

/// RUF070
pub(crate) fn float_equality_comparison(checker: &Checker, compare: &ast::ExprCompare) {
    let locator = checker.locator();
    let semantic = checker.semantic();

    for (left, right, operand) in std::iter::once(&*compare.left)
        .chain(&compare.comparators)
        .tuple_windows()
        .zip(&compare.ops)
        .filter(|(_, op)| matches!(op, CmpOp::Eq | CmpOp::NotEq))
        .filter(|((left, right), _)| has_float(left, semantic) || has_float(right, semantic))
        .map(|((left, right), op)| (left, right, op))
    {
        checker.report_diagnostic(
            FloatEqualityComparison {
                left: locator.slice(left.range()),
                right: locator.slice(right.range()),
                operand: operand.as_str(),
            },
            TextRange::new(left.start(), right.end()),
        );
    }
}

fn has_float(expr: &Expr, semantic: &SemanticModel) -> bool {
    match ResolvedPythonType::from(expr) {
        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float)) => true,
        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Complex)) => true,
        _ => {
            match expr {
                Expr::Call(ast::ExprCall { func, .. }) => {
                    semantic.match_builtin_expr(func, "float")
                }
                // Division always returns float in Python
                // https://docs.python.org/3/tutorial/introduction.html#numbers
                Expr::BinOp(ast::ExprBinOp {
                    left,
                    right,
                    op: ast::Operator::Div,
                    ..
                }) => {
                    // Only trigger for numeric divisions, not path operations
                    is_numeric_expr(left) || is_numeric_expr(right)
                }
                _ => false,
            }
        }
    }
}

fn is_numeric_expr(expr: &Expr) -> bool {
    match expr {
        Expr::NumberLiteral(_) => true,
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            is_numeric_expr(left) || is_numeric_expr(right)
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => is_numeric_expr(operand),
        _ => false,
    }
}

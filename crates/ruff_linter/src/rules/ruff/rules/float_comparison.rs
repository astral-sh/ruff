use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

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
#[violation_metadata(preview_since = "0.14.3")]
pub(crate) struct FloatComparison {
    pub left: String,
    pub right: String,
    pub operand: String,
}

impl Violation for FloatComparison {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Comparison `{} {} {}` should be replaced by `math.isclose()` or `numpy.isclose()`",
            self.left, self.operand, self.right,
        )
    }
}

/// RUF067
pub(crate) fn float_comparison(checker: &Checker, compare: &ast::ExprCompare) {
    let locator = checker.locator();

    for (left, right, operand) in std::iter::once(&*compare.left)
        .chain(&compare.comparators)
        .tuple_windows()
        .zip(&compare.ops)
        .filter(|(_, op)| matches!(op, CmpOp::Eq | CmpOp::NotEq))
        .filter(|((left, right), _)| has_float(left) || has_float(right))
        .map(|((left, right), op)| (left, right, op))
    {
        checker.report_diagnostic(
            FloatComparison {
                left: locator.slice(left.range()).to_string(),
                right: locator.slice(right.range()).to_string(),
                operand: operand.to_string(),
            },
            TextRange::new(left.start(), right.end()),
        );
    }
}

fn has_float(expr: &Expr) -> bool {
    match expr {
        Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
            matches!(value, ast::Number::Float(_))
        }
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => has_float(left) || has_float(right),
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => has_float(operand),
        _ => false,
    }
}

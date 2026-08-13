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
use crate::linter::float::is_infinity_string_literal;

/// ## What it does
/// Checks for comparisons between floating-point values using `==` or `!=`.
///
/// ## Why is this bad?
/// Directly comparing floating-point numbers for exact equality or inequality
/// can lead to incorrect and non-deterministic outcomes. This is because
/// floating-point values are finite binary approximations of real numbers.
/// Many decimal values, such as `0.1`, cannot be represented exactly in binary,
/// leading to small rounding errors. Furthermore, the results of arithmetic
/// operations are subject to rounding at each step, and the order of operations
/// can influence the final result. Consequently, two mathematically equivalent
/// computations may yield binary floating-point values that differ by a tiny
/// margin. Relying on exact comparison treats these semantically equal values
/// as different, breaking logical correctness.
///
/// ## How to fix
/// For general Python code, use tolerance-based comparison functions from the
/// standard library:
///
/// - Use `math.isclose()` for scalar comparisons
/// - Use `cmath.isclose()` for complex numbers comparisons
/// - Use `unittest.assertAlmostEqual()` in tests with `unittest`
///
/// Note that `math.isclose()` and `cmath.isclose()` have a default absolute tolerance of zero, so
/// when comparing values near zero, you must explicitly specify an `abs_tol`
/// parameter.
///
/// Many frameworks and libraries provide their own specialized functions for
/// floating-point comparison, often with different default tolerances optimized
/// for their specific use cases:
///
/// - For `NumPy` arrays: use `numpy.isclose()` or `numpy.allclose()`
/// - For `PyTorch` tensors: use `torch.isclose()`
/// - Check your framework's / library's documentation for equivalent functions
///
/// For scenarios requiring exact decimal arithmetic, consider using the
/// `Decimal` class from the `decimal` module instead of floating-point numbers.
///
/// ## Example
///
/// ```python
/// assert 0.1 + 0.2 == 0.3  # AssertionError
///
/// assert complex(0.3, 0.1) == complex(0.1 + 0.2, 0.1)  # AssertionError
/// ```
///
/// Use instead:
///
/// ```python
/// import cmath
/// import math
///
/// # Scalar comparison
/// assert math.isclose(0.1 + 0.2, 0.3, abs_tol=1e-9)
/// # Complex numbers comparison
/// assert cmath.isclose(complex(0.3, 0.1), complex(0.1 + 0.2, 0.1), abs_tol=1e-9)
/// ```
///
/// ## Ecosystem-specific alternatives
///
/// ```python
/// import numpy as np
///
/// arr1 = np.sum(np.array([0.1, 0.2]))
///
/// assert np.all(arr1 == 0.3)  # AssertionError
/// ```
///
/// Use instead:
///
/// ```python
/// import numpy as np
///
/// arr1 = np.sum(np.array([0.1, 0.2]))
///
/// assert np.all(np.isclose(arr1, 0.3, rtol=1e-9, atol=1e-9))
/// # or
/// assert np.allclose(arr1, 0.3, rtol=1e-9, atol=1e-9)
/// ```
///
/// ## References
/// - [Python documentation: Floating Point Arithmetic: Issues and Limitations](https://docs.python.org/3/tutorial/floatingpoint.html#floating-point-arithmetic-issues-and-limitations)
/// - [Decimal fixed point and floating point arithmetic](https://docs.python.org/3/library/decimal.html#module-decimal)
/// - [Python documentation: `math.isclose`](https://docs.python.org/3/library/math.html#math.isclose)
/// - [Python documentation: `cmath.isclose`](https://docs.python.org/3/library/cmath.html#cmath.isclose)
/// - [Python documentation: `unittest.assertAlmostEqual`](https://docs.python.org/3/library/unittest.html#unittest.TestCase.assertAlmostEqual)
/// - [NumPy documentation: `numpy.isclose`](https://numpy.org/doc/stable/reference/generated/numpy.isclose.html#numpy-isclose)
/// - [NumPy documentation: `numpy.allclose`](https://numpy.org/doc/stable/reference/generated/numpy.allclose.html#numpy-allclose)
/// - [PyTorch documentation: `torch.isclose`](https://docs.pytorch.org/docs/stable/generated/torch.isclose.html#torch-isclose)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.1")]
pub(crate) struct FloatEqualityComparison<'a> {
    left: &'a str,
    right: &'a str,
    operator: &'a str,
}

impl Violation for FloatEqualityComparison<'_> {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FloatEqualityComparison {
            left,
            right,
            operator,
        } = self;
        format!("Unreliable floating point equality comparison `{left} {operator} {right}`")
    }
}

/// RUF069
pub(crate) fn float_equality_comparison(checker: &Checker, compare: &ast::ExprCompare) {
    let locator = checker.locator();
    let semantic = checker.semantic();

    for (left, right, operator) in std::iter::once(&*compare.left)
        .chain(&compare.comparators)
        .tuple_windows()
        .zip(&compare.ops)
        .filter(|(_, op)| matches!(op, CmpOp::Eq | CmpOp::NotEq))
        .filter(|((left, right), _)| {
            if should_skip_comparison(left, semantic) || should_skip_comparison(right, semantic) {
                return false;
            }

            has_float(left, semantic) || has_float(right, semantic)
        })
        .map(|((left, right), op)| (left, right, op))
    {
        checker.report_diagnostic(
            FloatEqualityComparison {
                left: locator.slice(left.range()),
                right: locator.slice(right.range()),
                operator: operator.as_str(),
            },
            TextRange::new(left.start(), right.end()),
        );
    }
}

fn has_float(expr: &Expr, semantic: &SemanticModel) -> bool {
    match ResolvedPythonType::from(expr) {
        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float | NumberLike::Complex)) => {
            true
        }
        _ => {
            match expr {
                Expr::Call(ast::ExprCall { func, .. }) => ["float", "complex"]
                    .iter()
                    .any(|s| semantic.match_builtin_expr(func, s)),
                Expr::BinOp(ast::ExprBinOp {
                    left, right, op, ..
                }) => {
                    // Division always returns float in Python
                    // https://docs.python.org/3/tutorial/introduction.html#numbers
                    match op {
                        ast::Operator::Div => {
                            // Only trigger for numeric divisions, not path operations
                            // Ex) `Path(__file__).parents[2] / "text.txt"`
                            is_numeric_expr(left) || is_numeric_expr(right)
                        }
                        _ => has_float(left, semantic) || has_float(right, semantic),
                    }
                }
                Expr::Named(ast::ExprNamed { value, .. }) => has_float(value, semantic),
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
        Expr::Named(ast::ExprNamed { value, .. }) => is_numeric_expr(value),
        _ => false,
    }
}

fn should_skip_comparison(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
            // Skip `pytest.approx`
            if let Some(qualified_name) = semantic.resolve_qualified_name(func) {
                if matches!(qualified_name.segments(), ["pytest", "approx"]) {
                    return true;
                }
            }

            // Skip `float("inf" / "-inf" / etc.)` / `complex("inf" / "-inf" / etc.)`
            if ["float", "complex"]
                .iter()
                .any(|s| semantic.match_builtin_expr(func, s))
            {
                return arguments.args.len() == 1
                    && arguments.keywords.is_empty()
                    && is_infinity_string_literal(&arguments.args[0]).is_some();
            }

            false
        }

        // Skip `inf` when imported from `math`, `cmath`, `numpy` or `torch`
        // and `infj` from `cmath`
        _ => semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["math" | "numpy" | "torch", "inf"] | ["cmath", "inf" | "infj"]
                )
            }),
    }
}

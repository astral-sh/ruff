use num_traits::One;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, Ranged};

use crate::checkers::ast::Checker;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};
use ruff_diagnostics::Diagnostic;

/// ## What it does
/// Check for the use of `.nunique()` for determining if a Pandas Series is constant.
///
/// ## Why is this bad?
/// Let's take the example of a series of increasing integers (1, 2, 3, 4) of length `n`.
/// While walking through the series, we already know at observing the second value that
/// the series is not unique. However, using `.nunique()`, we will count till the end of
/// the series before returning the result. This is computationally inefficient.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// data = pd.Series(range(1000))
/// if data.nunique() <= 1:
///     print("Series is constant")
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// data = pd.Series(range(1000))
/// v = s.to_numpy()
/// if v.shape[0] == 0 or (s[0] == s).all():
///     print("Series is constant")
/// ```
///
/// The [Pandas Cookbook](https://pandas.pydata.org/docs/user_guide/cookbook.html#constant-series) provides additional examples in case that the Series contain missing values.
///
/// ## References
/// - [Pandas documentation: `nunique`](https://pandas.pydata.org/docs/reference/api/pandas.Series.nunique.html)
#[violation]
pub struct PandasNuniqueConstantSeriesCheck;

impl Violation for PandasNuniqueConstantSeriesCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `series.nunique()` for checking that a series is constant is inefficient")
    }
}

/// Return `true` if an [`Expr`] is a constant `1`.
fn is_constant_one(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            Constant::Int(int) => int.is_one(),
            _ => false,
        },
        _ => false,
    }
}

/// PD801
pub(crate) fn pandas_nunique_constant_series_check(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    // Operators may be ==, !=, <=, >
    if !matches!(op, CmpOp::Eq | CmpOp::NotEq | CmpOp::LtE | CmpOp::Gt,) {
        return;
    }

    // Right should be the integer 1
    if !is_constant_one(right) {
        return;
    }

    // Check if call is .nuniuqe()
    let Expr::Call(ast::ExprCall {func, .. }) = left else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return;
    };

    if attr.as_str() != "nunique" {
        return;
    }

    // Avoid flagging on non-Series (e.g., `{"a": 1}.at[0]`).
    if !matches!(
        test_expression(value, checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        PandasNuniqueConstantSeriesCheck,
        expr.range(),
    ));
}

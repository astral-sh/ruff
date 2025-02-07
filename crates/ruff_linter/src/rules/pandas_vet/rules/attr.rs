use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};

/// ## What it does
/// Checks for uses of `.values` on Pandas Series and Index objects.
///
/// ## Why is this bad?
/// The `.values` attribute is ambiguous as its return type is unclear. As
/// such, it is no longer recommended by the Pandas documentation.
///
/// Instead, use `.to_numpy()` to return a NumPy array, or `.array` to return a
/// Pandas `ExtensionArray`.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// animals = pd.read_csv("animals.csv").values  # Ambiguous.
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// animals = pd.read_csv("animals.csv").to_numpy()  # Explicit.
/// ```
///
/// ## References
/// - [Pandas documentation: Accessing the values in a Series or Index](https://pandas.pydata.org/pandas-docs/stable/whatsnew/v0.24.0.html#accessing-the-values-in-a-series-or-index)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotValues;

impl Violation for PandasUseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `.to_numpy()` instead of `.values`".to_string()
    }
}

/// PD011
pub(crate) fn attr(checker: &Checker, attribute: &ast::ExprAttribute) {
    if !checker.semantic().seen_module(Modules::PANDAS) {
        return;
    }

    // Avoid, e.g., `x.values = y`.
    if !attribute.ctx.is_load() {
        return;
    }

    // Check for, e.g., `df.values`.
    if attribute.attr.as_str() != "values" {
        return;
    }

    // Avoid flagging on function calls (e.g., `df.values()`).
    if checker
        .semantic()
        .current_expression_parent()
        .is_some_and(Expr::is_call_expr)
    {
        return;
    }

    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`), and on irrelevant bindings
    // (like imports).
    if !matches!(
        test_expression(attribute.value.as_ref(), checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(PandasUseOfDotValues, attribute.range()));
}

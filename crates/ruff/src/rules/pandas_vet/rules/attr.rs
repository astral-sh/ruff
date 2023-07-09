use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};

/// ## What it does
/// Checks for uses of `.values` on a Pandas Series or Index.
///
/// ## Why is this bad?
/// `.values` is ambiguous as it is unclear what it returns; thus, it is no
/// longer recommended by the Pandas documentation. Instead, to return a NumPy
/// array, use `.to_numpy()`. Or, to return a Pandas array, use `.array`.
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
#[violation]
pub struct PandasUseOfDotValues;

impl Violation for PandasUseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.to_numpy()` instead of `.values`")
    }
}

pub(crate) fn attr(checker: &mut Checker, attr: &str, value: &Expr, attr_expr: &Expr) {
    let rules = &checker.settings.rules;
    let violation: DiagnosticKind = match attr {
        "values" if rules.enabled(Rule::PandasUseOfDotValues) => PandasUseOfDotValues.into(),
        _ => return,
    };

    // Avoid flagging on function calls (e.g., `df.values()`).
    if let Some(parent) = checker.semantic().expr_parent() {
        if matches!(parent, Expr::Call(_)) {
            return;
        }
    }

    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`), and on irrelevant bindings
    // (like imports).
    if !matches!(
        test_expression(value, checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, attr_expr.range()));
}

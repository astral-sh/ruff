use ruff_python_ast::{self as ast, Expr, ExprContext};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};

/// ## What it does
/// Checks for uses of `.values` on Pandas Series and Index objects.
///
/// ## Why is this bad?
/// The `.values` attribute is ambiguous as it's return type is unclear. As
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
#[violation]
pub struct PandasUseOfDotValues;

impl Violation for PandasUseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.to_numpy()` instead of `.values`")
    }
}

pub(crate) fn attr(checker: &mut Checker, attribute: &ast::ExprAttribute) {
    // Avoid, e.g., `x.values = y`.
    if matches!(attribute.ctx, ExprContext::Store | ExprContext::Del) {
        return;
    }

    let violation: DiagnosticKind = match attribute.attr.as_str() {
        // PD011
        "values" if checker.settings.rules.enabled(Rule::PandasUseOfDotValues) => {
            PandasUseOfDotValues.into()
        }
        _ => return,
    };

    // Avoid flagging on function calls (e.g., `df.values()`).
    if let Some(parent) = checker.semantic().current_expression_parent() {
        if matches!(parent, Expr::Call(_)) {
            return;
        }
    }

    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`), and on irrelevant bindings
    // (like imports).
    if !matches!(
        test_expression(attribute.value.as_ref(), checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, attribute.range()));
}

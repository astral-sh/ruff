use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for assignments to the variable `df`.
///
/// ## Why is this bad?
/// Although `df` is a common variable name for a Pandas DataFrame, it's not a
/// great variable name for production code, as it's non-descriptive and
/// prone to name conflicts.
///
/// Instead, use a more descriptive variable name.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// df = pd.read_csv("animals.csv")
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// animals = pd.read_csv("animals.csv")
/// ```
#[violation]
pub struct PandasDfVariableName;

impl Violation for PandasDfVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid using the generic variable name `df` for DataFrames")
    }
}

/// PD901
pub(crate) fn assignment_to_df(targets: &[Expr]) -> Option<Diagnostic> {
    let [target] = targets else {
        return None;
    };
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return None;
    };
    if id != "df" {
        return None;
    }
    Some(Diagnostic::new(PandasDfVariableName, target.range()))
}

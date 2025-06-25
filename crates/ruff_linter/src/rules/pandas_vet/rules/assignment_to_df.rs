use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::{Violation, checkers::ast::Checker};

/// ## Deprecated
///
/// This rule has been deprecated as it's highly opinionated and overly strict in most cases.
///
/// ## What it does
/// Checks for assignments to the variable `df`.
///
/// ## Why is this bad?
/// Although `df` is a common variable name for a Pandas `DataFrame`, it's not a
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
#[derive(ViolationMetadata)]
pub(crate) struct PandasDfVariableName;

impl Violation for PandasDfVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid using the generic variable name `df` for DataFrames".to_string()
    }
}

/// PD901
pub(crate) fn assignment_to_df(checker: &Checker, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };
    if id != "df" {
        return;
    }
    checker.report_diagnostic(PandasDfVariableName, target.range());
}

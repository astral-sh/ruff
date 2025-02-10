use ruff_python_ast::{self as ast, Expr};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `pd.merge` on Pandas objects.
///
/// ## Why is this bad?
/// In Pandas, the `.merge` method (exposed on, e.g., `DataFrame` objects) and
/// the `pd.merge` function (exposed on the Pandas module) are equivalent.
///
/// For consistency, prefer calling `.merge` on an object over calling
/// `pd.merge` on the Pandas module, as the former is more idiomatic.
///
/// Further, `pd.merge` is not a method, but a function, which prohibits it
/// from being used in method chains, a common pattern in Pandas code.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// cats_df = pd.read_csv("cats.csv")
/// dogs_df = pd.read_csv("dogs.csv")
/// rabbits_df = pd.read_csv("rabbits.csv")
/// pets_df = pd.merge(pd.merge(cats_df, dogs_df), rabbits_df)  # Hard to read.
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// cats_df = pd.read_csv("cats.csv")
/// dogs_df = pd.read_csv("dogs.csv")
/// rabbits_df = pd.read_csv("rabbits.csv")
/// pets_df = cats_df.merge(dogs_df).merge(rabbits_df)
/// ```
///
/// ## References
/// - [Pandas documentation: `merge`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.merge.html#pandas.DataFrame.merge)
/// - [Pandas documentation: `pd.merge`](https://pandas.pydata.org/docs/reference/api/pandas.merge.html#pandas.merge)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfPdMerge;

impl Violation for PandasUseOfPdMerge {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `.merge` method instead of `pd.merge` function. They have equivalent \
             functionality."
            .to_string()
    }
}

/// PD015
pub(crate) fn use_of_pd_merge(checker: &Checker, func: &Expr) {
    if !checker.semantic().seen_module(Modules::PANDAS) {
        return;
    }

    if let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func {
        if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
            if id == "pd" && attr == "merge" {
                checker.report_diagnostic(Diagnostic::new(PandasUseOfPdMerge, func.range()));
            }
        }
    }
}

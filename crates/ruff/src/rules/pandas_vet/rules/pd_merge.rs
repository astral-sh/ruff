use rustpython_parser::ast::{self, Expr, Ranged};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for uses of `pd.merge`.
///
/// ## Why is this bad?
/// `.merge` and `pd.merge` are equivalent. For consistency, use `.merge` over
/// `pd.merge`, which is more idiomatic.
///
/// Further, `pd.merge` is not a method, but a function. This means that it
/// cannot be used in method chains, which is a common pattern in Pandas code.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// cats_df = pd.read_csv("cats.csv")
/// dogs_df = pd.read_csv("dogs.csv")
/// rabbits_df = pd.read_csv("rabbits.csv")
/// pets_df = pd.merge(pd.merge(cats_df, dogs_df), rabbits_df)
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
#[violation]
pub struct PandasUseOfPdMerge;

impl Violation for PandasUseOfPdMerge {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use `.merge` method instead of `pd.merge` function. They have equivalent \
             functionality."
        )
    }
}

/// PD015
pub(crate) fn use_of_pd_merge(checker: &mut Checker, func: &Expr) {
    if let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func {
        if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
            if id == "pd" && attr == "merge" {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PandasUseOfPdMerge, func.range()));
            }
        }
    }
}

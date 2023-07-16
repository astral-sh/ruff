use rustpython_parser::ast;
use rustpython_parser::ast::{Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `.read_table` to read CSV files.
///
/// ## Why is this bad?
/// In the Pandas API, `.read_csv` and `.read_table` are equivalent apart from
/// the default separator; `.read_csv` uses a comma (`,`) as the default
/// separator, while `.read_table` uses a tab (`\t`) as the default separator.
///
/// Prefer `.read_csv` over `.read_table` to read CSV files, since it is
/// clearer and more idiomatic.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// cities_df = pd.read_table("cities.csv", sep=",")
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// cities_df = pd.read_csv("cities.csv")
/// ```
///
/// ## References
/// - [Pandas documentation: `read_csv`](https://pandas.pydata.org/docs/reference/api/pandas.read_csv.html#pandas.read_csv)
/// - [Pandas documentation: `read_table`](https://pandas.pydata.org/docs/reference/api/pandas.read_table.html#pandas.read_table)
#[violation]
pub struct PandasUseOfDotReadTable;

impl Violation for PandasUseOfDotReadTable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.read_csv` instead of `.read_table` to read CSV files")
    }
}

/// PD012
pub(crate) fn use_of_read_table(checker: &mut Checker, func: &Expr, keywords: &[Keyword]) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["pandas", "read_table"])
        })
    {
        let Some(sep) = keywords
            .iter()
            .find(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .map_or(false, |keyword| keyword.as_str() == "sep")
            })
            .map(|keyword| &keyword.value)
        else {
            return;
        };
        if let Expr::Constant(ast::ExprConstant {
            value: Constant::Str(value),
            ..
        }) = &sep
        {
            if value.as_str() == "," {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PandasUseOfDotReadTable, func.range()));
            }
        }
    }
}

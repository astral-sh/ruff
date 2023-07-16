use rustpython_parser::ast;
use rustpython_parser::ast::{Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct PandasUseOfDotReadTable;

impl Violation for PandasUseOfDotReadTable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `pandas.read_csv` instead of `pandas.read_table` to read CSV files")
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

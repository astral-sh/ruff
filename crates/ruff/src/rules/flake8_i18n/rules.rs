use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
// use ruff_python_ast::helpers::format_call_path;
// use ruff_python_ast::types::Range;

#[violation]
pub struct FStringInI18NFuncCall {}

impl Violation for FStringInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("fstring is resolved before function call")
    }
}

#[violation]
pub struct FormatInI18NFuncCall {}

impl Violation for FormatInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("format is resolved before function call")
    }
}
#[violation]
pub struct PrintFInI18NFuncCall {}

impl Violation for PrintFInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("printf is resolved before function call")
    }
}

fn i18n_function_names() -> Vec<String> {
    return vec![
        "_".to_string(),
        "gettext".to_string(),
        "ngettext".to_string(),
    ];
}

pub fn format_in_i18n_func_call(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    functions_names: &Vec<String>,
) -> Option<Diagnostic> {
    None
}

pub fn f_string_in_i18n_func_call(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    functions_names: &Vec<String>,
) -> Option<Diagnostic> {
    None
}

pub fn printf_in_i18n_func_call(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    functions_names: &Vec<String>,
) -> Option<Diagnostic> {
    None
}

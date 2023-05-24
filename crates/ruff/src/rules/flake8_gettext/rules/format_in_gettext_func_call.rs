use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct FormatInGetTextFuncCall;

impl Violation for FormatInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`format` method argument is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// INT002
pub(crate) fn format_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let Expr::Call(ast::ExprCall { func, .. }) = &first {
            if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() {
                if attr == "format" {
                    return Some(Diagnostic::new(FormatInGetTextFuncCall {}, first.range()));
                }
            }
        }
    }
    None
}

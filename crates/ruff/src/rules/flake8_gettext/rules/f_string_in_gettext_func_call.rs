use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct FStringInGetTextFuncCall;

impl Violation for FStringInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// INT001
pub(crate) fn f_string_in_gettext_func_call(checker: &mut Checker, args: &[Expr]) {
    if let Some(first) = args.first() {
        if first.is_joined_str_expr() {
            checker
                .diagnostics
                .push(Diagnostic::new(FStringInGetTextFuncCall {}, first.range()));
        }
    }
}

use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct ParamikoCalls;

impl Violation for ParamikoCalls {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible shell injection via Paramiko call, check inputs are properly sanitized")
    }
}

/// S601
pub(crate) fn paramiko_calls(checker: &mut Checker, func: &Expr) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["paramiko", "exec_command"]
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(ParamikoCalls, func.range()));
    }
}

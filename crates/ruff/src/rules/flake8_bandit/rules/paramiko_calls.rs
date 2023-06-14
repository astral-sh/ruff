use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct ParamikoCall;

impl Violation for ParamikoCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible shell injection via Paramiko call; check inputs are properly sanitized")
    }
}

/// S601
pub(crate) fn paramiko_call(checker: &mut Checker, func: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["paramiko", "exec_command"])
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(ParamikoCall, func.range()));
    }
}

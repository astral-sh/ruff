use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;

use crate::checkers::ast::Checker;

use super::helpers::{is_empty_or_null_string, is_pytest_fail};

#[violation]
pub struct PytestFailWithoutMessage;

impl Violation for PytestFailWithoutMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No message passed to `pytest.fail()`")
    }
}

pub(crate) fn fail_call(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if is_pytest_fail(checker.semantic_model(), func) {
        let call_args = SimpleCallArgs::new(args, keywords);
        let msg = call_args.argument("msg", 0);

        if let Some(msg) = msg {
            if is_empty_or_null_string(msg) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PytestFailWithoutMessage, func.range()));
            }
        } else {
            checker
                .diagnostics
                .push(Diagnostic::new(PytestFailWithoutMessage, func.range()));
        }
    }
}

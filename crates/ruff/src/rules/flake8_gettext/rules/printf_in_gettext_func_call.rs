use rustpython_parser::ast::{self, Constant, Expr, Operator, Ranged};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct PrintfInGetTextFuncCall;

impl Violation for PrintfInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("printf-style format is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// INT003
pub(crate) fn printf_in_gettext_func_call(checker: &mut Checker, args: &[Expr]) {
    if let Some(first) = args.first() {
        if let Expr::BinOp(ast::ExprBinOp {
            op: Operator::Mod { .. },
            left,
            ..
        }) = &first
        {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(_),
                ..
            }) = left.as_ref()
            {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PrintfInGetTextFuncCall {}, first.range()));
            }
        }
    }
}

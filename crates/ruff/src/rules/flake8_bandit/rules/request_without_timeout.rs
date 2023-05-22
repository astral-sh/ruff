use rustpython_parser::ast::{self, Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;

use crate::checkers::ast::Checker;

#[violation]
pub struct RequestWithoutTimeout {
    pub timeout: Option<String>,
}

impl Violation for RequestWithoutTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithoutTimeout { timeout } = self;
        match timeout {
            Some(value) => {
                format!("Probable use of requests call with timeout set to `{value}`")
            }
            None => format!("Probable use of requests call without timeout"),
        }
    }
}

const HTTP_VERBS: [&str; 7] = ["get", "options", "head", "post", "put", "patch", "delete"];

/// S113
pub(crate) fn request_without_timeout(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .semantic_model()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            HTTP_VERBS
                .iter()
                .any(|func_name| call_path.as_slice() == ["requests", func_name])
        })
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(timeout_arg) = call_args.keyword_argument("timeout") {
            if let Some(timeout) = match timeout_arg {
                Expr::Constant(ast::ExprConstant {
                    value: value @ Constant::None,
                    ..
                }) => Some(checker.generator().constant(value)),
                _ => None,
            } {
                checker.diagnostics.push(Diagnostic::new(
                    RequestWithoutTimeout {
                        timeout: Some(timeout),
                    },
                    timeout_arg.range(),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                RequestWithoutTimeout { timeout: None },
                func.range(),
            ));
        }
    }
}

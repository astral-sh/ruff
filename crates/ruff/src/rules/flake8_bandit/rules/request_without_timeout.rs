use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::helpers::{unparse_constant, SimpleCallArgs};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RequestWithoutTimeout {
        pub timeout: Option<String>,
    }
);
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
pub fn request_without_timeout(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        HTTP_VERBS
            .iter()
            .any(|func_name| call_path.as_slice() == ["requests", func_name])
    }) {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(timeout_arg) = call_args.get_argument("timeout", None) {
            if let Some(timeout) = match &timeout_arg.node {
                ExprKind::Constant {
                    value: value @ Constant::None,
                    ..
                } => Some(unparse_constant(value, checker.stylist)),
                _ => None,
            } {
                checker.diagnostics.push(Diagnostic::new(
                    RequestWithoutTimeout {
                        timeout: Some(timeout),
                    },
                    Range::from_located(timeout_arg),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                RequestWithoutTimeout { timeout: None },
                Range::from_located(func),
            ));
        }
    }
}

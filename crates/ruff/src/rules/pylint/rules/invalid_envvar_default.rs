use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::ast::{Constant, Expr, ExprKind};

#[violation]
pub struct InvalidEnvvarDefault;

impl Violation for InvalidEnvvarDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid type for environment variable default, must be `str` or `none`")
    }
}

/// PLW1508
pub fn invalid_envvar_default(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if attr != "getenv" {
            return;
        }
        let ExprKind::Name {id, ..} = &value.node else {
            return;
        };
        if id != "os" {
            return;
        }

        let Some(expr_par) = checker.ctx.current_expr_parent() else {
            return;
        };
        let ExprKind::Call {args, ..} = &expr_par.node else {
            return;
        };

        for arg in args {
            if let ExprKind::Constant { value, .. } = &arg.node {
                if !matches!(value, Constant::Str { .. } | Constant::None { .. }) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(InvalidEnvvarDefault, Range::from(expr)));
                }
            } else {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidEnvvarDefault, Range::from(expr)));
            }
        }
    }
}

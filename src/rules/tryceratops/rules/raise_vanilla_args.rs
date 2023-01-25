use ruff_macros::derive_message_formats;
use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RaiseVanillaArgs;
);
impl Violation for RaiseVanillaArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid specifying long messages outside the exception class")
    }
}

const WHITESPACE: &str = " ";

fn get_string_arg_value(arg: &Expr) -> Option<String> {
    match &arg.node {
        ExprKind::JoinedStr { values } => {
            let value = values
                .iter()
                .map(|val| get_string_arg_value(val).unwrap_or_default())
                .collect::<String>();
            Some(value)
        }
        ExprKind::Constant {
            value: Constant::Str(val),
            ..
        } => Some(val.to_string()),
        _ => None,
    }
}

/// TRY003
pub fn raise_vanilla_args(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Call { args, .. } = &expr.node {
        if let Some(arg_value) = get_string_arg_value(&args[0]) {
            if arg_value.contains(WHITESPACE) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(RaiseVanillaArgs, Range::from_located(expr)));
            }
        }
    }
}

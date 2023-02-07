use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
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

fn collect_strings_impl<'a>(expr: &'a Expr, parts: &mut Vec<&'a str>) {
    match &expr.node {
        ExprKind::JoinedStr { values } => {
            for value in values {
                collect_strings_impl(value, parts);
            }
        }
        ExprKind::Constant {
            value: Constant::Str(val),
            ..
        } => parts.push(val),
        _ => {}
    }
}

fn collect_strings(expr: &Expr) -> Vec<&str> {
    let mut parts = Vec::new();
    collect_strings_impl(expr, &mut parts);
    parts
}

/// TRY003
pub fn raise_vanilla_args(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Call { args, .. } = &expr.node {
        if let Some(arg) = args.first() {
            if collect_strings(arg)
                .iter()
                .any(|part| part.chars().any(char::is_whitespace))
            {
                checker
                    .diagnostics
                    .push(Diagnostic::new(RaiseVanillaArgs, Range::from_located(expr)));
            }
        }
    }
}

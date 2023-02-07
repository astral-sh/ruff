use itertools::Itertools;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct StripWithMultiCharacters;
);
impl Violation for StripWithMultiCharacters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `.strip()` with multi-character strings is misleading the reader")
    }
}

/// B005
pub fn strip_with_multi_characters(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let ExprKind::Attribute { attr, .. } = &func.node else {
        return;
    };
    if !matches!(attr.as_str(), "strip" | "lstrip" | "rstrip") {
        return;
    }
    if args.len() != 1 {
        return;
    }

    let ExprKind::Constant {
        value: Constant::Str(value),
        ..
    } = &args[0].node else {
        return;
    };

    if value.len() > 1 && value.chars().unique().count() != value.len() {
        checker.diagnostics.push(Diagnostic::new(
            StripWithMultiCharacters,
            Range::from_located(expr),
        ));
    }
}

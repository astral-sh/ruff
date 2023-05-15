use itertools::Itertools;
use rustpython_parser::ast::{self, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct StripWithMultiCharacters;

impl Violation for StripWithMultiCharacters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `.strip()` with multi-character strings is misleading the reader")
    }
}

/// B005
pub(crate) fn strip_with_multi_characters(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let ExprKind::Attribute(ast::ExprAttribute { attr, .. }) = &func.node else {
        return;
    };
    if !matches!(attr.as_str(), "strip" | "lstrip" | "rstrip") {
        return;
    }
    if args.len() != 1 {
        return;
    }

    let ExprKind::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    } )= &args[0].node else {
        return;
    };

    let num_chars = value.chars().count();
    if num_chars > 1 && num_chars != value.chars().unique().count() {
        checker
            .diagnostics
            .push(Diagnostic::new(StripWithMultiCharacters, expr.range()));
    }
}

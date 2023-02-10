use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnnecessaryLiteralDict {
        pub obj_type: String,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralDict { obj_type } = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `dict` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` literal".to_string()
    }
}

/// C406 (`dict([(1, 2)])`)
pub fn unnecessary_literal_dict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function("dict", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("dict") {
        return;
    }
    let (kind, elts) = match argument {
        ExprKind::Tuple { elts, .. } => ("tuple", elts),
        ExprKind::List { elts, .. } => ("list", elts),
        _ => return,
    };
    // Accept `dict((1, 2), ...))` `dict([(1, 2), ...])`.
    if !elts
        .iter()
        .all(|elt| matches!(&elt.node, ExprKind::Tuple { elts, .. } if elts.len() == 2))
    {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralDict {
            obj_type: kind.to_string(),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        match fixes::fix_unnecessary_literal_dict(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

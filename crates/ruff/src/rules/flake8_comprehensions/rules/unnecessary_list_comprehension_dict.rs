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
    pub struct UnnecessaryListComprehensionDict;
);
impl AlwaysAutofixableViolation for UnnecessaryListComprehensionDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` comprehension (rewrite as a `dict` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }
}

/// C404 (`dict([...])`)
pub fn unnecessary_list_comprehension_dict(
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
    let ExprKind::ListComp { elt, .. } = &argument else {
        return;
    };
    let ExprKind::Tuple { elts, .. } = &elt.node else {
        return;
    };
    if elts.len() != 2 {
        return;
    }
    let mut diagnostic =
        Diagnostic::new(UnnecessaryListComprehensionDict, Range::from_located(expr));
    if checker.patch(diagnostic.kind.rule()) {
        match fixes::fix_unnecessary_list_comprehension_dict(checker.locator, checker.stylist, expr)
        {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, Keyword};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnnecessaryCollectionCall {
        pub obj_type: String,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryCollectionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCollectionCall { obj_type } = self;
        format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a literal".to_string()
    }
}

/// C408
pub fn unnecessary_collection_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !args.is_empty() {
        return;
    }
    let Some(id) = helpers::function_name(func) else {
        return;
    };
    match id {
        "dict" if keywords.is_empty() || keywords.iter().all(|kw| kw.node.arg.is_some()) => {
            // `dict()` or `dict(a=1)` (as opposed to `dict(**a)`)
        }
        "list" | "tuple" => {
            // `list()` or `tuple()`
        }
        _ => return,
    };
    if !checker.is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryCollectionCall {
            obj_type: id.to_string(),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        match fixes::fix_unnecessary_collection_call(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

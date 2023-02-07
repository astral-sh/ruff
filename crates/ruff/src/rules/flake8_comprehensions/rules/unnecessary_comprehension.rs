use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Comprehension, Expr, ExprKind};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnnecessaryComprehension {
        pub obj_type: String,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryComprehension { obj_type } = self;
        format!("Unnecessary `{obj_type}` comprehension (rewrite using `{obj_type}()`)")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryComprehension { obj_type } = self;
        format!("Rewrite using `{obj_type}()`")
    }
}

/// C416
pub fn unnecessary_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) {
    if generators.len() != 1 {
        return;
    }
    let generator = &generators[0];
    if !(generator.ifs.is_empty() && generator.is_async == 0) {
        return;
    }
    let Some(elt_id) = helpers::function_name(elt) else {
        return;
    };

    let Some(target_id) = helpers::function_name(&generator.target) else {
        return;
    };
    if elt_id != target_id {
        return;
    }
    let id = match &expr.node {
        ExprKind::ListComp { .. } => "list",
        ExprKind::SetComp { .. } => "set",
        _ => return,
    };
    if !checker.is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryComprehension {
            obj_type: id.to_string(),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        match fixes::fix_unnecessary_comprehension(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

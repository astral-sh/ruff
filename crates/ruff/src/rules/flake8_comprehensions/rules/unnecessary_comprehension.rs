use log::error;
use rustpython_parser::ast::{Comprehension, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

#[violation]
pub struct UnnecessaryComprehension {
    pub obj_type: String,
}

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

/// Add diagnostic for C416 based on the expression node id.
fn add_diagnostic(checker: &mut Checker, expr: &Expr) {
    let id = match &expr.node {
        ExprKind::ListComp { .. } => "list",
        ExprKind::SetComp { .. } => "set",
        ExprKind::DictComp { .. } => "dict",
        _ => return,
    };
    if !checker.ctx.is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryComprehension {
            obj_type: id.to_string(),
        },
        Range::from(expr),
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

/// C416
pub fn unnecessary_dict_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    key: &Expr,
    value: &Expr,
    generators: &[Comprehension],
) {
    if generators.len() != 1 {
        return;
    }
    let generator = &generators[0];
    if !(generator.ifs.is_empty() && generator.is_async == 0) {
        return;
    }
    let Some(key_id) = helpers::function_name(key) else {
        return;
    };
    let Some(value_id) = helpers::function_name(value) else {
        return;
    };
    let target_ids: Vec<&str> = match &generator.target.node {
        ExprKind::Tuple { elts, .. } => elts.iter().filter_map(helpers::function_name).collect(),
        _ => return,
    };
    if target_ids.len() != 2 {
        return;
    }
    if key_id != target_ids[0] && value_id != target_ids[1] {
        return;
    }
    add_diagnostic(checker, expr);
}

/// C416
pub fn unnecessary_list_set_comprehension(
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
    add_diagnostic(checker, expr);
}

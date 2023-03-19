use itertools::{EitherOrBoth, Itertools};
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

/// C416
pub fn unnecessary_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    elts: &[&Expr],
    generators: &[Comprehension],
) {
    if generators.len() != 1 {
        return;
    }
    let generator = &generators[0];
    if !(generator.ifs.is_empty() && generator.is_async == 0) {
        return;
    }
    let elt_ids: Vec<Option<&str>> = elts.iter().map(|&e| helpers::function_name(e)).collect();

    let target_ids: Vec<Option<&str>> = match &generator.target.node {
        ExprKind::Name { id, .. } => vec![Some(id.as_str())],
        ExprKind::Tuple { elts, .. } => {
            // If the target is a tuple of length 1, the comprehension is
            // used to flatten the iterable. E.g., [(1,), (2,)] => [1, 2]
            if elts.len() == 1 {
                return;
            } else {
                elts.iter().map(helpers::function_name).collect()
            }
        }
        _ => return,
    };

    if !elt_ids
        .iter()
        .zip_longest(target_ids.iter())
        .all(|e| match e {
            EitherOrBoth::Both(&Some(e), &Some(t)) => e == t,
            _ => false,
        })
    {
        return;
    }

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

use rustpython_parser::ast::{self, Comprehension, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary `dict`, `list`, and `set` comprehension.
///
/// ## Why is this bad?
/// It's unnecessary to use a `dict`/`list`/`set` comprehension to build a
/// data structure if the elements are unchanged. Wrap the iterable with
/// `dict()`, `list()`, or `set()` instead.
///
/// ## Examples
/// ```python
/// {a: b for a, b in iterable}
/// [x for x in iterable]
/// {x for x in iterable}
/// ```
///
/// Use instead:
/// ```python
/// dict(iterable)
/// list(iterable)
/// set(iterable)
/// ```
#[violation]
pub struct UnnecessaryComprehension {
    obj_type: String,
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
    let id = match expr {
        Expr::ListComp(_) => "list",
        Expr::SetComp(_) => "set",
        Expr::DictComp(_) => "dict",
        _ => return,
    };
    if !checker.semantic().is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryComprehension {
            obj_type: id.to_string(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.try_set_fix_from_edit(|| {
            fixes::fix_unnecessary_comprehension(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}

/// C416
pub(crate) fn unnecessary_dict_comprehension(
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
    if !generator.ifs.is_empty() || generator.is_async {
        return;
    }
    let Some(key_id) = helpers::expr_name(key) else {
        return;
    };
    let Some(value_id) = helpers::expr_name(value) else {
        return;
    };
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = &generator.target else {
        return;
    };
    if elts.len() != 2 {
        return;
    }
    let Some(target_key_id) = helpers::expr_name(&elts[0]) else {
        return;
    };
    if target_key_id != key_id {
        return;
    }
    let Some(target_value_id) = helpers::expr_name(&elts[1]) else {
        return;
    };
    if target_value_id != value_id {
        return;
    }
    add_diagnostic(checker, expr);
}

/// C416
pub(crate) fn unnecessary_list_set_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) {
    if generators.len() != 1 {
        return;
    }
    let generator = &generators[0];
    if !generator.ifs.is_empty() || generator.is_async {
        return;
    }
    let Some(elt_id) = helpers::expr_name(elt) else {
        return;
    };
    let Some(target_id) = helpers::expr_name(&generator.target) else {
        return;
    };
    if elt_id != target_id {
        return;
    }
    add_diagnostic(checker, expr);
}

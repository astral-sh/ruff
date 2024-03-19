use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtFor};
use ruff_python_semantic::analyze::typing::is_set;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `set` being modified during the iteration on the set.
///
/// ## Why is this bad?
/// If `set` is modified during the iteration, it will cause `RuntimeError`.
/// This could be fixed by using temporal copy of the set to iterate.
///
/// ## Example
/// ```python
/// nums = {1, 2, 3}
/// for num in nums:
///     nums.add(num + 5)
/// ```
///
/// Use instead:
/// ```python
/// nums = {1, 2, 3}
/// for num in nums.copy():
///     nums.add(num + 5)
/// ```
///
/// ## References
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[violation]
pub struct ModifiedIteratingSet {
    name: String,
}

impl AlwaysFixableViolation for ModifiedIteratingSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Iterated set `{}` is being modified inside for loop body.",
            self.name
        )
    }

    fn fix_title(&self) -> String {
        format!("Consider iterating through a copy of it instead.")
    }
}

fn is_method_modifying(identifier: &str) -> bool {
    (identifier == "add")
        || (identifier == "clear")
        || (identifier == "discard")
        || (identifier == "pop")
        || (identifier == "remove")
}

// PLE4703
pub(crate) fn modified_iterating_set(checker: &mut Checker, for_stmt: &StmtFor) {
    let Some(name) = for_stmt.iter.as_name_expr() else {
        return;
    };

    let Some(binding_id) = checker.semantic().only_binding(name) else {
        return;
    };
    if !is_set(checker.semantic().binding(binding_id), checker.semantic()) {
        return;
    }

    let mut is_modified = false;
    for stmt in &for_stmt.body {
        // name_of_set.modify_method()
        // ^---------^ ^-----------^
        //    value        attr
        // ^-----------------------^
        //           func
        // ^-------------------------^
        //        expr, stmt
        let Stmt::Expr(ast::StmtExpr { value: expr, .. }) = stmt else {
            continue;
        };

        let Some(func) = expr.as_call_expr().map(|exprcall| &exprcall.func) else {
            continue;
        };

        let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
            continue;
        };

        let Some(value) = value.as_name_expr() else {
            continue;
        };

        let Some(binding_id_value) = checker.semantic().only_binding(value) else {
            continue;
        };
        if binding_id == binding_id_value && is_method_modifying(attr.as_str()) {
            is_modified = true;
        }
    }

    if is_modified {
        let mut diagnostic = Diagnostic::new(
            ModifiedIteratingSet {
                name: name.id.clone(),
            },
            for_stmt.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            format!("{}.copy()", name.id),
            name.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

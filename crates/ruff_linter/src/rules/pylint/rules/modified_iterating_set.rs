use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_body;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr, StmtFor};
use ruff_python_semantic::analyze::typing::is_set;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for loops in which a `set` is modified during iteration.
///
/// ## Why is this bad?
/// If a `set` is modified during iteration, it will cause a `RuntimeError`.
///
/// If you need to modify a `set` within a loop, consider iterating over a copy
/// of the `set` instead.
///
/// ## Known problems
/// This rule favors false negatives over false positives. Specifically, it
/// will only detect variables that can be inferred to be a `set` type based on
/// local type inference, and will only detect modifications that are made
/// directly on the variable itself (e.g., `set.add()`), as opposed to
/// modifications within other function calls (e.g., `some_function(set)`).
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
#[derive(ViolationMetadata)]
pub(crate) struct ModifiedIteratingSet {
    name: Name,
}

impl AlwaysFixableViolation for ModifiedIteratingSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ModifiedIteratingSet { name } = self;
        format!("Iterated set `{name}` is modified within the `for` loop",)
    }

    fn fix_title(&self) -> String {
        let ModifiedIteratingSet { name } = self;
        format!("Iterate over a copy of `{name}`")
    }
}

/// PLE4703
pub(crate) fn modified_iterating_set(checker: &Checker, for_stmt: &StmtFor) {
    let Some(name) = for_stmt.iter.as_name_expr() else {
        return;
    };

    let Some(binding_id) = checker.semantic().only_binding(name) else {
        return;
    };
    if !is_set(checker.semantic().binding(binding_id), checker.semantic()) {
        return;
    }

    let is_modified = any_over_body(&for_stmt.body, &|expr| {
        let Some(func) = expr.as_call_expr().map(|call| &call.func) else {
            return false;
        };

        let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
            return false;
        };

        let Some(value) = value.as_name_expr() else {
            return false;
        };

        let Some(value_id) = checker.semantic().only_binding(value) else {
            return false;
        };

        binding_id == value_id && modifies_set(attr.as_str())
    });

    if is_modified {
        let mut diagnostic = Diagnostic::new(
            ModifiedIteratingSet {
                name: name.id.clone(),
            },
            for_stmt.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            format!("{}.copy()", checker.locator().slice(name)),
            name.range(),
        )));
        checker.report_diagnostic(diagnostic);
    }
}

/// Returns `true` if the method modifies the set.
fn modifies_set(identifier: &str) -> bool {
    matches!(identifier, "add" | "clear" | "discard" | "pop" | "remove")
}

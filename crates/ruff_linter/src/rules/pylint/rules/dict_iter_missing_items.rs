use ruff_python_ast::{Expr, ExprTuple};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for dictionary unpacking in a for loop without calling `.items()`.
///
/// ## Why is this bad?
/// When iterating over a dictionary in a for loop, if a dictionary is unpacked
/// without calling `.items()`, it could lead to a runtime error if the keys are not
/// a tuple of two elements.
///
/// It is likely that you're looking for an iteration over (key, value) pairs which
/// can only be achieved when calling `.items()`.
///
/// ## Example
/// ```python
/// data = {"Paris": 2_165_423, "New York City": 8_804_190, "Tokyo": 13_988_129}
///
/// for city, population in data:
///     print(f"{city} has population {population}.")
/// ```
///
/// Use instead:
/// ```python
/// data = {"Paris": 2_165_423, "New York City": 8_804_190, "Tokyo": 13_988_129}
///
/// for city, population in data.items():
///     print(f"{city} has population {population}.")
/// ```
#[violation]
pub struct DictIterMissingItems;

impl AlwaysFixableViolation for DictIterMissingItems {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unpacking a dictionary in iteration without calling `.items()`")
    }

    fn fix_title(&self) -> String {
        format!("Add a call to `.items()`")
    }
}

pub(crate) fn dict_iter_missing_items(checker: &mut Checker, target: &Expr, iter: &Expr) {
    let Expr::Tuple(ExprTuple { elts, .. }) = target else {
        return;
    };

    if elts.len() != 2 {
        return;
    };

    let Some(name) = iter.as_name_expr() else {
        return;
    };

    let Some(binding) = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_dict(binding, checker.semantic()) {
        return;
    }

    // If we can reliably determine that a dictionary has keys that are tuples of two we don't warn
    if is_dict_key_tuple_with_two_elements(checker.semantic(), binding) {
        return;
    }

    let mut diagnostic = Diagnostic::new(DictIterMissingItems, iter.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!("{}.items()", name.id),
        iter.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// Returns true if the binding is a dictionary where each key is a tuple with two elements.
fn is_dict_key_tuple_with_two_elements(semantic: &SemanticModel, binding: &Binding) -> bool {
    let Some(statement) = binding.statement(semantic) else {
        return false;
    };

    let Some(assign_stmt) = statement.as_assign_stmt() else {
        return false;
    };

    let Some(dict_expr) = assign_stmt.value.as_dict_expr() else {
        return false;
    };

    dict_expr.keys.iter().all(|elt| {
        elt.as_ref().is_some_and(|x| {
            if let Some(tuple) = x.as_tuple_expr() {
                return tuple.elts.len() == 2;
            }
            false
        })
    })
}

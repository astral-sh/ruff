use ruff_python_ast::{Expr, ExprTuple};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unpacking a dictionary in a for loop without calling `.items()`.
///
/// ## Why is this bad?
/// You are likely looking for an iteration over key, value pairs which can only be achieved
/// when calling `.items()`.
///
/// ## Example
/// ```python
/// data = {"Paris": 2_165_423, "New York City": 8_804_190, "Tokyo": 13_988_129}
/// for city, population in data:
///     print(f"{city} has population {population}.")
/// ```
///
/// Use instead:
/// ```python
/// data = {"Paris": 2_165_423, "New York City": 8_804_190, "Tokyo": 13_988_129}
/// for city, population in data.items():
///     print(f"{city} has population {population}.")
/// ```
#[violation]
pub struct DictIterMissingItems;

impl Violation for DictIterMissingItems {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Call `items()` when unpacking a dictionary for iteration")
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

    checker
        .diagnostics
        .push(Diagnostic::new(DictIterMissingItems, iter.range()));
}

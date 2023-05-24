use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

use super::super::helpers::{shadows_builtin, AnyShadowing};

/// ## What it does
/// Checks for variable (and function) assignments that use the same name
/// as a builtin.
///
/// ## Why is this bad?
/// Reusing a builtin name for the name of a variable increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the variable for the
/// builtin and vice versa.
///
/// Builtins can be marked as exceptions to this rule via the
/// [`flake8-builtins.builtins-ignorelist`] configuration option.
///
/// ## Options
///
/// - `flake8-builtins.builtins-ignorelist`
///
/// ## Example
/// ```python
/// def find_max(list_of_lists):
///     max = 0
///     for flat_list in list_of_lists:
///         for value in flat_list:
///             max = max(max, value)  # TypeError: 'int' object is not callable
///     return max
/// ```
///
/// Use instead:
/// ```python
/// def find_max(list_of_lists):
///     result = 0
///     for flat_list in list_of_lists:
///         for value in flat_list:
///             result = max(result, value)
///     return result
/// ```
///
/// - [_Why is it a bad idea to name a variable `id` in Python?_](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)
#[violation]
pub struct BuiltinVariableShadowing {
    name: String,
}

impl Violation for BuiltinVariableShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinVariableShadowing { name } = self;
        format!("Variable `{name}` is shadowing a Python builtin")
    }
}

/// A001
pub(crate) fn builtin_variable_shadowing(
    checker: &mut Checker,
    name: &str,
    shadowing: AnyShadowing,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        checker.diagnostics.push(Diagnostic::new(
            BuiltinVariableShadowing {
                name: name.to_string(),
            },
            shadowing.range(checker.locator),
        ));
    }
}

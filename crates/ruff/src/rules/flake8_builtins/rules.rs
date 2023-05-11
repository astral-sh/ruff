use rustpython_parser::ast::Attributed;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::builtins::BUILTINS;

use crate::checkers::ast::Checker;

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

/// ## What it does
/// Checks for any function arguments that use the same name as a builtin.
///
/// ## Why is this bad?
/// Reusing a builtin name for the name of an argument increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the argument for the
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
/// def remove_duplicates(list, list2):
///     result = set()
///     for value in list:
///         result.add(value)
///     for value in list2:
///         result.add(value)
///     return list(result)  # TypeError: 'list' object is not callable
/// ```
///
/// Use instead:
/// ```python
/// def remove_duplicates(list1, list2):
///     result = set()
///     for value in list1:
///         result.add(value)
///     for value in list2:
///         result.add(value)
///     return list(result)
/// ```
///
/// ## References
/// - [_Is it bad practice to use a built-in function name as an attribute or method identifier?_](https://stackoverflow.com/questions/9109333/is-it-bad-practice-to-use-a-built-in-function-name-as-an-attribute-or-method-ide)
/// - [_Why is it a bad idea to name a variable `id` in Python?_](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)
#[violation]
pub struct BuiltinArgumentShadowing {
    name: String,
}

impl Violation for BuiltinArgumentShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinArgumentShadowing { name } = self;
        format!("Argument `{name}` is shadowing a Python builtin")
    }
}

/// ## What it does
/// Checks for any class attributes that use the same name as a builtin.
///
/// ## Why is this bad?
/// Reusing a builtin name for the name of an attribute increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the attribute for the
/// builtin and vice versa.
///
/// Builtins can be marked as exceptions to this rule via the
/// [`flake8-builtins.builtins-ignorelist`] configuration option, or
/// converted to the appropriate dunder method.
///
/// ## Options
///
/// - `flake8-builtins.builtins-ignorelist`
///
/// ## Example
/// ```python
/// class Shadow:
///     def int():
///         return 0
/// ```
///
/// Use instead:
/// ```python
/// class Shadow:
///     def to_int():
///         return 0
/// ```
///
/// Or:
/// ```python
/// class Shadow:
///     # Callable as `int(shadow)`
///     def __int__():
///         return 0
/// ```
///
/// ## References
/// - [_Is it bad practice to use a built-in function name as an attribute or method identifier?_](https://stackoverflow.com/questions/9109333/is-it-bad-practice-to-use-a-built-in-function-name-as-an-attribute-or-method-ide)
/// - [_Why is it a bad idea to name a variable `id` in Python?_](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)
#[violation]
pub struct BuiltinAttributeShadowing {
    name: String,
}

impl Violation for BuiltinAttributeShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinAttributeShadowing { name } = self;
        format!("Class attribute `{name}` is shadowing a Python builtin")
    }
}

fn shadows_builtin(name: &str, ignorelist: &[String]) -> bool {
    BUILTINS.contains(&name) && ignorelist.iter().all(|ignore| ignore != name)
}

/// A001
pub fn builtin_variable_shadowing<T>(
    checker: &mut Checker,
    name: &str,
    attributed: &Attributed<T>,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        checker.diagnostics.push(Diagnostic::new(
            BuiltinVariableShadowing {
                name: name.to_string(),
            },
            attributed.range(),
        ));
    }
}

/// A002
pub fn builtin_argument_shadowing<T>(
    checker: &mut Checker,
    name: &str,
    attributed: &Attributed<T>,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        checker.diagnostics.push(Diagnostic::new(
            BuiltinArgumentShadowing {
                name: name.to_string(),
            },
            attributed.range(),
        ));
    }
}

/// A003
pub fn builtin_attribute_shadowing<T>(
    checker: &mut Checker,
    name: &str,
    attributed: &Attributed<T>,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        checker.diagnostics.push(Diagnostic::new(
            BuiltinAttributeShadowing {
                name: name.to_string(),
            },
            attributed.range(),
        ));
    }
}

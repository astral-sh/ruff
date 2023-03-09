use rustpython_parser::ast::Located;

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_stdlib::builtins::BUILTINS;

use super::types::ShadowingType;

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
    pub name: String,
}

impl Violation for BuiltinVariableShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinVariableShadowing { name } = self;
        format!("Variable `{name}` is shadowing a python builtin")
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
    pub name: String,
}

impl Violation for BuiltinArgumentShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinArgumentShadowing { name } = self;
        format!("Argument `{name}` is shadowing a python builtin")
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
    pub name: String,
}

impl Violation for BuiltinAttributeShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinAttributeShadowing { name } = self;
        format!("Class attribute `{name}` is shadowing a python builtin")
    }
}

/// Check builtin name shadowing.
pub fn builtin_shadowing<T>(
    name: &str,
    located: &Located<T>,
    node_type: ShadowingType,
    ignorelist: &[String],
) -> Option<Diagnostic> {
    if BUILTINS.contains(&name) && !ignorelist.contains(&name.to_string()) {
        Some(Diagnostic::new::<DiagnosticKind>(
            match node_type {
                ShadowingType::Variable => BuiltinVariableShadowing {
                    name: name.to_string(),
                }
                .into(),
                ShadowingType::Argument => BuiltinArgumentShadowing {
                    name: name.to_string(),
                }
                .into(),
                ShadowingType::Attribute => BuiltinAttributeShadowing {
                    name: name.to_string(),
                }
                .into(),
            },
            Range::from(located),
        ))
    } else {
        None
    }
}

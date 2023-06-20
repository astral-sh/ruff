use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use rustpython_parser::ast;

use crate::checkers::ast::Checker;

use super::super::helpers::{shadows_builtin, AnyShadowing};

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
/// ## Options
/// - `flake8-builtins.builtins-ignorelist`
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

/// A003
pub(crate) fn builtin_attribute_shadowing(
    checker: &mut Checker,
    class_def: &ast::StmtClassDef,
    name: &str,
    shadowing: AnyShadowing,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        // Ignore shadowing within `TypedDict` definitions, since these are only accessible through
        // subscripting and not through attribute access.
        if class_def
            .bases
            .iter()
            .any(|base| checker.semantic().match_typing_expr(base, "TypedDict"))
        {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            BuiltinAttributeShadowing {
                name: name.to_string(),
            },
            shadowing.identifier(),
        ));
    }
}

use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, Decorator};
use ruff_text_size::TextRange;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::rules::flake8_builtins::helpers::shadows_builtin;

/// ## What it does
/// Checks for any class attributes or methods that use the same name as a
/// builtin.
///
/// ## Why is this bad?
/// Reusing a builtin name for the name of an attribute increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the attribute for the
/// builtin and vice versa.
///
/// Builtins can be marked as exceptions to this rule via the
/// [`flake8-builtins.builtins-ignorelist`] configuration option, or
/// converted to the appropriate dunder method. Methods decorated with
/// `@typing.override` or `@typing_extensions.override` are also
/// ignored.
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
    range: TextRange,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        // Ignore shadowing within `TypedDict` definitions, since these are only accessible through
        // subscripting and not through attribute access.
        if class_def
            .bases()
            .iter()
            .any(|base| checker.semantic().match_typing_expr(base, "TypedDict"))
        {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            BuiltinAttributeShadowing {
                name: name.to_string(),
            },
            range,
        ));
    }
}

/// A003
pub(crate) fn builtin_method_shadowing(
    checker: &mut Checker,
    class_def: &ast::StmtClassDef,
    name: &str,
    decorator_list: &[Decorator],
    range: TextRange,
) {
    if shadows_builtin(name, &checker.settings.flake8_builtins.builtins_ignorelist) {
        // Ignore some standard-library methods. Ideally, we'd ignore all overridden methods, since
        // those should be flagged on the superclass, but that's more difficult.
        if is_standard_library_override(name, class_def, checker.semantic()) {
            return;
        }

        // Ignore explicit overrides.
        if decorator_list.iter().any(|decorator| {
            checker
                .semantic()
                .match_typing_expr(&decorator.expression, "override")
        }) {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            BuiltinAttributeShadowing {
                name: name.to_string(),
            },
            range,
        ));
    }
}

/// Return `true` if an attribute appears to be an override of a standard-library method.
fn is_standard_library_override(
    name: &str,
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
) -> bool {
    let Some(Arguments { args: bases, .. }) = class_def.arguments.as_deref() else {
        return false;
    };
    match name {
        // Ex) `Event.set`
        "set" => bases.iter().any(|base| {
            semantic
                .resolve_call_path(base)
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["threading", "Event"]))
        }),
        // Ex) `Filter.filter`
        "filter" => bases.iter().any(|base| {
            semantic
                .resolve_call_path(base)
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["logging", "Filter"]))
        }),
        _ => false,
    }
}

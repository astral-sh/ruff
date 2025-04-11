use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{analyze::function_type, ScopeKind};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Detects attempts to use `super` without parentheses.
///
/// ## Why is this bad?
/// The [`super()` callable](https://docs.python.org/3/library/functions.html#super)
/// can be used inside method definitions to create a proxy object that
/// delegates attribute access to a superclass of the current class. Attempting
/// to access attributes on `super` itself, however, instead of the object
/// returned by a call to `super()`, will raise `AttributeError`.
///
/// ## Example
/// ```python
/// class Animal:
///     @staticmethod
///     def speak():
///         return "This animal says something."
///
///
/// class Dog(Animal):
///     @staticmethod
///     def speak():
///         original_speak = super.speak()  # ERROR: `super.speak()`
///         return f"{original_speak} But as a dog, it barks!"
/// ```
///
/// Use instead:
/// ```python
/// class Animal:
///     @staticmethod
///     def speak():
///         return "This animal says something."
///
///
/// class Dog(Animal):
///     @staticmethod
///     def speak():
///         original_speak = super().speak()  # Correct: `super().speak()`
///         return f"{original_speak} But as a dog, it barks!"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuperWithoutBrackets;

impl AlwaysFixableViolation for SuperWithoutBrackets {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`super` call is missing parentheses".to_string()
    }

    fn fix_title(&self) -> String {
        "Add parentheses to `super` call".to_string()
    }
}

/// PLW0245
pub(crate) fn super_without_brackets(checker: &Checker, func: &Expr) {
    // The call must be to `super` (without parentheses).
    let Expr::Attribute(ast::ExprAttribute { value, .. }) = func else {
        return;
    };

    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };

    if id.as_str() != "super" {
        return;
    }

    if !checker.semantic().has_builtin_binding(id.as_str()) {
        return;
    }

    let scope = checker.semantic().current_scope();

    // The current scope _must_ be a function.
    let ScopeKind::Function(function_def) = scope.kind else {
        return;
    };

    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    // The function must be a method, class method, or static method.
    let classification = function_type::classify(
        &function_def.name,
        &function_def.decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );
    if !matches!(
        classification,
        function_type::FunctionType::Method
            | function_type::FunctionType::ClassMethod
            | function_type::FunctionType::StaticMethod
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(SuperWithoutBrackets, value.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "super()".to_string(),
        value.range(),
    )));

    checker.report_diagnostic(diagnostic);
}

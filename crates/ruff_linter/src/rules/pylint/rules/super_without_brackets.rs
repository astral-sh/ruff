use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{analyze::function_type, ScopeKind};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `super` calls without parentheses.
///
/// ## Why is this bad?
/// When `super` is used without parentheses, it is not an actual call, and
/// thus has no effect.
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
///         original_speak = super.speak()
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
///         original_speak = super().speak()
///         return f"{original_speak} But as a dog, it barks!"
/// ```
#[violation]
pub struct SuperWithoutBrackets;

impl AlwaysFixableViolation for SuperWithoutBrackets {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`super` call is missing parentheses")
    }

    fn fix_title(&self) -> String {
        "Add parentheses to `super` call".to_string()
    }
}

/// PLW0245
pub(crate) fn super_without_brackets(checker: &mut Checker, func: &Expr) {
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

    if !checker.semantic().is_builtin(id.as_str()) {
        return;
    }

    let scope = checker.semantic().current_scope();

    // The current scope _must_ be a function.
    let ScopeKind::Function(function_def) = scope.kind else {
        return;
    };

    let Some(parent) = &checker.semantic().first_non_type_parent_scope(scope) else {
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
        function_type::FunctionType::Method { .. }
            | function_type::FunctionType::ClassMethod { .. }
            | function_type::FunctionType::StaticMethod { .. }
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(SuperWithoutBrackets, value.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "super()".to_string(),
        value.range(),
    )));

    checker.diagnostics.push(diagnostic);
}

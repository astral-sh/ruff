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
/// When `super` is used without parentheses, it is not an actual call.
///
/// ## Example
/// ```python
/// class Soup:
///     @staticmethod
///     def temp():
///         print("Soup is hot!")
///
///
/// class TomatoSoup(Soup):
///     @staticmethod
///     def temp():
///         super.temp()  # [super-without-brackets]
///         print("But tomato soup is even hotter!")
/// ```
///
/// Use instead:
/// ```python
/// class Soup:
///     @staticmethod
///     def temp():
///         print("Soup is hot!")
///
///
/// class TomatoSoup(Soup):
///     @staticmethod
///     def temp():
///         super().temp()
///         print("But tomato soup is even hotter!")
/// ```
#[violation]
pub struct SuperWithoutBrackets;

impl AlwaysFixableViolation for SuperWithoutBrackets {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`super` call without parentheses")
    }

    fn fix_title(&self) -> String {
        "Add parentheses to `super` call".to_string()
    }
}

/// PLW0245
pub(crate) fn super_without_brackets(checker: &mut Checker, func: &Expr) {
    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    }) = checker.semantic().current_scope().kind
    else {
        return;
    };

    let Some(parent) = &checker
        .semantic()
        .first_non_type_parent_scope(checker.semantic().current_scope())
    else {
        return;
    };

    match function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    ) {
        function_type::FunctionType::Method { .. }
        | function_type::FunctionType::ClassMethod { .. }
        | function_type::FunctionType::StaticMethod { .. } => {}
        function_type::FunctionType::Function { .. } => return,
    }

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

    let mut diagnostic = Diagnostic::new(SuperWithoutBrackets, value.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "super()".to_string(),
        value.range(),
    )));

    checker.diagnostics.push(diagnostic);
}

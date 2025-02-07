use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::Scope;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for methods decorated with `@singledispatch`.
///
/// ## Why is this bad?
/// The `@singledispatch` decorator is intended for use with functions, not methods.
///
/// Instead, use the `@singledispatchmethod` decorator, or migrate the method to a
/// standalone function.
///
/// ## Example
///
/// ```python
/// from functools import singledispatch
///
///
/// class Class:
///     @singledispatch
///     def method(self, arg): ...
/// ```
///
/// Use instead:
///
/// ```python
/// from functools import singledispatchmethod
///
///
/// class Class:
///     @singledispatchmethod
///     def method(self, arg): ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as migrating from `@singledispatch` to
/// `@singledispatchmethod` may change the behavior of the code.
#[derive(ViolationMetadata)]
pub(crate) struct SingledispatchMethod;

impl Violation for SingledispatchMethod {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`@singledispatch` decorator should not be used on methods".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `@singledispatchmethod`".to_string())
    }
}

/// E1519
pub(crate) fn singledispatch_method(checker: &Checker, scope: &Scope) {
    let Some(func) = scope.kind.as_function() else {
        return;
    };

    let ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    } = func;

    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    let type_ = function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );
    if !matches!(
        type_,
        function_type::FunctionType::Method
            | function_type::FunctionType::ClassMethod
            | function_type::FunctionType::StaticMethod
    ) {
        return;
    }

    for decorator in decorator_list {
        if checker
            .semantic()
            .resolve_qualified_name(&decorator.expression)
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["functools", "singledispatch"])
            })
        {
            let mut diagnostic = Diagnostic::new(SingledispatchMethod, decorator.range());
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import("functools", "singledispatchmethod"),
                    decorator.start(),
                    checker.semantic(),
                )?;
                Ok(Fix::unsafe_edits(
                    Edit::range_replacement(binding, decorator.expression.range()),
                    [import_edit],
                ))
            });
            checker.report_diagnostic(diagnostic);
        }
    }
}

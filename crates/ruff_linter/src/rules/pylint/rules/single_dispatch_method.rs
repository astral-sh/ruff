use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::{analyze::function_type, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for singledispatch decorators on class methods.
///
/// ## Why is this bad?
/// Single dispatch must happen on the type of first non self argument
#[violation]
pub struct SingleDispatchMethod;

impl Violation for SingleDispatchMethod {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("singledispatch decorator should not be used with methods, use singledispatchmethod instead.")
    }
}

/// E1519
pub(crate) fn single_dispatch_method(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(func) = scope.kind.as_function() else {
        return;
    };

    let ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    } = func;

    let Some(parent) = &checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            parent,
            checker.semantic(),
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method | function_type::FunctionType::ClassMethod
    ) {
        return;
    }

    for decorator in decorator_list {
        if checker
            .semantic()
            .resolve_call_path(&decorator.expression)
            .is_some_and(|call_path| {
                matches!(call_path.as_slice(), ["functools", "singledispatch"])
            })
        {
            diagnostics.push(Diagnostic::new(SingleDispatchMethod {}, decorator.range()));
        }
    }
}

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, identifier::Identifier};
use ruff_python_semantic::analyze::{function_type, visibility};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions that include too many positional arguments.
///
/// By default, this rule allows up to five arguments, as configured by the
/// [`lint.pylint.max-positional-args`] option.
///
/// ## Why is this bad?
/// Functions with many arguments are harder to understand, maintain, and call.
/// This is especially true for functions with many positional arguments, as
/// providing arguments positionally is more error-prone and less clear to
/// readers than providing arguments by name.
///
/// Consider refactoring functions with many arguments into smaller functions
/// with fewer arguments, using objects to group related arguments, or migrating to
/// [keyword-only arguments](https://docs.python.org/3/tutorial/controlflow.html#special-parameters).
///
/// ## Example
///
/// ```python
/// def plot(x, y, z, color, mark, add_trendline): ...
///
///
/// plot(1, 2, 3, "r", "*", True)
/// ```
///
/// Use instead:
///
/// ```python
/// def plot(x, y, z, *, color, mark, add_trendline): ...
///
///
/// plot(1, 2, 3, color="r", mark="*", add_trendline=True)
/// ```
///
/// ## Options
/// - `lint.pylint.max-positional-args`
#[violation]
pub struct TooManyPositionalArguments {
    c_pos: usize,
    max_pos: usize,
}

impl Violation for TooManyPositionalArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyPositionalArguments { c_pos, max_pos } = self;
        format!("Too many positional arguments ({c_pos}/{max_pos})")
    }
}

/// PLR0917
pub(crate) fn too_many_positional_arguments(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    let semantic = checker.semantic();

    // Count the number of positional arguments.
    let num_positional_args = function_def
        .parameters
        .posonlyargs
        .iter()
        .chain(&function_def.parameters.args)
        .filter(|param| {
            !checker
                .settings
                .dummy_variable_rgx
                .is_match(&param.parameter.name)
        })
        .count();

    if num_positional_args <= checker.settings.pylint.max_positional_args {
        return;
    }

    // Allow excessive arguments in `@override` or `@overload` methods, since they're required
    // to adhere to the parent signature.
    if visibility::is_override(&function_def.decorator_list, semantic)
        || visibility::is_overload(&function_def.decorator_list, semantic)
    {
        return;
    }

    // Check if the function is a method or class method.
    let num_positional_args = if matches!(
        function_type::classify(
            &function_def.name,
            &function_def.decorator_list,
            semantic.current_scope(),
            semantic,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method | function_type::FunctionType::ClassMethod
    ) {
        // If so, we need to subtract one from the number of positional arguments, since the first
        // argument is always `self` or `cls`.
        num_positional_args.saturating_sub(1)
    } else {
        num_positional_args
    };

    if num_positional_args <= checker.settings.pylint.max_positional_args {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        TooManyPositionalArguments {
            c_pos: num_positional_args,
            max_pos: checker.settings.pylint.max_positional_args,
        },
        function_def.identifier(),
    ));
}

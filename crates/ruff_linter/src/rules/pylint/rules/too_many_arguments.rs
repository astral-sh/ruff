use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::{function_type, visibility};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions that include too many arguments.
///
/// By default, this rule allows up to five arguments, as configured by the
/// [`lint.pylint.max-args`] option.
///
/// ## Why is this bad?
/// Functions with many arguments are harder to understand, maintain, and call.
/// Consider refactoring functions with many arguments into smaller functions
/// with fewer arguments, or using objects to group related arguments.
///
/// ## Example
/// ```python
/// def calculate_position(x_pos, y_pos, z_pos, x_vel, y_vel, z_vel, time):
///     new_x = x_pos + x_vel * time
///     new_y = y_pos + y_vel * time
///     new_z = z_pos + z_vel * time
///     return new_x, new_y, new_z
/// ```
///
/// Use instead:
/// ```python
/// from typing import NamedTuple
///
///
/// class Vector(NamedTuple):
///     x: float
///     y: float
///     z: float
///
///
/// def calculate_position(pos: Vector, vel: Vector, time: float) -> Vector:
///     return Vector(*(p + v * time for p, v in zip(pos, vel)))
/// ```
///
/// ## Options
/// - `lint.pylint.max-args`
#[derive(ViolationMetadata)]
pub(crate) struct TooManyArguments {
    c_args: usize,
    max_args: usize,
}

impl Violation for TooManyArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyArguments { c_args, max_args } = self;
        format!("Too many arguments in function definition ({c_args} > {max_args})")
    }
}

/// PLR0913
pub(crate) fn too_many_arguments(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    // https://github.com/astral-sh/ruff/issues/14535
    if checker.source_type.is_stub() {
        return;
    }
    let semantic = checker.semantic();

    let num_arguments = function_def
        .parameters
        .iter_non_variadic_params()
        .filter(|param| !checker.settings.dummy_variable_rgx.is_match(param.name()))
        .count();

    if num_arguments <= checker.settings.pylint.max_args {
        return;
    }

    // Allow excessive arguments in `@override` or `@overload` methods, since they're required
    // to adhere to the parent signature.
    if visibility::is_override(&function_def.decorator_list, checker.semantic())
        || visibility::is_overload(&function_def.decorator_list, checker.semantic())
    {
        return;
    }

    // Check if the function is a method or class method.
    let num_arguments = if matches!(
        function_type::classify(
            &function_def.name,
            &function_def.decorator_list,
            semantic.current_scope(),
            semantic,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
            | function_type::FunctionType::ClassMethod
            | function_type::FunctionType::NewMethod
    ) {
        // If so, we need to subtract one from the number of positional arguments, since the first
        // argument is always `self` or `cls`.
        num_arguments.saturating_sub(1)
    } else {
        num_arguments
    };

    if num_arguments <= checker.settings.pylint.max_args {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(
        TooManyArguments {
            c_args: num_arguments,
            max_args: checker.settings.pylint.max_args,
        },
        function_def.identifier(),
    ));
}

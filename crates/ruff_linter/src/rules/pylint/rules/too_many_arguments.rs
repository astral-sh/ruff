use ruff_python_ast::{Parameters, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions that include too many arguments.
///
/// By default, this rule allows up to five arguments, as configured by the
/// [`pylint.max-args`] option.
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
/// - `pylint.max-args`
#[violation]
pub struct TooManyArguments {
    c_args: usize,
    max_args: usize,
}

impl Violation for TooManyArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyArguments { c_args, max_args } = self;
        format!("Too many arguments to function call ({c_args} > {max_args})")
    }
}

/// PLR0913
pub(crate) fn too_many_arguments(checker: &mut Checker, parameters: &Parameters, stmt: &Stmt) {
    let num_arguments = parameters
        .args
        .iter()
        .chain(&parameters.kwonlyargs)
        .chain(&parameters.posonlyargs)
        .filter(|arg| {
            !checker
                .settings
                .dummy_variable_rgx
                .is_match(&arg.parameter.name)
        })
        .count();
    if num_arguments > checker.settings.pylint.max_args {
        checker.diagnostics.push(Diagnostic::new(
            TooManyArguments {
                c_args: num_arguments,
                max_args: checker.settings.pylint.max_args,
            },
            stmt.identifier(),
        ));
    }
}

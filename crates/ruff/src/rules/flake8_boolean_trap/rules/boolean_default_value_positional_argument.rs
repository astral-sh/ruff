use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::{Decorator, ParameterWithDefault, Parameters};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_boolean_trap::helpers::{is_allowed_func_def, is_boolean};

/// ## What it does
/// Checks for the use of boolean positional arguments in function definitions,
/// as determined by the presence of a boolean default value.
///
/// ## Why is this bad?
/// Calling a function with boolean positional arguments is confusing as the
/// meaning of the boolean value is not clear to the caller and to future
/// readers of the code.
///
/// The use of a boolean will also limit the function to only two possible
/// behaviors, which makes the function difficult to extend in the future.
///
/// Instead, consider refactoring into separate implementations for the
/// `True` and `False` cases, using an `Enum`, or making the argument a
/// keyword-only argument, to force callers to be explicit when providing
/// the argument.
///
/// ## Example
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number, up=True):
///     return ceil(number) if up else floor(number)
///
///
/// round_number(1.5, True)  # What does `True` mean?
/// round_number(1.5, False)  # What does `False` mean?
/// ```
///
/// Instead, refactor into separate implementations:
/// ```python
/// from math import ceil, floor
///
///
/// def round_up(number):
///     return ceil(number)
///
///
/// def round_down(number):
///     return floor(number)
///
///
/// round_up(1.5)
/// round_down(1.5)
/// ```
///
/// Or, refactor to use an `Enum`:
/// ```python
/// from enum import Enum
///
///
/// class RoundingMethod(Enum):
///     UP = 1
///     DOWN = 2
///
///
/// def round_number(value, method):
///     ...
/// ```
///
/// Or, make the argument a keyword-only argument:
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number, *, up=True):
///     return ceil(number) if up else floor(number)
///
///
/// round_number(1.5, up=True)
/// round_number(1.5, up=False)
/// ```
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
#[violation]
pub struct BooleanDefaultValuePositionalArgument;

impl Violation for BooleanDefaultValuePositionalArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean default positional argument in function definition")
    }
}

pub(crate) fn boolean_default_value_positional_argument(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Decorator],
    parameters: &Parameters,
) {
    if is_allowed_func_def(name) {
        return;
    }

    if decorator_list.iter().any(|decorator| {
        collect_call_path(&decorator.expression)
            .is_some_and(|call_path| call_path.as_slice() == [name, "setter"])
    }) {
        return;
    }

    for ParameterWithDefault {
        parameter,
        default,
        range: _,
    } in parameters.posonlyargs.iter().chain(&parameters.args)
    {
        if default.as_ref().is_some_and(|default| is_boolean(default)) {
            checker.diagnostics.push(Diagnostic::new(
                BooleanDefaultValuePositionalArgument,
                parameter.name.range(),
            ));
        }
    }
}

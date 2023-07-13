use rustpython_parser::ast::{self, ArgWithDefault, Arguments, Constant, Decorator, Expr, Ranged};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;

use crate::checkers::ast::Checker;
use crate::rules::flake8_boolean_trap::helpers::is_allowed_func_def;

/// ## What it does
/// Checks for boolean positional arguments in function definitions.
///
/// ## Why is this bad?
/// Calling a function with boolean positional arguments is confusing as the
/// meaning of the boolean value is not clear to the caller, and to future
/// readers of the code.
///
/// The use of a boolean will also limit the function to only two possible
/// behaviors, which makes the function difficult to extend in the future.
///
/// ## Example
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number: float, up: bool) -> int:
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
/// def round_up(number: float) -> int:
///     return ceil(number)
///
///
/// def round_down(number: float) -> int:
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
/// def round_number(value: float, method: RoundingMethod) -> float:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
#[violation]
pub struct BooleanPositionalArgInFunctionDefinition;

impl Violation for BooleanPositionalArgInFunctionDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional arg in function definition")
    }
}

pub(crate) fn check_positional_boolean_in_def(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Decorator],
    arguments: &Arguments,
) {
    if is_allowed_func_def(name) {
        return;
    }

    if decorator_list.iter().any(|decorator| {
        collect_call_path(&decorator.expression)
            .map_or(false, |call_path| call_path.as_slice() == [name, "setter"])
    }) {
        return;
    }

    for ArgWithDefault {
        def,
        default: _,
        range: _,
    } in arguments.posonlyargs.iter().chain(&arguments.args)
    {
        if def.annotation.is_none() {
            continue;
        }
        let Some(expr) = &def.annotation else {
            continue;
        };

        // check for both bool (python class) and 'bool' (string annotation)
        let hint = match expr.as_ref() {
            Expr::Name(name) => &name.id == "bool",
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(value),
                ..
            }) => value == "bool",
            _ => false,
        };
        if !hint {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            BooleanPositionalArgInFunctionDefinition,
            def.range(),
        ));
    }
}

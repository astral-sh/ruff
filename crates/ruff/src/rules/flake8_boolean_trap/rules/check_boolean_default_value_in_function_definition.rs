use rustpython_parser::ast::{Arguments, Expr};

use ruff_diagnostics::Violation;

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;

use crate::checkers::ast::Checker;
use crate::rules::flake8_boolean_trap::helpers::add_if_boolean;

use super::super::helpers::FUNC_DEF_NAME_ALLOWLIST;

/// ## What it does
/// Checks for the use of booleans as default values in function definitions.
///
/// ## Why is this bad?
/// Calling a function with boolean default means that the keyword argument
/// argument can be omitted, which makes the function call ambiguous.
///
/// Instead, define the relevant argument as keyword-only.
///
/// ## Example
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number: float, *, up: bool = True) -> int:
///     return ceil(number) if up else floor(number)
///
///
/// round_number(1.5)
/// round_number(1.5, up=False)
/// ```
///
/// Use instead:
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number: float, *, up: bool) -> int:
///     return ceil(number) if up else floor(number)
///
///
/// round_number(1.5, up=True)
/// round_number(1.5, up=False)
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
#[violation]
pub struct BooleanDefaultValueInFunctionDefinition;

impl Violation for BooleanDefaultValueInFunctionDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean default value in function definition")
    }
}

pub(crate) fn check_boolean_default_value_in_function_definition(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Expr],
    arguments: &Arguments,
) {
    if FUNC_DEF_NAME_ALLOWLIST.contains(&name) {
        return;
    }

    if decorator_list.iter().any(|expr| {
        collect_call_path(expr).map_or(false, |call_path| call_path.as_slice() == [name, "setter"])
    }) {
        return;
    }

    for arg in &arguments.defaults {
        add_if_boolean(checker, arg, BooleanDefaultValueInFunctionDefinition.into());
    }
}

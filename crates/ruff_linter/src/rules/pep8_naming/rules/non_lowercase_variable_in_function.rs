use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::str;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pep8_naming::helpers;

/// ## What it does
/// Checks for the use of non-lowercase variable names in functions.
///
/// ## Why is this bad?
/// [PEP 8] recommends that all function variables use lowercase names:
///
/// > Function names should be lowercase, with words separated by underscores as necessary to
/// > improve readability. Variable names follow the same convention as function names. mixedCase
/// > is allowed only in contexts where that's already the prevailing style (e.g. threading.py),
/// > to retain backwards compatibility.
///
/// ## Example
/// ```python
/// def my_function(a):
///     B = a + 3
///     return B
/// ```
///
/// Use instead:
/// ```python
/// def my_function(a):
///     b = a + 3
///     return b
/// ```
///
/// ## Options
/// - `pep8-naming.ignore-names`
/// - `pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-variable-names
#[violation]
pub struct NonLowercaseVariableInFunction {
    name: String,
}

impl Violation for NonLowercaseVariableInFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonLowercaseVariableInFunction { name } = self;
        format!("Variable `{name}` in function should be lowercase")
    }
}

/// N806
pub(crate) fn non_lowercase_variable_in_function(checker: &mut Checker, expr: &Expr, name: &str) {
    if checker
        .settings
        .pep8_naming
        .ignore_names
        .iter()
        .any(|ignore_name| ignore_name.matches(name))
    {
        return;
    }

    if str::is_lowercase(name) {
        return;
    }

    let parent = checker.semantic().current_statement();
    if helpers::is_named_tuple_assignment(parent, checker.semantic())
        || helpers::is_typed_dict_assignment(parent, checker.semantic())
        || helpers::is_type_var_assignment(parent, checker.semantic())
        || helpers::is_type_alias_assignment(parent, checker.semantic())
    {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        NonLowercaseVariableInFunction {
            name: name.to_string(),
        },
        expr.range(),
    ));
}

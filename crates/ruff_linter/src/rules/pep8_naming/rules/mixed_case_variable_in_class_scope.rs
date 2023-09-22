use ruff_python_ast::{Arguments, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pep8_naming::helpers;

/// ## What it does
/// Checks for class variable names that follow the `mixedCase` convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends that variable names should be lower case and separated
/// by underscores (also known as `snake_case`).
///
/// > Function names should be lowercase, with words separated by underscores
/// as necessary to improve readability.
/// >
/// > Variable names follow the same convention as function names.
/// >
/// > mixedCase is allowed only in contexts where that’s already the
/// prevailing style (e.g. threading.py), to retain backwards compatibility.
///
/// ## Example
/// ```python
/// class MyClass:
///     myVariable = "hello"
///     another_variable = "world"
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     my_variable = "hello"
///     another_variable = "world"
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[violation]
pub struct MixedCaseVariableInClassScope {
    name: String,
}

impl Violation for MixedCaseVariableInClassScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInClassScope { name } = self;
        format!("Variable `{name}` in class scope should not be mixedCase")
    }
}

/// N815
pub(crate) fn mixed_case_variable_in_class_scope(
    checker: &mut Checker,
    expr: &Expr,
    name: &str,
    arguments: Option<&Arguments>,
) {
    if checker
        .settings
        .pep8_naming
        .ignore_names
        .iter()
        .any(|ignore_name| ignore_name.matches(name))
    {
        return;
    }
    if !helpers::is_mixed_case(name) {
        return;
    }

    let parent = checker.semantic().current_statement();

    if helpers::is_named_tuple_assignment(parent, checker.semantic())
        || helpers::is_typed_dict_class(arguments, checker.semantic())
    {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        MixedCaseVariableInClassScope {
            name: name.to_string(),
        },
        expr.range(),
    ));
}

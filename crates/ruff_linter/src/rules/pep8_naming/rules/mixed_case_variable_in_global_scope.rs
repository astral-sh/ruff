use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pep8_naming::helpers;

/// ## What it does
/// Checks for global variable names that follow the `mixedCase` convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends that global variable names should be lower case and
/// separated by underscores (also known as `snake_case`).
///
/// > ### Global Variable Names
/// > (Let’s hope that these variables are meant for use inside one module
/// only.) The conventions are about the same as those for functions.
/// >
/// > Modules that are designed for use via from M import * should use the
/// __all__ mechanism to prevent exporting globals, or use the older
/// convention of prefixing such globals with an underscore (which you might
/// want to do to indicate these globals are “module non-public”).
/// >
/// > ### Function and Variable Names
/// > Function names should be lowercase, with words separated by underscores
/// as necessary to improve readability.
/// >
/// > Variable names follow the same convention as function names.
/// >
/// > mixedCase is allowed only in contexts where that’s already the prevailing
/// style (e.g. threading.py), to retain backwards compatibility.
///
/// ## Example
/// ```python
/// myVariable = "hello"
/// another_variable = "world"
/// yet_anotherVariable = "foo"
/// ```
///
/// Use instead:
/// ```python
/// my_variable = "hello"
/// another_variable = "world"
/// yet_another_variable = "foo"
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#global-variable-names
#[violation]
pub struct MixedCaseVariableInGlobalScope {
    name: String,
}

impl Violation for MixedCaseVariableInGlobalScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInGlobalScope { name } = self;
        format!("Variable `{name}` in global scope should not be mixedCase")
    }
}

/// N816
pub(crate) fn mixed_case_variable_in_global_scope(checker: &mut Checker, expr: &Expr, name: &str) {
    if !helpers::is_mixed_case(name) {
        return;
    }

    let parent = checker.semantic().current_statement();
    if helpers::is_named_tuple_assignment(parent, checker.semantic()) {
        return;
    }

    if checker.settings.pep8_naming.ignore_names.matches(name) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        MixedCaseVariableInGlobalScope {
            name: name.to_string(),
        },
        expr.range(),
    ));
}

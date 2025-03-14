use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
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
/// > as necessary to improve readability.
/// >
/// > Variable names follow the same convention as function names.
/// >
/// > mixedCase is allowed only in contexts where that’s already the
/// > prevailing style (e.g. threading.py), to retain backwards compatibility.
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
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[derive(ViolationMetadata)]
pub(crate) struct MixedCaseVariableInClassScope {
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
    checker: &Checker,
    expr: &Expr,
    name: &str,
    class_def: &ast::StmtClassDef,
) {
    if !helpers::is_mixed_case(name) {
        return;
    }

    let parent = checker.semantic().current_statement();

    if helpers::is_named_tuple_assignment(parent, checker.semantic())
        || helpers::is_typed_dict_class(class_def, checker.semantic())
    {
        return;
    }

    if checker.settings.pep8_naming.ignore_names.matches(name) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(
        MixedCaseVariableInClassScope {
            name: name.to_string(),
        },
        expr.range(),
    ));
}

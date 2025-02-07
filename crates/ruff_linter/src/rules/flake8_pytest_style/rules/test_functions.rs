use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::rules::helpers::is_likely_pytest_test;
use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::StmtFunctionDef;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for parameters of test functions with default arguments.
///
/// ## Why is this bad?
/// Such a parameter will always have the default value during the test
/// regardless of whether a fixture with the same name is defined.
///
/// ## Example
///
/// ```python
/// def test_foo(a=1): ...
/// ```
///
/// Use instead:
///
/// ```python
/// def test_foo(a): ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as modifying a function signature can
/// change the behavior of the code.
///
/// ## References
/// - [Original Pytest issue](https://github.com/pytest-dev/pytest/issues/12693)
#[derive(ViolationMetadata)]
pub(crate) struct PytestParameterWithDefaultArgument {
    parameter_name: String,
}

impl Violation for PytestParameterWithDefaultArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Test function parameter `{}` has default argument",
            self.parameter_name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove default argument".to_string())
    }
}

/// PT028
pub(crate) fn parameter_with_default_argument(checker: &Checker, function_def: &StmtFunctionDef) {
    if !is_likely_pytest_test(function_def, checker) {
        return;
    }

    for parameter in function_def.parameters.iter_non_variadic_params() {
        let Some(default) = parameter.default() else {
            continue;
        };
        let parameter_name = parameter.name().to_string();
        let kind = PytestParameterWithDefaultArgument { parameter_name };
        let diagnostic = Diagnostic::new(kind, default.range());
        let edit = Edit::deletion(parameter.parameter.end(), parameter.end());
        let fix = Fix::display_only_edit(edit);
        checker.report_diagnostic(diagnostic.with_fix(fix));
    }
}

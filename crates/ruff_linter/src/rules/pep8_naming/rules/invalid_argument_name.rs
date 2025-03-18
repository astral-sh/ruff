use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{ExprLambda, Parameters, StmtFunctionDef};
use ruff_python_semantic::analyze::visibility::is_override;
use ruff_python_semantic::ScopeKind;
use ruff_python_stdlib::str;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for argument names that do not follow the `snake_case` convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends that function names should be lower case and separated
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
/// Methods decorated with `@typing.override` are ignored.
///
/// ## Example
/// ```python
/// def my_function(A, myArg):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def my_function(a, my_arg):
///     pass
/// ```
///
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct InvalidArgumentName {
    name: String,
}

impl Violation for InvalidArgumentName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidArgumentName { name } = self;
        format!("Argument name `{name}` should be lowercase")
    }
}

/// N803
pub(crate) fn invalid_argument_name_function(checker: &Checker, function_def: &StmtFunctionDef) {
    let semantic = checker.semantic();
    let scope = semantic.current_scope();

    if matches!(scope.kind, ScopeKind::Class(_))
        && is_override(&function_def.decorator_list, semantic)
    {
        return;
    }

    invalid_argument_name(checker, &function_def.parameters);
}

/// N803
pub(crate) fn invalid_argument_name_lambda(checker: &Checker, lambda: &ExprLambda) {
    let Some(parameters) = &lambda.parameters else {
        return;
    };

    invalid_argument_name(checker, parameters);
}

/// N803
fn invalid_argument_name(checker: &Checker, parameters: &Parameters) {
    let ignore_names = &checker.settings.pep8_naming.ignore_names;

    for parameter in parameters {
        let name = parameter.name().as_str();

        if str::is_lowercase(name) {
            continue;
        }

        if ignore_names.matches(name) {
            continue;
        }

        let diagnostic = Diagnostic::new(
            InvalidArgumentName {
                name: name.to_string(),
            },
            parameter.range(),
        );

        checker.report_diagnostic(diagnostic);
    }
}

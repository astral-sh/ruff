use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ruff_python_semantic::analyze::function_type;

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for the presence of [PEP 484]-style positional-only parameters.
///
/// ## Why is this bad?
/// Historically, [PEP 484] recommended prefixing parameter names with double
/// underscores (`__`) to indicate to a type checker that they were
/// positional-only. However, [PEP 570] (introduced in Python 3.8) introduced
/// dedicated syntax for positional-only arguments. If a forward slash (`/`) is
/// present in a function signature on Python 3.8+, all parameters prior to the
/// slash are interpreted as positional-only.
///
/// The new syntax should be preferred as it is more widely used, more concise
/// and more readable. It is also respected by Python at runtime, whereas the
/// old-style syntax was only understood by type checkers.
///
/// ## Example
///
/// ```pyi
/// def foo(__x: int) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def foo(x: int, /) -> None: ...
/// ```
///
/// ## Options
/// - `target-version`
///
/// [PEP 484]: https://peps.python.org/pep-0484/#positional-only-arguments
/// [PEP 570]: https://peps.python.org/pep-0570
#[derive(ViolationMetadata)]
pub(crate) struct Pep484StylePositionalOnlyParameter;

impl Violation for Pep484StylePositionalOnlyParameter {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use PEP 570 syntax for positional-only parameters".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add `/` to function signature".to_string())
    }
}

/// PYI063
pub(crate) fn pep_484_positional_parameter(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    // PEP 570 was introduced in Python 3.8.
    if checker.target_version() < PythonVersion::PY38 {
        return;
    }

    if !function_def.parameters.posonlyargs.is_empty() {
        return;
    }

    if function_def.parameters.args.is_empty() {
        return;
    }

    let semantic = checker.semantic();
    let scope = semantic.current_scope();
    let function_type = function_type::classify(
        &function_def.name,
        &function_def.decorator_list,
        scope,
        semantic,
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );

    // If the method has a `self` or `cls` argument, skip it.
    let skip = usize::from(matches!(
        function_type,
        function_type::FunctionType::Method | function_type::FunctionType::ClassMethod
    ));

    if let Some(arg) = function_def.parameters.args.get(skip) {
        if is_old_style_positional_only(arg) {
            checker.report_diagnostic(Diagnostic::new(
                Pep484StylePositionalOnlyParameter,
                arg.identifier(),
            ));
        }
    }
}

/// Returns `true` if the [`ParameterWithDefault`] is an old-style positional-only parameter (i.e.,
/// its name starts with `__` and does not end with `__`).
fn is_old_style_positional_only(param: &ParameterWithDefault) -> bool {
    let arg_name = param.name();
    arg_name.starts_with("__") && !arg_name.ends_with("__")
}

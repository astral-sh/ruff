use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ruff_python_semantic::analyze::function_type;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for the presence of [PEP 484]-style positional-only arguments.
///
/// ## Why is this bad?
/// Historically, [PEP 484] recommended prefixing positional-only arguments
/// with a double underscore (`__`). However, [PEP 570] introduced a dedicated
/// syntax for positional-only arguments, which should be preferred.
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
/// [PEP 484]: https://peps.python.org/pep-0484/#positional-only-arguments
/// [PEP 570]: https://peps.python.org/pep-0570
#[violation]
pub struct PrePep570PositionalArgument;

impl Violation for PrePep570PositionalArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use PEP 570 syntax for positional-only arguments")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add `/` to function signature".to_string())
    }
}

/// PYI063
pub(crate) fn pre_pep570_positional_argument(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    // PEP 570 was introduced in Python 3.8.
    if checker.settings.target_version < PythonVersion::Py38 {
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
        if is_pre_pep570_positional_only(arg) {
            checker.diagnostics.push(Diagnostic::new(
                PrePep570PositionalArgument,
                arg.identifier(),
            ));
        }
    }
}

/// Returns `true` if the [`ParameterWithDefault`] is an old-style positional-only argument (i.e.,
/// its name starts with `__` and does not end with `__`).
fn is_pre_pep570_positional_only(arg: &ParameterWithDefault) -> bool {
    let arg_name = &arg.parameter.name;
    arg_name.starts_with("__") && !arg_name.ends_with("__")
}

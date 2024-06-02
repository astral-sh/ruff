use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ParameterWithDefault;
use ruff_python_semantic::{analyze::function_type, Definition};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of old-style positional-only arguments in stub files.
///
/// ## Why is this bad?
/// [PEP 570][1] defines new syntax for positional-only arguments, that should
/// be preferred over the syntax defined in [PEP 484][2].
///
/// ## Example
/// ```python
/// def foo(__x: int) -> None:
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int, /) -> None: ...
/// ```
///
/// [1]: https://peps.python.org/pep-0570
/// [2]: https://peps.python.org/pep-0484/#positional-only-arguments
#[violation]
pub struct OldStylePositionalOnlyArg;

impl Violation for OldStylePositionalOnlyArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer PEP 570 syntax for positional-only arguments in stubs")
    }
}

/// PYI063
pub(crate) fn old_style_positional_only_arg(checker: &mut Checker, definition: &Definition) {
    let Some(function) = definition.as_function_def() else {
        return;
    };
    if !function.parameters.posonlyargs.is_empty() {
        return;
    }

    let mut args = function.parameters.args.iter();
    let Some(first_arg) = args.next() else {
        return;
    };

    let semantic = checker.semantic();
    // TODO: this scope is wrong.
    let scope = semantic.current_scope();
    let function_type = function_type::classify(
        &function.name,
        &function.decorator_list,
        scope,
        semantic,
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );
    if is_old_style_positional_only_arg(first_arg) {
        checker.diagnostics.push(Diagnostic::new(
            OldStylePositionalOnlyArg,
            first_arg.range(),
        ));
    }
    if matches!(function_type, function_type::FunctionType::Method) {
        if let Some(second_arg) = args.next() {
            if is_old_style_positional_only_arg(second_arg) {
                checker.diagnostics.push(Diagnostic::new(
                    OldStylePositionalOnlyArg,
                    second_arg.range(),
                ));
            }
        }
    }
}

fn is_old_style_positional_only_arg(arg: &ParameterWithDefault) -> bool {
    let arg_name = &arg.parameter.name;
    arg_name.starts_with("__") && !arg_name.ends_with("__")
}

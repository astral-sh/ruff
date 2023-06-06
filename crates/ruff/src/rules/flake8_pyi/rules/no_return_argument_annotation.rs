use itertools::chain;
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::Arguments;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion::Py311;

#[violation]
pub struct NoReturnArgumentAnnotationInStub {
    is_never_builtin: bool,
}

/// ## What it does
/// Checks for usages of `typing.NoReturn` or `typing_extensions.NoReturn` in stubs.
///
/// ## Why is this bad?
/// Use of `typing.Never` or `typing_extensions.Never` can make your intentions clearer. This is a
/// purely stylistic choice in the name of readability.
///
/// ## Example
/// ```python
/// def foo(arg: typing.NoReturn): ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(arg: typing.Never): ...
/// ```
impl Violation for NoReturnArgumentAnnotationInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let typing_name = if self.is_never_builtin {
            "typing"
        } else {
            "typing_extensions"
        };
        format!("Prefer {typing_name}.Never over NoReturn for argument annotations.")
    }
}

/// PYI050
pub(crate) fn no_return_argument_annotation(checker: &mut Checker, args: &Arguments) {
    let annotations = chain!(
        args.args.iter(),
        args.posonlyargs.iter(),
        args.kwonlyargs.iter()
    )
    .filter_map(|arg| arg.annotation.as_ref());

    let is_never_builtin = checker.settings.target_version >= Py311;

    for annotation in annotations {
        if checker
            .semantic_model()
            .match_typing_expr(annotation, "NoReturn")
        {
            checker.diagnostics.push(Diagnostic::new(
                NoReturnArgumentAnnotationInStub { is_never_builtin },
                annotation.range(),
            ));
        }
    }
}

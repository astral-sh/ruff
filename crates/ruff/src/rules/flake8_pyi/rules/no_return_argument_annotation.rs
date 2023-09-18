use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Parameters;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion::Py311;

#[violation]
pub struct NoReturnArgumentAnnotationInStub {
    module: TypingModule,
}

/// ## What it does
/// Checks for uses of `typing.NoReturn` (and `typing_extensions.NoReturn`) in
/// stubs.
///
/// ## Why is this bad?
/// Prefer `typing.Never` (or `typing_extensions.Never`) over `typing.NoReturn`,
/// as the former is more explicit about the intent of the annotation. This is
/// a purely stylistic choice, as the two are semantically equivalent.
///
/// ## Example
/// ```python
/// from typing import NoReturn
///
///
/// def foo(x: NoReturn): ...
/// ```
///
/// Use instead:
/// ```python
/// from typing import Never
///
///
/// def foo(x: Never): ...
/// ```
impl Violation for NoReturnArgumentAnnotationInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoReturnArgumentAnnotationInStub { module } = self;
        format!("Prefer `{module}.Never` over `NoReturn` for argument annotations")
    }
}

/// PYI050
pub(crate) fn no_return_argument_annotation(checker: &mut Checker, parameters: &Parameters) {
    for annotation in parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
        .filter_map(|arg| arg.parameter.annotation.as_ref())
    {
        if checker.semantic().match_typing_expr(annotation, "NoReturn") {
            checker.diagnostics.push(Diagnostic::new(
                NoReturnArgumentAnnotationInStub {
                    module: if checker.settings.target_version >= Py311 {
                        TypingModule::Typing
                    } else {
                        TypingModule::TypingExtensions
                    },
                },
                annotation.range(),
            ));
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TypingModule {
    Typing,
    TypingExtensions,
}

impl fmt::Display for TypingModule {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TypingModule::Typing => fmt.write_str("typing"),
            TypingModule::TypingExtensions => fmt.write_str("typing_extensions"),
        }
    }
}

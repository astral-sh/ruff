use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for uses of `typing.NoReturn` (and `typing_extensions.NoReturn`) for
/// parameter annotations.
///
/// ## Why is this bad?
/// Prefer `Never` over `NoReturn` for parameter annotations. `Never` has a
/// clearer name in these contexts, since it makes little sense to talk about a
/// parameter annotation "not returning".
///
/// This is a purely stylistic lint: the two types have identical semantics for
/// type checkers. Both represent Python's "[bottom type]" (a type that has no
/// members).
///
/// ## Example
/// ```pyi
/// from typing import NoReturn
///
/// def foo(x: NoReturn): ...
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import Never
///
/// def foo(x: Never): ...
/// ```
///
/// ## References
/// - [Python documentation: `typing.Never`](https://docs.python.org/3/library/typing.html#typing.Never)
/// - [Python documentation: `typing.NoReturn`](https://docs.python.org/3/library/typing.html#typing.NoReturn)
///
/// [bottom type]: https://en.wikipedia.org/wiki/Bottom_type
#[derive(ViolationMetadata)]
pub(crate) struct NoReturnArgumentAnnotationInStub {
    module: TypingModule,
}

impl Violation for NoReturnArgumentAnnotationInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoReturnArgumentAnnotationInStub { module } = self;
        format!("Prefer `{module}.Never` over `NoReturn` for argument annotations")
    }
}

/// PYI050
pub(crate) fn no_return_argument_annotation(checker: &Checker, parameters: &ast::Parameters) {
    // Ex) def func(arg: NoReturn): ...
    // Ex) def func(arg: NoReturn, /): ...
    // Ex) def func(*, arg: NoReturn): ...
    // Ex) def func(*args: NoReturn): ...
    // Ex) def func(**kwargs: NoReturn): ...
    for annotation in parameters
        .iter()
        .filter_map(ast::AnyParameterRef::annotation)
    {
        if is_no_return(annotation, checker) {
            checker.report_diagnostic(Diagnostic::new(
                NoReturnArgumentAnnotationInStub {
                    module: if checker.target_version() >= PythonVersion::PY311 {
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

fn is_no_return(expr: &ast::Expr, checker: &Checker) -> bool {
    checker.match_maybe_stringized_annotation(expr, |expr| {
        checker.semantic().match_typing_expr(expr, "NoReturn")
    })
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

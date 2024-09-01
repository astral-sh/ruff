use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__post_init__` dataclass methods with argument defaults.
///
/// ## Why is this bad?
/// Variables that are only used during initialization should be instantiated
/// as an init-only pseudo-field using `dataclasses.InitVar`. According to the
/// [documentation]:
///
/// > Init-only fields are added as parameters to the generated `__init__()`
/// > method, and are passed to the optional `__post_init__()` method. They are
/// > not otherwise used by dataclasses.
///
/// Default values for `__post_init__` arguments that exist as init-only field
/// as well will be overridden by the `dataclasses.InitVar` value.
///
/// ## Example
/// ```python
/// from dataclasses import InitVar, dataclass
///
///
/// @dataclass
/// class Foo:
///     bar: InitVar[int] = 0
///
///     def __post_init__(self, bar: int = 1, baz: int = 2) -> None:
///         print(bar, baz)
///
///
/// foo = Foo()  # Prints '0 2'.
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import InitVar, dataclass
///
///
/// @dataclass
/// class Foo:
///     bar: InitVar[int] = 1
///     baz: InitVar[int] = 2
///
///     def __post_init__(self, bar, baz) -> None:
///         print(bar, baz)
///
///
/// foo = Foo()  # Prints '1 2'.
/// ```
///
/// ## References
/// - [Python documentation: Post-init processing](https://docs.python.org/3/library/dataclasses.html#post-init-processing)
/// - [Python documentation: Init-only variables](https://docs.python.org/3/library/dataclasses.html#init-only-variables)
///
/// [documentation]: https://docs.python.org/3/library/dataclasses.html#init-only-variables
#[violation]
pub struct PostInitDefault;

impl Violation for PostInitDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__post_init__` method has argument defaults, use `dataclasses.InitVar` instead")
    }
}

/// RUF033
pub(crate) fn post_init_default(checker: &mut Checker, function_def: &ast::StmtFunctionDef) {
    if &function_def.name != "__post_init__" {
        return;
    }

    for ParameterWithDefault {
        parameter: _,
        default,
        range: _,
    } in function_def.parameters.iter_non_variadic_params()
    {
        let Some(default) = default else {
            continue;
        };
        checker
            .diagnostics
            .push(Diagnostic::new(PostInitDefault, default.range()));
    }
}

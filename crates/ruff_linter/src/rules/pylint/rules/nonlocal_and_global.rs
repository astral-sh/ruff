use ruff_python_ast::Identifier;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables which are both declared as `nonlocal` and `global`.
///
/// ## Why is this bad?
/// `nonlocal` is included in the scope of `global`, thus this should be replaced with single `global`.
///
/// ## Example
/// ```python
/// counter = 0
/// def increment():
///     global counter
///     nonlocal counter
///     counter += 1
/// ```
///
/// Use instead:
/// ```python
/// counter = 0
/// def increment():
///     global counter
///     counter += 1
/// ```
///
/// ## References
/// - [Pylint documentation: `nonlocal-and-global`](https://pylint.readthedocs.io/en/stable/user_guide/messages/error/nonlocal-and-global.html)
/// - [Python documentation: The `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
/// - [Python documentation: The `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#nonlocal)
#[violation]
pub struct NonlocalAndGlobal {
    pub(crate) name: String,
}

impl Violation for NonlocalAndGlobal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalAndGlobal { name } = self;
        format!("Name `{name}` is nonlocal and global")
    }
}

/// E115
pub(crate) fn nonlocal_and_global(checker: &mut Checker, names: &Vec<Identifier>) {
    // when this function is called, names are all nonlocals.
    // thus we check only if the names are global.
    for name in names {
        if let Some(global_range) = checker.semantic().global(name) {
            checker.diagnostics.push(Diagnostic::new(
                NonlocalAndGlobal {
                    name: name.to_string(),
                },
                global_range,
            ));
        }
    }
}

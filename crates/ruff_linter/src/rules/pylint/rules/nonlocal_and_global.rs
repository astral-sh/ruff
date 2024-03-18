use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables which are both declared as both `nonlocal` and
/// `global`.
///
/// ## Why is this bad?
/// A `nonlocal` variable is a variable that is defined in the nearest
/// enclosing scope, but not in the global scope, while a `global` variable is
/// a variable that is defined in the global scope.
///
/// Declaring a variable as both `nonlocal` and `global` is contradictory and
/// will raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// counter = 0
///
///
/// def increment():
///     global counter
///     nonlocal counter
///     counter += 1
/// ```
///
/// Use instead:
/// ```python
/// counter = 0
///
///
/// def increment():
///     global counter
///     counter += 1
/// ```
///
/// ## References
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
        format!("Name `{name}` is both `nonlocal` and `global`")
    }
}

/// E115
pub(crate) fn nonlocal_and_global(checker: &mut Checker, nonlocal: &ast::StmtNonlocal) {
    // Determine whether any of the newly declared `nonlocal` variables are already declared as
    // `global`.
    for name in &nonlocal.names {
        if let Some(global) = checker.semantic().global(name) {
            checker.diagnostics.push(Diagnostic::new(
                NonlocalAndGlobal {
                    name: name.to_string(),
                },
                global,
            ));
        }
    }
}

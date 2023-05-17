use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for the use of `global` on a variable that is not assigned a value.
///
/// ## Why is this bad?
/// The `global` keyword is needed in an inner scope to modify a variable in an
/// outer scope. If the variable is not assigned a value in the inner scope,
/// there is no need to use `global`.
///
/// ## Example
/// ```python
/// DEBUG = True
///
///
/// def foo():
///     global DEBUG
///     if DEBUG:
///         print("foo() called")
///     ...
/// ```
///
/// Use instead:
/// ```python
/// DEBUG = True
///
///
/// def foo():
///     if DEBUG:
///         print("foo() called")
///     ...
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
#[violation]
pub struct GlobalVariableNotAssigned {
    pub name: String,
}

impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned { name } = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}

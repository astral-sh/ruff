use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_source_file::SourceRow;

/// ## What it does
/// Checks for uses of names that are declared as `global` prior to the
/// relevant `global` declaration.
///
/// ## Why is this bad?
/// The `global` declaration applies to the entire scope. Using a name that's
/// declared as `global` in a given scope prior to the relevant `global`
/// declaration is a `SyntaxError`.
///
/// ## Example
/// ```python
/// counter = 1
///
///
/// def increment():
///     print(f"Adding 1 to {counter}")
///     global counter
///     counter += 1
/// ```
///
/// Use instead:
/// ```python
/// counter = 1
///
///
/// def increment():
///     global counter
///     print(f"Adding 1 to {counter}")
///     counter += 1
/// ```
///
/// ## References
/// - [Python documentation: The `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
#[derive(ViolationMetadata)]
pub(crate) struct LoadBeforeGlobalDeclaration {
    pub(crate) name: String,
    pub(crate) row: SourceRow,
}

impl Violation for LoadBeforeGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoadBeforeGlobalDeclaration { name, row } = self;
        format!("Name `{name}` is used prior to global declaration on {row}")
    }
}

use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for functions/methods that include too many local variables.
///
/// By default, this rule allows up to eleven arguments, as configured by the
/// [`pylint.max-locals`] option.
///
/// ## Why is this bad?
/// Functions with many local variables are harder to understand and maintain.
///
/// Consider refactoring functions with many local variables into smaller
/// functions with fewer assignments.
///
/// ## Options
/// - `pylint.max-locals`
#[violation]
pub struct TooManyLocals {
    pub(crate) current_amount: usize,
    pub(crate) max_amount: usize,
}

impl Violation for TooManyLocals {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyLocals {
            current_amount,
            max_amount,
        } = self;
        format!("Too many local variables: ({current_amount}/{max_amount})")
    }
}

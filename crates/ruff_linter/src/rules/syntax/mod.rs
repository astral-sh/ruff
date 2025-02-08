use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_syntax_errors::{SyntaxError, SyntaxErrorKind};

/// Create wrapper `Violation` types for `SyntaxError`s.
macro_rules! syntax_errors {
    ($($(#[$outer:meta])*$error_type:ident$(,)*)*) => {
        $(#[derive(ViolationMetadata)]
        $(#[$outer])*
        pub(crate) struct $error_type(SyntaxError);

        impl Violation for $error_type {
            #[derive_message_formats]
            fn message(&self) -> String {
                format!("{}", self.0.message())
            }
        })*
    };
}

syntax_errors! {
    /// ## What it does
    ///
    /// Checks for the use of the `match` statement before Python 3.10.
    ///
    /// ## Why is this bad?
    ///
    /// Such usage causes a `SyntaxError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// match var:
    ///     case 1:
    ///         print("it's one")
    ///     case 2:
    ///         print("it's two")
    /// ```
    ///
    /// Use instead:
    ///
    /// ```python
    /// if var == 1:
    ///     print("it's one")
    /// elif var == 2:
    ///     print("it's two")
    /// ```
    MatchBeforePython310,
}

pub(crate) fn diagnostic_from_syntax_error(
    error @ SyntaxError { kind, range, .. }: SyntaxError,
) -> Diagnostic {
    match kind {
        SyntaxErrorKind::MatchBeforePy310 => Diagnostic::new(MatchBeforePython310(error), range),
    }
}

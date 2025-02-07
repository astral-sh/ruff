use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_syntax_errors::{SyntaxError, SyntaxErrorKind};

/// Create wrapper `Violation` types for `SyntaxError`s.
macro_rules! syntax_errors {
    ($($error_type:ident$(,)*)*) => {
        $(#[derive(ViolationMetadata)]
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
    MatchBeforePython310,
}

pub(crate) fn diagnostic_from_syntax_error(
    error @ SyntaxError { kind, range, .. }: SyntaxError,
) -> Diagnostic {
    match kind {
        SyntaxErrorKind::MatchBeforePy310 => Diagnostic::new(MatchBeforePython310(error), range),
    }
}

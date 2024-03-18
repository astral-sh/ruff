use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{ExceptHandler, StmtTry};

const MAX_EXCEPTS: usize = 3;

#[violation]
pub struct TooManyExcepts(usize);

impl Violation for TooManyExcepts {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Too many `except` statements: ({} > {})",
            self.0, MAX_EXCEPTS
        )
    }
}

pub(crate) fn too_many_excepts(stmt: &StmtTry) -> Option<Diagnostic> {
    stmt.handlers
        .iter()
        .skip(MAX_EXCEPTS)
        .take(1)
        .next()
        .map(|handler| {
            Diagnostic::new(TooManyExcepts(stmt.handlers.len()), {
                match handler {
                    ExceptHandler::ExceptHandler(handler) => handler.range,
                }
            })
        })
}

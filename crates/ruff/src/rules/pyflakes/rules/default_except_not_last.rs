use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::except_range;
use ruff_python_ast::source_code::Locator;

#[violation]
pub struct DefaultExceptNotLast;

impl Violation for DefaultExceptNotLast {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("An `except` block as not the last exception handler")
    }
}

/// F707
pub fn default_except_not_last(
    handlers: &[Excepthandler],
    locator: &Locator,
) -> Option<Diagnostic> {
    for (idx, handler) in handlers.iter().enumerate() {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if type_.is_none() && idx < handlers.len() - 1 {
            return Some(Diagnostic::new(
                DefaultExceptNotLast,
                except_range(handler, locator),
            ));
        }
    }

    None
}

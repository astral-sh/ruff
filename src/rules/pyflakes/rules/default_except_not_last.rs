use crate::ast::helpers::except_range;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::source_code::Locator;

use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Excepthandler, ExcepthandlerKind};

define_simple_violation!(
    DefaultExceptNotLast,
    "An `except` block as not the last exception handler"
);

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

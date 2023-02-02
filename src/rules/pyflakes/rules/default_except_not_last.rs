use crate::ast::helpers::except_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violations;
use rustpython_ast::{Excepthandler, ExcepthandlerKind};

/// F707
pub fn default_except_not_last(
    handlers: &[Excepthandler],
    locator: &Locator,
) -> Option<Diagnostic> {
    for (idx, handler) in handlers.iter().enumerate() {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if type_.is_none() && idx < handlers.len() - 1 {
            return Some(Diagnostic::new(
                violations::DefaultExceptNotLast,
                except_range(handler, locator),
            ));
        }
    }

    None
}

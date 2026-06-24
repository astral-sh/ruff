use std::sync::Arc;

use crate::external::ast::registry::ExternalLintRegistry;

/// Shareable handle to the external lint runtime state.
#[derive(Clone, Debug, Default)]
pub struct ExternalLintRuntimeHandle {
    registry: Arc<ExternalLintRegistry>,
}

impl ExternalLintRuntimeHandle {
    pub fn new(registry: ExternalLintRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    pub fn registry(&self) -> &ExternalLintRegistry {
        &self.registry
    }
}

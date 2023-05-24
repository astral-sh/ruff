use ruff_text_size::TextRange;

use ruff_index::{newtype_index, IndexVec};

use crate::context::ExecutionContext;
use crate::scope::ScopeId;

#[derive(Debug, Clone)]
pub struct Reference {
    /// The scope in which the reference is defined.
    scope_id: ScopeId,
    /// The range of the reference in the source code.
    range: TextRange,
    /// The context in which the reference occurs.
    context: ExecutionContext,
}

impl Reference {
    pub const fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    pub const fn range(&self) -> TextRange {
        self.range
    }

    pub const fn context(&self) -> &ExecutionContext {
        &self.context
    }
}

/// Id uniquely identifying a read reference in a program.
#[newtype_index]
pub struct ReferenceId;

/// The references of a program indexed by [`ReferenceId`].
#[derive(Debug, Default)]
pub struct References(IndexVec<ReferenceId, Reference>);

impl References {
    /// Pushes a new read reference and returns its unique id.
    pub fn push(
        &mut self,
        scope_id: ScopeId,
        range: TextRange,
        context: ExecutionContext,
    ) -> ReferenceId {
        self.0.push(Reference {
            scope_id,
            range,
            context,
        })
    }

    /// Returns the [`Reference`] with the given id.
    pub fn resolve(&self, id: ReferenceId) -> &Reference {
        &self.0[id]
    }
}

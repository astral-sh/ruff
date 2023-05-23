use ruff_text_size::TextRange;

use ruff_index::{newtype_index, IndexVec};

use crate::scope::ScopeId;

#[derive(Debug, Clone)]
pub struct Reference {
    /// The scope in which the reference is defined.
    scope_id: ScopeId,
    /// The range of the reference in the source code.
    range: TextRange,
    /// The context in which the reference occurs.
    context: ReferenceContext,
}

impl Reference {
    pub const fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    pub const fn range(&self) -> TextRange {
        self.range
    }

    pub const fn context(&self) -> &ReferenceContext {
        &self.context
    }
}

#[derive(Debug, Clone)]
pub enum ReferenceContext {
    /// The reference occurs in a runtime context.
    Runtime,
    /// The reference occurs in a typing-only context.
    Typing,
    /// The reference occurs in a synthetic context, used for `__future__` imports, explicit
    /// re-exports, and other bindings that should be considered used even if they're never
    /// "referenced".
    Synthetic,
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
        context: ReferenceContext,
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

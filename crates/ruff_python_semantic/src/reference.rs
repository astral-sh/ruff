use std::num::TryFromIntError;
use std::ops::{Deref, Index, IndexMut};

use crate::binding::BindingId;
use ruff_text_size::TextRange;

use crate::scope::ScopeId;

// enum Context {
//     /// The reference occurs in a runtime context.
//     Runtime,
//     /// The reference occurs in a typing-only context.
//     Typing,
//     /// The reference occurs in a synthetic context, used for `__future__` imports, explicit
//     /// re-exports, and other bindings that should be considered used even if they're never
//     /// "referenced".
//     Synthetic,
// }

#[derive(Debug, Clone)]
pub struct Reference {
    /// The scope in which the reference is defined.
    scope_id: ScopeId,
    /// The range of the reference in the source code.
    range: TextRange,
}

impl Reference {
    pub const fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    pub const fn range(&self) -> TextRange {
        self.range
    }
}

/// Id uniquely identifying a reference in a program.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ReferenceId(u32);

impl TryFrom<usize> for ReferenceId {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl From<ReferenceId> for usize {
    fn from(value: ReferenceId) -> Self {
        value.0 as usize
    }
}

/// The references of a program indexed by [`ReferenceId`]
#[derive(Debug)]
pub struct References(Vec<Reference>);

impl References {
    /// Pushes a new reference and returns its unique id
    pub fn push_reference(&mut self, scope_id: ScopeId, range: TextRange) -> ReferenceId {
        let next_id = ReferenceId::try_from(self.0.len()).unwrap();
        self.0.push(Reference { scope_id, range });
        next_id
    }
}

impl Default for References {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl Index<ReferenceId> for References {
    type Output = Reference;

    fn index(&self, index: ReferenceId) -> &Self::Output {
        &self.0[usize::from(index)]
    }
}

impl IndexMut<ReferenceId> for References {
    fn index_mut(&mut self, index: ReferenceId) -> &mut Self::Output {
        &mut self.0[usize::from(index)]
    }
}

impl Deref for References {
    type Target = [Reference];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

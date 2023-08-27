use std::ops::Deref;

use bitflags::bitflags;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::context::ExecutionContext;
use crate::scope::ScopeId;
use crate::{Exceptions, SemanticModelFlags};

/// A resolved read reference to a name in a program.
#[derive(Debug, Clone)]
pub struct ResolvedReference {
    /// The scope in which the reference is defined.
    scope_id: ScopeId,
    /// The range of the reference in the source code.
    range: TextRange,
    /// The model state in which the reference occurs.
    flags: SemanticModelFlags,
}

impl ResolvedReference {
    /// The scope in which the reference is defined.
    pub const fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    /// The [`ExecutionContext`] of the reference.
    pub const fn context(&self) -> ExecutionContext {
        if self.flags.intersects(SemanticModelFlags::TYPING_CONTEXT) {
            ExecutionContext::Typing
        } else {
            ExecutionContext::Runtime
        }
    }
}

impl Ranged for ResolvedReference {
    /// The range of the reference in the source code.
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Id uniquely identifying a read reference in a program.
#[newtype_index]
pub struct ResolvedReferenceId;

/// The references of a program indexed by [`ResolvedReferenceId`].
#[derive(Debug, Default)]
pub(crate) struct ResolvedReferences(IndexVec<ResolvedReferenceId, ResolvedReference>);

impl ResolvedReferences {
    /// Pushes a new [`ResolvedReference`] and returns its [`ResolvedReferenceId`].
    pub(crate) fn push(
        &mut self,
        scope_id: ScopeId,
        range: TextRange,
        flags: SemanticModelFlags,
    ) -> ResolvedReferenceId {
        self.0.push(ResolvedReference {
            scope_id,
            range,
            flags,
        })
    }
}

impl Deref for ResolvedReferences {
    type Target = IndexSlice<ResolvedReferenceId, ResolvedReference>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An unresolved read reference to a name in a program.
#[derive(Debug, Clone)]
pub struct UnresolvedReference {
    /// The range of the reference in the source code.
    range: TextRange,
    /// The set of exceptions that were handled when resolution was attempted.
    exceptions: Exceptions,
    /// Flags indicating the context in which the reference occurs.
    flags: UnresolvedReferenceFlags,
}

impl UnresolvedReference {
    /// Returns the name of the reference.
    pub fn name<'a>(&self, locator: &Locator<'a>) -> &'a str {
        locator.slice(self.range)
    }

    /// The range of the reference in the source code.
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// The set of exceptions that were handled when resolution was attempted.
    pub const fn exceptions(&self) -> Exceptions {
        self.exceptions
    }

    /// Returns `true` if the unresolved reference may be resolved by a wildcard import.
    pub const fn is_wildcard_import(&self) -> bool {
        self.flags
            .contains(UnresolvedReferenceFlags::WILDCARD_IMPORT)
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug)]
    pub struct UnresolvedReferenceFlags: u8 {
        /// The unresolved reference may be resolved by a wildcard import.
        ///
        /// For example, the reference `x` in the following code may be resolved by the wildcard
        /// import of `module`:
        /// ```python
        /// from module import *
        ///
        /// print(x)
        /// ```
        const WILDCARD_IMPORT = 1 << 0;
    }
}

#[derive(Debug, Default)]
pub(crate) struct UnresolvedReferences(Vec<UnresolvedReference>);

impl UnresolvedReferences {
    /// Pushes a new [`UnresolvedReference`].
    pub(crate) fn push(
        &mut self,
        range: TextRange,
        exceptions: Exceptions,
        flags: UnresolvedReferenceFlags,
    ) {
        self.0.push(UnresolvedReference {
            range,
            exceptions,
            flags,
        });
    }
}

impl Deref for UnresolvedReferences {
    type Target = Vec<UnresolvedReference>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

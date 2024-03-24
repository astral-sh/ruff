use std::ops::Deref;

use bitflags::bitflags;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_python_ast::ExprContext;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::scope::ScopeId;
use crate::{Exceptions, NodeId, SemanticModelFlags};

/// A resolved read reference to a name in a program.
#[derive(Debug, Clone)]
pub struct ResolvedReference {
    /// The expression that the reference occurs in. `None` if the reference is a global
    /// reference or a reference via an augmented assignment.
    node_id: Option<NodeId>,
    /// The scope in which the reference is defined.
    scope_id: ScopeId,
    /// The expression context in which the reference occurs (e.g., `Load`, `Store`, `Del`).
    ctx: ExprContext,
    /// The model state in which the reference occurs.
    flags: SemanticModelFlags,
    /// The range of the reference in the source code.
    range: TextRange,
}

impl ResolvedReference {
    /// The expression that the reference occurs in.
    pub const fn expression_id(&self) -> Option<NodeId> {
        self.node_id
    }

    /// The scope in which the reference is defined.
    pub const fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    /// Return `true` if the reference occurred in a `Load` operation.
    pub const fn is_load(&self) -> bool {
        self.ctx.is_load()
    }

    /// Return `true` if the context is in a typing context.
    pub const fn in_typing_context(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::TYPING_CONTEXT)
    }

    /// Return `true` if the context is in a runtime context.
    pub const fn in_runtime_context(&self) -> bool {
        !self.flags.intersects(SemanticModelFlags::TYPING_CONTEXT)
    }

    /// Return `true` if the context is in a typing-only type annotation.
    pub const fn in_typing_only_annotation(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::TYPING_ONLY_ANNOTATION)
    }

    /// Return `true` if the context is in a runtime-required type annotation.
    pub const fn in_runtime_evaluated_annotation(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::RUNTIME_EVALUATED_ANNOTATION)
    }

    /// Return `true` if the context is in a "simple" string type definition.
    pub const fn in_simple_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::SIMPLE_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a "complex" string type definition.
    pub const fn in_complex_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::COMPLEX_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a `__future__` type definition.
    pub const fn in_future_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::FUTURE_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in any kind of deferred type definition.
    pub const fn in_deferred_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::DEFERRED_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a type-checking block.
    pub const fn in_type_checking_block(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::TYPE_CHECKING_BLOCK)
    }

    /// Return `true` if the context is in the r.h.s. of an `__all__` definition.
    pub const fn in_dunder_all_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::DUNDER_ALL_DEFINITION)
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
        node_id: Option<NodeId>,
        ctx: ExprContext,
        flags: SemanticModelFlags,
        range: TextRange,
    ) -> ResolvedReferenceId {
        self.0.push(ResolvedReference {
            node_id,
            scope_id,
            ctx,
            flags,
            range,
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

//! Key observations
//!
//! The HIR avoids allocations to large extends by:
//! * Using an arena per node type
//! * using ids and id ranges to reference items.
//!
//! Using separate arena per node type has the advantage that the IDs are relatively stable, because
//! they only change when a node of the same kind has been added or removed. (What's unclear is if that matters or if
//! it still triggers a re-compute because the AST-id in the node has changed).
//!
//! The HIR does not store all details. It mainly stores the *public* interface. There's a reference
//! back to the AST node to get more details.
//!
//!

use crate::ast_ids::{HasAstId, TypedAstId};
use crate::files::FileId;
use std::fmt::Formatter;
use std::hash::{Hash, Hasher};

pub mod definition;

pub struct HirAstId<N: HasAstId> {
    file_id: FileId,
    node_id: TypedAstId<N>,
}

impl<N: HasAstId> Copy for HirAstId<N> {}
impl<N: HasAstId> Clone for HirAstId<N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N: HasAstId> PartialEq for HirAstId<N> {
    fn eq(&self, other: &Self) -> bool {
        self.file_id == other.file_id && self.node_id == other.node_id
    }
}

impl<N: HasAstId> Eq for HirAstId<N> {}

impl<N: HasAstId> std::fmt::Debug for HirAstId<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HirAstId")
            .field("file_id", &self.file_id)
            .field("node_id", &self.node_id)
            .finish()
    }
}

impl<N: HasAstId> Hash for HirAstId<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file_id.hash(state);
        self.node_id.hash(state);
    }
}

impl<N: HasAstId> HirAstId<N> {
    pub fn upcast<M: HasAstId>(self) -> HirAstId<M>
    where
        N: Into<M>,
    {
        HirAstId {
            file_id: self.file_id,
            node_id: self.node_id.upcast(),
        }
    }
}

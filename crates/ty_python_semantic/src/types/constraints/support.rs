//! Tracks the support of each constraint and interior node in a BDD.
//!
//! The support of a constraint is the set of typevars mentioned anywhere in the constraint
//! (either the subject, or anywhere in the lower or upper bound).
//!
//! The support of a node is the union of the supports of every constraint reachable from that
//! node.

use std::ops::{BitOr, BitOrAssign};

use crate::Db;
use crate::types::TypeVarSet;
use crate::types::constraints::{ConstraintId, ConstraintSetStorage, TypeVarId};

use ruff_index::newtype_index;
use smallvec::SmallVec;

#[newtype_index]
#[derive(get_size2::GetSize)]
pub(super) struct SupportId;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct Support {
    chunks: SmallVec<[usize; 2]>,
}

const CHUNK_SIZE: usize = usize::BITS as usize;

impl Support {
    pub(super) fn from_typevar_set<'db>(
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        typevars: TypeVarSet<'db>,
    ) -> Self {
        let mut result = Self::default();
        for typevar in typevars.iter(db) {
            let typevar = storage.intern_typevar(db, typevar);
            result.insert(typevar);
        }
        result
    }

    /// Adds a typevar to this support.
    pub(super) fn insert(&mut self, typevar: TypeVarId) {
        let index = typevar.index();
        let chunks_needed = (index + 1).div_ceil(CHUNK_SIZE);
        if self.chunks.len() < chunks_needed {
            self.chunks.resize(chunks_needed, 0);
        }

        let chunk_index = index / CHUNK_SIZE;
        let bit_index_within_chunk = index % CHUNK_SIZE;
        let bit_mask_within_chunk = 1 << bit_index_within_chunk;
        self.chunks[chunk_index] |= bit_mask_within_chunk;
    }

    fn iter_chunks(&self) -> impl Iterator<Item = usize> + '_ {
        self.chunks.iter().copied()
    }

    /// Returns an iterator of all of the typevars in this support.
    pub(super) fn iter(&self) -> impl Iterator<Item = TypeVarId> + '_ {
        // Iterate through all of the chunks
        let mut next_chunk_start = 0;
        self.iter_chunks().flat_map(move |mut chunk| {
            // Figure out the starting index of this chunk
            let chunk_start = next_chunk_start;
            next_chunk_start += CHUNK_SIZE;

            // Iterate through the set bits in this chunk
            std::iter::from_fn(move || {
                // Find the lowest set bit, if there is one
                let index = chunk.trailing_zeros() as usize;
                if index == CHUNK_SIZE {
                    return None;
                }

                // Clear out the bit we just found.
                chunk ^= 1 << index;

                // And then return it, converted into a TypeVarId
                Some(TypeVarId::from_usize(chunk_start + index))
            })
        })
    }

    /// Returns whether this support contains any typevars that are not in `other`.
    fn contains_more_than(&self, other: &Self) -> bool {
        let lhs = self.iter_chunks();
        let rhs = std::iter::chain(other.iter_chunks(), std::iter::repeat(0));
        std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (lhs & !rhs) != 0)
    }

    /// Returns whether this support contains any typevars in common with `other`.
    pub(super) fn overlaps_with(&self, other: &Self) -> bool {
        let lhs = self.iter_chunks();
        let rhs = other.iter_chunks();
        std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (lhs & rhs) != 0)
    }

    /// Closes this support over a set of constraints.
    ///
    /// We perform a fixed-point loop, where we find the constraints that mention any of the
    /// typevars in the support, and add any _other_ typevars they mention. (That might add
    /// additional typevars that cause more constraints to become eligible, and so on.)
    pub(super) fn close_over_constraints<'db>(
        &mut self,
        storage: &ConstraintSetStorage<'db>,
        constraints: impl Iterator<Item = ConstraintId> + Clone,
    ) {
        loop {
            let mut any_added = false;
            for constraint in constraints.clone() {
                let constraint_support = storage.constraint_support(constraint);
                if constraint_support.overlaps_with(self)
                    && constraint_support.contains_more_than(self)
                {
                    any_added = true;
                    *self |= constraint_support;
                }
            }

            if !any_added {
                return;
            }
        }
    }
}

impl BitOr for &Support {
    type Output = Support;
    fn bitor(self, rhs: &Support) -> Support {
        let mut result = self.clone();
        result |= rhs;
        result
    }
}

impl BitOrAssign<&Self> for Support {
    fn bitor_assign(&mut self, rhs: &Self) {
        if self.chunks.len() < rhs.chunks.len() {
            self.chunks.resize(rhs.chunks.len(), 0);
        }
        for (lhs, rhs) in std::iter::zip(&mut self.chunks, &rhs.chunks) {
            *lhs |= *rhs;
        }
    }
}

impl BitOrAssign<Option<&Self>> for Support {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}

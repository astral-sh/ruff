//! Tracks type-variable occurrences, declaration dependencies, free dependencies, and
//! existential locals for each constraint and interior node in a BDD.
//!
//! Atomic constraints collect these supports together. Existential constraints remove their
//! locals only from free support; interior nodes union the supports of their constraint and edges.

use std::ops::{BitOrAssign, Sub};

use crate::Db;
use crate::types::constraints::{ConstraintId, ConstraintSetStorage, TypeVarId};
use crate::types::typevar::TypeVarSet;

use ruff_index::newtype_index;
use smallvec::SmallVec;

#[newtype_index]
#[derive(get_size2::GetSize)]
pub(super) struct SupportId;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct Support {
    chunks: SmallVec<[usize; 2]>,
    has_skipped_lazy_attributes: bool,
}

const CHUNK_SIZE: usize = usize::BITS as usize;

impl Support {
    pub(super) fn from_typevars(typevars: impl IntoIterator<Item = TypeVarId>) -> Self {
        let mut result = Self::default();
        for typevar in typevars {
            result.insert(typevar);
        }
        result
    }

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

    pub(super) fn contains(&self, typevar: TypeVarId) -> bool {
        let index = typevar.index();
        self.chunks
            .get(index / CHUNK_SIZE)
            .is_some_and(|chunk| chunk & (1 << (index % CHUNK_SIZE)) != 0)
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
                let index = chunk.lowest_one()? as usize;

                // Clear out the bit we just found.
                chunk ^= 1 << index;

                // And then return it, converted into a TypeVarId
                Some(TypeVarId::from_usize(chunk_start + index))
            })
        })
    }

    /// Returns whether this support contains any typevars that are not in `other`.
    pub(super) fn contains_more_than(&self, other: &Self) -> bool {
        let lhs = self.iter_chunks();
        let rhs = std::iter::chain(other.iter_chunks(), std::iter::repeat(0));
        std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (lhs & !rhs) != 0)
    }

    /// Returns whether this support contains any type variables in common with `other`.
    pub(super) fn overlaps_with(&self, other: &Self) -> bool {
        let lhs = self.iter_chunks();
        let rhs = other.iter_chunks();
        std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (lhs & rhs) != 0)
    }

    /// Records that lazy type attributes may contain additional type variables.
    pub(super) fn mark_incomplete(&mut self) {
        self.has_skipped_lazy_attributes = true;
    }

    /// Returns whether all type attributes were inspected while collecting this support.
    pub(super) fn is_complete(&self) -> bool {
        !self.has_skipped_lazy_attributes
    }

    /// Closes this support over a set of constraints.
    ///
    /// We perform a fixed-point loop, where we find the constraints that mention any of the
    /// typevars in the support, and add any _other_ typevars they mention. (That might add
    /// additional typevars that cause more constraints to become eligible, and so on.)
    #[expect(clippy::needless_pass_by_value)]
    pub(super) fn close_over_constraints(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
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

impl BitOrAssign<&Self> for Support {
    fn bitor_assign(&mut self, rhs: &Self) {
        if self.chunks.len() < rhs.chunks.len() {
            self.chunks.resize(rhs.chunks.len(), 0);
        }
        for (lhs, rhs) in std::iter::zip(&mut self.chunks, &rhs.chunks) {
            *lhs |= *rhs;
        }
        self.has_skipped_lazy_attributes |= rhs.has_skipped_lazy_attributes;
    }
}

impl BitOrAssign<Option<&Self>> for Support {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}

impl Sub<&Support> for &Support {
    type Output = Support;

    fn sub(self, rhs: &Support) -> Support {
        let mut result = self.clone();
        for (lhs, rhs) in std::iter::zip(&mut result.chunks, &rhs.chunks) {
            *lhs &= !(*rhs);
        }
        result
    }
}

/// Supports collected together when a constraint is constructed.
///
/// Occurrences retain quantified locals for solution extraction. Validation contains variables
/// with declared bounds or constraints, and their transitive dependencies. An unbounded variable
/// contributes to validation only when another variable's declaration depends on it.
/// Free dependencies include declarations but respect the binders inside types and existential
/// bodies, so they can establish independence from an enclosing BDD.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ConstraintSupports {
    pub(super) occurrences: Support,
    pub(super) validation: Support,
    pub(super) free: Support,
    /// Existential locals that must be freshened when this constraint set is reused.
    pub(super) quantified: Support,
}

impl BitOrAssign<&Self> for ConstraintSupports {
    fn bitor_assign(&mut self, rhs: &Self) {
        self.occurrences |= &rhs.occurrences;
        self.validation |= &rhs.validation;
        self.free |= &rhs.free;
        self.quantified |= &rhs.quantified;
    }
}

impl BitOrAssign<Option<&Self>> for ConstraintSupports {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}

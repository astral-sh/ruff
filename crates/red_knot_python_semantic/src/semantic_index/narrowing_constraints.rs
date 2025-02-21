use std::ops::Index;

use ruff_index::list::{ListBuilder, ListIterator, ListStorage};
use ruff_index::{newtype_index, IndexVec};

use crate::semantic_index::constraint::ScopedConstraintId;

#[newtype_index]
#[derive(Ord, PartialOrd)]
pub(crate) struct ScopedNarrowingConstraintId;

#[newtype_index]
pub(crate) struct ScopedNarrowingConstraintSetId;

/// A single narrowing constraint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NarrowingConstraint {
    pub(crate) constraint: ScopedConstraintId,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct NarrowingConstraints {
    constraints: IndexVec<ScopedNarrowingConstraintId, NarrowingConstraint>,
    lists: ListStorage<ScopedNarrowingConstraintSetId, ScopedNarrowingConstraintId, ()>,
}

impl Index<ScopedNarrowingConstraintId> for NarrowingConstraints {
    type Output = NarrowingConstraint;

    #[inline]
    fn index(&self, index: ScopedNarrowingConstraintId) -> &NarrowingConstraint {
        &self.constraints[index]
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct NarrowingConstraintsBuilder {
    constraints: IndexVec<ScopedNarrowingConstraintId, NarrowingConstraint>,
    lists: ListBuilder<ScopedNarrowingConstraintSetId, ScopedNarrowingConstraintId, ()>,
}

impl NarrowingConstraintsBuilder {
    pub(crate) fn build(mut self) -> NarrowingConstraints {
        self.constraints.shrink_to_fit();
        NarrowingConstraints {
            constraints: self.constraints,
            lists: self.lists.build(),
        }
    }

    pub(crate) fn add_constraint(
        &mut self,
        constraint: ScopedConstraintId,
    ) -> ScopedNarrowingConstraintId {
        self.constraints.push(NarrowingConstraint { constraint })
    }

    pub(crate) fn insert_into_set(
        &mut self,
        set: Option<ScopedNarrowingConstraintSetId>,
        element: ScopedNarrowingConstraintId,
    ) -> Option<ScopedNarrowingConstraintSetId> {
        self.lists.insert_if_needed(set, element, ())
    }

    pub(crate) fn intersect_sets(
        &mut self,
        a: Option<ScopedNarrowingConstraintSetId>,
        b: Option<ScopedNarrowingConstraintSetId>,
    ) -> Option<ScopedNarrowingConstraintSetId> {
        self.lists.intersect(a, b, |(), ()| ())
    }
}

pub(crate) struct NarrowingConstraintsIterator<'a> {
    wrapped: ListIterator<'a, ScopedNarrowingConstraintSetId, ScopedNarrowingConstraintId, ()>,
}

impl NarrowingConstraints {
    pub(crate) fn iter_constraints(
        &self,
        set: Option<ScopedNarrowingConstraintSetId>,
    ) -> NarrowingConstraintsIterator<'_> {
        NarrowingConstraintsIterator {
            wrapped: self.lists.iter(set),
        }
    }
}

impl NarrowingConstraintsBuilder {
    // This is currently only used in tests, but needs to be defined here to not overly publicize
    // our internal fields.
    #[cfg(test)]
    pub(crate) fn iter_constraints(
        &self,
        set: Option<ScopedNarrowingConstraintSetId>,
    ) -> NarrowingConstraintsIterator<'_> {
        NarrowingConstraintsIterator {
            wrapped: self.lists.iter(set),
        }
    }
}

impl Iterator for NarrowingConstraintsIterator<'_> {
    type Item = ScopedNarrowingConstraintId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (key, ()) = self.wrapped.next()?;
        Some(*key)
    }
}

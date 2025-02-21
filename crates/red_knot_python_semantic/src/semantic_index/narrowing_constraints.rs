use ruff_index::list::{ListBuilder, ListIterator, ListStorage};
use ruff_index::newtype_index;

use crate::semantic_index::constraint::ScopedConstraintId;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScopedNarrowingConstraintId(u32);

impl ScopedNarrowingConstraintId {
    #[inline]
    pub(crate) fn constraint(self) -> ScopedConstraintId {
        ScopedConstraintId::from(self.0)
    }
}

impl From<ScopedConstraintId> for ScopedNarrowingConstraintId {
    #[inline]
    fn from(constraint: ScopedConstraintId) -> ScopedNarrowingConstraintId {
        ScopedNarrowingConstraintId(constraint.as_u32())
    }
}

#[newtype_index]
pub(crate) struct ScopedNarrowingConstraintSetId;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct NarrowingConstraints {
    lists: ListStorage<ScopedNarrowingConstraintSetId, ScopedNarrowingConstraintId, ()>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct NarrowingConstraintsBuilder {
    lists: ListBuilder<ScopedNarrowingConstraintSetId, ScopedNarrowingConstraintId, ()>,
}

impl NarrowingConstraintsBuilder {
    pub(crate) fn build(self) -> NarrowingConstraints {
        NarrowingConstraints {
            lists: self.lists.build(),
        }
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

impl Iterator for NarrowingConstraintsIterator<'_> {
    type Item = ScopedNarrowingConstraintId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (key, ()) = self.wrapped.next()?;
        Some(*key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ScopedNarrowingConstraintId {
        pub(crate) fn as_u32(self) -> u32 {
            self.0
        }
    }

    impl NarrowingConstraintsBuilder {
        pub(crate) fn iter_constraints(
            &self,
            set: Option<ScopedNarrowingConstraintSetId>,
        ) -> NarrowingConstraintsIterator<'_> {
            NarrowingConstraintsIterator {
                wrapped: self.lists.iter(set),
            }
        }
    }
}

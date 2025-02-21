use ruff_index::list::{ListBuilder, ListIterator, ListStorage};
use ruff_index::newtype_index;

use crate::semantic_index::constraint::ScopedConstraintId;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScopedNarrowingConstraintClause(u32);

impl ScopedNarrowingConstraintClause {
    #[inline]
    pub(crate) fn constraint(self) -> ScopedConstraintId {
        ScopedConstraintId::from(self.0)
    }
}

impl From<ScopedConstraintId> for ScopedNarrowingConstraintClause {
    #[inline]
    fn from(constraint: ScopedConstraintId) -> ScopedNarrowingConstraintClause {
        ScopedNarrowingConstraintClause(constraint.as_u32())
    }
}

#[newtype_index]
pub(crate) struct ScopedNarrowingConstraintId;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct NarrowingConstraints {
    lists: ListStorage<ScopedNarrowingConstraintId, ScopedNarrowingConstraintClause, ()>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct NarrowingConstraintsBuilder {
    lists: ListBuilder<ScopedNarrowingConstraintId, ScopedNarrowingConstraintClause, ()>,
}

impl NarrowingConstraintsBuilder {
    pub(crate) fn build(self) -> NarrowingConstraints {
        NarrowingConstraints {
            lists: self.lists.build(),
        }
    }

    pub(crate) fn add(
        &mut self,
        set: Option<ScopedNarrowingConstraintId>,
        element: ScopedNarrowingConstraintClause,
    ) -> Option<ScopedNarrowingConstraintId> {
        self.lists.insert_if_needed(set, element, ())
    }

    pub(crate) fn intersect(
        &mut self,
        a: Option<ScopedNarrowingConstraintId>,
        b: Option<ScopedNarrowingConstraintId>,
    ) -> Option<ScopedNarrowingConstraintId> {
        self.lists.intersect(a, b, |(), ()| ())
    }
}

pub(crate) struct NarrowingConstraintsIterator<'a> {
    wrapped: ListIterator<'a, ScopedNarrowingConstraintId, ScopedNarrowingConstraintClause, ()>,
}

impl NarrowingConstraints {
    pub(crate) fn iter_clauses(
        &self,
        set: Option<ScopedNarrowingConstraintId>,
    ) -> NarrowingConstraintsIterator<'_> {
        NarrowingConstraintsIterator {
            wrapped: self.lists.iter(set),
        }
    }
}

impl Iterator for NarrowingConstraintsIterator<'_> {
    type Item = ScopedNarrowingConstraintClause;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (key, ()) = self.wrapped.next()?;
        Some(*key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ScopedNarrowingConstraintClause {
        pub(crate) fn as_u32(self) -> u32 {
            self.0
        }
    }

    impl NarrowingConstraintsBuilder {
        pub(crate) fn iter_constraints(
            &self,
            set: Option<ScopedNarrowingConstraintId>,
        ) -> NarrowingConstraintsIterator<'_> {
            NarrowingConstraintsIterator {
                wrapped: self.lists.iter(set),
            }
        }
    }
}

use std::ops::Index;

use ruff_index::IndexVec;

use crate::semantic_index::{ScopedConstraintId, ScopedVisibilityConstraintId};

/// TODO
///
/// Used to represent active branching conditions that apply to a particular definition.
/// A definition can either be conditional on a specific constraint from a `if`, `elif`,
/// `while` statement, an `if`-expression, or a Boolean expression. Or it can be marked
/// as 'ambiguous' if it occurred in a control-flow path that is not conditional on any
/// specific expression that can be statically analyzed (`for` loop, `try` ... `except`).
///
///
/// For example:
/// ```py
/// a = 1  # no visibility constraints
///
/// if test1:
///     b = 1  # Constraint(test1)
///
///     if test2:
///         c = 1  # Constraint(test1), Constraint(test2)
///
///     for _ in range(10):
///         d = 1  # Constraint(test1), Ambiguous
/// else:
///    d = 1  # Constraint(~test1)
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VisibilityConstraintRef {
    None,
    Single(ScopedConstraintId),
    Negated(ScopedVisibilityConstraintId),
    Sequence(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
    Merged(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VisibilityConstraints {
    constraints: IndexVec<ScopedVisibilityConstraintId, VisibilityConstraintRef>,
}

impl VisibilityConstraints {
    pub(crate) fn new() -> Self {
        Self {
            constraints: IndexVec::from_iter([VisibilityConstraintRef::None]),
        }
    }

    pub(crate) fn add(
        &mut self,
        constraint: VisibilityConstraintRef,
    ) -> ScopedVisibilityConstraintId {
        self.constraints.push(constraint)
    }

    pub(crate) fn add_merged(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (&self.constraints[a], &self.constraints[b]) {
            (_, VisibilityConstraintRef::Negated(id)) if a == *id => {
                ScopedVisibilityConstraintId::from_u32(0)
            }
            (VisibilityConstraintRef::Negated(id), _) if *id == b => {
                ScopedVisibilityConstraintId::from_u32(0)
            }
            _ => self.add(VisibilityConstraintRef::Merged(a, b)),
        }
    }
}

impl Index<ScopedVisibilityConstraintId> for VisibilityConstraints {
    type Output = VisibilityConstraintRef;

    fn index(&self, index: ScopedVisibilityConstraintId) -> &Self::Output {
        &self.constraints[index]
    }
}

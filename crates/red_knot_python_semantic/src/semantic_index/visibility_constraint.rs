use crate::semantic_index::use_def::{ScopedConstraintId, ScopedVisibilityConstraintId};

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
    And(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
    Or(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
}

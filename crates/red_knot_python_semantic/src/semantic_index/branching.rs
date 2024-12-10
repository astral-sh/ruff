use super::constraint::Constraint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BranchingCondition<'db> {
    ConditionalOn(Constraint<'db>),
    Ambiguous,
}

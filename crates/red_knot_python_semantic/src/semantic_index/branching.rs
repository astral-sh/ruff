use super::constraint::Constraint;

pub(crate) enum BranchingCondition<'db> {
    Conditional(Constraint<'db>),
    Unconditional,
}

//! Applies TDD operators using one stack for all child diagrams and mixed operators.

use std::cmp::Ordering;
use std::ops::ControlFlow;

use smallvec::SmallVec;

use super::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork, Unrestricted, hash_access,
    reserve_map, reserve_smallvec, unrestricted,
};
use super::storage::PendingNode;
use super::{ALWAYS_FALSE, ALWAYS_TRUE, ConstraintSetStorage, InteriorNodeData, Node, NodeId};

#[derive(Clone, Copy)]
pub(super) enum Operation {
    And(NodeId, NodeId),
    Or(NodeId, NodeId),
    Negate(NodeId),
}

impl Operation {
    #[inline]
    fn cached(self, storage: &ConstraintSetStorage<'_>) -> Option<NodeId> {
        unrestricted(self.cached_with(storage, &mut Unrestricted))
    }

    #[inline]
    fn cached_with<C: TddControl>(
        self,
        storage: &ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<Option<NodeId>, TddError<C::Error>> {
        Ok(match self {
            Self::And(left, right) => {
                if left == right {
                    return Ok(Some(left));
                }
                match (left.node(), right.node()) {
                    (Node::AlwaysFalse, _) | (_, Node::AlwaysFalse) => Some(ALWAYS_FALSE),
                    (Node::AlwaysTrue, _) => Some(right),
                    (_, Node::AlwaysTrue) => Some(left),
                    (Node::Interior(_), Node::Interior(_)) => {
                        hash_access(control, TableKind::And, storage.and_cache.capacity())?;
                        storage.and_cache.get(&(left, right)).copied()
                    }
                }
            }
            Self::Or(left, right) => match (left.node(), right.node()) {
                (Node::AlwaysTrue, _) | (_, Node::AlwaysTrue) => Some(ALWAYS_TRUE),
                (Node::AlwaysFalse, _) => Some(right),
                (_, Node::AlwaysFalse) => Some(left),
                (Node::Interior(_), Node::Interior(_)) => {
                    hash_access(control, TableKind::Or, storage.or_cache.capacity())?;
                    storage.or_cache.get(&(left, right)).copied()
                }
            },
            Self::Negate(node) => match node.node() {
                Node::AlwaysTrue => Some(ALWAYS_FALSE),
                Node::AlwaysFalse => Some(ALWAYS_TRUE),
                Node::Interior(_) => {
                    hash_access(control, TableKind::Negate, storage.negate_cache.capacity())?;
                    storage.negate_cache.get(&node).copied()
                }
            },
        })
    }

    pub(super) fn prepare_cache<C: TddControl>(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<(), TddError<C::Error>> {
        match self {
            Self::And(..) => reserve_map(&mut storage.and_cache, TableKind::And, control),
            Self::Or(..) => reserve_map(&mut storage.or_cache, TableKind::Or, control),
            Self::Negate(..) => reserve_map(&mut storage.negate_cache, TableKind::Negate, control),
        }
    }

    pub(super) fn cache(self, storage: &mut ConstraintSetStorage<'_>, result: NodeId) {
        match self {
            Self::And(left, right) => {
                storage.and_cache.insert((left, right), result);
            }
            Self::Or(left, right) => {
                storage.or_cache.insert((left, right), result);
            }
            Self::Negate(node) => {
                storage.negate_cache.insert(node, result);
            }
        }
    }

    #[inline]
    pub(super) fn apply(self, storage: &mut ConstraintSetStorage<'_>) -> NodeId {
        // Avoid constructing the frame stack for terminal operands and completed cache entries.
        self.cached(storage)
            .unwrap_or_else(|| self.apply_uncached(storage))
    }

    // Keep the cursor's storage out of callers that only need the terminal/cache fast path.
    #[inline(never)]
    fn apply_uncached(self, storage: &mut ConstraintSetStorage<'_>) -> NodeId {
        let mut cursor = TddApply::new(self);
        unrestricted(cursor.descend(storage, self, &mut Unrestricted));
        cursor.finish(storage)
    }
}

#[derive(Clone, Copy)]
enum Branch {
    True,
    Uncertain,
    False,
}

#[derive(Clone, Copy)]
enum Step {
    Operation(Operation),
    Value(NodeId),
}

#[derive(Clone, Copy)]
enum RemainingBranches {
    UncertainThenFalse(Operation, Operation),
    False(Operation),
    Done,
}

impl RemainingBranches {
    fn next(&mut self) -> Option<(Branch, Operation)> {
        let (remaining, child) = match *self {
            Self::UncertainThenFalse(uncertain, otherwise) => {
                (Self::False(otherwise), (Branch::Uncertain, uncertain))
            }
            Self::False(child) => (Self::Done, (Branch::False, child)),
            Self::Done => return None,
        };
        *self = remaining;
        Some(child)
    }
}

struct Branches {
    operation: Operation,
    result: InteriorNodeData,
    branch: Branch,
    remaining: RemainingBranches,
}

#[derive(Clone, Copy)]
enum EqualConjunctionStep {
    RightTrue,
    TrueFromTrue,
    TrueFromUncertain {
        true_from_true: NodeId,
    },
    True,
    Uncertain {
        if_true: NodeId,
    },
    RightFalse {
        if_true: NodeId,
        if_uncertain: NodeId,
    },
    FalseFromFalse {
        if_true: NodeId,
        if_uncertain: NodeId,
    },
    FalseFromUncertain {
        if_true: NodeId,
        if_uncertain: NodeId,
        false_from_false: NodeId,
    },
    False {
        if_true: NodeId,
        if_uncertain: NodeId,
    },
}

struct EqualConjunction {
    operation: Operation,
    left: InteriorNodeData,
    right: InteriorNodeData,
    step: EqualConjunctionStep,
}

#[derive(Clone, Copy)]
enum NegationStep {
    NegateTrue,
    NegateUncertain {
        not_true: NodeId,
    },
    NegateFalse {
        not_true: NodeId,
        not_uncertain: NodeId,
    },
    True {
        not_uncertain: NodeId,
        not_false: NodeId,
    },
    False {
        if_true: NodeId,
    },
}

struct Negation {
    operation: Operation,
    data: InteriorNodeData,
    step: NegationStep,
}

enum Frame {
    Branches(Branches),
    EqualConjunction(EqualConjunction),
    Negation(Negation),
}

enum Resume {
    Child(Operation),
    Complete(Operation, InteriorNodeData),
}

impl Frame {
    #[inline]
    fn resume(&mut self, value: NodeId) -> Resume {
        match self {
            Self::Branches(frame) => {
                match frame.branch {
                    Branch::True => frame.result.if_true = value,
                    Branch::Uncertain => frame.result.if_uncertain = value,
                    Branch::False => frame.result.if_false = value,
                }
                if let Some((branch, operation)) = frame.remaining.next() {
                    frame.branch = branch;
                    Resume::Child(operation)
                } else {
                    Resume::Complete(frame.operation, frame.result)
                }
            }
            Self::EqualConjunction(frame) => {
                let (step, child) = match frame.step {
                    EqualConjunctionStep::RightTrue => (
                        EqualConjunctionStep::TrueFromTrue,
                        Operation::And(frame.left.if_true, value),
                    ),
                    EqualConjunctionStep::TrueFromTrue => (
                        EqualConjunctionStep::TrueFromUncertain {
                            true_from_true: value,
                        },
                        Operation::And(frame.left.if_uncertain, frame.right.if_true),
                    ),
                    EqualConjunctionStep::TrueFromUncertain { true_from_true } => (
                        EqualConjunctionStep::True,
                        Operation::Or(true_from_true, value),
                    ),
                    EqualConjunctionStep::True => (
                        EqualConjunctionStep::Uncertain { if_true: value },
                        Operation::And(frame.left.if_uncertain, frame.right.if_uncertain),
                    ),
                    EqualConjunctionStep::Uncertain { if_true } => (
                        EqualConjunctionStep::RightFalse {
                            if_true,
                            if_uncertain: value,
                        },
                        Operation::Or(frame.right.if_uncertain, frame.right.if_false),
                    ),
                    EqualConjunctionStep::RightFalse {
                        if_true,
                        if_uncertain,
                    } => (
                        EqualConjunctionStep::FalseFromFalse {
                            if_true,
                            if_uncertain,
                        },
                        Operation::And(frame.left.if_false, value),
                    ),
                    EqualConjunctionStep::FalseFromFalse {
                        if_true,
                        if_uncertain,
                    } => (
                        EqualConjunctionStep::FalseFromUncertain {
                            if_true,
                            if_uncertain,
                            false_from_false: value,
                        },
                        Operation::And(frame.left.if_uncertain, frame.right.if_false),
                    ),
                    EqualConjunctionStep::FalseFromUncertain {
                        if_true,
                        if_uncertain,
                        false_from_false,
                    } => (
                        EqualConjunctionStep::False {
                            if_true,
                            if_uncertain,
                        },
                        Operation::Or(false_from_false, value),
                    ),
                    EqualConjunctionStep::False {
                        if_true,
                        if_uncertain,
                    } => {
                        return Resume::Complete(
                            frame.operation,
                            InteriorNodeData {
                                constraint: frame.left.constraint,
                                if_true,
                                if_uncertain,
                                if_false: value,
                            },
                        );
                    }
                };
                frame.step = step;
                Resume::Child(child)
            }
            Self::Negation(frame) => {
                let (step, child) = match frame.step {
                    NegationStep::NegateTrue => (
                        NegationStep::NegateUncertain { not_true: value },
                        Operation::Negate(frame.data.if_uncertain),
                    ),
                    NegationStep::NegateUncertain { not_true } => (
                        NegationStep::NegateFalse {
                            not_true,
                            not_uncertain: value,
                        },
                        Operation::Negate(frame.data.if_false),
                    ),
                    NegationStep::NegateFalse {
                        not_true,
                        not_uncertain,
                    } => (
                        NegationStep::True {
                            not_uncertain,
                            not_false: value,
                        },
                        Operation::And(not_true, not_uncertain),
                    ),
                    NegationStep::True {
                        not_uncertain,
                        not_false,
                    } => (
                        NegationStep::False { if_true: value },
                        Operation::And(not_false, not_uncertain),
                    ),
                    NegationStep::False { if_true } => {
                        return Resume::Complete(
                            frame.operation,
                            InteriorNodeData {
                                constraint: frame.data.constraint,
                                if_true,
                                if_uncertain: ALWAYS_FALSE,
                                if_false: value,
                            },
                        );
                    }
                };
                frame.step = step;
                Resume::Child(child)
            }
        }
    }
}

/// Retains mixed operator calls between short storage accesses. Completed children enter the
/// original caches in depth-first order, preserving interning order and local reductions.
pub(super) struct TddApply {
    step: Step,
    frames: SmallVec<[Frame; 4]>,
    pending: Option<PendingNode>,
}

impl TddApply {
    pub(super) fn new(operation: Operation) -> Self {
        Self {
            step: Step::Operation(operation),
            frames: SmallVec::new(),
            pending: None,
        }
    }

    pub(super) fn finish(mut self, storage: &mut ConstraintSetStorage<'_>) -> NodeId {
        loop {
            if let ControlFlow::Break(result) = self.advance(storage) {
                return result;
            }
        }
    }

    #[inline]
    fn advance(&mut self, storage: &mut ConstraintSetStorage<'_>) -> ControlFlow<NodeId> {
        unrestricted(self.advance_with(storage, &mut Unrestricted))
    }

    /// After an error, abandon this cursor and retry with a fresh cursor on the same builder.
    /// Completed nodes and cache entries remain valid.
    pub(super) fn advance_with<C: TddControl>(
        &mut self,
        storage: &mut ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<NodeId>, TddError<C::Error>> {
        control.admit(TddWork::Advance)?;
        if let Some(pending) = &mut self.pending {
            let group_ready_phases = pending.has_ready_completion_phase(storage);
            // Reduce, find an existing identity, cache it, and deliver it in at most four
            // phases. Keep each phase's admission order and stop before suspended storage work.
            for phase in 0..4 {
                if phase != 0 {
                    control.admit(TddWork::Advance)?;
                }
                if let ControlFlow::Break(result) = pending.advance(storage, control)? {
                    self.pending = None;
                    self.step = Step::Value(result);
                    return Ok(ControlFlow::Continue(()));
                }
                if !group_ready_phases || !pending.has_ready_completion_phase(storage) {
                    break;
                }
            }
            return Ok(ControlFlow::Continue(()));
        }
        let mut value = match self.step {
            Step::Operation(operation) => {
                if let Some(result) = operation.cached_with(storage, control)? {
                    result
                } else {
                    self.descend(storage, operation, control)?;
                    return Ok(ControlFlow::Continue(()));
                }
            }
            Step::Value(value) => value,
        };
        let Some(frame) = self.frames.last_mut() else {
            return Ok(ControlFlow::Break(value));
        };
        // Each frame has at most nine child requests. Complete ready children in place;
        // admitting an unresolved child or pending interner returns control to the driver.
        loop {
            match frame.resume(value) {
                Resume::Child(operation) => {
                    if let Some(result) = operation.cached_with(storage, control)? {
                        value = result;
                    } else {
                        self.descend(storage, operation, control)?;
                        return Ok(ControlFlow::Continue(()));
                    }
                }
                Resume::Complete(operation, data) => {
                    self.frames.pop();
                    self.pending = Some(PendingNode::new(data, Some(operation), true));
                    return Ok(ControlFlow::Continue(()));
                }
            }
        }
    }

    fn descend<C: TddControl>(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
        operation: Operation,
        control: &mut C,
    ) -> Result<(), TddError<C::Error>> {
        let (frame, child) = match operation {
            Operation::Negate(node) => {
                // negate(n ? C : U : D) = n ? negate(or(C, U)) : 0 : negate(or(D, U))
                //
                // The uncertain branch is absorbed into both guarded branches. Distributing
                // negation retains the three child negations followed by two conjunctions.
                // When U = 0, this reduces to the usual binary BDD leaf swap.
                let data = storage.interior_node_data(node);
                (
                    Frame::Negation(Negation {
                        operation,
                        data,
                        step: NegationStep::NegateTrue,
                    }),
                    Operation::Negate(data.if_true),
                )
            }
            Operation::And(left, right) => {
                let left_data = storage.interior_node_data(left);
                let right_data = storage.interior_node_data(right);
                let ordering = left_data
                    .constraint
                    .ordering()
                    .cmp(&right_data.constraint.ordering());
                if ordering == Ordering::Equal {
                    // Duboc propagates the uncertain input branches into the result, instead
                    // of always distributing them into both guarded branches as Frisch does:
                    // n ? (C1 ∧ (C2 ∨ U2)) ∨ (U1 ∧ C2) : U1 ∧ U2 : (D1 ∧ (U2 ∨ D2)) ∨ (U1 ∧ D2)
                    // See [Duboc2026], §11.2 for more details.
                    (
                        Frame::EqualConjunction(EqualConjunction {
                            operation,
                            left: left_data,
                            right: right_data,
                            step: EqualConjunctionStep::RightTrue,
                        }),
                        Operation::Or(right_data.if_true, right_data.if_uncertain),
                    )
                } else {
                    let (data, operands) = if ordering == Ordering::Less {
                        (
                            left_data,
                            [
                                Operation::And(left_data.if_true, right),
                                Operation::And(left_data.if_uncertain, right),
                                Operation::And(left_data.if_false, right),
                            ],
                        )
                    } else {
                        (
                            right_data,
                            [
                                Operation::And(left, right_data.if_true),
                                Operation::And(left, right_data.if_uncertain),
                                Operation::And(left, right_data.if_false),
                            ],
                        )
                    };
                    (
                        Frame::Branches(Branches {
                            operation,
                            result: data,
                            branch: Branch::True,
                            remaining: RemainingBranches::UncertainThenFalse(
                                operands[1],
                                operands[2],
                            ),
                        }),
                        operands[0],
                    )
                }
            }
            Operation::Or(left, right) => {
                let left_data = storage.interior_node_data(left);
                let right_data = storage.interior_node_data(right);
                // Frisch's union parks the later operand in the uncertain branch instead
                // of distributing it into both guarded branches. It is evaluated lazily.
                let (data, branch, child, remaining) = match left_data
                    .constraint
                    .ordering()
                    .cmp(&right_data.constraint.ordering())
                {
                    Ordering::Equal => (
                        left_data,
                        Branch::True,
                        Operation::Or(left_data.if_true, right_data.if_true),
                        RemainingBranches::UncertainThenFalse(
                            Operation::Or(left_data.if_uncertain, right_data.if_uncertain),
                            Operation::Or(left_data.if_false, right_data.if_false),
                        ),
                    ),
                    Ordering::Less => (
                        left_data,
                        Branch::Uncertain,
                        Operation::Or(left_data.if_uncertain, right),
                        RemainingBranches::Done,
                    ),
                    Ordering::Greater => (
                        right_data,
                        Branch::Uncertain,
                        Operation::Or(left, right_data.if_uncertain),
                        RemainingBranches::Done,
                    ),
                };
                (
                    Frame::Branches(Branches {
                        operation,
                        result: data,
                        branch,
                        remaining,
                    }),
                    child,
                )
            }
        };
        reserve_smallvec(&mut self.frames, 1, AllocationKind::Frames, control)?;
        self.frames.push(frame);
        self.step = Step::Operation(child);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

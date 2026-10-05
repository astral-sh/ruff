//! Graph combination followed by ordered source-history combination.

#[cfg(any(test, feature = "experimental-analysis"))]
use std::ops::ControlFlow;

use super::apply::Operation;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::apply::TddApply;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::control::{TddControl, TddError, TddWork};
use super::fold::ConstraintFoldKind;
use super::source_order::OrderedSource;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::source_order::PendingSourceOrder;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::{ConstraintSet, ConstraintSetBuilder};
use super::{ConstraintSetStorage, NodeId, SourceOrderId};

pub(super) type Value = (NodeId, Option<SourceOrderId>);

#[derive(Clone, Copy)]
pub(super) struct Combination {
    operation: Operation,
    left_source: Option<SourceOrderId>,
    right_source: Option<SourceOrderId>,
}

impl Combination {
    pub(super) fn new(kind: ConstraintFoldKind, left: Value, right: Value) -> Self {
        let operation = match kind {
            ConstraintFoldKind::All => Operation::And(left.0, right.0),
            ConstraintFoldKind::Any => Operation::Or(left.0, right.0),
        };
        Self {
            operation,
            left_source: left.1,
            right_source: right.1,
        }
    }

    fn after_graph(self, node: NodeId) -> SourceCombination {
        SourceCombination {
            node,
            source: OrderedSource::new(self.left_source, self.right_source),
        }
    }

    #[inline]
    pub(super) fn finish(self, storage: &mut ConstraintSetStorage<'_>) -> Value {
        let node = self.operation.apply(storage);
        self.after_graph(node).finish(storage)
    }
}

#[derive(Clone, Copy)]
struct SourceCombination {
    node: NodeId,
    source: OrderedSource,
}

impl SourceCombination {
    fn finish(self, storage: &mut ConstraintSetStorage<'_>) -> Value {
        (self.node, self.source.finish(storage))
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
enum PendingSource {
    Existing(Option<SourceOrderId>),
    Intern(PendingSourceOrder),
}

#[cfg(any(test, feature = "experimental-analysis"))]
#[expect(
    clippy::large_enum_variant,
    reason = "Store the graph inline without another allocation, and drop it before sidecar work"
)]
enum Pending {
    Graph {
        cursor: TddApply,
        continuation: Combination,
    },
    Source {
        node: NodeId,
        source: PendingSource,
    },
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) struct PendingCombination {
    pending: Pending,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl PendingCombination {
    pub(super) fn new(combination: Combination) -> Self {
        Self {
            pending: Pending::Graph {
                cursor: TddApply::new(combination.operation),
                continuation: combination,
            },
        }
    }

    /// Abandon this cursor after an error; completed graph and sidecar identities remain valid.
    pub(super) fn advance_with<C: TddControl>(
        &mut self,
        storage: &mut ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<Value>, TddError<C::Error>> {
        control.admit(TddWork::CombinationAdvance)?;
        match &mut self.pending {
            Pending::Graph {
                cursor,
                continuation,
            } => {
                if let ControlFlow::Break(node) = cursor.advance_with(storage, control)? {
                    let continuation = continuation.after_graph(node);
                    let source = match continuation.source {
                        OrderedSource::Existing(value) => PendingSource::Existing(value),
                        OrderedSource::Intern(data) => {
                            PendingSource::Intern(PendingSourceOrder::new(data))
                        }
                    };
                    self.pending = Pending::Source {
                        node: continuation.node,
                        source,
                    };
                }
                // Release the completed graph cursor before any sidecar suspension or growth.
                // Source-order publication belongs to a later advance than graph completion.
                Ok(ControlFlow::Continue(()))
            }
            Pending::Source { node, source } => {
                let source = match source {
                    PendingSource::Intern(cursor) => match cursor.advance_with(storage, control)? {
                        ControlFlow::Continue(()) => return Ok(ControlFlow::Continue(())),
                        ControlFlow::Break(source) => Some(source),
                    },
                    PendingSource::Existing(source) => *source,
                };
                Ok(ControlFlow::Break((*node, source)))
            }
        }
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct ConstraintCombination<'db, 'c> {
    builder: &'c ConstraintSetBuilder<'db>,
    pending: PendingCombination,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'db, 'c> ConstraintCombination<'db, 'c> {
    pub(in crate::types) fn new(
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Self {
        left.verify_builder(builder);
        right.verify_builder(builder);
        Self {
            builder,
            pending: PendingCombination::new(Combination::new(
                kind,
                (left.node, left.source_order),
                (right.node, right.source_order),
            )),
        }
    }

    /// After refusal, drop this cursor and retry on the same builder. No partial constraint
    /// pair is returned, although completed graph or source identities can remain reusable.
    pub(in crate::types) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, TddError<C::Error>> {
        Ok(self
            .pending
            .advance_with(&mut self.builder.storage.borrow_mut(), control)?
            .map_break(|(node, source)| ConstraintSet::from_node(self.builder, node, source)))
    }
}

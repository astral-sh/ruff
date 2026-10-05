//! Admitted source folds whose operands do not require persistent constraint storage.

use std::ops::ControlFlow;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::control::attempt::{EndpointAdmission, ExecutionControl};
use super::control::{AllocationKind, TddControl, TddError, TddWork};
use super::fold::{FoldFinish, FoldPush, PreparedFoldFinish, PreparedFoldPush};
use super::{
    ConstraintCombination, ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::relation::execution::{ExecutionAdmission, ExecutionWork};

/// Unsupported operands or inner effects must be reported by the enclosing source operation.
pub(in crate::types) enum SourceStructuralResult<T> {
    Complete(T),
    Unsupported,
}

/// Drives the ordinary fold cursors while retaining the caller's fold until
/// [`TaskEndpoint::local_call`] has drained its queued execution-runtime children.
///
/// Supported operands are terminals without source history. The fold can grow its own flat
/// accumulator, but neither graph storage nor source-history storage can change. A failed local
/// completion can follow acceptance of an input; discard that fold instead of replaying the input.
pub(in crate::types) struct SourceStructural<'effect, 'run, 'db: 'run> {
    endpoint: &'effect TaskEndpoint<'run, 'db>,
    #[cfg(test)]
    observer: Option<&'effect dyn Fn(SourceBoundary) -> RunResult<()>>,
}

impl<'effect, 'run, 'db: 'run> SourceStructural<'effect, 'run, 'db> {
    pub(in crate::types) fn new(endpoint: &'effect TaskEndpoint<'run, 'db>) -> Self {
        Self {
            endpoint,
            #[cfg(test)]
            observer: None,
        }
    }

    pub(in crate::types) async fn combine<'c>(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<SourceStructuralResult<ConstraintSet<'db, 'c>>> {
        let cursor = self
            .endpoint
            .local_call(|| {
                self.cursor_work::<ConstraintCombination<'db, 'c>>(2)?;
                verify_builder(builder, left)?;
                verify_builder(builder, right)?;
                Ok(
                    (left.is_source_free_terminal() && right.is_source_free_terminal())
                        .then(|| ConstraintCombination::new(builder, kind, left, right)),
                )
            })
            .await;
        match cursor {
            Some(cursor) => self.drive(cursor).await,
            None => Ok(SourceStructuralResult::Unsupported),
        }
    }

    pub(in crate::types) async fn push<'c>(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<SourceStructuralResult<ControlFlow<ConstraintSet<'db, 'c>>>> {
        let cursor = self
            .endpoint
            .local_call(|| {
                self.cursor_work::<PreparedFoldPush<'_, 'db, 'c>>(fold.accumulator.len())?;
                verify_builder(fold.builder(), next)?;
                Ok((next.is_source_free_terminal() && supported_fold(fold))
                    .then(|| fold.prepare_push(next)))
            })
            .await;
        match cursor {
            Some(cursor) => self.drive_prepared(cursor).await,
            None => Ok(SourceStructuralResult::Unsupported),
        }
    }

    pub(in crate::types) async fn finish<'c>(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> RunResult<SourceStructuralResult<ConstraintSet<'db, 'c>>> {
        let cursor = self
            .endpoint
            .local_call(|| {
                self.cursor_work::<PreparedFoldFinish<'_, 'db, 'c>>(fold.accumulator.len())?;
                Ok(supported_fold(fold).then(|| fold.prepare_finish()))
            })
            .await;
        match cursor {
            Some(cursor) => self.drive_prepared(cursor).await,
            None => Ok(SourceStructuralResult::Unsupported),
        }
    }

    fn cursor_work<C>(&self, entries: usize) -> RunResult<()> {
        let work = entries
            .checked_add(1)
            .ok_or(RunError::Contract("constraint cursor quotation overflow"))?;
        let bytes = size_of::<C>()
            .checked_mul(2)
            .ok_or(RunError::Contract("constraint cursor quotation overflow"))?;
        self.endpoint.admit_work(work)?;
        self.endpoint
            .admit(salsa::execution_probe::ExecutionWork::Resource {
                requested_bytes: bytes,
            })
    }

    async fn drive_prepared<P: SourcePreparation>(
        &self,
        prepared: P,
    ) -> RunResult<SourceStructuralResult<P::Value>> {
        let admission = EndpointAdmission(self.endpoint);
        let mut control = SourceControl::new(&admission);
        // Keep both the prepared operation and promoted cursor outside `local_call` callbacks
        // so queued execution-runtime children drain before either releases the fold borrow,
        // including when promotion's admission refuses.
        let mut prepared = Some(prepared);
        let progress = self
            .endpoint
            .local_call(|| {
                let prepared = prepared
                    .as_mut()
                    .ok_or(RunError::Contract("source fold preparation was retired"))?;
                let progress = source_result(prepared.advance(&mut control))?;
                #[cfg(test)]
                if let Some(observer) = self.observer {
                    observer(SourceBoundary::Prepared)?;
                }
                Ok(progress)
            })
            .await;
        match progress {
            SourceStructuralResult::Complete(ControlFlow::Break(value)) => {
                self.endpoint
                    .local_call(|| {
                        drop(prepared.take());
                        #[cfg(test)]
                        if let Some(observer) = self.observer {
                            observer(SourceBoundary::Retired)?;
                        }
                        Ok(())
                    })
                    .await;
                Ok(SourceStructuralResult::Complete(value))
            }
            SourceStructuralResult::Complete(ControlFlow::Continue(())) => {
                let mut cursor = None;
                self.endpoint
                    .local_call(|| {
                        #[cfg(test)]
                        if let Some(observer) = self.observer {
                            observer(SourceBoundary::Promoting)?;
                        }
                        self.cursor_work::<P::Cursor>(0)?;
                        cursor = prepared.take().map(SourcePreparation::into_cursor);
                        Ok(())
                    })
                    .await;
                let cursor = cursor.ok_or(RunError::Contract(
                    "source fold promotion did not produce a cursor",
                ))?;
                self.drive(cursor).await
            }
            SourceStructuralResult::Unsupported => Ok(SourceStructuralResult::Unsupported),
        }
    }

    async fn drive<C: SourceCursor>(
        &self,
        cursor: C,
    ) -> RunResult<SourceStructuralResult<C::Value>> {
        let admission = EndpointAdmission(self.endpoint);
        let mut control = SourceControl::new(&admission);
        // Keep the cursor outside each callback: refusal can queue a child that must drain
        // before this cursor releases its borrow of the enclosing fold and builder.
        let mut cursor = Some(cursor);
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    let cursor = cursor
                        .as_mut()
                        .ok_or(RunError::Contract("source constraint cursor was retired"))?;
                    source_result(cursor.advance(&mut control))
                })
                .await;
            match progress {
                SourceStructuralResult::Complete(ControlFlow::Continue(())) => {}
                SourceStructuralResult::Complete(ControlFlow::Break(value)) => {
                    self.endpoint
                        .local_call(|| {
                            drop(cursor.take());
                            Ok(())
                        })
                        .await;
                    return Ok(SourceStructuralResult::Complete(value));
                }
                SourceStructuralResult::Unsupported => {
                    return Ok(SourceStructuralResult::Unsupported);
                }
            }
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceBoundary {
    Prepared,
    Promoting,
    Retired,
}

fn source_result<T>(
    result: Result<T, TddError<SourceControlError<RunError>>>,
) -> RunResult<SourceStructuralResult<T>> {
    match result {
        Ok(value) => Ok(SourceStructuralResult::Complete(value)),
        Err(TddError::Refused(SourceControlError::Unsupported)) => {
            Ok(SourceStructuralResult::Unsupported)
        }
        Err(TddError::Refused(SourceControlError::Admission(error))) => Err(error),
        Err(TddError::CapacityExhausted) => Err(RunError::Refused(
            salsa::attempt_probe::Incomplete::RequestedAllocation,
        )),
    }
}

fn verify_builder<'db>(
    builder: &ConstraintSetBuilder<'db>,
    value: ConstraintSet<'db, '_>,
) -> RunResult<()> {
    if std::ptr::eq(builder, value.builder) {
        Ok(())
    } else {
        Err(RunError::Contract(
            "source constraint belongs to another builder",
        ))
    }
}

fn supported_fold(fold: &ConstraintFold<'_, '_>) -> bool {
    fold.accumulator
        .iter()
        .all(|(node, source, _)| node.is_terminal() && source.is_none())
}

#[derive(Debug, Eq, PartialEq)]
enum SourceControlError<E> {
    Admission(E),
    Unsupported,
}

struct SourceControl<'a, A: ?Sized> {
    admission: &'a A,
    control: ExecutionControl<'a, A>,
}

impl<'a, A: ExecutionAdmission + ?Sized> SourceControl<'a, A> {
    fn new(admission: &'a A) -> Self {
        Self {
            admission,
            control: ExecutionControl::new(admission),
        }
    }
}

impl<A: ExecutionAdmission + ?Sized> TddControl for SourceControl<'_, A> {
    type Error = SourceControlError<A::Error>;

    fn admit(&mut self, work: TddWork) -> Result<(), Self::Error> {
        match work {
            TddWork::FoldAdvance
            | TddWork::FoldCommit { .. }
            | TddWork::CombinationAdvance
            | TddWork::Advance => {}
            TddWork::Grow {
                allocation: AllocationKind::FoldAccumulator,
                ..
            } => {
                // Accumulator entries are Copy. Its only additional disposal is freeing the
                // backing allocation, prepaid before the ordinary growth admission and reserve.
                self.admission
                    .admit(ExecutionWork::Work { units: 1 })
                    .map_err(SourceControlError::Admission)?;
            }
            _ => return Err(SourceControlError::Unsupported),
        }
        self.control
            .admit(work)
            .map_err(SourceControlError::Admission)
    }
}

trait SourceCursor {
    type Value;

    fn advance<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<Self::Value>, TddError<C::Error>>;
}

trait SourcePreparation: SourceCursor {
    type Cursor: SourceCursor<Value = Self::Value>;

    fn into_cursor(self) -> Self::Cursor;
}

impl<'db, 'c> SourceCursor for PreparedFoldPush<'_, 'db, 'c> {
    type Value = ControlFlow<ConstraintSet<'db, 'c>>;

    fn advance<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<Self::Value>, TddError<C::Error>> {
        self.advance_with(control)
    }
}

impl<'fold, 'db, 'c> SourcePreparation for PreparedFoldPush<'fold, 'db, 'c> {
    type Cursor = FoldPush<'fold, 'db, 'c>;

    fn into_cursor(self) -> Self::Cursor {
        self.into_cursor()
    }
}

impl<'db, 'c> SourceCursor for PreparedFoldFinish<'_, 'db, 'c> {
    type Value = ConstraintSet<'db, 'c>;

    fn advance<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<Self::Value>, TddError<C::Error>> {
        self.advance_with(control)
    }
}

impl<'fold, 'db, 'c> SourcePreparation for PreparedFoldFinish<'fold, 'db, 'c> {
    type Cursor = FoldFinish<'fold, 'db, 'c>;

    fn into_cursor(self) -> Self::Cursor {
        self.into_cursor()
    }
}

impl<'db, 'c> SourceCursor for ConstraintCombination<'db, 'c> {
    type Value = ConstraintSet<'db, 'c>;

    fn advance<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<Self::Value>, TddError<C::Error>> {
        self.advance_with(control)
    }
}

impl<'db, 'c> SourceCursor for FoldPush<'_, 'db, 'c> {
    type Value = ControlFlow<ConstraintSet<'db, 'c>>;

    fn advance<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<Self::Value>, TddError<C::Error>> {
        self.advance_with(control)
    }
}

impl<'db, 'c> SourceCursor for FoldFinish<'_, 'db, 'c> {
    type Value = ConstraintSet<'db, 'c>;

    fn advance<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<Self::Value>, TddError<C::Error>> {
        self.advance_with(control)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::ops::ControlFlow;

    use salsa::execution_probe::{
        Demand, ExecutionAdmission as RuntimeAdmission, ExecutionWork as RuntimeWork,
        RegistryBuilder, RunError, RunResult,
    };

    use super::{
        SourceBoundary, SourceControl, SourceControlError, SourceCursor, SourceStructural,
        SourceStructuralResult, supported_fold,
    };
    use crate::db::tests::setup_db;
    use crate::types::constraints::control::{TddControl, TddError};
    use crate::types::constraints::storage::OverlayIdentityState;
    use crate::types::constraints::{
        ALWAYS_FALSE, ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
        NodeId, SourceOrderId,
    };
    use crate::types::constructor::expansion_probe::{self, Incomplete};
    use crate::types::relation::execution::{ExecutionAdmission, ExecutionWork, SchedulingFailure};

    #[derive(Default)]
    struct Admission {
        events: RefCell<Vec<ExecutionWork>>,
        refuse_allocation: Cell<bool>,
    }

    impl ExecutionAdmission for Admission {
        type Error = ();

        fn admit(&self, work: ExecutionWork) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(work);
            if self.refuse_allocation.get() && matches!(work, ExecutionWork::Allocation { .. }) {
                Err(())
            } else {
                Ok(())
            }
        }

        fn scheduling_failure(&self, _failure: SchedulingFailure) {}
    }

    fn complete<C: SourceCursor, A: TddControl>(
        mut cursor: C,
        control: &mut A,
    ) -> Result<C::Value, TddError<A::Error>> {
        loop {
            if let ControlFlow::Break(value) = cursor.advance(control)? {
                return Ok(value);
            }
        }
    }

    #[test]
    fn terminal_fold_refuses_spill_before_changing_accepted_entries()
    -> Result<(), TddError<SourceControlError<()>>> {
        let builder = ConstraintSetBuilder::new();
        let mut fold = ConstraintFold::new(&builder, ConstraintFoldKind::Any);
        let admission = Admission::default();
        let mut control = SourceControl::new(&admission);
        let value = ConstraintSet::from_bool(&builder, false);
        for _ in 0..510 {
            assert!(complete(fold.begin_push(value), &mut control)?.is_continue());
        }
        let accepted = fold.accumulator.clone();
        let capacity = fold.accumulator.capacity();
        admission.refuse_allocation.set(true);
        assert!(matches!(
            complete(fold.begin_push(value), &mut control),
            Err(TddError::Refused(SourceControlError::Admission(())))
        ));
        assert_eq!(fold.accumulator, accepted);
        assert_eq!(fold.accumulator.capacity(), capacity);
        {
            let events = admission.events.borrow();
            assert_eq!(
                &events[events.len() - 3..events.len() - 1],
                &[
                    ExecutionWork::Work { units: 1 },
                    ExecutionWork::Work { units: capacity },
                ]
            );
        }
        admission.refuse_allocation.set(false);
        assert!(complete(fold.begin_push(value), &mut control)?.is_continue());
        assert!(fold.accumulator.spilled());
        assert!(supported_fold(&fold));
        assert!(complete(fold.begin_finish(), &mut control)?.is_trivially_never_satisfied());
        let storage = builder.storage.borrow();
        assert_eq!(storage.overlay_identity_state, OverlayIdentityState::Start);
        assert_eq!(
            [
                storage.nodes.len(),
                storage.supports.len(),
                storage.source_orders.len(),
                storage.node_cache.len(),
                storage.source_order_cache.len(),
                storage.and_cache.len(),
                storage.or_cache.len(),
            ],
            [0; 7]
        );
        Ok(())
    }

    #[test]
    fn fold_preflight_rejects_history_and_nonterminal_entries() {
        let builder = ConstraintSetBuilder::new();
        let mut fold = ConstraintFold::new(&builder, ConstraintFoldKind::Any);
        assert!(supported_fold(&fold));
        for (node, source) in [
            (ALWAYS_FALSE, Some(SourceOrderId::from_usize(0))),
            (NodeId::from_usize(0), None),
        ] {
            // Preflight examines only the handles. Neither rejected entry reaches storage.
            fold.accumulator.clear();
            fold.accumulator.push((node, source, 0));
            assert!(!supported_fold(&fold));
            assert!(!ConstraintSet::from_node(&builder, node, source).is_source_free_terminal());
        }
    }

    #[derive(Default)]
    struct RuntimeControl {
        refuse_work: Cell<bool>,
    }

    impl RuntimeAdmission for RuntimeControl {
        fn admit(&self, work: RuntimeWork) -> RunResult<()> {
            if self.refuse_work.get() && matches!(work, RuntimeWork::Work { .. }) {
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance,
                ))
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct Lifetime {
        fold_live: Cell<bool>,
        child_dropped: Cell<bool>,
        fold_dropped: Cell<bool>,
        returned: Cell<bool>,
    }

    struct FoldOwner<'a, 'db, 'c> {
        fold: ConstraintFold<'db, 'c>,
        lifetime: &'a Lifetime,
    }

    impl Drop for FoldOwner<'_, '_, '_> {
        fn drop(&mut self) {
            assert!(self.lifetime.child_dropped.get());
            assert_eq!(self.fold.accumulator.as_slice(), [(ALWAYS_FALSE, None, 0)]);
            self.lifetime.fold_live.set(false);
            self.lifetime.fold_dropped.set(true);
        }
    }

    struct QueuedChild<'a, 'db> {
        lifetime: &'a Lifetime,
        builder: &'a ConstraintSetBuilder<'db>,
    }

    impl Drop for QueuedChild<'_, '_> {
        fn drop(&mut self) {
            assert!(self.lifetime.fold_live.get());
            assert!(self.builder.storage.try_borrow_mut().is_ok());
            self.lifetime.child_dropped.set(true);
        }
    }

    #[test]
    fn source_preparation_failures_drain_children_before_dropping_the_fold() {
        for boundary in [
            SourceBoundary::Prepared,
            SourceBoundary::Promoting,
            SourceBoundary::Retired,
        ] {
            let db = setup_db();
            let builder = ConstraintSetBuilder::new();
            let lifetime = Lifetime::default();
            let admission = RuntimeControl::default();
            let pending: RefCell<Option<Demand<()>>> = RefCell::new(None);
            let (result, _) = expansion_probe::run(&db, usize::MAX, || {
                RegistryBuilder::new(&db, &admission)?
                    .seal()?
                    .run(|endpoint| {
                        let builder = &builder;
                        let lifetime = &lifetime;
                        let admission = &admission;
                        let pending = &pending;
                        async move {
                            let observer = |actual| {
                                if actual != boundary {
                                    return Ok(());
                                }
                                let child = QueuedChild { lifetime, builder };
                                *pending.borrow_mut() =
                                    Some(endpoint.demand(move || async move {
                                        let _child = child;
                                        Ok(())
                                    })?);
                                if boundary == SourceBoundary::Promoting {
                                    admission.refuse_work.set(true);
                                    Ok(())
                                } else {
                                    Err(RunError::Refused(
                                        salsa::attempt_probe::Incomplete::Allowance,
                                    ))
                                }
                            };
                            let mut operations = SourceStructural::new(&endpoint);
                            operations.observer = Some(&observer);
                            lifetime.fold_live.set(true);
                            let mut owner = FoldOwner {
                                fold: ConstraintFold::new(builder, ConstraintFoldKind::Any),
                                lifetime,
                            };
                            let value = ConstraintSet::from_bool(builder, false);
                            if boundary == SourceBoundary::Promoting {
                                assert!(owner.fold.push(value).is_continue());
                            }
                            let _ = operations.push(&mut owner.fold, value).await?;
                            lifetime.returned.set(true);
                            Ok(())
                        }
                    })
            });
            assert!(matches!(result, Err(Incomplete::Allowance)), "{boundary:?}");
            assert!(lifetime.child_dropped.get());
            assert!(lifetime.fold_dropped.get());
            assert!(!lifetime.returned.get());
            assert!(builder.storage.try_borrow_mut().is_ok());
            pending.borrow_mut().take();
        }
    }

    #[test]
    fn source_preparation_completes_without_promotion_and_preserves_rejection() {
        let db = setup_db();
        let builder = ConstraintSetBuilder::new();
        let admission = RuntimeControl::default();
        let promotions = Cell::new(0);
        let (result, _) = expansion_probe::run(&db, usize::MAX, || {
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| {
                    let builder = &builder;
                    let promotions = &promotions;
                    async move {
                        let observer = |boundary| {
                            if boundary == SourceBoundary::Promoting {
                                promotions.set(promotions.get() + 1);
                            }
                            Ok(())
                        };
                        let mut operations = SourceStructural::new(&endpoint);
                        operations.observer = Some(&observer);
                        let mut fold = ConstraintFold::new(builder, ConstraintFoldKind::Any);
                        let SourceStructuralResult::Complete(empty) =
                            operations.finish(&mut fold).await?
                        else {
                            return Err(RunError::Contract("empty source fold was unsupported"));
                        };
                        assert!(empty.is_trivially_never_satisfied());
                        assert!(matches!(
                            operations.push(&mut fold, empty).await?,
                            SourceStructuralResult::Complete(ControlFlow::Continue(()))
                        ));
                        assert_eq!(promotions.get(), 0);
                        assert!(matches!(
                            operations.push(&mut fold, empty).await?,
                            SourceStructuralResult::Complete(ControlFlow::Continue(()))
                        ));
                        assert_eq!(promotions.get(), 1);
                        let accepted = fold.accumulator.clone();
                        for (node, source) in [
                            (ALWAYS_FALSE, Some(SourceOrderId::from_usize(0))),
                            (NodeId::from_usize(0), None),
                        ] {
                            assert!(matches!(
                                operations
                                    .push(
                                        &mut fold,
                                        ConstraintSet::from_node(builder, node, source)
                                    )
                                    .await?,
                                SourceStructuralResult::Unsupported
                            ));
                            assert_eq!(fold.accumulator, accepted);
                        }
                        Ok(())
                    }
                })
        });
        assert!(matches!(result, Ok(Ok(()))));
    }
}

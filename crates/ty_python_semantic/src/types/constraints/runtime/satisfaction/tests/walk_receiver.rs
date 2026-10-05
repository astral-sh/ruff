//! Destruction ordering for receiver cursors removed from the runtime walk's pending stack.

use super::*;
use crate::types::BindingContext;
use crate::types::constraints::{OwnedConstraintTypeCursor, ReceiverCursorState};
use crate::types::infer::legacy_callable_observations as observations;

/// The admissions after `take_action` has removed a pending frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterPop {
    SelectionWork,
    SelectionResource,
    TransferWork,
    TransferResource,
}

impl AfterPop {
    /// Returns the zero-based admission offset within `take_action`, after its pop-admission pair.
    const fn offset(self) -> usize {
        match self {
            Self::SelectionWork => 2,
            Self::SelectionResource => 3,
            Self::TransferWork => 4,
            Self::TransferResource => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Refusal,
    NativePanic,
}

/// Injects a refusal or native panic through the existing child-cleanup admission fixture.
struct ReceiverAdmission<'a, 'run, 'db: 'run> {
    inner: CacheAdmission<'run, 'db>,
    failure: Failure,
    work: &'a RefCell<Vec<ExecutionWork>>,
    fired: &'a Cell<bool>,
}

impl ExecutionAdmission for ReceiverAdmission<'_, '_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.work.borrow_mut().push(work);
        let result = self.inner.admit(work);
        self.fired.set(self.inner.fired.get());
        if result.is_err() && self.failure == Failure::NativePanic {
            std::panic::panic_any(WalkNativePanic);
        }
        result
    }
}

/// Results and physical-owner observations from one receiver-frame removal.
#[derive(Debug)]
struct ReceiverRun<'db> {
    result: Result<Result<RunResult<bool>, Incomplete>, ()>,
    work: Vec<ExecutionWork>,
    operation_range: std::ops::Range<usize>,
    snapshot: WalkOwnerSnapshot<'db>,
    initial: ReceiverCursorState,
    at_child_cleanup: Option<observations::Snapshot>,
    after: observations::Snapshot,
}

/// Removes a populated receiver cursor, optionally failing one admission after the pop.
/// The queued child's destructor snapshots the real cursor's observation journal before the
/// enclosing walk owner is destroyed; the snapshot never retains the cursor itself.
fn run_receiver_pop<'db>(
    db: &'db TestDb,
    owned: &'db OwnedConstraintSet<'db>,
    types: &[Type<'db>; 2],
    refused: Option<usize>,
    failure: Failure,
) -> ReceiverRun<'db> {
    let live = Cell::new(false);
    let delivered = Cell::new(false);
    let journal = RefCell::new(Vec::new());
    let snapshot = RefCell::new(None);
    let initial = Cell::new(None);
    let work = RefCell::new(Vec::new());
    let fired = Cell::new(false);
    let operation_start = Cell::new(0);
    let operation_end = Cell::new(0);
    let child_started = Cell::new(false);
    let child_drops = Cell::new(0);
    let at_child_cleanup = RefCell::new(None);
    let cleanup_owner = Cell::new(None);
    let cleanup = || {
        *at_child_cleanup.borrow_mut() = Some(observations::snapshot());
        cleanup_owner.set(Some((live.get(), delivered.get(), snapshot.borrow().is_none())));
        child_drops.set(child_drops.get() + 1);
        journal.borrow_mut().push("child");
    };
    let recording = observations::Recording::start(observations::Cancellation::Never);
    let captured = prepared_source_probe::capture(db, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            expansion_probe::run(db, usize::MAX, || {
                let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
                let pending = RefCell::new(None);
                let admission = ReceiverAdmission {
                    inner: CacheAdmission {
                        events: RefCell::new(Vec::new()),
                        refuse: refused,
                        fired: Cell::new(false),
                        endpoint: &endpoint_slot,
                        pending: &pending,
                        cleanup: &cleanup,
                        child_started: &child_started,
                    },
                    failure,
                    work: &work,
                    fired: &fired,
                };
                let _reset = CacheReset {
                    endpoint: &endpoint_slot,
                    pending: &pending,
                };
                let registry = RegistryBuilder::new(db, &admission)?;
                let admission = &admission;
                let live = &live;
                let delivered = &delivered;
                let journal = &journal;
                let snapshot = &snapshot;
                let initial = &initial;
                let operation_start = &operation_start;
                let operation_end = &operation_end;
                registry.seal()?.run(move |endpoint| {
                    **admission.inner.endpoint.borrow_mut() = Some(endpoint.clone());
                    async move {
                        let mut receiver = OwnedConstraintTypeCursor::new(owned);
                        assert!(matches!(
                            unrestricted(receiver.next_with(&mut UnrestrictedCollections)),
                            Some(Some(_))
                        ));
                        initial.set(Some(receiver.state()));
                        let mut cursor = TypeWalkCursor {
                            pending: types.iter().copied().map(WalkAction::Visit).collect(),
                        };
                        cursor.pending.push(WalkAction::ConstraintTypes(receiver));
                        live.set(true);
                        let mut owner = WalkOwner {
                            cursor,
                            seen: TypeCollector::default(),
                            active: FxHashSet::default(),
                            target: types[0],
                            live,
                            journal,
                            snapshot,
                        };
                        let mut effects =
                            type_search::RuntimeTypeWalk::new(db, &endpoint, BoundSearch::TypeVar);
                        operation_start.set(admission.inner.events.borrow().len());
                        operation_end.set(operation_start.get());
                        let action = effects.take_action(&mut owner.cursor).await?;
                        operation_end.set(admission.inner.events.borrow().len());
                        let found = matches!(&action, Some(WalkAction::ConstraintTypes(_)));
                        delivered.set(true);
                        Ok(found)
                    }
                })
            })
        }))
    })
    .expect("receiver pop capture");
    let after = observations::snapshot();
    drop(recording);
    let result = captured.value.map(|(result, _)| result).map_err(|payload| {
        assert!(payload.is::<WalkNativePanic>());
    });
    assert!(captured.reads.is_empty());
    assert_eq!(fired.get(), refused.is_some());
    assert!(!live.get());
    assert!(!child_started.get());
    assert_eq!(child_drops.get(), usize::from(refused.is_some()));
    if refused.is_some() {
        assert_eq!(cleanup_owner.get(), Some((true, false, true)));
        assert!(!delivered.get());
        assert_eq!(&*journal.borrow(), &["child", "walk"]);
    } else {
        assert_eq!(cleanup_owner.get(), None);
        assert!(delivered.get());
        assert_eq!(&*journal.borrow(), &["walk"]);
    }
    ReceiverRun {
        result,
        work: work.into_inner(),
        operation_range: operation_start.get()..operation_end.get(),
        snapshot: snapshot.into_inner().expect("the walk owner was destroyed"),
        initial: initial.get().expect("the receiver cursor was populated"),
        at_child_cleanup: at_child_cleanup.into_inner(),
        after,
    }
}

/// Refusal or native panic at every post-pop admission retains the removed receiver cursor
/// through queued child cleanup. Its destructor enters exactly once afterward, with the same
/// populated state; a funded retry on the same constraint set returns the receiver frame.
#[test_case::test_case(AfterPop::SelectionWork, Failure::Refusal)]
#[test_case::test_case(AfterPop::SelectionResource, Failure::Refusal)]
#[test_case::test_case(AfterPop::TransferWork, Failure::Refusal)]
#[test_case::test_case(AfterPop::TransferResource, Failure::Refusal)]
#[test_case::test_case(AfterPop::SelectionWork, Failure::NativePanic)]
#[test_case::test_case(AfterPop::SelectionResource, Failure::NativePanic)]
#[test_case::test_case(AfterPop::TransferWork, Failure::NativePanic)]
#[test_case::test_case(AfterPop::TransferResource, Failure::NativePanic)]
fn removed_receiver_survives_post_pop_failure(boundary: AfterPop, failure: Failure) {
    let db = setup_db();
    let env = db.program_environment();
    let variable = BoundTypeVarInstance::synthetic_self(
        &db,
        Type::object(),
        BindingContext::Synthetic(env.program(&db)),
    );
    let owned = ConstraintSetBuilder::new().into_owned(|builder| {
        ConstraintSet::constrain_typevar_lower_bound(
            &db,
            &env,
            builder,
            variable,
            Type::int_literal(1),
        )
    });
    let types = [Type::int_literal(2), Type::int_literal(3)];
    let complete = run_receiver_pop(&db, &owned, &types, None, failure);
    assert_eq!(complete.result, Ok(Ok(Ok(true))));
    // Pop, quote selection, and result transfer each admit work followed by fixed resource bytes.
    // Only the first pair precedes removal of the owning ConstraintTypes frame.
    //
    assert!(matches!(
        &complete.work[complete.operation_range.clone()],
        [ExecutionWork::Work { .. }, ExecutionWork::Resource { .. },
         ExecutionWork::Work { .. }, ExecutionWork::Resource { .. },
         ExecutionWork::Work { .. }, ExecutionWork::Resource { .. }]
    ));
    assert!(complete.initial.next > 0);
    assert!(complete.initial.seen_len > 0);
    assert!(complete.initial.seen_capacity > 0);
    assert_eq!(complete.snapshot.pending_len, 2);
    assert_eq!(complete.snapshot.pending, types);
    assert!(complete.at_child_cleanup.is_none());

    let index = complete.operation_range.start + boundary.offset();
    let failed = run_receiver_pop(&db, &owned, &types, Some(index), failure);
    assert_eq!(
        failed.result,
        match failure {
            Failure::Refusal => Ok(Err(Incomplete::Allowance)),
            Failure::NativePanic => Err(()),
        }
    );
    assert_eq!(failed.work[..=index], complete.work[..=index]);
    assert_eq!(failed.initial, complete.initial);
    assert_eq!(failed.snapshot.pending_len, 2);
    assert_eq!(failed.snapshot.pending, types);
    assert_eq!(failed.snapshot.pending_capacity, complete.snapshot.pending_capacity);
    let during = failed.at_child_cleanup.expect("the queued child was cleaned up");
    assert_eq!(during.live_cursors, 1);
    assert!(matches!(
        during.events.as_slice(),
        [observations::Event::CursorCreated(id)] if Some(*id) == failed.initial.observation_id
    ));
    // `receiver_cursor_drop` observes destructor entry, before the cursor's fields finish dropping.
    //
    assert_eq!(failed.after.live_cursors, 0);
    assert!(matches!(
        failed.after.events.as_slice(),
        [observations::Event::CursorCreated(created), observations::Event::CursorDrop { id, state }]
            if created == id && state == &failed.initial
    ));

    let retry = run_receiver_pop(&db, &owned, &types, None, failure);
    assert_eq!(retry.result, complete.result);
    assert_eq!(retry.work, complete.work);
    assert_eq!(retry.snapshot, complete.snapshot);
    assert_eq!(retry.initial, complete.initial);
    assert_eq!(retry.after.live_cursors, 0);
    assert!(matches!(
        retry.after.events.as_slice(),
        [observations::Event::CursorCreated(created), observations::Event::CursorDrop { id, state }]
            if created == id && state == &retry.initial
    ));
}

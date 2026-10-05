use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::ops::{ControlFlow, Deref, DerefMut};
use std::panic::{AssertUnwindSafe, catch_unwind};

use ruff_python_ast::name::Name;
use salsa::Database;
use salsa::execution_probe::{
    Demand, ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};

use super::RuntimeStructural;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::control::{TddControl, TddError, TddWork};
use crate::types::constraints::{
    ConstraintCombination, ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::{BoundTypeVarInstance, KnownClass, TypeVarVariance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StructuralOperation {
    Combine,
    Push,
    Finish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StructuralBoundary {
    AfterAdvanceReturned {
        operation: StructuralOperation,
        outer_complete: bool,
    },
    BeforeRetirementAcceptance {
        operation: StructuralOperation,
    },
}

#[derive(Default)]
pub(super) struct CursorLifetime {
    pub(super) live: Cell<usize>,
    pub(super) drops_started: Cell<usize>,
}

pub(super) struct ObservedCursor<'a, C> {
    cursor: C,
    lifetime: Option<&'a CursorLifetime>,
}

impl<'a, C> ObservedCursor<'a, C> {
    pub(super) fn new(cursor: C, lifetime: Option<&'a CursorLifetime>) -> Self {
        if let Some(lifetime) = lifetime {
            lifetime.live.set(lifetime.live.get() + 1);
        }
        Self { cursor, lifetime }
    }
}

impl<C> Deref for ObservedCursor<'_, C> {
    type Target = C;

    fn deref(&self) -> &C {
        &self.cursor
    }
}

impl<C> DerefMut for ObservedCursor<'_, C> {
    fn deref_mut(&mut self) -> &mut C {
        &mut self.cursor
    }
}

impl<C> Drop for ObservedCursor<'_, C> {
    fn drop(&mut self) {
        if let Some(lifetime) = self.lifetime {
            lifetime.live.set(lifetime.live.get() - 1);
            lifetime.drops_started.set(lifetime.drops_started.get() + 1);
        }
    }
}

#[derive(Default)]
struct BoundaryObservation {
    snapshots: RefCell<Vec<(bool, usize)>>,
    advances_returned: Cell<usize>,
    completions_returned: Cell<usize>,
    retirements: Cell<usize>,
    delivered: Cell<bool>,
    runtime_error: Cell<Option<RunError>>,
}

fn inputs<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
) -> [ConstraintSet<'db, 'c>; 7] {
    let env = db.program_environment();
    let int = KnownClass::Int.to_instance(db, &env);
    ["A", "B", "C", "D", "E", "F", "G"].map(|name| {
        let typevar = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, typevar, int)
    })
}

pub(super) fn assert_same<'db>(left: ConstraintSet<'db, '_>, right: ConstraintSet<'db, '_>) {
    assert!(std::ptr::eq(left.builder, right.builder));
    assert_eq!(left.node, right.node);
    assert_eq!(left.source_order, right.source_order);
}

pub(super) struct Admission<'a, 'db> {
    pub(super) builder: &'a ConstraintSetBuilder<'db>,
    pub(super) events: RefCell<Vec<ExecutionWork>>,
    pub(super) polls: Cell<usize>,
    pub(super) refuse: Option<usize>,
}

impl<'a, 'db> Admission<'a, 'db> {
    pub(super) fn new(_db: &'db TestDb, builder: &'a ConstraintSetBuilder<'db>) -> Self {
        Self {
            builder,
            events: RefCell::new(Vec::new()),
            polls: Cell::new(0),
            refuse: None,
        }
    }
}

impl ExecutionAdmission for Admission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let index = self.events.borrow().len();
        self.events.borrow_mut().push(work);
        if work == ExecutionWork::Poll {
            assert!(self.builder.storage.try_borrow_mut().is_ok());
            self.polls.set(self.polls.get() + 1);
        }
        if self.refuse == Some(index) {
            return Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            ));
        }
        Ok(())
    }
}

fn combine_run<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    admission: &Admission<'_, 'db>,
    kind: ConstraintFoldKind,
    left: ConstraintSet<'db, 'c>,
    right: ConstraintSet<'db, 'c>,
    observation: &BoundaryObservation,
) -> Result<RunResult<ConstraintSet<'db, 'c>>, Incomplete> {
    let observer = |_: &TaskEndpoint<'_, '_>, boundary| {
        let storage = builder
            .storage
            .try_borrow_mut()
            .expect("local boundaries release the engine's storage borrow");
        match boundary {
            StructuralBoundary::AfterAdvanceReturned {
                operation,
                outer_complete,
            } => {
                assert_eq!(operation, StructuralOperation::Combine);
                observation
                    .advances_returned
                    .set(observation.advances_returned.get() + 1);
                if outer_complete {
                    observation
                        .completions_returned
                        .set(observation.completions_returned.get() + 1);
                }
                let pair = (left.node, right.node);
                observation.snapshots.borrow_mut().push((
                    storage.and_cache.contains_key(&pair) || storage.or_cache.contains_key(&pair),
                    storage.source_orders.raw.len(),
                ));
            }
            StructuralBoundary::BeforeRetirementAcceptance { .. } => {
                observation
                    .retirements
                    .set(observation.retirements.get() + 1);
            }
        }
        Ok(())
    };
    expansion_probe::run(db, usize::MAX, || {
        let result = (|| {
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(|endpoint| {
                    let observer = &observer;
                    async move {
                        let mut operations = RuntimeStructural::new(db, endpoint);
                        operations.observer = Some(observer);
                        let result = operations.combine(builder, kind, left, right).await;
                        observation.delivered.set(true);
                        result
                    }
                })
        })();
        observation
            .runtime_error
            .set(result.as_ref().err().copied());
        result
    })
    .0
}

#[test]
fn borrowed_fold_and_real_child_share_one_registered_run() {
    let db = setup_db();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = ConstraintSetBuilder::new();
        let values = inputs(&db, &builder);
        let admission = Admission::new(&db, &builder);
        let accepted = RefCell::new(Vec::new());
        let (result, _) = expansion_probe::run(&db, usize::MAX, || {
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| {
                    let builder = &builder;
                    let accepted = &accepted;
                    let db = &db;
                    async move {
                        let operations = RuntimeStructural::new(db, endpoint.clone());
                        let mut fold = ConstraintFold::new(builder, kind);
                        for (index, mut next) in values.into_iter().enumerate() {
                            if index == 3 {
                                next = endpoint.demand(move || async move { Ok(next) })?.await?;
                            }
                            assert!(operations.push(&mut fold, next).await?.is_continue());
                        }
                        *accepted.borrow_mut() = fold.accumulator.to_vec();
                        operations.finish(&mut fold).await
                    }
                })
        });
        let actual = result
            .expect("complete attempt")
            .expect("complete runtime task");
        let mut ordinary = ConstraintFold::new(&builder, kind);
        for next in values {
            assert!(ordinary.push(next).is_continue());
        }
        assert_eq!(*accepted.borrow(), ordinary.accumulator.as_slice());
        assert_same(actual, ordinary.finish());
        assert!(!actual.node.is_terminal());
        assert_eq!(admission.polls.get(), 3);
        assert_eq!(
            admission
                .events
                .borrow()
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Task { .. }))
                .count(),
            2,
            "local progress creates no child tasks"
        );
        assert!(builder.storage.try_borrow_mut().is_ok());
    }
}

#[derive(Default)]
struct StructuralTrace(Vec<ExecutionWork>);

impl TddControl for StructuralTrace {
    type Error = Infallible;

    fn admit(&mut self, work: TddWork) -> Result<(), Infallible> {
        self.0.push(ExecutionWork::Work {
            units: work.work_units(),
        });
        if work.requested_payload_bytes() != 0 {
            self.0.push(ExecutionWork::Resource {
                requested_bytes: work.requested_payload_bytes(),
            });
        }
        Ok(())
    }
}

#[test]
fn nonterminal_combination_exhausts_shared_work_and_retries_on_the_same_builder() {
    let db = setup_db();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        // Measure one actual advance with the same cold input graphs. Resource reports are
        // telemetry; the allowance is exactly the semantic work accepted by this advance.
        let calibration_builder = ConstraintSetBuilder::new();
        let [calibration_left, calibration_right, ..] = inputs(&db, &calibration_builder);
        let mut calibration = ConstraintCombination::new(
            &calibration_builder,
            kind,
            calibration_left,
            calibration_right,
        );
        let mut trace = StructuralTrace::default();
        assert!(
            calibration
                .advance_with(&mut trace)
                .expect("finite advance")
                .is_continue()
        );
        let allowance = trace
            .0
            .iter()
            .map(|work| match work {
                ExecutionWork::Work { units } => *units,
                _ => 0,
            })
            .sum::<usize>();
        assert!(allowance > 0);

        let builder = ConstraintSetBuilder::new();
        let [left, right, ..] = inputs(&db, &builder);
        assert!(!left.node.is_terminal());
        assert!(!right.node.is_terminal());
        let admission = Admission::new(&db, &builder);
        let lifetime = CursorLifetime::default();
        let advances = Cell::new(0);
        let delivered = Cell::new(false);
        let runtime_error = Cell::new(None);
        let observer = |_: &TaskEndpoint<'_, '_>, boundary| {
            assert!(builder.storage.try_borrow_mut().is_ok());
            assert_eq!(lifetime.live.get(), 1);
            assert_eq!(lifetime.drops_started.get(), 0);
            assert_eq!(
                boundary,
                StructuralBoundary::AfterAdvanceReturned {
                    operation: StructuralOperation::Combine,
                    outer_complete: false,
                }
            );
            advances.set(advances.get() + 1);
            Ok(())
        };
        let outcome = expansion_probe::run(&db, allowance, || {
            let result = RegistryBuilder::new(&db, &admission)
                .and_then(RegistryBuilder::seal)
                .and_then(|registry| {
                    let db = &db;
                    let builder = &builder;
                    let lifetime = &lifetime;
                    let observer = &observer;
                    let delivered = &delivered;
                    registry.run(move |endpoint| async move {
                        let mut operations = RuntimeStructural::new(db, endpoint);
                        operations.cursor_lifetime = Some(lifetime);
                        operations.observer = Some(observer);
                        let result = operations.combine(builder, kind, left, right).await;
                        delivered.set(true);
                        result
                    })
                });
            runtime_error.set(result.as_ref().err().copied());
            result
        })
        .0;
        assert!(matches!(outcome, Err(Incomplete::Allowance)), "{outcome:?}");
        assert_eq!(
            runtime_error.get(),
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            ))
        );
        assert_eq!(advances.get(), 1);
        assert!(!delivered.get());
        assert_eq!(lifetime.live.get(), 0);
        assert_eq!(lifetime.drops_started.get(), 1);
        assert!(builder.storage.try_borrow_mut().is_ok());
        let events = admission.events.borrow();
        let root_poll = events
            .iter()
            .position(|work| *work == ExecutionWork::Poll)
            .expect("root poll");
        assert_eq!(&events[root_poll + 1..], trace.0.as_slice());
        drop(events);

        let retry_admission = Admission::new(&db, &builder);
        let retry_observation = BoundaryObservation::default();
        let actual = combine_run(
            &db,
            &builder,
            &retry_admission,
            kind,
            left,
            right,
            &retry_observation,
        )
        .expect("fresh allowance")
        .expect("fresh combination");
        let mut ordinary = left;
        let expected = match kind {
            ConstraintFoldKind::All => ordinary.intersect(&db, &builder, right),
            ConstraintFoldKind::Any => ordinary.union(&db, &builder, right),
        };
        assert_same(actual, expected);
        assert!(!actual.node.is_terminal());
        assert!(retry_observation.delivered.get());
        assert_eq!(retry_observation.retirements.get(), 1);
        assert!(builder.storage.try_borrow_mut().is_ok());
    }
}

#[test]
fn combination_keeps_structural_order_and_observes_graph_before_source() {
    let db = setup_db();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = ConstraintSetBuilder::new();
        let [left, right, ..] = inputs(&db, &builder);
        let initial_sources = builder.storage.borrow().source_orders.raw.len();
        let admission = Admission::new(&db, &builder);
        let observation = BoundaryObservation::default();
        let actual = combine_run(&db, &builder, &admission, kind, left, right, &observation)
            .expect("complete attempt")
            .expect("complete combination");
        let mut ordinary = left;
        let expected = match kind {
            ConstraintFoldKind::All => ordinary.intersect(&db, &builder, right),
            ConstraintFoldKind::Any => ordinary.union(&db, &builder, right),
        };
        assert_same(actual, expected);
        assert!(
            observation
                .snapshots
                .borrow()
                .iter()
                .any(|&(graph_complete, sources)| graph_complete && sources == initial_sources)
        );
        assert!(builder.storage.borrow().source_orders.raw.len() > initial_sources);

        // Recreate only the explicit input graphs; the expected cursor starts cold as well.
        let expected_builder = ConstraintSetBuilder::new();
        let [expected_left, expected_right, ..] = inputs(&db, &expected_builder);
        let mut cursor =
            ConstraintCombination::new(&expected_builder, kind, expected_left, expected_right);
        let mut trace = StructuralTrace::default();
        let mut advances = 0;
        loop {
            advances += 1;
            if cursor
                .advance_with(&mut trace)
                .expect("finite structural operation")
                .is_break()
            {
                break;
            }
        }
        let events = admission.events.borrow();
        let root_poll = events
            .iter()
            .position(|work| *work == ExecutionWork::Poll)
            .expect("root poll");
        assert_eq!(&events[root_poll + 1..], trace.0.as_slice());
        assert_eq!(admission.polls.get(), 1);
        assert_eq!(observation.advances_returned.get(), advances);
        assert_eq!(observation.completions_returned.get(), 1);
        assert_eq!(observation.retirements.get(), 1);
        assert!(observation.delivered.get());
        assert_eq!(
            admission
                .events
                .borrow()
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Task { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn every_combination_admission_can_refuse_and_retry_on_the_original_builder() {
    let db = setup_db();
    let env = db.program_environment();
    let prefix: Vec<_> = (0..64 * usize::BITS as usize)
        .map(|index| {
            BoundTypeVarInstance::synthetic(
                &db,
                &env,
                Name::new(format!("Support{index}")),
                TypeVarVariance::Invariant,
            )
        })
        .collect();
    for prefix in [&[][..], prefix.as_slice()] {
        let seeded = || {
            let builder = ConstraintSetBuilder::new();
            for variable in prefix {
                builder.storage.borrow_mut().intern_typevar(&db, *variable);
            }
            builder
        };
        for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
            let builder = seeded();
            let [left, right, ..] = inputs(&db, &builder);
            let admission = Admission::new(&db, &builder);
            let observation = BoundaryObservation::default();
            assert!(matches!(
                combine_run(&db, &builder, &admission, kind, left, right, &observation,),
                Ok(Ok(_))
            ));
            let events = admission.events.borrow();
            let root_poll = events
                .iter()
                .position(|work| *work == ExecutionWork::Poll)
                .expect("root poll");
            assert!(
                events[root_poll + 1..]
                    .iter()
                    .any(|work| matches!(work, ExecutionWork::Resource { .. }))
            );
            assert!(
                events[root_poll + 1..]
                    .iter()
                    .any(|work| matches!(work, ExecutionWork::Work { .. }))
            );
            if !prefix.is_empty() {
                assert!(
                    builder
                        .storage
                        .borrow()
                        .supports
                        .iter()
                        .any(|support| support.words().len() > 64)
                );
            }
            for refused in 0..events.len() {
                let builder = seeded();
                let [left, right, ..] = inputs(&db, &builder);
                let mut admission = Admission::new(&db, &builder);
                admission.refuse = Some(refused);
                let observation = BoundaryObservation::default();
                assert!(
                    matches!(
                        combine_run(&db, &builder, &admission, kind, left, right, &observation,),
                        Err(Incomplete::Allowance)
                    ),
                    "refusal at {refused}"
                );
                assert_eq!(
                    observation.runtime_error.get(),
                    Some(RunError::Refused(
                        salsa::attempt_probe::Incomplete::Allowance
                    ))
                );
                assert!(!observation.delivered.get());
                assert_eq!(observation.completions_returned.get(), 0);
                assert_eq!(observation.retirements.get(), 0);
                assert_eq!(*admission.events.borrow(), events[..=refused]);
                assert!(builder.storage.try_borrow_mut().is_ok());
                let retry = Admission::new(&db, &builder);
                let actual = combine_run(
                    &db,
                    &builder,
                    &retry,
                    kind,
                    left,
                    right,
                    &BoundaryObservation::default(),
                )
                .expect("retry attempt")
                .expect("retry operation");
                let mut expected = left;
                let expected = match kind {
                    ConstraintFoldKind::All => expected.intersect(&db, &builder, right),
                    ConstraintFoldKind::Any => expected.union(&db, &builder, right),
                };
                assert_same(actual, expected);
            }
        }
    }
}

#[test]
fn absorption_stops_before_producing_another_input() {
    let db = setup_db();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = ConstraintSetBuilder::new();
        let [a, b, c, d, ..] = inputs(&db, &builder);
        let opposite = c.negate(&db, &builder);
        let admission = Admission::new(&db, &builder);
        let produced = Cell::new(0);
        let (result, _) = expansion_probe::run(&db, usize::MAX, || {
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| {
                    let builder = &builder;
                    let produced = &produced;
                    let db = &db;
                    async move {
                        let operations = RuntimeStructural::new(db, endpoint);
                        let mut fold = ConstraintFold::new(builder, kind);
                        for next in [a, b, c, opposite, d] {
                            produced.set(produced.get() + 1);
                            if let ControlFlow::Break(result) =
                                operations.push(&mut fold, next).await?
                            {
                                return Ok(result);
                            }
                        }
                        operations.finish(&mut fold).await
                    }
                })
        });
        let actual = result.expect("complete attempt").expect("absorbed result");
        assert_eq!(produced.get(), 4);
        let mut expected = c;
        let expected = match kind {
            ConstraintFoldKind::All => expected.intersect(&db, &builder, opposite),
            ConstraintFoldKind::Any => expected.union(&db, &builder, opposite),
        };
        assert_same(actual, expected);
        assert_eq!(actual.node, kind.absorbing());
        assert_eq!(admission.polls.get(), 1);
        assert_eq!(
            admission
                .events
                .borrow()
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Task { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn completion_refusal_discards_an_already_accepted_fold() {
    let db = setup_db();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = ConstraintSetBuilder::new();
        let [next, ..] = inputs(&db, &builder);
        let admission = Admission::new(&db, &builder);
        let completed_at = Cell::new(None);
        let fold_observation = FoldObservation::default();
        let cursor_lifetime = CursorLifetime::default();
        let after_await = Cell::new(false);
        let observer = |_: &TaskEndpoint<'_, '_>, boundary| {
            assert!(builder.storage.try_borrow_mut().is_ok());
            if boundary
                == (StructuralBoundary::BeforeRetirementAcceptance {
                    operation: StructuralOperation::Push,
                })
            {
                assert_eq!(cursor_lifetime.live.get(), 0);
                assert_eq!(cursor_lifetime.drops_started.get(), 1);
                completed_at.set(Some(admission.events.borrow().len()));
                expansion_probe::refuse(&db, Incomplete::Interrupted);
            }
            Ok(())
        };
        let (result, _) = expansion_probe::run(&db, usize::MAX, || {
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| {
                    let builder = &builder;
                    let db = &db;
                    let observer = &observer;
                    let fold_observation = &fold_observation;
                    let cursor_lifetime = &cursor_lifetime;
                    let after_await = &after_await;
                    async move {
                        let mut operations = RuntimeStructural::new(db, endpoint);
                        operations.observer = Some(observer);
                        operations.cursor_lifetime = Some(cursor_lifetime);
                        let mut owner = ObservedFold::new(builder, kind, next, fold_observation);
                        let result = operations.push(&mut owner.fold, next).await;
                        after_await.set(true);
                        result.map(|_| ())
                    }
                })
        });
        assert!(matches!(result, Err(Incomplete::Interrupted)));
        assert!(fold_observation.accepted_at_drop.get());
        assert_eq!(fold_observation.drops.get(), 1);
        assert!(!fold_observation.live.get());
        assert!(!after_await.get());
        assert_eq!(
            completed_at.get(),
            Some(admission.events.borrow().len()),
            "retirement acceptance charges no work"
        );
        let retry = Admission::new(&db, &builder);
        let (result, _) = expansion_probe::run(&db, usize::MAX, || {
            RegistryBuilder::new(&db, &retry)?.seal()?.run(|endpoint| {
                let builder = &builder;
                let db = &db;
                async move {
                    let operations = RuntimeStructural::new(db, endpoint);
                    let mut fold = ConstraintFold::new(builder, kind);
                    assert!(operations.push(&mut fold, next).await?.is_continue());
                    operations.finish(&mut fold).await
                }
            })
        });
        assert_same(result.expect("fresh attempt").expect("fresh fold"), next);
    }
}

#[test]
fn capacity_mapping_preserves_the_first_operational_reason() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    for earlier in [None, Some(salsa::attempt_probe::Incomplete::Allowance)] {
        let admission = Admission::new(&db, &builder);
        let observed = Cell::new(None);
        let returned = Cell::new(None);
        let after_await = Cell::new(false);
        let (result, _) = expansion_probe::run(&db, usize::MAX, || {
            let result = RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| {
                    let observed = &observed;
                    let after_await = &after_await;
                    let db = &db;
                    async move {
                        let operations = RuntimeStructural::new(db, endpoint.clone());
                        endpoint
                            .local_call(|| {
                                if let Some(earlier) = earlier {
                                    salsa::attempt_probe::report_incomplete(db, earlier);
                                }
                                let error = operations.error(TddError::CapacityExhausted);
                                observed.set(Some(error));
                                Err::<(), _>(error)
                            })
                            .await;
                        after_await.set(true);
                        Ok(())
                    }
                });
            returned.set(result.as_ref().err().copied());
            result
        });
        let expected = if earlier.is_some() {
            Incomplete::Allowance
        } else {
            Incomplete::ConstraintCapacityExhausted
        };
        assert!(matches!(result, Err(reason) if reason == expected));
        assert_eq!(
            observed.get(),
            Some(RunError::Refused(
                earlier.unwrap_or(salsa::attempt_probe::Incomplete::Interrupted)
            ))
        );
        assert_eq!(returned.get(), observed.get());
        assert!(!after_await.get());
    }
}

#[derive(Default)]
struct FoldObservation {
    live: Cell<bool>,
    drops: Cell<usize>,
    accepted_at_drop: Cell<bool>,
}

struct ObservedFold<'a, 'db, 'c> {
    fold: ConstraintFold<'db, 'c>,
    expected: ConstraintSet<'db, 'c>,
    observation: &'a FoldObservation,
}

impl<'a, 'db, 'c> ObservedFold<'a, 'db, 'c> {
    fn new(
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        expected: ConstraintSet<'db, 'c>,
        observation: &'a FoldObservation,
    ) -> Self {
        observation.live.set(true);
        Self {
            fold: ConstraintFold::new(builder, kind),
            expected,
            observation,
        }
    }
}

impl Drop for ObservedFold<'_, '_, '_> {
    fn drop(&mut self) {
        self.observation.accepted_at_drop.set(
            self.fold.accumulator.as_slice()
                == [(self.expected.node, self.expected.source_order, 0)],
        );
        self.observation.live.set(false);
        self.observation.drops.set(self.observation.drops.get() + 1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectionPhase {
    Advance,
    Retirement,
}

#[derive(Clone, Copy, Debug)]
enum InjectedFailure {
    Refuse,
    Panic,
    Cancel,
}

#[derive(Default)]
struct InjectionObservation {
    fold: FoldObservation,
    cursor: CursorLifetime,
    child_drops: Cell<usize>,
    child_saw_fold: Cell<bool>,
    child_saw_cursors: Cell<usize>,
    child_saw_storage_released: Cell<bool>,
    child_factory_ran: Cell<bool>,
    after_await: Cell<bool>,
    boundaries_after_failure: Cell<usize>,
}

struct InjectAdmission {
    db: &'static TestDb,
    builder: &'static ConstraintSetBuilder<'static>,
    observation: &'static InjectionObservation,
    endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
    pending: RefCell<Option<Demand<()>>>,
    events: RefCell<Vec<ExecutionWork>>,
    armed: Cell<bool>,
    fired: Cell<bool>,
    injected_at: Cell<Option<usize>>,
    phase: InjectionPhase,
    failure: InjectedFailure,
}

impl InjectAdmission {
    fn inject(&self, endpoint: &TaskEndpoint<'static, 'static>) -> RunResult<()> {
        self.fired.set(true);
        let child = PendingChild {
            builder: self.builder,
            observation: self.observation,
        };
        let observation = self.observation;
        let demand = endpoint.demand(move || {
            observation.child_factory_ran.set(true);
            async move {
                let _child = child;
                Ok(())
            }
        })?;
        *self.pending.borrow_mut() = Some(demand);
        self.injected_at.set(Some(self.events.borrow().len()));
        match self.failure {
            InjectedFailure::Refuse => Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            )),
            InjectedFailure::Panic => panic!("pending structural task panic"),
            InjectedFailure::Cancel => {
                self.db.cancellation_token().cancel();
                Ok(())
            }
        }
    }
}

impl ExecutionAdmission for InjectAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.events.borrow_mut().push(work);
        if self.phase == InjectionPhase::Advance
            && self.armed.get()
            && !self.fired.get()
            && matches!(work, ExecutionWork::Work { .. })
            && self.builder.storage.try_borrow_mut().is_err()
        {
            let endpoint = self
                .endpoint
                .borrow()
                .clone()
                .ok_or(RunError::Contract("structural injection has no endpoint"))?;
            self.inject(&endpoint)?;
        }
        Ok(())
    }
}

struct PendingChild {
    builder: &'static ConstraintSetBuilder<'static>,
    observation: &'static InjectionObservation,
}

impl Drop for PendingChild {
    fn drop(&mut self) {
        self.observation
            .child_drops
            .set(self.observation.child_drops.get() + 1);
        self.observation
            .child_saw_fold
            .set(self.observation.fold.live.get());
        self.observation
            .child_saw_cursors
            .set(self.observation.cursor.live.get());
        self.observation
            .child_saw_storage_released
            .set(self.builder.storage.try_borrow_mut().is_ok());
    }
}

struct ResetInjection(&'static InjectAdmission);

impl Drop for ResetInjection {
    fn drop(&mut self) {
        self.0.armed.set(false);
        self.0.pending.borrow_mut().take();
        self.0.endpoint.borrow_mut().take();
    }
}

fn injected_failure(phase: InjectionPhase, failure: InjectedFailure) {
    // Only this fixed fault matrix uses static fixtures. The root-local fold still has a short
    // mutable borrow, while the cleared slots let admission callbacks queue real descendants.
    let db: &'static TestDb = Box::leak(Box::new(setup_db()));
    let builder: &'static ConstraintSetBuilder<'static> =
        Box::leak(Box::new(ConstraintSetBuilder::new()));
    let observation: &'static InjectionObservation = Box::leak(Box::default());
    let admission: &'static InjectAdmission = Box::leak(Box::new(InjectAdmission {
        db,
        builder,
        observation,
        endpoint: RefCell::new(None),
        pending: RefCell::new(None),
        events: RefCell::new(Vec::new()),
        armed: Cell::new(false),
        fired: Cell::new(false),
        injected_at: Cell::new(None),
        phase,
        failure,
    }));
    let observer: &'static _ = Box::leak(Box::new(
        move |endpoint: &TaskEndpoint<'static, 'static>, boundary| {
            if admission.fired.get() {
                observation
                    .boundaries_after_failure
                    .set(observation.boundaries_after_failure.get() + 1);
            }
            assert!(builder.storage.try_borrow_mut().is_ok());
            if phase == InjectionPhase::Retirement
                && admission.armed.get()
                && boundary
                    == (StructuralBoundary::BeforeRetirementAcceptance {
                        operation: StructuralOperation::Push,
                    })
            {
                assert_eq!(observation.cursor.live.get(), 0);
                return admission.inject(endpoint);
            }
            Ok(())
        },
    ));
    let reset = ResetInjection(admission);
    let [left, right, ..] = inputs(db, builder);
    let returned = Cell::new(None);
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        expansion_probe::run(db, usize::MAX, || {
            let result = RegistryBuilder::<'static, 'static>::new(db, admission)?
                .seal()?
                .run(move |endpoint| {
                    *admission.endpoint.borrow_mut() = Some(endpoint.clone());
                    async move {
                        let mut operations = RuntimeStructural::new(db, endpoint);
                        operations.observer = Some(observer);
                        operations.cursor_lifetime = Some(&observation.cursor);
                        let mut owner = ObservedFold::new(
                            builder,
                            ConstraintFoldKind::All,
                            left,
                            &observation.fold,
                        );
                        if phase == InjectionPhase::Advance {
                            assert!(operations.push(&mut owner.fold, left).await?.is_continue());
                            admission.armed.set(true);
                            let _ = operations.push(&mut owner.fold, right).await?;
                        } else {
                            admission.armed.set(true);
                            let _ = operations.push(&mut owner.fold, left).await?;
                        }
                        observation.after_await.set(true);
                        Ok(())
                    }
                });
            returned.set(result.as_ref().err().copied());
            result
        })
    }));
    assert!(admission.fired.get());
    match failure {
        InjectedFailure::Refuse => {
            let (result, _) = unwind.expect("expected refusal does not unwind");
            assert!(matches!(result, Err(Incomplete::Allowance)));
            assert_eq!(
                returned.get(),
                Some(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance
                ))
            );
        }
        InjectedFailure::Panic => {
            let payload = unwind.expect_err("structural callback must unwind");
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"pending structural task panic")
            );
        }
        InjectedFailure::Cancel => {
            let payload = unwind.expect_err("local cancellation must unwind");
            assert!(matches!(
                payload.downcast_ref::<salsa::Cancelled>(),
                Some(salsa::Cancelled::Local)
            ));
        }
    }
    assert!(!observation.child_factory_ran.get());
    assert_eq!(observation.child_drops.get(), 1);
    assert!(observation.child_saw_fold.get());
    assert!(observation.child_saw_storage_released.get());
    assert_eq!(
        observation.child_saw_cursors.get(),
        usize::from(phase == InjectionPhase::Advance)
    );
    assert_eq!(observation.cursor.live.get(), 0);
    assert_eq!(
        observation.cursor.drops_started.get(),
        if phase == InjectionPhase::Advance {
            2
        } else {
            1
        }
    );
    assert!(observation.fold.accepted_at_drop.get());
    assert_eq!(observation.fold.drops.get(), 1);
    assert!(!observation.fold.live.get());
    assert!(!observation.after_await.get());
    assert_eq!(observation.boundaries_after_failure.get(), 0);
    let events = admission.events.borrow();
    assert_eq!(admission.injected_at.get(), Some(events.len()));
    assert_eq!(
        events
            .iter()
            .filter(|work| matches!(work, ExecutionWork::Task { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|work| **work == ExecutionWork::Poll)
            .count(),
        1
    );
    assert!(builder.storage.try_borrow_mut().is_ok());
    drop(reset);
    assert!(admission.endpoint.borrow().is_none());
    assert!(admission.pending.borrow().is_none());
    if matches!(failure, InjectedFailure::Cancel) {
        return;
    }

    let input_count = if phase == InjectionPhase::Advance {
        2
    } else {
        1
    };
    let retry = Admission::new(db, builder);
    let (result, _) = expansion_probe::run(db, usize::MAX, || {
        RegistryBuilder::new(db, &retry)?
            .seal()?
            .run(|endpoint| async move {
                let operations = RuntimeStructural::new(db, endpoint);
                let mut fold = ConstraintFold::new(builder, ConstraintFoldKind::All);
                for next in [left, right].into_iter().take(input_count) {
                    assert!(operations.push(&mut fold, next).await?.is_continue());
                }
                operations.finish(&mut fold).await
            })
    });
    let actual = result
        .expect("fresh attempt after cleanup")
        .expect("fresh fold");
    assert_eq!(retry.polls.get(), 1);
    assert_eq!(
        retry
            .events
            .borrow()
            .iter()
            .filter(|work| matches!(work, ExecutionWork::Task { .. }))
            .count(),
        1
    );
    let mut ordinary = ConstraintFold::new(builder, ConstraintFoldKind::All);
    for next in [left, right].into_iter().take(input_count) {
        assert!(ordinary.push(next).is_continue());
    }
    assert_same(actual, ordinary.finish());
}

#[test]
fn failed_local_advance_drops_the_queued_child_before_the_cursor_and_fold() {
    for failure in [
        InjectedFailure::Refuse,
        InjectedFailure::Panic,
        InjectedFailure::Cancel,
    ] {
        injected_failure(InjectionPhase::Advance, failure);
    }
}

#[test]
fn failed_retirement_keeps_the_accepted_fold_until_the_queued_child_drops() {
    injected_failure(InjectionPhase::Retirement, InjectedFailure::Refuse);
}

mod local_drain_benchmark;

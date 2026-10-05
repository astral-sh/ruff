use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;

use ruff_db::files::system_path_to_file;
use rustc_hash::FxHashMap;
use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
use salsa::execution_probe::{
    Demand, ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};
use ty_python_core::ProgramFile;

use super::{
    CycleCacheLayout, TransformationLayout, TypeTransformationStorage, progress_cache_growth,
};
use super::{
    InlineTypeTransformationControl, TypeIdentity, TypeTransformationControl,
    TypeTransformationGrowth, TypeTransformationScope, TypeTransformationWork, TypeTransformer,
    TypeTransformerVisit,
};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::control::{GrowthPlan, TddError, hash_slots};
use crate::types::{MaterializationKind, Type, todo_type};

struct Tag;
type Transformer<'db> = TypeTransformer<'db, Tag>;

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn pending<'a, 'db>(
    transformer: &'a Transformer<'db>,
    db: &'db dyn Db,
    ty: Type<'db>,
) -> anyhow::Result<TypeTransformationScope<'a, 'db, Tag>> {
    match infallible(transformer.begin_visit_with(db, ty, &InlineTypeTransformationControl)) {
        TypeTransformerVisit::Pending(scope) => Ok(scope),
        TypeTransformerVisit::Ready(ty) => anyhow::bail!("unexpected ready transformation: {ty:?}"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Work(TypeTransformationWork),
    Identity,
    Resource(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

struct Recording {
    events: RefCell<Vec<Event>>,
    refuse_at: Option<usize>,
    remaining: Cell<Option<usize>>,
}

impl Recording {
    fn new(refuse_at: Option<usize>) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            refuse_at,
            remaining: Cell::new(None),
        }
    }

    fn record(&self, event: Event) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        let units = match event {
            Event::Work(work) => work.quote().ok_or(Refused(index))?.work_units,
            Event::Identity => 1,
            Event::Resource(_) => 0,
        };
        if let Some(remaining) = self.remaining.get() {
            let remaining = remaining.checked_sub(units).ok_or(Refused(index))?;
            self.remaining.set(Some(remaining));
        }
        if self.refuse_at == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }
}

impl TypeTransformationControl for Recording {
    type Error = Refused;

    fn checkpoint(&self, work: TypeTransformationWork) -> Result<(), Refused> {
        self.record(Event::Work(work))?;
        let quote = work.quote().ok_or(Refused(self.events.borrow().len()))?;
        if quote.requested_payload_bytes != 0 {
            self.record(Event::Resource(quote.requested_payload_bytes))?;
        }
        Ok(())
    }

    fn prepare_growth(
        &self,
        request: TypeTransformationGrowth,
    ) -> Result<Option<GrowthPlan>, Refused> {
        let plan = request
            .checked_plan()
            .ok_or(Refused(self.events.borrow().len()))?;
        self.checkpoint(request.work(plan))?;
        Ok(Some(plan))
    }

    fn identity<'db>(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<TypeIdentity<'db>, Refused> {
        self.record(Event::Identity)?;
        Ok(ty.to_type_identity(db))
    }
}

#[test]
fn unfinished_scopes_abort_and_only_successful_finish_caches() -> anyhow::Result<()> {
    let db = setup_db();
    let transformer = Transformer::default();
    let first = Type::int_literal(1);
    let second = Type::int_literal(2);
    let outer = pending(&transformer, &db, first)?;
    let inner = pending(&transformer, &db, second)?;
    assert_eq!(transformer.seen.borrow().len(), 2);
    drop(inner);
    assert_eq!(transformer.seen.borrow().len(), 1);
    assert_eq!(transformer.cache.borrow().len(), 0);
    let inner = pending(&transformer, &db, second)?;
    assert_eq!(
        infallible(inner.finish_with(Type::Never, &InlineTypeTransformationControl)),
        Type::Never
    );
    drop(outer);
    assert!(transformer.seen.borrow().is_empty());
    assert_eq!(transformer.cache.borrow().get(&first), None);
    assert_eq!(transformer.cache.borrow().get(&second), Some(&Type::Never));
    drop(pending(&transformer, &db, first)?);
    assert!(transformer.seen.borrow().is_empty());
    Ok(())
}

#[test]
fn completed_cache_lookup_precedes_identity_and_active_checks() -> anyhow::Result<()> {
    let db = setup_db();
    let transformer = Transformer::default();
    let ty = Type::int_literal(1);
    infallible(
        pending(&transformer, &db, ty)?.finish_with(Type::Never, &InlineTypeTransformationControl),
    );
    let control = Recording::new(Some(1));
    assert!(matches!(
        transformer.begin_visit_with(&db, ty, &control),
        Ok(TypeTransformerVisit::Ready(Type::Never))
    ));
    assert_eq!(
        *control.events.borrow(),
        [Event::Work(TypeTransformationWork::CacheLookup { len: 1 })]
    );
    assert!(transformer.seen.borrow().is_empty());
    Ok(())
}

#[test]
fn every_begin_refusal_leaves_existing_scopes_and_storage_unchanged() -> anyhow::Result<()> {
    let db = setup_db();
    for depth in [0, 3, 6, 12] {
        let transformer = Transformer::default();
        let mut parents = Vec::new();
        for index in 0..depth {
            parents.push(pending(
                &transformer,
                &db,
                Type::int_literal(i64::try_from(index)?),
            )?);
        }
        let ty = Type::int_literal(100);
        let control = Recording::new(None);
        let visit = transformer.begin_visit_with(&db, ty, &control);
        let Ok(TypeTransformerVisit::Pending(scope)) = visit else {
            anyhow::bail!("expected a new visit");
        };
        drop(scope);
        let expected = control.events.into_inner();
        assert_eq!(
            expected[0],
            Event::Work(TypeTransformationWork::CacheLookup { len: 0 })
        );
        assert_eq!(expected[1], Event::Identity);
        assert_eq!(
            expected
                .iter()
                .filter(|event| **event == Event::Work(TypeTransformationWork::AncestorComparison))
                .count(),
            depth
        );
        assert!(
            expected.iter().any(|event| matches!(event, Event::Work(TypeTransformationWork::ActiveStorage { len, .. }) if *len == depth))
        );
        while let Some(scope) = parents.pop() {
            drop(scope);
        }

        for index in 0..expected.len() {
            let transformer = Transformer::default();
            let mut parents = Vec::new();
            for index in 0..depth {
                parents.push(pending(
                    &transformer,
                    &db,
                    Type::int_literal(i64::try_from(index)?),
                )?);
            }
            let before_capacity = transformer.seen.borrow().capacity();
            let control = Recording::new(Some(index));
            assert!(
                matches!(transformer.begin_visit_with(&db, ty, &control), Err(Refused(at)) if at == index)
            );
            assert_eq!(*control.events.borrow(), expected[..=index]);
            assert_eq!(transformer.seen.borrow().len(), depth);
            assert_eq!(transformer.seen.borrow().capacity(), before_capacity);
            assert_eq!(transformer.cache.borrow().len(), 0);
            let retry_control = Recording::new(None);
            let Ok(TypeTransformerVisit::Pending(retry)) =
                transformer.begin_visit_with(&db, ty, &retry_control)
            else {
                anyhow::bail!("same-owner controlled retry did not start")
            };
            assert_eq!(
                retry.finish_with(Type::Never, &retry_control),
                Ok(Type::Never)
            );
            while let Some(scope) = parents.pop() {
                drop(scope);
            }
            assert!(transformer.seen.borrow().is_empty());
            assert_eq!(transformer.cache.borrow().get(&ty), Some(&Type::Never));
        }
    }
    Ok(())
}

#[test]
fn finish_refusal_precedes_inline_spill_hash_growth_and_publication() -> anyhow::Result<()> {
    let db = setup_db();
    let mut saw_inline_spill = false;
    let mut saw_hash_growth = false;
    for completed in 0..9 {
        let transformer = Transformer::default();
        for index in 0..completed {
            infallible(
                pending(&transformer, &db, Type::int_literal(i64::try_from(index)?))?
                    .finish_with(Type::Never, &InlineTypeTransformationControl),
            );
        }
        let ty = Type::int_literal(100);
        let scope = pending(&transformer, &db, ty)?;
        let capacity = transformer.cache.borrow().capacity();
        saw_inline_spill |= completed == 2;
        saw_hash_growth |= completed > 2 && completed == capacity;
        let control = Recording::new(Some(0));
        assert_eq!(scope.finish_with(Type::Never, &control), Err(Refused(0)));
        assert_eq!(
            *control.events.borrow(),
            [Event::Work(TypeTransformationWork::CacheStorage {
                len: completed,
                capacity
            })]
        );
        assert!(transformer.seen.borrow().is_empty());
        assert_eq!(transformer.cache.borrow().len(), completed);
        assert_eq!(transformer.cache.borrow().capacity(), capacity);
        assert_eq!(transformer.cache.borrow().get(&ty), None);
        infallible(
            pending(&transformer, &db, ty)?
                .finish_with(Type::unknown(), &InlineTypeTransformationControl),
        );
        assert_eq!(transformer.cache.borrow().get(&ty), Some(&Type::unknown()));
    }
    assert!(saw_inline_spill && saw_hash_growth);
    Ok(())
}

#[test]
fn borrowed_finish_refusal_retains_the_active_scope_until_drop() -> anyhow::Result<()> {
    let db = setup_db();
    let mut saw_inline_spill = false;
    let mut saw_hash_growth = false;
    for completed in 0..9 {
        let transformer = Transformer::default();
        for index in 0..completed {
            infallible(
                pending(&transformer, &db, Type::int_literal(i64::try_from(index)?))?
                    .finish_with(Type::Never, &InlineTypeTransformationControl),
            );
        }
        let ty = Type::int_literal(100);
        let mut scope = pending(&transformer, &db, ty)?;
        let capacity = transformer.cache.borrow().capacity();
        saw_inline_spill |= completed == 2;
        saw_hash_growth |= completed > 2 && completed == capacity;
        let control = Recording::new(Some(0));
        assert_eq!(
            scope.finish_in_place_with(Type::Never, &control),
            Err(Refused(0))
        );
        assert_eq!(
            *control.events.borrow(),
            [Event::Work(TypeTransformationWork::CacheStorage {
                len: completed,
                capacity,
            })]
        );
        assert_eq!(transformer.seen.borrow().len(), 1);
        assert_eq!(transformer.seen.borrow()[0].ty, ty);
        assert_eq!(transformer.cache.borrow().len(), completed);
        assert_eq!(transformer.cache.borrow().capacity(), capacity);
        assert_eq!(transformer.cache.borrow().get(&ty), None);
        drop(scope);
        assert!(transformer.seen.borrow().is_empty());

        let mut retry = pending(&transformer, &db, ty)?;
        assert_eq!(
            infallible(
                retry.finish_in_place_with(Type::unknown(), &InlineTypeTransformationControl)
            ),
            Type::unknown()
        );
        assert!(transformer.seen.borrow().is_empty());
        assert_eq!(transformer.cache.borrow().get(&ty), Some(&Type::unknown()));
        drop(retry);
        assert!(transformer.seen.borrow().is_empty());
    }
    assert!(saw_inline_spill && saw_hash_growth);
    Ok(())
}

#[test]
fn prepared_finish_retains_scope_prefix_and_reserved_capacity() -> anyhow::Result<()> {
    let db = setup_db();
    let mut saw_inline_spill = false;
    let mut saw_hash_growth = false;
    for completed in 0..9 {
        for abort in [false, true] {
            let transformer = Transformer::default();
            for index in 0..completed {
                infallible(
                    pending(&transformer, &db, Type::int_literal(i64::try_from(index)?))?
                        .finish_with(Type::Never, &InlineTypeTransformationControl),
                );
            }
            let parent = pending(&transformer, &db, Type::int_literal(101))?;
            let ty = Type::int_literal(100);
            let mut scope = pending(&transformer, &db, ty)?;
            let before = transformer.cache.borrow().layout();
            saw_inline_spill |= before.variant == 2;
            saw_hash_growth |= before.variant == 3 && before.needs_growth();
            let control = Recording::new(None);
            let Ok(prepared) = scope.prepare_finish_with(Type::unknown(), &control) else {
                anyhow::bail!("unlimited preparation was refused");
            };
            assert!(transformer.seen.try_borrow_mut().is_ok());
            assert!(transformer.cache.try_borrow_mut().is_ok());
            assert_eq!(transformer.seen.borrow().len(), 2);
            assert_eq!(transformer.seen.borrow()[1].ty, ty);
            assert_eq!(transformer.cache.borrow().get(&ty), None);
            assert_eq!(transformer.cache.borrow().len(), completed);
            for index in 0..completed {
                assert_eq!(
                    transformer
                        .cache
                        .borrow()
                        .get(&Type::int_literal(i64::try_from(index)?)),
                    Some(&Type::Never)
                );
            }
            let reserved = transformer.cache.borrow().layout();
            let reserved_capacity = transformer.cache.borrow().capacity();
            assert!(!reserved.needs_growth());
            assert!(reserved.capacity >= before.capacity);
            if before.needs_growth() {
                assert!(reserved.capacity > before.capacity);
            }
            let events = control.events.borrow().clone();
            if abort {
                drop(prepared);
                assert_eq!(scope.ty, Some(ty));
                assert_eq!(transformer.seen.borrow().len(), 2);
                drop(scope);
                assert_eq!(transformer.cache.borrow().layout(), reserved);
                assert_eq!(transformer.cache.borrow().get(&ty), None);
            } else {
                let Ok(result) = prepared.try_commit() else {
                    anyhow::bail!("fresh preparation was rejected");
                };
                assert_eq!(result, Type::unknown());
                assert_eq!(scope.ty, None);
                assert_eq!(transformer.cache.borrow().capacity(), reserved_capacity);
                assert_eq!(transformer.cache.borrow().len(), completed + 1);
                assert_eq!(transformer.cache.borrow().get(&ty), Some(&result));
                assert_eq!(*control.events.borrow(), events);
                assert!(matches!(
                    infallible(transformer.begin_visit_with(&db, ty, &InlineTypeTransformationControl)),
                    TypeTransformerVisit::Ready(value) if value == result
                ));
                // Repeated finishing returns its argument without overwriting the first result.
                assert_eq!(
                    infallible(
                        scope.finish_in_place_with(Type::Never, &InlineTypeTransformationControl)
                    ),
                    Type::Never
                );
                let repeated = infallible(
                    scope.prepare_finish_with(Type::Never, &InlineTypeTransformationControl),
                );
                assert!(matches!(repeated.try_commit(), Ok(Type::Never)));
                assert_eq!(transformer.cache.borrow().get(&ty), Some(&result));
                drop(scope);
            }
            assert_eq!(*control.events.borrow(), events);
            for index in 0..completed {
                assert_eq!(
                    transformer
                        .cache
                        .borrow()
                        .get(&Type::int_literal(i64::try_from(index)?)),
                    Some(&Type::Never)
                );
            }
            assert_eq!(transformer.seen.borrow().len(), 1);
            assert_eq!(transformer.seen.borrow()[0].ty, Type::int_literal(101));
            drop(parent);
            assert!(transformer.seen.borrow().is_empty());
        }
    }
    assert!(saw_inline_spill && saw_hash_growth);
    Ok(())
}

#[test]
fn deferred_finish_requires_reserved_space_but_ordinary_finish_keeps_native_growth()
-> anyhow::Result<()> {
    let db = setup_db();
    let mut saw_inline_spill = false;
    let mut saw_hash_growth = false;
    for completed in 0..9 {
        for consume in [false, true] {
            let transformer = Transformer::default();
            for index in 0..completed {
                infallible(
                    pending(&transformer, &db, Type::int_literal(i64::try_from(index)?))?
                        .finish_with(Type::Never, &InlineTypeTransformationControl),
                );
            }
            let ty = Type::int_literal(100);
            let mut scope = pending(&transformer, &db, ty)?;
            let before = transformer.cache.borrow().layout();
            if before.needs_growth() {
                saw_inline_spill |= before.variant == 2;
                saw_hash_growth |= before.variant == 3;
                let prepared = infallible(
                    scope.prepare_finish_with(Type::unknown(), &InlineTypeTransformationControl),
                );
                let Err(rejected) = prepared.try_commit() else {
                    anyhow::bail!("deferred commit accepted an unreserved allocation");
                };
                assert_eq!(transformer.cache.borrow().layout(), before);
                assert_eq!(transformer.seen.borrow().len(), 1);
                assert_eq!(transformer.cache.borrow().get(&ty), None);
                drop(rejected);
            }
            let result = if consume {
                infallible(scope.finish_with(Type::unknown(), &InlineTypeTransformationControl))
            } else {
                infallible(
                    scope.finish_in_place_with(Type::unknown(), &InlineTypeTransformationControl),
                )
            };
            assert_eq!(result, Type::unknown());
            assert_eq!(transformer.cache.borrow().len(), completed + 1);
            assert_eq!(transformer.cache.borrow().get(&ty), Some(&result));
            assert!(transformer.seen.borrow().is_empty());
            if before.needs_growth() {
                assert!(transformer.cache.borrow().capacity() > before.capacity);
            }
            assert!(matches!(
                infallible(transformer.begin_visit_with(&db, ty, &InlineTypeTransformationControl)),
                TypeTransformerVisit::Ready(value) if value == result
            ));
        }
    }
    assert!(saw_inline_spill && saw_hash_growth);
    Ok(())
}

#[test]
fn prepared_finish_rejects_a_live_nested_scope_without_consuming_its_owner() -> anyhow::Result<()> {
    let db = setup_db();
    let transformer = Transformer::default();
    let ty = Type::int_literal(100);
    let mut scope = pending(&transformer, &db, ty)?;
    let prepared =
        infallible(scope.prepare_finish_with(Type::unknown(), &InlineTypeTransformationControl));
    let child = pending(&transformer, &db, Type::int_literal(101))?;
    let Err(rejected) = prepared.try_commit() else {
        anyhow::bail!("a live nested scope must invalidate preparation");
    };
    assert_eq!(rejected.result, Type::unknown());
    assert_eq!(rejected.scope.ty, Some(ty));
    assert_eq!(transformer.seen.borrow().len(), 2);
    assert_eq!(transformer.cache.borrow().get(&ty), None);
    drop(child);
    drop(rejected);
    assert_eq!(scope.ty, Some(ty));
    drop(scope);
    assert!(transformer.seen.borrow().is_empty());
    assert_eq!(transformer.cache.borrow().len(), 0);
    Ok(())
}

#[test]
fn prepared_finish_rejects_a_completed_child_but_accepts_an_aborted_child() -> anyhow::Result<()> {
    let db = setup_db();
    for complete_child in [false, true] {
        let transformer = Transformer::default();
        let ty = Type::int_literal(100);
        let child_ty = Type::int_literal(101);
        let mut scope = pending(&transformer, &db, ty)?;
        let prepared = infallible(
            scope.prepare_finish_with(Type::unknown(), &InlineTypeTransformationControl),
        );
        let child = pending(&transformer, &db, child_ty)?;
        if complete_child {
            infallible(child.finish_with(Type::Never, &InlineTypeTransformationControl));
            let Err(rejected) = prepared.try_commit() else {
                anyhow::bail!("a completed child must invalidate preparation");
            };
            assert_eq!(rejected.result, Type::unknown());
            assert_eq!(rejected.scope.ty, Some(ty));
            assert_eq!(transformer.seen.borrow().len(), 1);
            assert_eq!(transformer.cache.borrow().get(&ty), None);
            assert_eq!(
                transformer.cache.borrow().get(&child_ty),
                Some(&Type::Never)
            );
            drop(rejected);
            assert_eq!(scope.ty, Some(ty));
            drop(scope);
            assert_eq!(transformer.cache.borrow().len(), 1);
            assert_eq!(
                transformer.cache.borrow().get(&child_ty),
                Some(&Type::Never)
            );
        } else {
            drop(child);
            assert!(matches!(prepared.try_commit(), Ok(result) if result == Type::unknown()));
            drop(scope);
            assert_eq!(transformer.cache.borrow().len(), 1);
            assert_eq!(transformer.cache.borrow().get(&ty), Some(&Type::unknown()));
            assert_eq!(transformer.cache.borrow().get(&child_ty), None);
        }
        assert!(transformer.seen.borrow().is_empty());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinishPoint {
    InitialCharge,
    CacheStorage,
    Growth,
    RehashKey,
    Resource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinishFailure {
    Refusal,
    InjectedNativePanic,
}

#[derive(Default)]
struct FinishJournal {
    pending: RefCell<Option<Demand<()>>>,
    queued: Cell<usize>,
    child_factory_ran: Cell<bool>,
    owner_live: Cell<bool>,
    after_await: Cell<bool>,
    inner_error: Cell<Option<RunError>>,
    initial_charges: Cell<usize>,
    cache_work: Cell<Option<TypeTransformationWork>>,
    fault_work: Cell<Option<ExecutionWork>>,
    drops: RefCell<Vec<&'static str>>,
    panic_identity: Arc<()>,
}

struct InjectedFinishPanic(Arc<()>);

struct FinishAdmission<'a> {
    journal: &'a FinishJournal,
    failure: FinishFailure,
    armed: Cell<bool>,
    fired: Cell<usize>,
}

impl ExecutionAdmission for FinishAdmission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.armed.replace(false) {
            self.fired.set(self.fired.get() + 1);
            self.journal.fault_work.set(Some(work));
            match self.failure {
                FinishFailure::Refusal => Err(RunError::Refused(Incomplete::Interrupted)),
                // This payload is injected by the harness at the admission boundary.
                FinishFailure::InjectedNativePanic => {
                    panic_any(InjectedFinishPanic(self.journal.panic_identity.clone()))
                }
            }
        } else {
            Ok(())
        }
    }
}

struct FinishChild<'a, 'db> {
    transformer: &'a Transformer<'db>,
    ty: Type<'db>,
    journal: &'a FinishJournal,
}

impl Drop for FinishChild<'_, '_> {
    fn drop(&mut self) {
        assert!(self.journal.owner_live.get());
        let active = self.transformer.seen.borrow();
        assert_eq!(
            active.len(),
            1,
            "the child must drop before the actual scope"
        );
        assert_eq!(active[0].ty, self.ty);
        assert_eq!(self.transformer.cache.borrow().get(&self.ty), None);
        self.journal.drops.borrow_mut().push("child");
    }
}

struct FinishOwner<'a, 'db> {
    transformer: &'a Transformer<'db>,
    ty: Type<'db>,
    journal: &'a FinishJournal,
}

impl Drop for FinishOwner<'_, '_> {
    fn drop(&mut self) {
        assert!(self.transformer.seen.borrow().is_empty());
        assert_eq!(self.transformer.cache.borrow().get(&self.ty), None);
        assert!(self.journal.owner_live.replace(false));
        self.journal.drops.borrow_mut().push("owner");
    }
}

struct EndpointFinishControl<'call, 'run, 'db: 'run> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    transformer: &'run Transformer<'db>,
    ty: Type<'db>,
    admission: &'run FinishAdmission<'run>,
    fault: Option<FinishPoint>,
}

impl EndpointFinishControl<'_, '_, '_> {
    fn admit(&self, point: FinishPoint, work: ExecutionWork) -> RunResult<()> {
        if self.fault == Some(point) {
            let child = FinishChild {
                transformer: self.transformer,
                ty: self.ty,
                journal: self.admission.journal,
            };
            let reply = self.endpoint.demand(move || {
                child.journal.child_factory_ran.set(true);
                async move {
                    let _child = child;
                    std::future::pending::<RunResult<()>>().await
                }
            })?;
            assert!(
                self.admission
                    .journal
                    .pending
                    .borrow_mut()
                    .replace(reply)
                    .is_none()
            );
            self.admission
                .journal
                .queued
                .set(self.admission.journal.queued.get() + 1);
            self.admission.armed.set(true);
        }
        self.endpoint.admit(work)
    }

    fn charge(&self, point: FinishPoint, units: usize) -> RunResult<()> {
        self.admit(point, ExecutionWork::Work { units })
    }

    fn initial_charge(&self) -> RunResult<()> {
        let count = &self.admission.journal.initial_charges;
        count.set(count.get() + 1);
        self.charge(FinishPoint::InitialCharge, 1)
    }
}

impl TypeTransformationControl for EndpointFinishControl<'_, '_, '_> {
    type Error = RunError;

    fn checkpoint(&self, work: TypeTransformationWork) -> RunResult<()> {
        let point = match work {
            TypeTransformationWork::CacheStorage { .. } => {
                self.admission.journal.cache_work.set(Some(work));
                FinishPoint::CacheStorage
            }
            TypeTransformationWork::Grow { .. } => FinishPoint::Growth,
            TypeTransformationWork::RehashKey { .. } => FinishPoint::RehashKey,
            _ => {
                return Err(RunError::Contract(
                    "finish requested unrelated transformation work",
                ));
            }
        };
        let quote = work
            .quote()
            .ok_or(RunError::Refused(Incomplete::Allowance))?;
        self.charge(point, quote.work_units)?;
        if quote.requested_payload_bytes != 0 {
            self.admit(
                FinishPoint::Resource,
                ExecutionWork::Resource {
                    requested_bytes: quote.requested_payload_bytes,
                },
            )?;
        }
        Ok(())
    }

    fn prepare_growth(&self, request: TypeTransformationGrowth) -> RunResult<Option<GrowthPlan>> {
        let plan = request
            .checked_plan()
            .ok_or(RunError::Refused(Incomplete::Allowance))?;
        self.checkpoint(request.work(plan))?;
        Ok(Some(plan))
    }

    fn identity<'db>(&self, _db: &'db dyn Db, _ty: Type<'db>) -> RunResult<TypeIdentity<'db>> {
        Err(RunError::Contract("finish requested an identity"))
    }
}

async fn finish_on_endpoint<'db>(
    mut scope: TypeTransformationScope<'_, 'db, Tag>,
    control: &EndpointFinishControl<'_, '_, 'db>,
    result: Type<'db>,
) -> Type<'db> {
    let prepared = control
        .endpoint
        .local_call(|| {
            control.initial_charge()?;
            scope.prepare_finish_with(result, control)
        })
        .await;
    match prepared.try_commit() {
        Ok(result) => result,
        Err(rejected) => {
            let result = control
                .endpoint
                .local_call(|| {
                    let _held = &rejected;
                    Err::<Type<'db>, _>(RunError::Contract(
                        "transformation finish preparation became stale",
                    ))
                })
                .await;
            drop(rejected);
            result
        }
    }
}

fn endpoint_finish_retains_scope(failure: FinishFailure) -> anyhow::Result<()> {
    let db = setup_db();
    let mut saw_inline_spill = false;
    let mut saw_hash_growth = false;
    for point in [
        FinishPoint::InitialCharge,
        FinishPoint::CacheStorage,
        FinishPoint::Growth,
        FinishPoint::RehashKey,
        FinishPoint::Resource,
    ] {
        for completed in 0..9 {
            let transformer = Transformer::default();
            for index in 0..completed {
                infallible(
                    pending(&transformer, &db, Type::int_literal(i64::try_from(index)?))?
                        .finish_with(Type::Never, &InlineTypeTransformationControl),
                );
            }
            let ty = Type::int_literal(100);
            let capacity = transformer.cache.borrow().capacity();
            let growth = TypeTransformationGrowth {
                layout: TransformationLayout::Cache(transformer.cache.borrow().layout()),
            }
            .checked_plan();
            if matches!(
                point,
                FinishPoint::Growth | FinishPoint::RehashKey | FinishPoint::Resource
            ) && growth.is_none()
            {
                continue;
            }
            saw_inline_spill |= point == FinishPoint::CacheStorage && completed == 2;
            saw_hash_growth |=
                point == FinishPoint::CacheStorage && completed > 2 && completed == capacity;
            let journal = FinishJournal::default();
            let admission = FinishAdmission {
                journal: &journal,
                failure,
                armed: Cell::new(false),
                fired: Cell::new(0),
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(&db, 1_000_000, || {
                    let db = &db;
                    let transformer = &transformer;
                    let admission = &admission;
                    let journal = &journal;
                    let result = RegistryBuilder::new(db, admission)?.seal()?.run(
                        move |endpoint| async move {
                            assert!(!journal.owner_live.replace(true));
                            let _owner = FinishOwner {
                                transformer,
                                ty,
                                journal,
                            };
                            let visit = endpoint
                                .local_call(|| {
                                    Ok(infallible(transformer.begin_visit_with(
                                        db,
                                        ty,
                                        &InlineTypeTransformationControl,
                                    )))
                                })
                                .await;
                            let TypeTransformerVisit::Pending(scope) = visit else {
                                panic!("expected an unfinished transformation");
                            };
                            let control = EndpointFinishControl {
                                endpoint: &endpoint,
                                transformer,
                                ty,
                                admission,
                                fault: Some(point),
                            };
                            let result = finish_on_endpoint(scope, &control, Type::unknown()).await;
                            journal.after_await.set(true);
                            Ok(result)
                        },
                    );
                    journal.inner_error.set(result.as_ref().err().copied());
                    result
                })
            }));
            match failure {
                FinishFailure::Refusal => {
                    assert!(matches!(
                        outcome,
                        Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted)))
                    ));
                    assert_eq!(
                        journal.inner_error.get(),
                        Some(RunError::Refused(Incomplete::Interrupted))
                    );
                }
                FinishFailure::InjectedNativePanic => {
                    let Err(payload) = outcome else {
                        anyhow::bail!("injected native payload did not escape the driver");
                    };
                    let Some(InjectedFinishPanic(identity)) =
                        payload.downcast_ref::<InjectedFinishPanic>()
                    else {
                        anyhow::bail!("driver replaced the injected native payload");
                    };
                    assert!(Arc::ptr_eq(identity, &journal.panic_identity));
                    assert_eq!(journal.inner_error.get(), None);
                }
            }
            assert_eq!(admission.fired.get(), 1);
            assert!(!admission.armed.get());
            assert_eq!(journal.initial_charges.get(), 1);
            let expected_work = TypeTransformationWork::CacheStorage {
                len: completed,
                capacity,
            };
            assert_eq!(
                journal.cache_work.get(),
                (point != FinishPoint::InitialCharge).then_some(expected_work)
            );
            assert_eq!(
                journal.fault_work.get(),
                Some(match point {
                    FinishPoint::Resource => ExecutionWork::Resource {
                        requested_bytes: growth.unwrap().requested_payload_bytes
                    },
                    FinishPoint::Growth => ExecutionWork::Work {
                        units: growth.unwrap().relocation_units
                    },
                    _ => ExecutionWork::Work { units: 1 },
                })
            );
            assert_eq!(journal.queued.get(), 1);
            assert!(!journal.child_factory_ran.get());
            assert!(!journal.after_await.get());
            assert!(!journal.owner_live.get());
            assert_eq!(&*journal.drops.borrow(), &["child", "owner"]);
            assert!(journal.pending.borrow_mut().take().is_some());
            assert!(transformer.seen.borrow().is_empty());
            assert_eq!(transformer.cache.borrow().len(), completed);
            assert_eq!(transformer.cache.borrow().capacity(), capacity);
            assert_eq!(transformer.cache.borrow().get(&ty), None);
            assert_eq!(
                salsa::attempt_probe::remaining_allowance_for_diagnostics(&db),
                None
            );

            // No native query is entered by this owner control, so a harness panic cannot poison
            // a memo. A fresh driver retries the same transformation after either failure.
            let retry = try_with_attempt(&db, 1_000_000, || {
                let db = &db;
                let transformer = &transformer;
                let admission = &admission;
                RegistryBuilder::new(db, admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let visit = endpoint
                            .local_call(|| {
                                Ok(infallible(transformer.begin_visit_with(
                                    db,
                                    ty,
                                    &InlineTypeTransformationControl,
                                )))
                            })
                            .await;
                        let TypeTransformerVisit::Pending(scope) = visit else {
                            panic!("the refused transformation must not have a completed entry");
                        };
                        let control = EndpointFinishControl {
                            endpoint: &endpoint,
                            transformer,
                            ty,
                            admission,
                            fault: None,
                        };
                        Ok(finish_on_endpoint(scope, &control, Type::unknown()).await)
                    })
            });
            assert!(
                matches!(retry, Ok(AttemptOutcome::Complete(Ok(result))) if result == Type::unknown())
            );
            assert!(transformer.seen.borrow().is_empty());
            assert_eq!(transformer.cache.borrow().get(&ty), Some(&Type::unknown()));
            assert_eq!(transformer.cache.borrow().len(), completed + 1);
            for index in 0..completed {
                assert_eq!(
                    transformer
                        .cache
                        .borrow()
                        .get(&Type::int_literal(i64::try_from(index)?)),
                    Some(&Type::Never)
                );
            }
            assert_eq!(
                salsa::attempt_probe::remaining_allowance_for_diagnostics(&db),
                None
            );
        }
    }
    assert!(saw_inline_spill && saw_hash_growth);
    Ok(())
}

#[test]
fn endpoint_finish_refusal_drops_queued_child_before_scope() -> anyhow::Result<()> {
    endpoint_finish_retains_scope(FinishFailure::Refusal)
}

#[test]
fn endpoint_finish_injected_native_panic_retains_payload_and_scope() -> anyhow::Result<()> {
    endpoint_finish_retains_scope(FinishFailure::InjectedNativePanic)
}

struct OriginalTransformer<'db> {
    active: RefCell<Vec<(Type<'db>, TypeIdentity<'db>)>>,
    cache: RefCell<FxHashMap<Type<'db>, Type<'db>>>,
}

impl<'db> OriginalTransformer<'db> {
    fn new() -> Self {
        Self {
            active: RefCell::new(Vec::new()),
            cache: RefCell::new(FxHashMap::default()),
        }
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        if let Some(result) = self.cache.borrow().get(&ty) {
            return *result;
        }
        let identity = ty.to_type_identity(db);
        if self
            .active
            .borrow()
            .iter()
            .any(|(active, id)| *active == ty || *id == identity)
        {
            return ty;
        }
        self.active.borrow_mut().push((ty, identity));
        let result = compute();
        assert_eq!(self.active.borrow_mut().pop().map(|(ty, _)| ty), Some(ty));
        self.cache.borrow_mut().insert(ty, result);
        result
    }
}

#[test]
fn ordinary_visits_preserve_exact_abstract_and_completed_results() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/transform.py", "def function(value): return value\n")
        .build()?;
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/transform.py")?,
        db.program_environment().program(&db),
    );
    let Type::FunctionLiteral(function) = global_symbol(&db, file, "function").place.expect_type()
    else {
        anyhow::bail!("expected a function literal");
    };
    let first = Type::FunctionLiteral(function);
    let other = Type::FunctionLiteral(
        function.with_descriptor_kind(&db, CallableTypeKind::StaticMethodLike),
    );
    assert_ne!(first, other);
    assert_eq!(first.to_type_identity(&db), other.to_type_identity(&db));
    let controlled = Transformer::default();
    let outer = pending(&controlled, &db, first)?;
    for input in [first, other] {
        let control = Recording::new(None);
        assert!(
            matches!(controlled.begin_visit_with(&db, input, &control), Ok(TypeTransformerVisit::Ready(result)) if result == input)
        );
        assert_eq!(
            control
                .events
                .borrow()
                .iter()
                .filter(|event| **event == Event::Work(TypeTransformationWork::AncestorComparison))
                .count(),
            1
        );
    }
    drop(outer);

    let original = OriginalTransformer::new();
    let transformer = Transformer::default();
    let old_computations = Cell::new(0);
    let new_computations = Cell::new(0);
    let old = original.visit(&db, first, || {
        old_computations.set(old_computations.get() + 1);
        assert_eq!(
            original.visit(&db, first, || {
                old_computations.set(100);
                Type::Never
            }),
            first
        );
        original.visit(&db, other, || {
            old_computations.set(100);
            Type::Never
        })
    });
    let new = transformer.visit_type(&db, first, || {
        new_computations.set(new_computations.get() + 1);
        assert_eq!(
            transformer.visit_type(&db, first, || {
                new_computations.set(100);
                Type::Never
            }),
            first
        );
        transformer.visit_type(&db, other, || {
            new_computations.set(100);
            Type::Never
        })
    });
    assert_eq!(old, other);
    assert_eq!(new, old);
    assert_eq!(new_computations.get(), old_computations.get());
    assert_eq!(new_computations.get(), 1);
    for ty in [
        first,
        other,
        Type::int_literal(1),
        Type::int_literal(2),
        first,
        Type::int_literal(1),
    ] {
        let old = original.visit(&db, ty, || Type::Never);
        let new = transformer.visit_type(&db, ty, || Type::Never);
        assert_eq!(new, old);
    }
    assert!(transformer.seen.borrow().is_empty());
    assert_eq!(
        transformer.cache.borrow().len(),
        original.cache.borrow().len()
    );
    Ok(())
}

fn controlled_fill<'db>(
    db: &'db TestDb,
    transformer: &Transformer<'db>,
    width: usize,
    control: &Recording,
) {
    let env = db.program_environment();
    for index in 0..width {
        let input = Type::int_literal(index as i64);
        let before = transformer.cache.borrow().layout();
        let start = control.events.borrow().len();
        let Ok(TypeTransformerVisit::Pending(mut scope)) =
            transformer.begin_visit_with(db, input, control)
        else {
            panic!("distinct input must start a transformation")
        };
        let actual = input.materialization(db, &env, MaterializationKind::Top);
        assert_eq!(scope.finish_in_place_with(actual, control), Ok(actual));
        let after = transformer.cache.borrow().layout();
        assert_eq!(after.len, index + 1);
        let events = control.events.borrow();
        let growth = events[start..]
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Work(TypeTransformationWork::Grow {
                        storage: TypeTransformationStorage::Cache,
                        ..
                    })
                )
            })
            .count();
        assert_eq!(growth, usize::from(before.needs_growth()));
        if !before.needs_growth() {
            assert!(!events[start..].iter().any(|event| matches!(
                event,
                Event::Work(TypeTransformationWork::RehashKey { .. })
            )));
        }
        assert_eq!(transformer.cache.borrow().get(&input), Some(&actual));
        assert!(transformer.seen.borrow().is_empty());
    }
}

#[test]
fn shallow_transformer_growth_is_geometric_and_hits_have_fixed_progress() {
    let db = setup_db();
    for width in [3, 32, 256] {
        let transformer = Transformer::default();
        let control = Recording::new(None);
        controlled_fill(&db, &transformer, width, &control);
        let layout = transformer.cache.borrow().layout();
        let events = control.events.borrow();
        let mut bulk = 0;
        let mut rehashed = 0;
        let mut logical = 0;
        let mut grows = 0;
        for event in events.iter() {
            if let Event::Work(work) = *event {
                match work {
                    TypeTransformationWork::Grow {
                        storage: TypeTransformationStorage::Cache,
                        requested_capacity,
                        relocation_units,
                        requested_payload_bytes,
                    } => {
                        grows += 1;
                        bulk += relocation_units;
                        assert_eq!(
                            requested_payload_bytes,
                            requested_capacity * size_of::<(Type<'_>, Type<'_>)>()
                        );
                    }
                    TypeTransformationWork::RehashKey {
                        inline_payload_bytes: 0,
                    } => rehashed += 1,
                    TypeTransformationWork::CacheLookup { .. }
                    | TypeTransformationWork::ActiveStorage { .. }
                    | TypeTransformationWork::CacheStorage { .. } => {
                        logical += work.quote().unwrap().work_units
                    }
                    _ => panic!("shallow fixed-payload visits have no ancestor or text work"),
                }
            }
        }
        assert!(grows >= 1);
        assert_eq!(logical, 3 * width);
        assert!(bulk + rehashed <= 64 * (layout.capacity + 1));
        assert!(layout.capacity <= 4 * width + 8);
        assert_eq!(transformer.seen.borrow().capacity(), 3);
        drop(events);
        let hits = Recording::new(None);
        hits.remaining.set(Some(5));
        for _ in 0..5 {
            assert!(
                matches!(transformer.begin_visit_with(&db, Type::int_literal(0), &hits), Ok(TypeTransformerVisit::Ready(ty)) if ty == Type::int_literal(0))
            );
        }
        assert!(matches!(
            transformer.begin_visit_with(&db, Type::int_literal(0), &hits),
            Err(Refused(5))
        ));
        assert_eq!(hits.events.borrow().len(), 6);
        assert!(hits.events.borrow().iter().all(|event| matches!(
            event,
            Event::Work(TypeTransformationWork::CacheLookup { .. })
        )));
        assert_eq!(transformer.cache.borrow().layout(), layout);
    }
}

#[test]
fn transformer_depth_keeps_actual_ancestor_comparisons() {
    let db = setup_db();
    for depth in [4, 32] {
        let transformer = Transformer::default();
        let control = Recording::new(None);
        let mut scopes = Vec::new();
        for index in 0..depth {
            let Ok(TypeTransformerVisit::Pending(scope)) =
                transformer.begin_visit_with(&db, Type::int_literal(index), &control)
            else {
                panic!("distinct active key")
            };
            scopes.push(scope);
        }
        assert_eq!(transformer.seen.borrow().len(), depth as usize);
        assert_eq!(
            control
                .events
                .borrow()
                .iter()
                .filter(|event| **event == Event::Work(TypeTransformationWork::AncestorComparison))
                .count(),
            (depth * (depth - 1) / 2) as usize
        );
        while let Some(scope) = scopes.pop() {
            drop(scope);
        }
        assert!(transformer.seen.borrow().is_empty());
        assert!(transformer.cache.borrow().len() == 0);
    }
}

#[test]
fn every_growth_admission_preserves_the_borrowed_scope_and_completed_cache() {
    let db = setup_db();
    for width in [2, 7, 14, 28, 56] {
        let trial = Transformer::default();
        controlled_fill(&db, &trial, width, &Recording::new(None));
        let layout = trial.cache.borrow().layout();
        if !layout.needs_growth() {
            continue;
        }
        let input = Type::int_literal(1000);
        let mut scope = pending(&trial, &db, input).unwrap();
        let control = Recording::new(None);
        assert_eq!(scope.finish_in_place_with(input, &control), Ok(input));
        let events = control.events.into_inner();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Work(TypeTransformationWork::RehashKey { .. })
                ))
                .count(),
            width
        );
        for at in 0..events.len() {
            let transformer = Transformer::default();
            controlled_fill(&db, &transformer, width, &Recording::new(None));
            let before = transformer.cache.borrow().layout();
            let mut scope = pending(&transformer, &db, input).unwrap();
            let rejected = Recording::new(Some(at));
            assert_eq!(
                scope.finish_in_place_with(input, &rejected),
                Err(Refused(at))
            );
            assert_eq!(&*rejected.events.borrow(), &events[..=at]);
            assert_eq!(transformer.cache.borrow().layout(), before);
            assert_eq!(transformer.seen.borrow().len(), 1);
            assert_eq!(transformer.cache.borrow().get(&input), None);
            for index in 0..width {
                assert_eq!(
                    transformer
                        .cache
                        .borrow()
                        .get(&Type::int_literal(index as i64)),
                    Some(&Type::int_literal(index as i64))
                );
            }
            assert_eq!(
                scope.finish_in_place_with(input, &Recording::new(None)),
                Ok(input)
            );
            assert!(transformer.seen.borrow().is_empty());
            assert_eq!(transformer.cache.borrow().get(&input), Some(&input));
            let finished = Recording::new(None);
            assert_eq!(scope.finish_in_place_with(input, &finished), Ok(input));
            assert!(
                !finished
                    .events
                    .borrow()
                    .iter()
                    .any(|event| matches!(event, Event::Work(TypeTransformationWork::Grow { .. })))
            );
        }
    }
}

#[test]
fn transformation_growth_uses_the_central_checked_geometry() {
    let spill = CycleCacheLayout {
        variant: 2,
        len: 2,
        capacity: 2,
    };
    let plan = progress_cache_growth::<Type<'_>, Type<'_>, Infallible>(spill)
        .unwrap()
        .unwrap();
    assert_eq!((plan.requested_capacity, plan.relocation_units), (4, 70));
    for capacity in [3usize, 7, 15, 31, 127] {
        let layout = CycleCacheLayout {
            variant: 3,
            len: capacity,
            capacity,
        };
        let plan = progress_cache_growth::<Type<'_>, Type<'_>, Infallible>(layout)
            .unwrap()
            .unwrap();
        assert_eq!(plan.requested_capacity, 2 * capacity);
        let old_extent = (capacity + 1) * 4 + 32;
        let new_extent = (2 * plan.requested_capacity + 1) * 4 + 32;
        assert_eq!(plan.relocation_units, old_extent + capacity + new_extent);
        assert_eq!(hash_slots::<Infallible>(capacity).unwrap(), old_extent);
        assert_eq!(
            progress_cache_growth::<Type<'_>, Type<'_>, Infallible>(CycleCacheLayout {
                len: capacity - 1,
                ..layout
            })
            .unwrap(),
            None
        );
    }
    for capacity in [
        usize::MAX,
        usize::MAX / 2 + 1,
        isize::MAX as usize / size_of::<(Type<'_>, Type<'_>)>(),
    ] {
        let layout = CycleCacheLayout {
            variant: 3,
            len: capacity,
            capacity,
        };
        assert!(matches!(
            progress_cache_growth::<Type<'_>, Type<'_>, Infallible>(layout),
            Err(TddError::CapacityExhausted)
        ));
        assert!(
            TypeTransformationGrowth {
                layout: TransformationLayout::Active {
                    len: capacity,
                    capacity
                }
            }
            .checked_plan()
            .is_none()
        );
    }
    assert!(
        TypeTransformationWork::InlinePayload {
            bytes: [usize::MAX, 1, 0, 0]
        }
        .quote()
        .is_none()
    );
    assert!(
        TypeTransformationWork::RehashKey {
            inline_payload_bytes: usize::MAX
        }
        .quote()
        .is_none()
    );
    let db = setup_db();
    let ordinary = Transformer::default();
    for i in 0..3 {
        infallible(
            pending(&ordinary, &db, Type::int_literal(i))
                .unwrap()
                .finish_with(Type::int_literal(i), &InlineTypeTransformationControl),
        );
    }
    let controlled = Transformer::default();
    controlled_fill(&db, &controlled, 3, &Recording::new(None));
    assert_eq!(ordinary.cache.borrow().capacity(), 3);
    assert!(controlled.cache.borrow().capacity() >= 4);
    for i in 0..3 {
        assert_eq!(
            ordinary.cache.borrow().get(&Type::int_literal(i)),
            controlled.cache.borrow().get(&Type::int_literal(i))
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn transformer_inline_text_is_charged_separately_from_logical_access() {
    let db = setup_db();
    let transformer = Transformer::default();
    let input = todo_type!("an inline payload");
    let control = Recording::new(None);
    let Ok(TypeTransformerVisit::Pending(mut scope)) =
        transformer.begin_visit_with(&db, input, &control)
    else {
        panic!("new text key")
    };
    assert_eq!(
        scope.finish_in_place_with(Type::unknown(), &control),
        Ok(Type::unknown())
    );
    let hit = Recording::new(None);
    assert!(
        matches!(transformer.begin_visit_with(&db, input, &hit), Ok(TypeTransformerVisit::Ready(ty)) if ty == Type::unknown())
    );
    assert_eq!(
        &*hit.events.borrow(),
        &[
            Event::Work(TypeTransformationWork::CacheLookup { len: 1 }),
            Event::Work(TypeTransformationWork::InlinePayload {
                bytes: [input.inline_payload_bytes(), 0, 0, 0]
            }),
        ]
    );
}

#[test]
#[cfg(debug_assertions)]
fn transformer_payload_refusals_precede_hashing_comparison_and_rehash() {
    let db = setup_db();
    let env = db.program_environment();
    let keys = [
        todo_type!("first retained payload"),
        todo_type!("second retained payload"),
    ];
    let transformer = Transformer::default();
    let control = Recording::new(Some(1));
    assert!(matches!(
        transformer.begin_visit_with(&db, keys[0], &control),
        Err(Refused(1))
    ));
    assert_eq!(
        control.events.borrow().last(),
        Some(&Event::Work(TypeTransformationWork::InlinePayload {
            bytes: [keys[0].inline_payload_bytes(), 0, 0, 0]
        }))
    );
    assert!(transformer.seen.borrow().is_empty());
    let parent = pending(&transformer, &db, keys[0]).unwrap();
    let trial = Recording::new(None);
    let Ok(TypeTransformerVisit::Pending(child)) =
        transformer.begin_visit_with(&db, keys[1], &trial)
    else {
        panic!("distinct text payload")
    };
    drop(child);
    let comparison = trial
        .events
        .borrow()
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::Work(TypeTransformationWork::AncestorComparison)
            )
        })
        .unwrap();
    assert_eq!(
        trial.events.borrow()[comparison + 1],
        Event::Work(TypeTransformationWork::InlinePayload {
            bytes: [
                keys[1].inline_payload_bytes(),
                keys[0].inline_payload_bytes(),
                keys[1].inline_payload_bytes(),
                keys[0].inline_payload_bytes()
            ]
        })
    );
    let refused = Recording::new(Some(comparison + 1));
    assert!(
        matches!(transformer.begin_visit_with(&db, keys[1], &refused), Err(Refused(at)) if at == comparison + 1)
    );
    assert_eq!(transformer.seen.borrow().len(), 1);
    drop(parent);

    for rejected_key in 0..2 {
        let transformer = Transformer::default();
        for key in keys {
            let actual = key.materialization(&db, &env, MaterializationKind::Top);
            let mut scope = pending(&transformer, &db, key).unwrap();
            assert_eq!(
                scope.finish_in_place_with(actual, &Recording::new(None)),
                Ok(actual)
            );
        }
        let before = transformer.cache.borrow().layout();
        let input = Type::int_literal(100);
        let mut scope = pending(&transformer, &db, input).unwrap();
        // CacheStorage, Grow, Resource precede the two retained inline keys.
        let refused = Recording::new(Some(3 + rejected_key));
        assert_eq!(
            scope.finish_in_place_with(input, &refused),
            Err(Refused(3 + rejected_key))
        );
        for (event, key) in refused.events.borrow()[3..].iter().zip(keys) {
            assert_eq!(
                *event,
                Event::Work(TypeTransformationWork::RehashKey {
                    inline_payload_bytes: key.inline_payload_bytes()
                })
            );
        }
        assert_eq!(transformer.cache.borrow().layout(), before);
        assert_eq!(transformer.seen.borrow().len(), 1);
        assert_eq!(transformer.cache.borrow().get(&input), None);
        assert_eq!(
            scope.finish_in_place_with(input, &Recording::new(None)),
            Ok(input)
        );
        for key in keys {
            assert_eq!(
                transformer.cache.borrow().get(&key),
                Some(&key.materialization(&db, &env, MaterializationKind::Top))
            );
        }
    }
}

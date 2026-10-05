use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::super::registration::{
    ExecutableRouteProvider, FinalSourceMemo, FinalSourceRoute, ProviderContext, RegistryBuilder,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, MemoReuse, try_with_attempt};
use crate::function::{Configuration, EvictionPolicy, IngredientImpl, Memo, VerifyResult};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id, Revision, Setter};

#[cfg(feature = "accumulator")]
mod accumulated;
mod certification;
mod ownership;
mod prepared;
mod replacement;

#[derive(Default)]
struct Counts {
    source: AtomicUsize,
    consumer: AtomicUsize,
    cancellations: AtomicUsize,
    events: Mutex<Vec<(bool, DatabaseKeyIndex)>>,
    read_allowances: Mutex<Vec<usize>>,
}

impl Counts {
    fn bodies(&self) -> (usize, usize) {
        (
            self.source.load(Ordering::Relaxed),
            self.consumer.load(Ordering::Relaxed),
        )
    }

    fn source_read_completed(&self, db: &dyn Database) {
        self.read_allowances
            .lock()
            .unwrap()
            .push(attempt_probe::remaining_allowance_for_diagnostics(db).unwrap());
    }
}

#[crate::db]
trait Db: Database {
    fn counts(&self) -> &Counts;
}

#[crate::db]
#[derive(Clone)]
struct TestDb {
    storage: crate::Storage<Self>,
    counts: Arc<Counts>,
}

impl Default for TestDb {
    fn default() -> Self {
        let counts = Arc::new(Counts::default());
        let observed = counts.clone();
        Self {
            storage: crate::Storage::new(Some(Box::new(move |event| match event.kind {
                EventKind::WillExecute { database_key } => {
                    observed.events.lock().unwrap().push((false, database_key))
                }
                EventKind::DidValidateMemoizedValue { database_key } => {
                    observed.events.lock().unwrap().push((true, database_key))
                }
                EventKind::WillCheckCancellation => {
                    observed.cancellations.fetch_add(1, Ordering::Relaxed);
                    ownership::on_cancellation();
                }
                _ => {}
            }))),
            counts,
        }
    }
}

#[crate::db]
impl Database for TestDb {}
#[crate::db]
impl Db for TestDb {
    fn counts(&self) -> &Counts {
        &self.counts
    }
}

#[crate::input(debug)]
struct Number {
    #[returns(copy)]
    value: u32,
}

thread_local! {
    static EVICTION: RefCell<Option<Rc<dyn Fn(Id)>>> = const { RefCell::new(None) };
}

struct RecordingEviction;
impl EvictionPolicy for RecordingEviction {
    fn new(_: usize) -> Self {
        Self
    }
    fn record_use(&self, id: Id) {
        let callback = EVICTION.with_borrow(Clone::clone);
        if let Some(callback) = callback {
            callback(id);
        }
    }
    fn set_capacity(&mut self, _: usize) {}
    fn for_each_evicted(&mut self, _: impl FnMut(Id)) {}
}

crate::plumbing::setup_tracked_fn! {
    attrs: [],
    vis: ,
    fn_name: source,
    db_lt: 'db,
    Db: Db,
    db: db,
    input_ids: [input],
    input_tys: [Number],
    interned_input_tys: [Number],
    output_ty: u32,
    inner_fn: {
        fn source_inner(db: &dyn Db, input: Number) -> u32 {
            db.counts().source.fetch_add(1, Ordering::Relaxed);
            input.value(db)
        }
    },
    cycle_recovery_fn: (salsa::plumbing::unexpected_cycle_recovery!),
    cycle_recovery_initial: (salsa::plumbing::unexpected_cycle_initial!),
    cycle_recovery_strategy: Panic,
    attempt_policy: CompleteOnly,
    is_specifiable: false,
    values_equal: {
        fn values_equal<'db>(old_value: &Self::Output<'db>, new_value: &Self::Output<'db>) -> bool {
            old_value == new_value
        }
    },
    needs_interner: false,
    heap_size_fn: ,
    eviction: RecordingEviction,
    lru: 0,
    return_mode: copy,
    persist: false,
    assert_interned_inputs_are_salsa_values: { crate::plumbing::assert_salsa_value::<Number>(); },
    assert_output_is_salsa_value_or_static: {
        fn assert_source_output() { crate::plumbing::assert_salsa_value::<u32>(); }
        let _ = assert_source_output;
    },
    unused_names: [source_salsa, SourceConfiguration, SourceInternedData, SOURCE_FN_CACHE, SOURCE_INTERN_CACHE, source_inner,]
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn consumer(db: &dyn Db, input: Number) -> u32 {
    db.counts().consumer.fetch_add(1, Ordering::Relaxed);
    source(db, input) + 1
}

struct Consumer<'db, C: Configuration> {
    source: FinalSourceRoute<'db, C>,
    reads: usize,
}

impl<'run, 'db: 'run, C, D> ExecutableRouteProvider<'run, 'db, D> for Consumer<'db, C>
where
    C: Configuration<DbView = dyn Db, Output<'db> = u32>,
    D: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    // Number conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, D, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Db,
        input: Number,
    ) -> RunResult<u32> {
        db.counts().consumer.fetch_add(1, Ordering::Relaxed);
        let mut value = context
            .endpoint()
            .read_final_source(&self.source, input.as_id())
            .await;
        db.counts().source_read_completed(db);
        for _ in 1..self.reads {
            value = context
                .endpoint()
                .read_final_source(&self.source, input.as_id())
                .await;
            db.counts().source_read_completed(db);
        }
        Ok(*value + 1)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _id: Id,
        _input: Number,
    ) -> RunResult<u32> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Number,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

#[derive(Default)]
struct Admission(RefCell<Vec<ExecutionWork>>);
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.0.borrow_mut().push(work);
        Ok(())
    }
}

fn stored<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
) -> &'db Memo<C> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .unwrap()
}

fn run_consumer<'db, C>(
    db: &'db dyn Db,
    ingredient: &'db IngredientImpl<C>,
    prepared: &[FinalSourceMemo<'db, C>],
    input: Number,
    admission: &Admission,
) -> RunResult<u32>
where
    C: Configuration<DbView = dyn Db, Output<'db> = u32>,
{
    run_consumer_reads(db, ingredient, prepared, input, admission, 1)
}

fn run_consumer_reads<'db, C>(
    db: &'db dyn Db,
    ingredient: &'db IngredientImpl<C>,
    prepared: &[FinalSourceMemo<'db, C>],
    input: Number,
    admission: &Admission,
    reads: usize,
) -> RunResult<u32>
where
    C: Configuration<DbView = dyn Db, Output<'db> = u32>,
{
    let provider;
    let mut registry = RegistryBuilder::new(db, admission)?;
    let source = registry.register_final_source(db, ingredient, prepared)?;
    assert_eq!(source.prepared_capacity(), prepared.len());
    provider = Consumer { source, reads };
    let route = registry.reserve(db, consumer::fn_ingredient_(db, db.zalsa()))?;
    let binding = registry.provider(&provider)?;
    registry.bind_executable(&route, &binding)?;
    let registry = registry.seal()?;
    let (entries, indices) = registry.registration_capacities();
    assert!(entries >= 2 && indices >= 2);
    registry.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&route, input.as_id())?
            .await?)
    })
}

fn assert_idle(db: &dyn Database) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
}

fn prepared_consumer(db: &TestDb, input: Number, expected: u32, expected_bodies: (usize, usize)) {
    let ingredient = source::fn_ingredient_(db, db.zalsa());
    let certificate = FinalSourceMemo::certify(db as &dyn Db, ingredient, input.as_id()).unwrap();
    let key = certificate.database_key();
    db.counts.events.lock().unwrap().clear();
    let admission = Admission::default();
    assert_eq!(
        try_with_attempt(db, 100_000, || run_consumer(
            db,
            ingredient,
            &[certificate],
            input,
            &admission
        )),
        Ok(AttemptOutcome::Complete(Ok(expected)))
    );
    assert_eq!(db.counts.bodies(), expected_bodies);
    assert!(
        !db.counts
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|(_, observed)| *observed == key)
    );
    let source = stored(db, ingredient, input.as_id());
    let consumer = stored(db, consumer::fn_ingredient_(db, db.zalsa()), input.as_id());
    assert_eq!(consumer.header.origin().inputs().collect::<Vec<_>>(), [key]);
    assert!(consumer.header.origin().outputs().next().is_none());
    assert_eq!(
        consumer.header.revisions.durability,
        source.header.revisions.durability
    );
    assert_eq!(
        consumer.header.revisions.changed_at,
        source.header.revisions.changed_at
    );
    assert!(!consumer.header.may_be_provisional());
    assert_eq!(
        consumer.header.attempt_reuse(db.zalsa()),
        MemoReuse::Ordinary
    );
    assert!(source.header.outputs_are_empty());
    assert_eq!(source.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
    assert!(
        admission
            .0
            .borrow()
            .contains(&ExecutionWork::Work { units: 16 })
    );
    assert_idle(db);
}

#[test]
fn finalized_source_records_real_dependencies_and_reuses_incremental_memos() {
    let mut db = TestDb::default();
    let input = Number::new(&db, 7);
    let unrelated = Number::new(&db, 50);
    let mut ordinary = TestDb::default();
    let ordinary_input = Number::new(&ordinary, 7);
    let ordinary_unrelated = Number::new(&ordinary, 50);
    assert_eq!(source(&db, input), 7);
    let first_change = stored(&db, source::fn_ingredient_(&db, db.zalsa()), input.as_id())
        .header
        .revisions
        .changed_at;
    for _ in 0..3 {
        let expected = consumer(&ordinary, ordinary_input);
        prepared_consumer(&db, input, expected, (1, 1));
    }
    unrelated.set_value(&mut db).to(51);
    ordinary_unrelated.set_value(&mut ordinary).to(51);
    assert_eq!(source(&db, input), 7);
    prepared_consumer(&db, input, consumer(&ordinary, ordinary_input), (1, 1));
    input.set_value(&mut db).to(8);
    ordinary_input.set_value(&mut ordinary).to(8);
    assert_eq!(source(&db, input), 8);
    for _ in 0..3 {
        prepared_consumer(&db, input, consumer(&ordinary, ordinary_input), (2, 2));
    }
    let ingredient = source::fn_ingredient_(&db, db.zalsa());
    let change = stored(&db, ingredient, input.as_id())
        .header
        .revisions
        .changed_at;
    assert!(change > first_change);
    for (revision, changed) in [(first_change, true), (change, false)] {
        let certificate =
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
        let key = certificate.database_key();
        let admission = Admission::default();
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            registry.register_final_source(&db as &dyn Db, ingredient, &[certificate])?;
            registry.seal()?.run(|endpoint| async move {
                let result = endpoint.validate(key, revision)?.await?;
                assert_eq!(matches!(result, VerifyResult::Changed), changed);
                Ok(())
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(db.counts.bodies(), (2, 2));
        assert!(
            admission
                .0
                .borrow()
                .contains(&ExecutionWork::Work { units: 33 })
        );
        assert_idle(&db);
    }
}

#[test]
fn repeated_finalized_source_reads_exhaust_before_consumer_publication() {
    let (two_reads, complete) = {
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        assert_eq!(source(&db, input), 7);
        let ingredient = source::fn_ingredient_(&db, db.zalsa());
        let prepared =
            [FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap()];
        let admission = Admission::default();
        let consumed = Cell::new(0);
        assert_eq!(
            try_with_attempt(&db, 100_000, || {
                let result = run_consumer_reads(&db, ingredient, &prepared, input, &admission, 3);
                consumed.set(
                    100_000 - attempt_probe::remaining_allowance_for_diagnostics(&db).unwrap(),
                );
                result
            }),
            Ok(AttemptOutcome::Complete(Ok(8)))
        );
        let remaining = db.counts.read_allowances.lock().unwrap();
        assert_eq!(remaining.len(), 3);
        (100_000 - remaining[1], consumed.get())
    };
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    assert_eq!(source(&db, input), 7);
    let stamp = Stamp::current(&db);
    let ingredient = source::fn_ingredient_(&db, db.zalsa());
    let consumer_ingredient = consumer::fn_ingredient_(&db, db.zalsa());
    let certificate = FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
    let key = certificate.database_key();
    let prepared = [certificate];
    let admission = Admission::default();
    // Fund exactly two completed reads, including their canonical dependency recording.
    let outcome = try_with_attempt(&db, two_reads, || {
        assert_eq!(
            run_consumer_reads(&db, ingredient, &prepared, input, &admission, 3),
            Err(RunError::Refused(Incomplete::Allowance))
        );
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(db.counts.bodies(), (1, 1));
    assert_eq!(db.counts.read_allowances.lock().unwrap().len(), 2);
    assert!(
        consumer_ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                consumer_ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .is_none()
    );
    assert_idle(&db);
    assert!(stamp.belongs_to(&db));

    admission.0.borrow_mut().clear();
    db.counts.read_allowances.lock().unwrap().clear();
    assert_eq!(
        // The independent cold run also funds completion and publication on this retry.
        try_with_attempt(&db, complete, || run_consumer_reads(
            &db, ingredient, &prepared, input, &admission, 3
        )),
        Ok(AttemptOutcome::Complete(Ok(8)))
    );
    assert_eq!(db.counts.bodies(), (1, 2));
    assert_eq!(db.counts.read_allowances.lock().unwrap().len(), 3);
    let memo = stored(&db, consumer_ingredient, input.as_id());
    assert_eq!(memo.header.origin().inputs().collect::<Vec<_>>(), [key]);
    assert_eq!(memo.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
    assert!(!memo.header.may_be_provisional());

    admission.0.borrow_mut().clear();
    assert_eq!(
        // The cached root read admits delivery without repeating the consumer's source reads.
        try_with_attempt(&db, 1, || run_consumer_reads(
            &db, ingredient, &prepared, input, &admission, 3
        )),
        Ok(AttemptOutcome::Complete(Ok(8)))
    );
    assert_eq!(db.counts.bodies(), (1, 2));
    assert_eq!(db.counts.read_allowances.lock().unwrap().len(), 3);
    assert!(stamp.belongs_to(&db));
    assert_idle(&db);
}

#[test]
fn repeated_finalized_source_validation_spends_the_shared_allowance() {
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    assert_eq!(source(&db, input), 7);
    let ingredient = source::fn_ingredient_(&db, db.zalsa());
    let certificate = FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
    let key = certificate.database_key();
    let revision = stored(&db, ingredient, input.as_id())
        .header
        .revisions
        .changed_at;
    let prepared = [certificate];
    let stamp = Stamp::current(&db);
    for (count, refuses) in [(3, true), (2, false)] {
        let admission = Admission::default();
        let completed = Cell::new(0);
        let outcome = try_with_attempt(&db, 2 * 33, || {
            let mut registry = RegistryBuilder::new(&db, &admission).unwrap();
            registry
                .register_final_source(&db as &dyn Db, ingredient, &prepared)
                .unwrap();
            let completed = &completed;
            let result = registry.seal().unwrap().run(|endpoint| async move {
                for _ in 0..count {
                    let value = endpoint.validate(key, revision)?.await?;
                    assert!(matches!(value, VerifyResult::Unchanged { .. }));
                    completed.set(completed.get() + 1);
                }
                Ok(())
            });
            assert_eq!(
                result,
                if refuses {
                    Err(RunError::Refused(Incomplete::Allowance))
                } else {
                    Ok(())
                }
            );
        });
        assert_eq!(
            outcome,
            Ok(if refuses {
                AttemptOutcome::Incomplete(Incomplete::Allowance)
            } else {
                AttemptOutcome::Complete(())
            })
        );
        assert_eq!(completed.get(), 2);
        assert_eq!(
            admission
                .0
                .borrow()
                .iter()
                .filter(|work| **work == (ExecutionWork::Work { units: 33 }))
                .count(),
            2
        );
        assert_eq!(db.counts.bodies(), (1, 0));
        assert!(stamp.belongs_to(&db));
        assert_idle(&db);
    }
}

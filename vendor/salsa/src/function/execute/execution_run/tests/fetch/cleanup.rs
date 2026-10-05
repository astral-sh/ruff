use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{BodyStep, Kind, Mode, Node, Seed, body_step, observation, seed};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
use crate::function::execute::execution_run::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use crate::function::execute::execution_run::tests::observation::Event;
use crate::function::execute::execution_run::tests::validation_trace::{self, TraceEvent};
use crate::function::execute::execution_run::{
    ExecutionAdmission, ExecutionWork, RunError, RunResult,
};
use crate::function::{Configuration, IngredientImpl};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id, Setter};

#[derive(Clone, Debug)]
struct Snapshot {
    frames: Option<Vec<(DatabaseKeyIndex, bool)>>,
    policy: QueryPolicy,
    depths: (usize, usize),
    reason: Option<Incomplete>,
    panicking: bool,
}

fn snapshot(db: &dyn Database) -> Snapshot {
    Snapshot {
        frames: db.zalsa_local().try_with_query_stack(|stack| {
            stack
                .iter()
                .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
                .collect()
        }),
        policy: attempt_probe::current_policy(),
        depths: attempt_probe::stack_depths(),
        reason: attempt_probe::current().and_then(|support| support.reason()),
        panicking: crate::sync::thread::panicking(),
    }
}

#[derive(Debug)]
struct OutputDrop {
    token: usize,
    owner: Option<Snapshot>,
}

#[derive(Default)]
struct Observations {
    constructed: usize,
    outputs: Vec<OutputDrop>,
    aborts: Vec<(usize, Snapshot)>,
    panic_on_drop: bool,
}

thread_local! {
    static OBSERVATIONS: RefCell<Option<Observations>> = const { RefCell::new(None) };
}

pub(in crate::function::execute::execution_run) fn observe_abort(db: &dyn Database, depth: usize) {
    if OBSERVATIONS.with_borrow(|observations| observations.is_some()) {
        let owner = snapshot(db);
        OBSERVATIONS.with_borrow_mut(|observations| {
            if let Some(observations) = observations {
                observations.aborts.push((depth, owner));
            }
        });
    }
}

fn collect<T>(panic_on_drop: bool, body: impl FnOnce() -> T) -> (T, Observations) {
    OBSERVATIONS.with_borrow_mut(|observations| {
        assert!(
            observations
                .replace(Observations {
                    panic_on_drop,
                    ..Observations::default()
                })
                .is_none()
        );
    });
    let reset = ResetObservations;
    let result = body();
    let observations = OBSERVATIONS.with_borrow_mut(|observations| observations.take().unwrap());
    drop(reset);
    (result, observations)
}

struct ResetObservations;

impl Drop for ResetObservations {
    fn drop(&mut self) {
        OBSERVATIONS.with_borrow_mut(|observations| *observations = None);
    }
}

#[derive(Debug)]
struct InitialValue {
    value: u32,
    token: Option<usize>,
}

impl PartialEq for InitialValue {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for InitialValue {}

impl Drop for InitialValue {
    fn drop(&mut self) {
        let Some(token) = self.token else {
            return;
        };
        if !OBSERVATIONS.with_borrow(|observations| observations.is_some()) {
            return;
        }
        let owner = crate::with_attached_database(snapshot);
        let panic = OBSERVATIONS.with_borrow_mut(|observations| {
            let Some(observations) = observations else {
                return false;
            };
            observations.outputs.push(OutputDrop { token, owner });
            std::mem::take(&mut observations.panic_on_drop)
        });
        if panic {
            panic!("returned initial output destructor panic");
        }
    }
}

fn initial_value(value: u32) -> InitialValue {
    let token = OBSERVATIONS.with_borrow_mut(|observations| {
        observations.as_mut().map(|observations| {
            let token = observations.constructed;
            observations.constructed += 1;
            token
        })
    });
    InitialValue { value, token }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn query(db: &dyn Database, node: Node) -> InitialValue {
    let value = match body_step(db, node, Mode::SelfCycle, Kind::Scalar) {
        BodyStep::Value(value) => value,
        BodyStep::Child(next, after) => after.resume(query(db, next).value),
    };
    InitialValue { value, token: None }
}

fn initial(db: &dyn Database, _id: Id, node: Node) -> InitialValue {
    initial_value(seed(db, node).0)
}

fn recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _old: &InitialValue,
    value: InitialValue,
    _node: Node,
) -> InitialValue {
    value
}

trait Output {
    const KIND: Kind;
    fn body(value: u32) -> Self;
    fn initial(value: u32) -> Self;
    fn value(&self) -> u32;
    fn token(&self) -> Option<usize>;
}

impl Output for InitialValue {
    const KIND: Kind = Kind::Scalar;
    fn body(value: u32) -> Self {
        Self { value, token: None }
    }
    fn initial(value: u32) -> Self {
        initial_value(value)
    }
    fn value(&self) -> u32 {
        self.value
    }
    fn token(&self) -> Option<usize> {
        self.token
    }
}

impl Output for Seed {
    const KIND: Kind = Kind::Seed;
    fn body(value: u32) -> Self {
        Self(value)
    }
    fn initial(value: u32) -> Self {
        Self(value)
    }
    fn value(&self) -> u32 {
        self.0
    }
    fn token(&self) -> Option<usize> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    Refuse,
    Panic,
    Cancel,
}

struct Boundary {
    token: usize,
    owner: Snapshot,
}

#[derive(Default)]
struct Script {
    boundary: RefCell<Option<Boundary>>,
    fired: Cell<bool>,
    initial_started: Cell<bool>,
}

struct Admission<'a> {
    db: &'a dyn Database,
    script: &'a Script,
    fault: Fault,
}

impl ExecutionAdmission for Admission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let boundary = self.script.boundary.borrow();
        let Some(boundary) = boundary.as_ref() else {
            return Ok(());
        };
        if self.script.fired.replace(true) {
            return Ok(());
        }
        assert_eq!(work, ExecutionWork::Work { units: 1 });
        let current = snapshot(self.db);
        assert_eq!(current.frames, boundary.owner.frames);
        assert_eq!(current.depths, boundary.owner.depths);
        match self.fault {
            Fault::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
            Fault::Panic => panic!("returned initial admission panic"),
            Fault::Cancel => {
                self.db.zalsa().runtime().set_cancellation_flag();
                Ok(())
            }
        }
    }
}

struct Providers<'a, 'db, Q: Configuration, S: Configuration> {
    query: Route<'db, Q>,
    seed: Route<'db, S>,
    script: &'a Script,
}

impl<'run, 'db: 'run, Q, S, C> ExecutableRouteProvider<'run, 'db, C> for Providers<'run, 'db, Q, S>
where
    Q: Configuration<DbView = dyn Database, Input<'db> = Node>,
    S: Configuration<DbView = dyn Database, Input<'db> = Node>,
    C: Configuration<DbView = dyn Database, Input<'db> = Node>,
    Q::Output<'db>: Output,
    S::Output<'db>: Output,
    C::Output<'db>: Output,
{
    // Node conversion is handle-only; InitialValue and Seed equality compare one u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<C::Output<'db>> {
        let value = match body_step(db, node, Mode::SelfCycle, <C::Output<'db> as Output>::KIND) {
            BodyStep::Value(value) => value,
            BodyStep::Child(next, after) => {
                after.resume(context.fetch_ref(&self.query, next.as_id())?.await?.value())
            }
        };
        Ok(<C::Output<'db> as Output>::body(value))
    }

    async fn initial(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        id: Id,
        node: Node,
    ) -> RunResult<C::Output<'db>> {
        self.script.initial_started.set(true);
        context.endpoint().demand(|| async { Ok(()) })?.await?;
        let seed = context.fetch_ref(&self.seed, node.as_id())?.await?.value();
        let owner = snapshot(db);
        let output = <C::Output<'db> as Output>::initial(seed);
        if let Some(token) = output.token()
            && self.script.boundary.borrow().is_none()
        {
            let ingredient = query::fn_ingredient_(db, db.zalsa());
            assert!(
                ingredient
                    .get_memo_from_table_for(
                        db.zalsa(),
                        id,
                        ingredient.memo_ingredient_index(db.zalsa(), id),
                    )
                    .is_none_or(|memo| memo.value().is_none())
            );
            // The next admission occurs only after fetch has guarded this returned value.
            *self.script.boundary.borrow_mut() = Some(Boundary { token, owner });
        }
        Ok(output)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call C::Output<'db>,
        value: C::Output<'db>,
        _node: Node,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        Ok(value)
    }
}

type FetchAttempt<'db> =
    Result<AttemptOutcome<RunResult<&'db InitialValue>>, attempt_probe::StartError>;

fn fetch<'db>(
    db: &'db DatabaseImpl,
    root: Node,
    script: &Script,
    admission: &dyn ExecutionAdmission,
) -> FetchAttempt<'db> {
    try_with_attempt(db, 100_000, || {
        let db: &dyn Database = db;
        let mut registry = RegistryBuilder::new(db, admission)?;
        let query = registry.reserve(db, query::fn_ingredient_(db, db.zalsa()))?;
        let seed = registry.reserve(db, seed::fn_ingredient_(db, db.zalsa()))?;
        let providers = Providers {
            query: query.clone(),
            seed: seed.clone(),
            script,
        };
        let mut registry = registry;
        let binding = registry.provider(&providers)?;
        registry.bind_executable(&query, &binding)?;
        registry.bind_executable(&seed, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            endpoint
                .provider(binding)?
                .fetch_ref(&query, root.as_id())?
                .await
        })
    })
}

fn fixture(value: u32) -> (DatabaseImpl, Node, Node) {
    let mut db = DatabaseImpl::default();
    let leaf = Node::new(&db, None, value);
    leaf.set_next(&mut db).to(Some(leaf));
    let root = Node::new(&db, Some(leaf), 0);
    (db, root, leaf)
}

struct ResetCancellation<'a>(&'a dyn Database);

impl Drop for ResetCancellation<'_> {
    fn drop(&mut self) {
        self.0.zalsa().runtime().reset_cancellation_flag();
    }
}

fn assert_restored(db: &DatabaseImpl, root: Node, leaf: Node) {
    let ingredient = query::fn_ingredient_(db, db.zalsa());
    for node in [root, leaf] {
        super::assert_idle(db, ingredient, node);
    }
    assert_eq!(
        db.zalsa()
            .attempt_operations
            .load(crate::sync::atomic::Ordering::SeqCst),
        0
    );
}

fn assert_no_value<C: Configuration>(
    db: &DatabaseImpl,
    ingredient: &IngredientImpl<C>,
    node: Node,
) {
    assert!(
        ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                node.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), node.as_id()),
            )
            .is_none_or(|memo| memo.value().is_none())
    );
}

fn retry(db: &DatabaseImpl, root: Node, seed_value: u32) {
    let script = Script::default();
    let actual = fetch(db, root, &script, &super::Admission).expect("a fresh attempt starts");
    let AttemptOutcome::Complete(Ok(actual)) = actual else {
        panic!("fresh fetch did not complete")
    };
    let (ordinary, ordinary_root, _) = fixture(seed_value);
    assert_eq!(actual.value, query(&ordinary, ordinary_root).value);
    let ingredient = query::fn_ingredient_(db, db.zalsa());
    assert!(std::ptr::eq(
        actual,
        super::memo(db, ingredient, root).value().unwrap()
    ));
}

fn exercise(fault: Fault, panic_on_drop: bool) {
    let (mut db, root, leaf) = fixture(0);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let root_key = ingredient.database_key_index(root.as_id());
    let leaf_key = ingredient.database_key_index(leaf.as_id());
    let script = Script::default();
    let admission = Admission {
        db: &db,
        script: &script,
        fault,
    };
    let ((outcome, polls), drops) = {
        let reset = ResetCancellation(&db);
        let result = collect(panic_on_drop, || {
            observation::collect(|| {
                catch_unwind(AssertUnwindSafe(|| {
                    crate::Cancelled::catch(AssertUnwindSafe(|| {
                        fetch(&db, root, &script, &admission)
                    }))
                }))
            })
        });
        drop(reset);
        result
    };
    assert!(script.fired.get());
    assert_eq!(polls.max_active_polls, 1);
    assert_eq!(drops.constructed, 1);
    assert_eq!(drops.outputs.len(), 1);
    let boundary = script.boundary.borrow();
    let boundary = boundary.as_ref().expect("initializer returned its output");
    let output = &drops.outputs[0];
    assert_eq!(output.token, boundary.token);
    let owner = output.owner.as_ref().expect("destructor has its database");
    assert_eq!(
        owner
            .frames
            .as_ref()
            .unwrap()
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        [root_key, leaf_key]
    );
    assert_eq!(owner.depths, boundary.owner.depths);
    assert_eq!(owner.policy, QueryPolicy::ReturnOnly);
    let cleanup_reason = match fault {
        Fault::Refuse => Some(Incomplete::Allowance),
        Fault::Cancel => Some(Incomplete::Interrupted),
        Fault::Panic => None,
    };
    if let Some(reason) = cleanup_reason {
        assert_eq!(owner.reason, Some(reason));
        assert!(owner.frames.as_ref().unwrap().last().unwrap().1);
        assert!(!owner.panicking);
    } else {
        assert!(owner.panicking);
    }
    match (fault, panic_on_drop) {
        (Fault::Refuse, false) => {
            assert!(matches!(
                outcome,
                Ok(Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))))
            ));
        }
        (Fault::Cancel, false) => {
            assert!(matches!(outcome, Ok(Err(crate::Cancelled::PendingWrite))));
        }
        _ => {
            let payload = outcome.expect_err("the selected panic is preserved");
            let expected = if panic_on_drop {
                "returned initial output destructor panic"
            } else {
                "returned initial admission panic"
            };
            assert_eq!(payload.downcast_ref::<&str>(), Some(&expected));
        }
    }
    if !panic_on_drop && let Some(reason) = cleanup_reason {
        assert_eq!(drops.aborts.len(), 2);
        for ((depth, owner), expected) in drops.aborts.iter().zip([leaf_key, root_key]) {
            let frames = owner.frames.as_ref().unwrap();
            assert_eq!(*depth, frames.len());
            assert_eq!(frames.last(), Some(&(expected, true)));
            assert_eq!(owner.reason, Some(reason));
            assert!(!owner.panicking);
        }
    }
    assert_no_value(&db, ingredient, root);
    assert_no_value(&db, ingredient, leaf);
    assert!(
        super::memo(&db, seed::fn_ingredient_(&db, db.zalsa()), leaf)
            .value()
            .is_some()
    );
    assert_eq!(
        polls
            .events
            .iter()
            .filter(|event| matches!(event, Event::Claim { key, .. } if *key == leaf_key))
            .count(),
        1
    );
    assert_restored(&db, root, leaf);
    if matches!(fault, Fault::Refuse | Fault::Cancel) && !panic_on_drop {
        retry(&db, root, 0);
    } else {
        assert!(matches!(
            try_with_attempt(&db, 100, || {
                let db: &dyn Database = &db;
                RegistryBuilder::new(db, &super::Admission)?
                    .seal()?
                    .run(|_| async { Ok(()) })
            }),
            Ok(AttemptOutcome::Complete(Ok(())))
        ));
        leaf.set_value(&mut db).to(1);
        retry(&db, root, 1);
    }
    assert_restored(&db, root, leaf);
}

#[test]
fn returned_initial_output_refusal_marks_owner_before_drop() {
    exercise(Fault::Refuse, false);
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn returned_initial_output_unwind_preserves_owner_and_payload() {
    for fault in [Fault::Panic, Fault::Cancel] {
        exercise(fault, false);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn returned_initial_output_destructor_panic_drains_ancestors() {
    exercise(Fault::Refuse, true);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    SetupOrDispatch,
    Body,
    InitialCallback,
    ReturnedInitial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdmissionRecord {
    work: ExecutionWork,
    stage: Stage,
    frames: Vec<(usize, bool)>,
    policy: QueryPolicy,
    depths: (usize, usize),
    token: Option<usize>,
}

struct PrefixAdmission<'a> {
    db: &'a dyn Database,
    script: &'a Script,
    keys: [DatabaseKeyIndex; 3],
    stop: Option<(usize, Fault)>,
    records: RefCell<Vec<AdmissionRecord>>,
    returned: Cell<bool>,
}

impl<'a> PrefixAdmission<'a> {
    fn new(
        db: &'a DatabaseImpl,
        root: Node,
        leaf: Node,
        script: &'a Script,
        stop: Option<(usize, Fault)>,
    ) -> Self {
        let query = query::fn_ingredient_(db, db.zalsa());
        let seed = seed::fn_ingredient_(db, db.zalsa());
        Self {
            db,
            script,
            keys: [
                query.database_key_index(root.as_id()),
                query.database_key_index(leaf.as_id()),
                seed.database_key_index(leaf.as_id()),
            ],
            stop,
            records: RefCell::new(Vec::new()),
            returned: Cell::new(false),
        }
    }
}

impl ExecutionAdmission for PrefixAdmission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.returned.get() {
            return Ok(());
        }
        let current = snapshot(self.db);
        let frames: Vec<_> = current
            .frames
            .as_ref()
            .expect("admission can read the query stack")
            .iter()
            .map(|(key, incomplete)| {
                (
                    self.keys
                        .iter()
                        .position(|known| known == key)
                        .expect("fixture query"),
                    *incomplete,
                )
            })
            .collect();
        let boundary = self.script.boundary.borrow();
        let token = boundary.as_ref().map(|boundary| boundary.token);
        let stage = if let Some(boundary) = boundary.as_ref() {
            assert_eq!(work, ExecutionWork::Work { units: 1 });
            assert_eq!(current.frames, boundary.owner.frames);
            assert_eq!(current.depths, boundary.owner.depths);
            self.returned.set(true);
            Stage::ReturnedInitial
        } else if self.script.initial_started.get() {
            Stage::InitialCallback
        } else if frames.is_empty() {
            Stage::SetupOrDispatch
        } else {
            Stage::Body
        };
        let ordinal = {
            let mut records = self.records.borrow_mut();
            let ordinal = records.len();
            records.push(AdmissionRecord {
                work,
                stage,
                frames,
                policy: current.policy,
                depths: current.depths,
                token,
            });
            ordinal
        };
        if let Some((stop, fault)) = self.stop
            && stop == ordinal
        {
            assert!(!self.script.fired.replace(true));
            match fault {
                Fault::Refuse => return Err(RunError::Refused(Incomplete::Allowance)),
                Fault::Panic => panic!("cold fetch admission prefix panic"),
                Fault::Cancel => self.db.zalsa().runtime().set_cancellation_flag(),
            }
        }
        Ok(())
    }
}

fn discover_prefix() -> Vec<AdmissionRecord> {
    let (db, root, leaf) = fixture(0);
    let script = Script::default();
    let admission = PrefixAdmission::new(&db, root, leaf, &script, None);
    let (((result, polls), _drops), trace) = validation_trace::collect(|| {
        collect(false, || {
            observation::collect(|| fetch(&db, root, &script, &admission))
        })
    });
    let Ok(AttemptOutcome::Complete(Ok(value))) = result else {
        panic!("prefix discovery must complete")
    };
    let (ordinary, ordinary_root, _) = fixture(0);
    assert_eq!(value.value, query(&ordinary, ordinary_root).value);
    assert_eq!(polls.max_active_polls, 1);
    assert!(admission.returned.get());
    assert!(!script.fired.get());
    assert_restored(&db, root, leaf);

    let outer: Vec<_> = trace
        .iter()
        .filter_map(|event| match event {
            TraceEvent::Outer {
                phase,
                operation,
                owner,
                query_depth,
                operation_depth,
                ..
            } => Some((*phase, *operation, *owner, *query_depth, *operation_depth)),
            _ => None,
        })
        .collect();
    let ready = outer
        .iter()
        .position(|event| event.0 == "fetch.initial.ready")
        .expect("successful fetch takes the returned initializer");
    let (_, operation, owner, query_depth, operation_depth) = outer[ready];
    assert_eq!(owner, Some(admission.keys[1]));
    assert_eq!(query_depth, 2);
    assert_eq!(
        operation_depth,
        script.boundary.borrow().as_ref().unwrap().owner.depths.0
    );
    assert!(outer[..ready].iter().any(|event| {
        *event
            == (
                "fetch.initial",
                operation,
                owner,
                query_depth,
                operation_depth,
            )
    }));
    assert_eq!(
        outer.get(ready + 1),
        Some(&(
            "fetch.initial.commit",
            operation,
            owner,
            query_depth,
            operation_depth
        ))
    );
    assert_eq!(
        outer.get(ready + 2),
        Some(&(
            "fetch.selected",
            operation,
            owner,
            query_depth,
            operation_depth
        ))
    );

    let records = admission.records.into_inner();
    assert_eq!(records.last().unwrap().stage, Stage::ReturnedInitial);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.token.is_some())
            .count(),
        1
    );
    records
}

fn replay_prefix(prefix: &[AdmissionRecord], ordinal: usize, fault: Fault) {
    let (mut db, root, leaf) = fixture(0);
    let script = Script::default();
    let admission = PrefixAdmission::new(&db, root, leaf, &script, Some((ordinal, fault)));
    let ((outcome, polls), drops) = {
        let reset = ResetCancellation(&db);
        let result = collect(false, || {
            observation::collect(|| {
                catch_unwind(AssertUnwindSafe(|| {
                    crate::Cancelled::catch(AssertUnwindSafe(|| {
                        fetch(&db, root, &script, &admission)
                    }))
                }))
            })
        });
        drop(reset);
        result
    };
    assert!(script.fired.get(), "{fault:?} at admission {ordinal}");
    assert_eq!(&*admission.records.borrow(), &prefix[..=ordinal]);
    assert!(polls.max_active_polls <= 1);
    let cleanup_reason = match fault {
        Fault::Refuse => Some(Incomplete::Allowance),
        Fault::Cancel => Some(Incomplete::Interrupted),
        Fault::Panic => None,
    };
    match fault {
        Fault::Refuse => {
            assert!(matches!(
                outcome,
                Ok(Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))))
            ));
        }
        Fault::Panic => {
            let payload = outcome.expect_err("admission panic is preserved");
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"cold fetch admission prefix panic")
            );
        }
        Fault::Cancel => {
            assert!(matches!(outcome, Ok(Err(crate::Cancelled::PendingWrite))));
        }
    }
    if let Some(reason) = cleanup_reason {
        let mut previous_depth = usize::MAX;
        for (depth, owner) in &drops.aborts {
            let frames = owner.frames.as_ref().unwrap();
            assert_eq!(*depth, frames.len());
            assert!(*depth < previous_depth);
            previous_depth = *depth;
            assert!(frames.last().unwrap().1);
            assert_eq!(owner.reason, Some(reason));
            assert!(!owner.panicking);
        }
    }
    if let Some(token) = prefix[ordinal].token {
        assert_eq!(drops.constructed, 1);
        assert_eq!(drops.outputs.len(), 1);
        assert_eq!(drops.outputs[0].token, token);
        let owner = drops.outputs[0].owner.as_ref().unwrap();
        assert_eq!(owner.depths, prefix[ordinal].depths);
        assert_eq!(owner.policy, QueryPolicy::ReturnOnly);
        let frames = owner.frames.as_ref().unwrap();
        assert_eq!(
            frames.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            admission.keys[..2]
        );
        if let Some(reason) = cleanup_reason {
            assert!(frames.last().unwrap().1);
            assert_eq!(owner.reason, Some(reason));
            assert!(!owner.panicking);
        } else {
            assert!(owner.panicking);
        }
    } else {
        assert_eq!(drops.constructed, 0);
        assert!(drops.outputs.is_empty());
    }
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    assert_no_value(&db, ingredient, root);
    assert_no_value(&db, ingredient, leaf);
    assert_restored(&db, root, leaf);
    if matches!(fault, Fault::Refuse | Fault::Cancel) {
        retry(&db, root, 0);
    } else {
        assert!(matches!(
            try_with_attempt(&db, 100, || {
                let db: &dyn Database = &db;
                RegistryBuilder::new(db, &super::Admission)?
                    .seal()?
                    .run(|_| async { Ok(()) })
            }),
            Ok(AttemptOutcome::Complete(Ok(())))
        ));
        leaf.set_value(&mut db).to(1);
        retry(&db, root, 1);
    }
    assert_restored(&db, root, leaf);
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn cold_initial_admission_prefix_cleanup_and_retry() {
    let prefix = discover_prefix();
    for ordinal in 0..prefix.len() {
        for fault in [Fault::Refuse, Fault::Panic, Fault::Cancel] {
            replay_prefix(&prefix, ordinal, fault);
        }
    }
}

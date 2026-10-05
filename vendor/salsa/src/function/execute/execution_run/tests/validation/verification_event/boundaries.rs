use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::super::super::super::registration::RouteProvider;
use super::*;
use crate::function::maybe_changed_after::VerifyResult;
use crate::plumbing::FromId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Binding {
    Executable,
    Missing,
    Proof,
}

#[derive(Clone, Copy, Debug)]
struct Options {
    fetch: bool,
    root: bool,
    binding: Binding,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fetch: false,
            root: false,
            binding: Binding::Executable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Hook {
    Admission(ExecutionWork),
    Cancellation,
    Interned(DatabaseKeyIndex),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Outer(&'static str, Option<DatabaseKeyIndex>),
    Verifier(&'static str, DatabaseKeyIndex),
    Request(DatabaseKeyIndex, DatabaseKeyIndex),
    Reply(DatabaseKeyIndex, DatabaseKeyIndex),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    hook: Hook,
    phase: Option<Phase>,
    task: usize,
    operations: Vec<usize>,
    caller: Option<DatabaseKeyIndex>,
    target_claimed: bool,
    leaf_bodies: usize,
    verified_at: Revision,
    memo: usize,
    resumes: usize,
    pending: Option<DatabaseKeyIndex>,
    dependency_current: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Failure {
    Child,
    Refuse,
    Panic,
    MixedReason,
}

impl Failure {
    fn error(self) -> Option<RunError> {
        match self {
            Self::Child => Some(RunError::Contract("completed task retained a child")),
            Self::Refuse => Some(RunError::Refused(Incomplete::Allowance)),
            Self::Panic => None,
            Self::MixedReason => Some(RunError::Refused(Incomplete::Interrupted)),
        }
    }

    fn reason(self) -> Option<Incomplete> {
        match self {
            Self::Child => Some(Incomplete::Interrupted),
            Self::Refuse | Self::MixedReason => Some(Incomplete::Allowance),
            Self::Panic => None,
        }
    }
}

#[derive(Clone)]
struct Trigger {
    prefix: Vec<Snapshot>,
    failure: Failure,
}

struct Script {
    options: Options,
    endpoint: Option<Endpoint<'static, 'static>>,
    tasks: Vec<Rc<Cell<bool>>>,
    observations: Vec<Snapshot>,
    trigger: Option<Trigger>,
    fired: Option<Snapshot>,
    child: Option<Snapshot>,
    busy: bool,
    interned_events: usize,
}

thread_local! {
    static SCRIPT: RefCell<Option<Script>> = const { RefCell::new(None) };
}

pub(super) fn clear() {
    SCRIPT.with_borrow_mut(|slot| *slot = None);
}

fn install(options: Options, trigger: Option<Trigger>) {
    SCRIPT.with_borrow_mut(|slot| {
        *slot = Some(Script {
            options,
            endpoint: None,
            tasks: Vec::new(),
            observations: Vec::new(),
            trigger,
            fired: None,
            child: None,
            busy: false,
            interned_events: 0,
        });
    });
}

pub(super) fn fetch_instead() -> bool {
    SCRIPT.with_borrow(|slot| slot.as_ref().is_some_and(|script| script.options.fetch))
}

pub(super) fn root() -> bool {
    SCRIPT.with_borrow(|slot| slot.as_ref().is_some_and(|script| script.options.root))
}

pub(super) fn binding() -> Binding {
    SCRIPT.with_borrow(|slot| {
        slot.as_ref()
            .map_or(Binding::Executable, |script| script.options.binding)
    })
}

pub(super) fn arm(endpoint: Endpoint<'static, 'static>) {
    SCRIPT.with_borrow_mut(|slot| {
        if let Some(script) = slot {
            let wanted = endpoint
                .queue
                .active_poll
                .borrow()
                .as_ref()
                .unwrap()
                .identity
                .wanted
                .clone();
            assert!(script.endpoint.replace(endpoint).is_none());
            script.tasks.push(wanted);
        }
    });
}

fn snapshot(endpoint: &Endpoint<'static, 'static>, hook: Hook, task: usize) -> Snapshot {
    let (target, leaf_bodies) = STATE.with_borrow(|slot| {
        let state = slot.as_ref().unwrap();
        (state.target, state.leaf_bodies)
    });
    let phase = validation_trace::latest_boundary().map(|event| match event {
        TraceEvent::Outer { phase, key, .. } => Phase::Outer(phase, key),
        TraceEvent::Verifier { phase, key, .. } => Phase::Verifier(phase, key),
        TraceEvent::Request { owner, key, .. } => Phase::Request(owner, key),
        TraceEvent::Reply { owner, key, .. } => Phase::Reply(owner, key),
        _ => panic!("the boundary reader returned an unrelated observation"),
    });
    let pending = validation_trace::pending_dependency(target);
    let zalsa = endpoint.context.db.zalsa();
    let dependency_current = pending.is_some_and(|key| {
        zalsa
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .and_then(|ingredient| ingredient.memo(zalsa, key.key_index()))
            .is_some_and(|memo| memo.header().verified_at.load() == zalsa.current_revision())
    });
    Snapshot {
        hook,
        phase,
        task,
        operations: endpoint.context.operations.borrow().clone(),
        caller: endpoint
            .context
            .db
            .zalsa_local()
            .active_query()
            .map(|(key, _)| key),
        target_claimed: claimed(endpoint.context.db, target),
        leaf_bodies,
        verified_at: selected(endpoint.context.db, target).verified_at,
        memo: selected(endpoint.context.db, target).address,
        resumes: validation_trace::dependency_resumes(target),
        pending,
        dependency_current,
    }
}

struct Child {
    endpoint: Endpoint<'static, 'static>,
    hook: Hook,
    task: usize,
}

impl Drop for Child {
    fn drop(&mut self) {
        let observed = snapshot(&self.endpoint, self.hook, self.task);
        SCRIPT.with_borrow_mut(|slot| slot.as_mut().unwrap().child = Some(observed));
        record("boundary.child");
        if let Hook::Interned(key) = self.hook {
            let before = SCRIPT.with_borrow(|slot| slot.as_ref().unwrap().interned_events);
            let token = Token::new(self.endpoint.context.db, 0);
            assert_eq!(token.as_id(), key.key_index());
            assert_eq!(
                SCRIPT.with_borrow(|slot| slot.as_ref().unwrap().interned_events),
                before,
                "the already-published interned revision pin must survive rejection"
            );
        }
    }
}

fn visit(hook: Hook) -> RunResult<()> {
    let endpoint = SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref()?;
        if script.busy || script.fired.is_some() {
            return None;
        }
        script.endpoint.clone()
    });
    let Some(endpoint) = endpoint else {
        return Ok(());
    };
    let Some(wanted) = endpoint
        .queue
        .active_poll
        .borrow()
        .as_ref()
        .map(|poll| poll.identity.wanted.clone())
    else {
        return Ok(());
    };
    let task = SCRIPT.with_borrow_mut(|slot| {
        let script = slot.as_mut().unwrap();
        if let Some(index) = script
            .tasks
            .iter()
            .position(|known| Rc::ptr_eq(known, &wanted))
        {
            index
        } else {
            let index = script.tasks.len();
            script.tasks.push(wanted);
            index
        }
    });
    let observed = snapshot(&endpoint, hook, task);
    let failure = SCRIPT.with_borrow_mut(|slot| {
        let script = slot.as_mut().unwrap();
        script.observations.push(observed.clone());
        let trigger = script.trigger.as_ref()?;
        if script.observations.len() != trigger.prefix.len() {
            return None;
        }
        assert_eq!(
            script.observations, trigger.prefix,
            "the semantic callback prefix changed"
        );
        script.fired = Some(observed);
        script.busy = true;
        Some(trigger.failure)
    });
    let Some(failure) = failure else {
        return Ok(());
    };
    record("boundary");
    let child = Child {
        endpoint: endpoint.clone(),
        hook,
        task,
    };
    let _reply = endpoint
        .demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _child = &child;
                panic!("a child queued by a rejected boundary must not execute")
            })
        })
        .expect("the boundary retains its active task");
    match failure {
        Failure::Child => Ok(()),
        Failure::Refuse => {
            attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Allowance);
            Err(RunError::Refused(Incomplete::Allowance))
        }
        Failure::MixedReason => {
            attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Allowance);
            Err(RunError::Refused(Incomplete::Interrupted))
        }
        Failure::Panic => {
            let identity = STATE.with_borrow(|slot| slot.as_ref().unwrap().identity.clone());
            panic_any(PanicMarker(identity));
        }
    }
}

pub(super) fn event(kind: &EventKind) {
    let hook = match kind {
        EventKind::WillCheckCancellation => Hook::Cancellation,
        EventKind::DidValidateInternedValue { key, .. } => Hook::Interned(*key),
        _ => return,
    };
    if matches!(hook, Hook::Interned(_)) {
        SCRIPT.with_borrow_mut(|slot| {
            if let Some(script) = slot {
                script.interned_events += 1;
            }
        });
    }
    // Unit-returning event callbacks communicate refusal through the real attempt reason.
    let _ = visit(hook);
}

pub(super) fn admit(work: ExecutionWork) -> RunResult<()> {
    visit(Hook::Admission(work))
}

impl<A: Configuration, C: Configuration> RouteProvider<'static, 'static, C> for Providers<A> {
    async fn body(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static C::DbView,
        _input: C::Input<'static>,
    ) -> RunResult<C::Output<'static>> {
        panic!("proof body cannot replace executable validation")
    }
    async fn verify(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static C::DbView,
        _id: Id,
        _revision: Revision,
    ) -> RunResult<VerifyResult> {
        panic!("provider-authored validity cannot replace executable validation")
    }
}

fn run(fixture: &Fixture, index: usize) -> (RunResult<u32>, Vec<TraceEvent>) {
    let mut returned = None;
    let (outcome, trace) = validation_trace::collect(|| {
        try_with_attempt(fixture.db, 100_000, || {
            let result = fixture.run(index);
            returned = Some(result);
            result
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(fixture.expected))));
    (returned.unwrap(), trace)
}

fn fresh(path: Path) -> Fixture {
    match path {
        Path::TrackedField(_) => field_fixture(),
        _ => Fixture::new(path, false),
    }
}

fn discover(path: Path, options: Options) -> Vec<Snapshot> {
    let fixture = fresh(path);
    let _clear = begin(&fixture);
    install(options, None);
    assert_eq!(run(&fixture, 0).0, Ok(fixture.expected));
    fixture.assert_idle(0);
    let observations = SCRIPT.with_borrow(|slot| slot.as_ref().unwrap().observations.clone());
    assert!(
        observations
            .iter()
            .all(|item| item.memo == fixture.old.address)
    );
    STATE.with_borrow(|slot| {
        let state = slot.as_ref().unwrap();
        eprintln!(
            "QUERY_BOUNDARY {path:?} {options:?}: admissions={:?} cancellation_callbacks={}",
            state.admissions, state.cancellations
        );
    });
    observations
}

fn exercise(path: Path, options: Options, mut trigger: Trigger) {
    let fixture = fresh(path);
    let _clear = begin(&fixture);
    let stamp = Stamp::current(fixture.db);
    let failure = trigger.failure;
    // Discovery checked the original memo in its own database; replay uses this database's memo.
    for expected in &mut trigger.prefix {
        expected.memo = fixture.old.address;
    }
    install(options, Some(trigger));
    let identity = STATE.with_borrow(|slot| slot.as_ref().unwrap().identity.clone());
    let mut returned = None;
    let (outcome, trace) = validation_trace::collect(|| {
        catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(fixture.db, 100_000, || {
                let result = fixture.run(0);
                returned = Some(result);
                result
            })
        }))
    });
    let (fired, child) = SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        (
            script
                .fired
                .clone()
                .expect("the selected boundary was reached"),
            script.child.clone().expect("queued child was destroyed"),
        )
    });
    // Semantic ownership precedes outcome and trace checks, including in predecessor controls.
    assert_eq!(
        child.operations, fired.operations,
        "operation escaped before queued-child cleanup"
    );
    assert_eq!(
        child.caller, fired.caller,
        "caller escaped before queued-child cleanup"
    );
    assert_eq!(
        child.target_claimed, fired.target_claimed,
        "claim escaped before queued-child cleanup"
    );
    assert_eq!(
        child.verified_at, fired.verified_at,
        "verification advanced after a rejected boundary"
    );
    assert_eq!(fired.memo, fixture.old.address);
    assert_eq!(
        child.memo, fired.memo,
        "the selected memo changed during cleanup"
    );
    assert_eq!(
        child.resumes, fired.resumes,
        "a rejected boundary advanced its cursor"
    );
    assert_eq!(child.pending, fired.pending);
    assert_eq!(child.dependency_current, fired.dependency_current);
    assert_eq!(Stamp::current(fixture.db), stamp);
    fixture.assert_idle(0);
    assert_eq!(returned, failure.error().map(Err));
    match failure {
        Failure::Panic => {
            let payload = outcome.expect_err("the original native panic escapes");
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<PanicMarker>().unwrap().0,
                &identity
            ));
        }
        _ => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(failure.reason().unwrap()))
        ),
    }
    STATE.with_borrow(|slot| {
        let state = slot.as_ref().unwrap();
        let boundary = state
            .observations
            .iter()
            .position(|item| item.stage == "boundary")
            .unwrap();
        let child = state
            .observations
            .iter()
            .position(|item| item.stage == "boundary.child")
            .unwrap();
        let caller = state
            .observations
            .iter()
            .position(|item| item.stage == if options.root { "root" } else { "caller" })
            .unwrap();
        assert!(boundary < child && child < caller);
        let child = &state.observations[child];
        assert_eq!(child.reason, failure.reason());
        assert_eq!(child.panicking, failure == Failure::Panic);
    });
    assert_eq!(trace.iter().filter(|event| matches!(event,
        TraceEvent::Outer { phase: "dependency.resume", key: Some(key), .. } if *key == fixture.target
    )).count(), fired.resumes, "a rejected dependency advanced its parent cursor: {trace:#?}");
    let completed_leaf_bodies = STATE.with_borrow(|slot| slot.as_ref().unwrap().leaf_bodies);
    fixture.reset(1, Fault::Quiet);
    install(options, None);
    assert_eq!(run(&fixture, 1).0, Ok(fixture.expected));
    fixture.assert_idle(1);
    if completed_leaf_bodies != 0 {
        assert_eq!(
            STATE.with_borrow(|slot| slot.as_ref().unwrap().leaf_bodies),
            completed_leaf_bodies,
            "a completed dependency executed again during retry"
        );
    }
    assert_eq!(Stamp::current(fixture.db), stamp);
}

fn sweep(
    path: Path,
    options: Options,
    select: impl Fn(&Snapshot) -> bool,
    failures: &[Failure],
) -> Vec<Snapshot> {
    let discovered = discover(path, options);
    let indexes: Vec<_> = discovered
        .iter()
        .enumerate()
        .filter_map(|(index, item)| select(item).then_some(index))
        .collect();
    assert!(
        !indexes.is_empty(),
        "the named semantic interval was not exercised: {discovered:#?}"
    );
    for index in &indexes {
        for failure in failures {
            if *failure == Failure::Panic && cfg!(feature = "shuttle") {
                continue;
            }
            exercise(
                path,
                options,
                Trigger {
                    prefix: discovered[..=*index].to_vec(),
                    failure: *failure,
                },
            );
        }
    }
    indexes
        .into_iter()
        .map(|index| discovered[index].clone())
        .collect()
}

#[test]
fn fetch_and_validation_entry_keep_the_original_caller() {
    for path in [Path::EagerFetch, Path::EagerValidate] {
        for root in [false, true] {
            let options = Options {
                root,
                ..Options::default()
            };
            let entry_depth = usize::from(!root);
            let samples = sweep(
                path,
                options,
                |item| {
                    item.task == 1
                        && !item.target_claimed
                        && item.leaf_bodies == 0
                        && item.operations.len() <= entry_depth + 1
                        && matches!(item.phase, None | Some(Phase::Outer("execute.start", _)))
                        && (matches!(item.hook,Hook::Admission(ExecutionWork::Resource{requested_bytes}) if requested_bytes == 4*size_of::<usize>())
                            || matches!(item.hook, Hook::Cancellation)
                                && item.operations.len() <= entry_depth + 1)
                },
                &[Failure::Child, Failure::Refuse, Failure::Panic],
            );
            assert!(
                samples
                    .iter()
                    .any(|item| item.operations.len() == entry_depth)
            );
            assert!(
                samples
                    .iter()
                    .any(|item| item.operations.len() == entry_depth + 1)
            );
        }
    }
}

#[test]
fn claimed_phase_checks_drain_children_before_the_claim() {
    for fetch in [false, true] {
        let options = Options {
            fetch,
            ..Options::default()
        };
        sweep(
            Path::Deep,
            options,
            |item| {
                item.task == 1
                    && item.target_claimed
                    && matches!(
                        item.hook,
                        Hook::Admission(ExecutionWork::Work { .. }) | Hook::Cancellation
                    )
                    && matches!(item.phase, Some(Phase::Outer(_, _) | Phase::Verifier(_, _)))
            },
            &[Failure::Refuse, Failure::Panic],
        );
        for commit in [false, true] {
            let commit_predecessor = if cfg!(feature = "accumulator") {
                "accumulated.published"
            } else {
                "finish"
            };
            sweep(
                Path::Deep,
                options,
                |item| {
                    item.task == 1
                        && item.target_claimed
                        && matches!(item.hook, Hook::Admission(ExecutionWork::Work { .. }))
                        && if commit {
                            // Finish selects Commit; Commit's own label follows its admission.
                            matches!(item.phase, Some(Phase::Verifier(phase, _)) if phase == commit_predecessor)
                        } else {
                            matches!(item.phase, Some(Phase::Outer("fetch.claim" | "claim", _)))
                        }
                },
                &[Failure::Child],
            );
        }
    }
}

#[test]
fn pending_dependency_keeps_the_cursor_and_claim() {
    let fixture = Fixture::new(Path::Deep, false);
    let leaf = fixture.calls[0]
        .target(fixture.db)
        .next(fixture.db)
        .unwrap();
    let wrapped_key =
        wrapped::fn_ingredient_(fixture.db, fixture.db.zalsa()).database_key_index(leaf.as_id());
    for fetch in [false, true] {
        let options = Options {
            fetch,
            ..Options::default()
        };
        let samples = sweep(
            Path::Deep,
            options,
            |item| {
                item.task == 1
                    && item.target_claimed
                    && item.pending == Some(wrapped_key)
                    && item.operations.len() == 2
                    && (matches!(item.hook, Hook::Admission(ExecutionWork::Task { .. }))
                        || matches!(item.hook, Hook::Cancellation)
                            && item.leaf_bodies == 1
                            && item.dependency_current)
            },
            &[Failure::Refuse, Failure::Panic],
        );
        assert!(
            samples
                .iter()
                .any(|item| matches!(item.hook, Hook::Admission(ExecutionWork::Task { .. })))
        );
        assert!(
            samples.iter().any(|item| {
                matches!(item.hook, Hook::Cancellation)
                    && item.leaf_bodies == 1
                    && item.dependency_current
            }),
            "the completed child was checked before its pending request resumed"
        );
    }
}

#[test]
fn fixed_input_leaf_admission_retains_the_parent_verifier() {
    let options = Options::default();
    sweep(
        Path::Deep,
        options,
        |item| {
            item.task > 1
                && item.target_claimed
                && item.leaf_bodies == 0
                && matches!(item.hook, Hook::Admission(ExecutionWork::Work { units: 1 }))
                && matches!(item.phase, Some(Phase::Request(_, _)))
        },
        &[Failure::Child, Failure::Refuse, Failure::Panic],
    );
}

#[test]
fn mixed_reason_admission_preserves_the_returned_error() {
    sweep(
        Path::EagerFetch,
        Options::default(),
        |item| {
            item.task == 1
                && item.operations.len() == 1
                && matches!(item.hook,Hook::Admission(ExecutionWork::Resource{requested_bytes}) if requested_bytes == 4*size_of::<usize>())
        },
        &[Failure::MixedReason],
    );
}

#[test]
fn dependency_registration_errors_do_not_execute_proof_callbacks() {
    for (binding, error) in [
        (
            Binding::Missing,
            RunError::Contract("validation route is not registered"),
        ),
        (
            Binding::Proof,
            RunError::Contract("validation route is not executable"),
        ),
    ] {
        let fixture = Fixture::new(Path::Deep, false);
        let _clear = begin(&fixture);
        install(
            Options {
                binding,
                ..Options::default()
            },
            None,
        );
        let mut returned = None;
        let outcome = try_with_attempt(fixture.db, 100_000, || {
            returned = Some(fixture.run(0));
        });
        assert_eq!(returned, Some(Err(error)));
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(
            selected(fixture.db, fixture.target).verified_at,
            fixture.old.verified_at
        );
        fixture.assert_idle(0);
        fixture.reset(1, Fault::Quiet);
        install(Options::default(), None);
        assert_eq!(run(&fixture, 1).0, Ok(fixture.expected));
    }
}

#[crate::interned]
struct Token<'db> {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
pub(super) fn interned_query(db: &dyn Database, node: Node) -> u32 {
    let next = node.next(db).unwrap();
    let token = Token::new(db, node.value(db));
    assert_eq!(token.value(db), 0);
    wrapped(db, next).0 + 1
}

pub(super) fn interned_key(db: &dyn Database, node: Node) -> DatabaseKeyIndex {
    interned_query::fn_ingredient_(db, db.zalsa()).database_key_index(node.as_id())
}

pub(super) fn run_interned(fixture: &Fixture, index: usize) -> RunResult<u32> {
    fixture.run_with(
        index,
        interned_query::fn_ingredient_(fixture.db, fixture.db.zalsa()),
    )
}

#[crate::tracked]
pub(super) struct Entity<'db> {
    #[tracked]
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy))]
fn producer(db: &dyn Database, node: Node) -> Entity<'_> {
    Entity::new(db, node.value(db))
}

#[crate::input]
#[derive(Debug)]
pub(super) struct FieldInput {
    #[returns(copy)]
    entity: Id,
}

impl FixtureInput for FieldInput {
    fn request(self) -> Request {
        Request::VerifyOnly
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
pub(super) fn field_query(db: &dyn Database, input: FieldInput) -> u32 {
    Entity::from_id(input.entity(db)).value(db)
}

pub(super) fn run_field(fixture: &Fixture, index: usize) -> RunResult<u32> {
    fixture.run_with(
        index,
        field_query::fn_ingredient_(fixture.db, fixture.db.zalsa()),
    )
}

fn field_fixture() -> Fixture {
    let mut db = HookDb::default();
    let node = Node::new(&db, None, 8);
    // Save the genuine output ID in its own database without extending a tracked handle's borrow.
    let input = FieldInput::new(&db, producer(&db, node).as_id());
    let path = Path::TrackedField(input);
    let calls = std::array::from_fn(|_| Call::new(&db, node, None, path));
    let expected = field_query(&db, input);
    let target = field_query::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let old = selected(&db, target);
    db.synthetic_write(Durability::LOW);
    assert!(old.verified_at < db.zalsa().current_revision());
    let db = Box::leak(Box::new(db));
    Fixture {
        db,
        path,
        calls,
        target,
        decoy: None,
        old,
        expected,
    }
}

#[test]
fn tracked_field_leaf_admission_retains_the_parent_verifier() {
    let fixture = field_fixture();
    let Path::TrackedField(input) = fixture.path else {
        unreachable!()
    };
    let entity_id = input.entity(fixture.db);
    sweep(
        fixture.path,
        Options::default(),
        |item| {
            item.task > 1
                && item.target_claimed
                && matches!(item.hook, Hook::Admission(ExecutionWork::Work { units: 1 }))
                && item.pending.is_some_and(|key| key.key_index() == entity_id)
        },
        &[Failure::Child, Failure::Refuse, Failure::Panic],
    );
}

#[test]
fn interned_leaf_event_releases_its_lock_before_terminal_cleanup() {
    let samples = sweep(
        Path::Interned,
        Options::default(),
        |item| item.target_claimed && matches!(item.hook, Hook::Interned(_)),
        &[Failure::Child, Failure::Refuse, Failure::Panic],
    );
    assert_eq!(
        samples.len(),
        1,
        "the fixture selects one actual interned dependency"
    );
}

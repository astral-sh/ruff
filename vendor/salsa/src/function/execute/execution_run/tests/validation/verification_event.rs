use std::cell::RefCell;
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
#[cfg(not(feature = "shuttle"))]
use std::sync::mpsc;
use std::task::Poll;
#[cfg(not(feature = "shuttle"))]
use std::time::Duration;

use super::super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use super::super::super::{Endpoint, ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::super::observation::{self, Event};
use super::super::validation_trace::{self, TraceEvent};
use super::{Node, TestOutput, body, cyclic, scalar, wrapped};
#[cfg(not(feature = "shuttle"))]
use crate::attempt_probe::paired_test_support::run_pair;
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, Durability, EventKind, Id, Revision, Setter};

mod boundaries;

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
enum Path {
    EagerFetch,
    EagerValidate,
    ClaimedShallow,
    Deep,
    Interned,
    TrackedField(boundaries::FieldInput),
}

impl Path {
    fn validates(self) -> bool {
        self != Self::EagerFetch
    }

    fn claimed(self) -> bool {
        matches!(
            self,
            Self::ClaimedShallow | Self::Deep | Self::Interned | Self::TrackedField(_)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Quiet,
    ChildOnly,
    ChildRefuse,
    ChildPanic,
}

fn faults() -> Vec<Fault> {
    let mut faults = vec![Fault::ChildOnly, Fault::ChildRefuse];
    if !cfg!(feature = "shuttle") {
        faults.push(Fault::ChildPanic);
    }
    faults.push(Fault::Quiet);
    faults
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Selected {
    address: usize,
    verified_at: Revision,
    changed_at: Revision,
    provisional: bool,
}

fn selected(db: &dyn Database, key: DatabaseKeyIndex) -> Selected {
    let memo = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .memo(db.zalsa(), key.key_index())
        .unwrap();
    let header = memo.header();
    Selected {
        address: std::ptr::from_ref(header).addr(),
        verified_at: header.verified_at.load(),
        changed_at: header.revisions.changed_at,
        provisional: header.may_be_provisional(),
    }
}

fn claimed(db: &dyn Database, key: DatabaseKeyIndex) -> bool {
    matches!(
        db.zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap()
            .sync_table()
            .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
        ClaimResult::Cycle { .. }
    )
}

#[derive(Debug)]
struct Observation {
    stage: &'static str,
    selected: Selected,
    caller: Option<DatabaseKeyIndex>,
    target_claimed: bool,
    caller_claimed: bool,
    depths: (usize, usize),
    reason: Option<Incomplete>,
    panicking: bool,
}

struct State {
    target: DatabaseKeyIndex,
    caller: DatabaseKeyIndex,
    fault: Fault,
    endpoint: Option<Endpoint<'static, 'static>>,
    identity: Arc<()>,
    fired: bool,
    observations: Vec<Observation>,
    events: Vec<DatabaseKeyIndex>,
    admissions: Vec<ExecutionWork>,
    cancellations: usize,
    leaf_bodies: usize,
    #[cfg(not(feature = "shuttle"))]
    verification_pause: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct ClearState;

impl Drop for ClearState {
    fn drop(&mut self) {
        boundaries::clear();
        STATE.with_borrow_mut(|state| *state = None);
    }
}

fn begin(fixture: &Fixture) -> ClearState {
    STATE.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(State {
            target: fixture.target,
            caller: fixture.caller_key(0),
            fault: Fault::Quiet,
            endpoint: None,
            identity: Arc::new(()),
            fired: false,
            observations: Vec::new(),
            events: Vec::new(),
            admissions: Vec::new(),
            cancellations: 0,
            leaf_bodies: 0,
            #[cfg(not(feature = "shuttle"))]
            verification_pause: None,
        });
    });
    ClearState
}

fn record(stage: &'static str) {
    let (target, caller) = STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        (state.target, state.caller)
    });
    let observation = crate::with_attached_database(|db| Observation {
        stage,
        selected: selected(db, target),
        caller: db.zalsa_local().active_query().map(|(key, _)| key),
        target_claimed: claimed(db, target),
        caller_claimed: claimed(db, caller),
        depths: attempt_probe::stack_depths(),
        reason: attempt_probe::current().and_then(|support| support.reason()),
        panicking: crate::sync::thread::panicking(),
    })
    .expect("verification cleanup retains its attached database");
    STATE.with_borrow_mut(|state| state.as_mut().unwrap().observations.push(observation));
}

struct Marker(&'static str);

impl Drop for Marker {
    fn drop(&mut self) {
        record(self.0);
    }
}

#[derive(Debug)]
struct PanicMarker(Arc<()>);

fn verification_event(key: DatabaseKeyIndex) {
    let hook = STATE.with_borrow_mut(|state| {
        let state = state.as_mut()?;
        state.events.push(key);
        if key != state.target || state.fired {
            return None;
        }
        state.fired = true;
        Some((state.endpoint.take(), state.fault, state.identity.clone()))
    });
    let Some((endpoint, fault, identity)) = hook else {
        return;
    };
    record("event");
    #[cfg(not(feature = "shuttle"))]
    if let Some((entered, resume)) =
        STATE.with_borrow_mut(|state| state.as_mut().unwrap().verification_pause.take())
    {
        entered
            .send(())
            .expect("independent verifier awaits the claimed event");
        resume
            .recv_timeout(Duration::from_secs(5))
            .expect("independent verifier completed");
    }
    if fault == Fault::Quiet {
        return;
    }
    if let Some(endpoint) = endpoint {
        // The factory owns the marker even though rejection prevents its first poll.
        let child = Marker("child");
        let _reply = endpoint
            .demand(move || {
                poll_fn(move |_| -> Poll<RunResult<()>> {
                    let _child = &child;
                    panic!("a child queued by a rejected verification event must not execute")
                })
            })
            .expect("the event retains an active task");
        if fault == Fault::ChildRefuse {
            attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Allowance);
        }
    } else {
        assert_eq!(
            fault,
            Fault::ChildPanic,
            "legacy events cannot queue children"
        );
    }
    if fault == Fault::ChildPanic {
        panic_any(PanicMarker(identity));
    }
}

#[crate::db]
#[derive(Clone)]
struct HookDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for HookDb {}

impl Default for HookDb {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(|event| {
                boundaries::event(&event.kind);
                match event.kind {
                    EventKind::DidValidateMemoizedValue { database_key } => {
                        verification_event(database_key);
                    }
                    EventKind::WillCheckCancellation => STATE.with_borrow_mut(|state| {
                        if let Some(state) = state {
                            state.cancellations += 1;
                        }
                    }),
                    _ => {}
                }
            }))),
        }
    }
}

#[crate::input]
struct Call {
    #[returns(copy)]
    target: Node,
    #[returns(copy)]
    decoy: Option<Node>,
    #[returns(copy)]
    path: Path,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn caller(db: &dyn Database, call: Call) -> u32 {
    match call.path(db) {
        Path::Interned => return boundaries::interned_query(db, call.target(db)),
        Path::TrackedField(input) => return boundaries::field_query(db, input),
        _ => {}
    }
    if let Some(decoy) = call.decoy(db) {
        assert_eq!(scalar(db, decoy), 0);
    }
    let node = call.target(db);
    if call.path(db).validates() {
        let key = if call.path(db) == Path::ClaimedShallow {
            cyclic::fn_ingredient_(db, db.zalsa()).database_key_index(node.as_id())
        } else {
            scalar::fn_ingredient_(db, db.zalsa()).database_key_index(node.as_id())
        };
        let revision = selected(db, key).verified_at;
        let unchanged = if call.path(db) == Path::ClaimedShallow {
            cyclic::fn_ingredient_(db, db.zalsa()).maybe_changed_after(db, node.as_id(), revision)
        } else {
            scalar::fn_ingredient_(db, db.zalsa()).maybe_changed_after(db, node.as_id(), revision)
        };
        assert!(unchanged.is_unchanged());
    }
    if call.path(db) == Path::ClaimedShallow {
        cyclic(db, node)
    } else {
        scalar(db, node)
    }
}

enum Request {
    Node(Node),
    Call(Call),
    VerifyOnly,
}

trait FixtureInput {
    fn request(self) -> Request;
}

impl FixtureInput for Node {
    fn request(self) -> Request {
        Request::Node(self)
    }
}

impl FixtureInput for Call {
    fn request(self) -> Request {
        Request::Call(self)
    }
}

struct Providers<A: Configuration> {
    target: Route<'static, A>,
    target_id: Id,
    revision: Revision,
}

impl<A, C> ExecutableRouteProvider<'static, 'static, C> for Providers<A>
where
    A: Configuration<DbView = dyn Database, Output<'static> = u32>,
    A::Input<'static>: FixtureInput,
    C: Configuration<DbView = dyn Database>,
    C::Input<'static>: FixtureInput,
    C::Output<'static>: TestOutput,
{
    // FixtureInput contains Node or Call handles; TestOutput contains u32 or Count.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        input: C::Input<'static>,
    ) -> RunResult<C::Output<'static>> {
        match input.request() {
            Request::Call(call) => {
                let _caller = Marker("caller");
                STATE.with_borrow_mut(|state| {
                    assert!(
                        state
                            .as_mut()
                            .unwrap()
                            .endpoint
                            .replace(context.endpoint().inner.clone())
                            .is_none()
                    );
                });
                boundaries::arm(context.endpoint().inner.clone());
                if let Some(decoy) = call.decoy(db) {
                    assert_eq!(*context.fetch_ref(&self.target, decoy.as_id())?.await?, 0);
                }
                if call.path(db).validates() && !boundaries::fetch_instead() {
                    assert!(
                        context
                            .validate(&self.target, self.target_id, self.revision)?
                            .await?
                            .is_unchanged()
                    );
                }
                Ok(<C::Output<'static> as TestOutput>::new(
                    *context.fetch_ref(&self.target, self.target_id)?.await?,
                ))
            }
            Request::VerifyOnly => panic!("the tracked-field reader must only validate"),
            Request::Node(node) => {
                assert!(
                    !<C::Output<'static> as TestOutput>::SCALAR,
                    "verification must not execute the target body"
                );
                STATE.with_borrow_mut(|state| state.as_mut().unwrap().leaf_bodies += 1);
                body(db, node, |_| Err(RunError::RequiresFetch))
            }
        }
    }

    async fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: C::Input<'static>,
    ) -> RunResult<C::Output<'static>> {
        Err(RunError::Contract(
            "verification requested an initial value",
        ))
    }

    async fn recover<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call C::Output<'static>,
        _value: C::Output<'static>,
        _input: C::Input<'static>,
    ) -> RunResult<C::Output<'static>>
    where
        'static: 'call,
    {
        Err(RunError::Contract("verification requested recovery"))
    }
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        STATE.with_borrow_mut(|state| state.as_mut().unwrap().admissions.push(work));
        boundaries::admit(work)
    }
}

static ADMISSION: Admission = Admission;

struct Fixture {
    db: &'static HookDb,
    path: Path,
    calls: [Call; 3],
    target: DatabaseKeyIndex,
    decoy: Option<DatabaseKeyIndex>,
    old: Selected,
    expected: u32,
}

impl Fixture {
    fn new(path: Path, with_decoy: bool) -> Self {
        let mut db = HookDb::default();
        let node = match path {
            Path::EagerFetch | Path::EagerValidate => {
                Node::builder(None, 4).durability(Durability::HIGH).new(&db)
            }
            Path::ClaimedShallow => {
                let head = Node::builder(None, 10)
                    .durability(Durability::HIGH)
                    .new(&db);
                let participant = Node::builder(Some(head), 20)
                    .durability(Durability::HIGH)
                    .new(&db);
                head.set_next(&mut db)
                    .with_durability(Durability::HIGH)
                    .to(Some(participant));
                assert_eq!(cyclic(&db, head), 10);
                participant
            }
            Path::Deep | Path::Interned => {
                let leaf = Node::new(&db, None, 4);
                Node::new(&db, Some(leaf), 0)
            }
            Path::TrackedField(_) => {
                panic!("tracked-field fixtures retain their producer separately")
            }
        };
        let decoy =
            with_decoy.then(|| Node::builder(None, 8).durability(Durability::HIGH).new(&db));
        if let Some(decoy) = decoy {
            assert_eq!(scalar(&db, decoy), 0);
        }
        let calls = std::array::from_fn(|_| Call::new(&db, node, decoy, path));
        let (target, expected) = if path == Path::ClaimedShallow {
            let ingredient = cyclic::fn_ingredient_(&db, db.zalsa());
            let memo = ingredient
                .get_memo_from_table_for(
                    db.zalsa(),
                    node.as_id(),
                    ingredient.memo_ingredient_index(db.zalsa(), node.as_id()),
                )
                .unwrap();
            assert!(memo.header.may_be_provisional());
            (
                ingredient.database_key_index(node.as_id()),
                *memo.value().unwrap(),
            )
        } else if path == Path::Interned {
            let expected = boundaries::interned_query(&db, node);
            (boundaries::interned_key(&db, node), expected)
        } else {
            let expected = scalar(&db, node);
            (
                scalar::fn_ingredient_(&db, db.zalsa()).database_key_index(node.as_id()),
                expected,
            )
        };
        let old = selected(&db, target);
        let decoy = decoy
            .map(|node| scalar::fn_ingredient_(&db, db.zalsa()).database_key_index(node.as_id()));
        if matches!(path, Path::Deep | Path::Interned) {
            node.next(&db).unwrap().set_value(&mut db).to(6);
        } else {
            db.synthetic_write(Durability::LOW);
        }
        assert!(old.verified_at < db.zalsa().current_revision());
        // These fixtures give TLS endpoints their actual lifetime without erasing a borrow.
        Self {
            db: Box::leak(Box::new(db)),
            path,
            calls,
            target,
            decoy,
            old,
            expected,
        }
    }

    fn caller_key(&self, index: usize) -> DatabaseKeyIndex {
        caller::fn_ingredient_(self.db, self.db.zalsa())
            .database_key_index(self.calls[index].as_id())
    }

    fn run(&self, index: usize) -> RunResult<u32> {
        match self.path {
            Path::ClaimedShallow => {
                self.run_with(index, cyclic::fn_ingredient_(self.db, self.db.zalsa()))
            }
            Path::Interned => boundaries::run_interned(self, index),
            Path::TrackedField(_) => boundaries::run_field(self, index),
            _ => self.run_with(index, scalar::fn_ingredient_(self.db, self.db.zalsa())),
        }
    }

    fn run_with<A>(&self, index: usize, ingredient: &'static IngredientImpl<A>) -> RunResult<u32>
    where
        A: Configuration<DbView = dyn Database, Output<'static> = u32>,
        A::Input<'static>: FixtureInput,
    {
        let db = self.db as &dyn Database;
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let target = registry.reserve(db, ingredient)?;
        let leaf = if boundaries::binding() == boundaries::Binding::Missing {
            RegistryBuilder::new(db, &ADMISSION)?
                .reserve(db, wrapped::fn_ingredient_(db, db.zalsa()))?
        } else {
            registry.reserve(db, wrapped::fn_ingredient_(db, db.zalsa()))?
        };
        let outer = registry.reserve(db, caller::fn_ingredient_(db, db.zalsa()))?;
        let provider: &'static _ = Box::leak(Box::new(Providers {
            target,
            target_id: self.target.key_index(),
            revision: self.old.verified_at,
        }));
        let binding = registry.provider(provider)?;
        registry.bind_executable(&provider.target, &binding)?;
        match boundaries::binding() {
            boundaries::Binding::Executable => registry.bind_executable(&leaf, &binding)?,
            boundaries::Binding::Missing => {}
            boundaries::Binding::Proof => registry.bind(&leaf, &binding)?,
        }
        registry.bind_executable(&outer, &binding)?;
        let call = self.calls[index];
        let root = boundaries::root();
        registry.seal()?.run(move |endpoint| async move {
            if root {
                let _root = Marker("root");
                boundaries::arm(endpoint.inner.clone());
                let context = endpoint.provider(binding)?;
                if call.path(db).validates() {
                    assert!(
                        context
                            .validate(&provider.target, provider.target_id, provider.revision)?
                            .await?
                            .is_unchanged()
                    );
                }
                Ok(*context
                    .fetch_ref(&provider.target, provider.target_id)?
                    .await?)
            } else {
                Ok(*endpoint
                    .provider(binding)?
                    .fetch_ref(&outer, call.as_id())?
                    .await?)
            }
        })
    }

    fn reset(&self, index: usize, fault: Fault) {
        STATE.with_borrow_mut(|state| {
            let state = state.as_mut().unwrap();
            state.caller = self.caller_key(index);
            state.fault = fault;
            state.endpoint = None;
            state.fired = false;
            state.observations.clear();
            state.events.clear();
            state.admissions.clear();
            state.cancellations = 0;
        });
    }

    fn assert_idle(&self, index: usize) {
        assert!(self.db.zalsa_local().active_query().is_none());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(!claimed(self.db, self.target));
        assert!(!claimed(self.db, self.caller_key(index)));
    }
}

fn check_run(fixture: &Fixture, index: usize, fault: Fault, old: bool) {
    fixture.reset(index, fault);
    let stamp = Stamp::current(fixture.db);
    let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
    let mut returned = None;
    let ((outcome, observations), trace) = validation_trace::collect(|| {
        observation::collect(|| {
            catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(fixture.db, 100_000, || {
                    let result = fixture.run(index);
                    returned = Some(result);
                    result
                })
            }))
        })
    });
    assert_eq!(Stamp::current(fixture.db), stamp);
    fixture.assert_idle(index);
    let now = selected(fixture.db, fixture.target);
    assert_eq!(now.address, fixture.old.address);
    assert_eq!(now.changed_at, fixture.old.changed_at);
    // Accepted-head finality is independent of the later verification-stamp write.
    assert!(!now.provisional);
    assert_eq!(
        now.verified_at,
        if fault == Fault::Quiet {
            fixture.db.zalsa().current_revision()
        } else {
            fixture.old.verified_at
        },
        "a rejected verification event must not publish its stamp",
    );
    assert_eq!(
        returned,
        match fault {
            Fault::Quiet => Some(Ok(fixture.expected)),
            Fault::ChildOnly => Some(Err(RunError::Contract("completed task retained a child"))),
            Fault::ChildRefuse => Some(Err(RunError::Refused(Incomplete::Allowance))),
            Fault::ChildPanic => None,
        },
    );
    match fault {
        Fault::Quiet => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Complete(Ok(fixture.expected)))
        ),
        Fault::ChildOnly => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        ),
        Fault::ChildRefuse => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        ),
        Fault::ChildPanic => {
            let payload = outcome.expect_err("verification preserves the native panic");
            let marker = payload
                .downcast_ref::<PanicMarker>()
                .expect("original payload type");
            assert!(Arc::ptr_eq(&marker.0, &identity));
        }
    }
    let target_claims: Vec<_> = observations
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Claim { key, serial, .. } if *key == fixture.target => Some(*serial),
            _ => None,
        })
        .collect();
    assert!(
        !observations
            .events
            .iter()
            .any(|event| matches!(event, Event::Execute {key, ..} if *key == fixture.target))
    );
    let holds_claim =
        old && fixture.path.claimed() && !(index != 0 && fixture.path == Path::ClaimedShallow);
    assert_eq!(target_claims.len(), usize::from(holds_claim), "{trace:#?}");
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert_eq!(
            state.events.iter().filter(|key| **key == fixture.target).count(),
            usize::from(old),
        );
        let stages: Vec<_> = state.observations.iter().map(|event| event.stage).collect();
        let expected_stages = if !old {
            vec!["caller"]
        } else if fault == Fault::Quiet {
            vec!["event", "caller"]
        } else {
            vec!["event", "child", "caller"]
        };
        assert_eq!(stages, expected_stages);
        for event in state.observations.iter().filter(|event| event.stage != "caller") {
            assert_eq!(event.selected.address, fixture.old.address, "{event:?}");
            assert_eq!(event.selected.verified_at, fixture.old.verified_at, "{event:?}");
            assert_eq!(event.target_claimed, holds_claim, "{event:?}");
            assert_eq!(event.caller, Some(fixture.caller_key(index)), "{event:?}");
            assert!(event.caller_claimed && event.depths.0 > 0, "{event:?}");
            let reason = match (event.stage, fault) {
                ("child", Fault::ChildOnly) => Some(Incomplete::Interrupted),
                ("child", Fault::ChildRefuse) => Some(Incomplete::Allowance),
                _ => None,
            };
            assert_eq!(event.reason, reason, "{event:?}");
            assert_eq!(
                event.panicking,
                event.stage == "child" && fault == Fault::ChildPanic,
                "{event:?}",
            );
        }
        assert_eq!(state.leaf_bodies, usize::from(fixture.path == Path::Deep));
        if index == 0 && fault == Fault::Quiet {
            let mut counts = [0; 4];
            let mut bytes = [0; 2];
            for admission in &state.admissions {
                match admission {
                    ExecutionWork::Task { requested_bytes } => {
                        counts[0] += 1;
                        bytes[0] += *requested_bytes;
                    }
                    ExecutionWork::Resource { requested_bytes } => {
                        counts[1] += 1;
                        bytes[1] += *requested_bytes;
                    }
                    ExecutionWork::Work { units } => counts[2] += *units,
                    ExecutionWork::Poll => counts[3] += 1,
                }
            }
            eprintln!("VERIFICATION_EVENT {:?}: Task={} Resource={} Work={} Poll={} TaskBytes={} ResourceBytes={} CancellationCallbacks={}", fixture.path, counts[0], counts[1], counts[2], counts[3], bytes[0], bytes[1], state.cancellations);
        }
    });
    if holds_claim {
        let expected_phase = if fixture.path == Path::Deep {
            "commit"
        } else {
            "shallow"
        };
        assert!(trace.iter().any(|event| matches!(event,
            TraceEvent::Verifier { phase, key, claim, .. }
                if *key == fixture.target && *claim == target_claims[0] && *phase == expected_phase
        )), "{expected_phase}: {trace:#?}");
        let published_phase = if fixture.path == Path::Deep {
            "commit.published"
        } else {
            "shallow.updated"
        };
        assert_eq!(
            trace.iter().any(|event| matches!(event,
                TraceEvent::Verifier { phase, key, verified_at, .. }
                    if *key == fixture.target && *phase == published_phase
                        && *verified_at == fixture.db.zalsa().current_revision()
            )),
            fault == Fault::Quiet,
            "{trace:#?}"
        );
    } else if old {
        let expected_phase = if fixture.path == Path::EagerFetch {
            "fetch.verification.event"
        } else {
            "validation.verification.event"
        };
        // Eager phase tracing occurs after callback acceptance; the hook above observes entry.
        assert_eq!(
            trace.iter().any(|event| matches!(event,
                TraceEvent::Outer { phase, key: Some(key), .. }
                    if *key == fixture.target && *phase == expected_phase
            )),
            fault == Fault::Quiet,
            "{expected_phase}: {trace:#?}"
        );
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn independent_eager_verification_can_publish_while_a_claimed_verifier_rejects() {
    let fixture = Fixture::new(Path::ClaimedShallow, false);
    let calls = fixture.calls;
    let target = fixture.target;
    let old = fixture.old;
    assert_eq!(fixture.expected, 20);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let (left, right) = run_pair(fixture.db,
        move |db, participant| {
            let fixture = Fixture { db: Box::leak(Box::new(db)), path: Path::ClaimedShallow, calls, target, decoy: None, old, expected: 20 };
            let _clear = begin(&fixture);
            fixture.reset(0, Fault::ChildRefuse);
            STATE.with_borrow_mut(|state| state.as_mut().unwrap().verification_pause = Some((entered_tx, resume_rx)));
            let ((outcome, ownership), trace) = validation_trace::collect(|| observation::collect(|| {
                participant.run(fixture.db, 100_000, || {
                    assert_eq!(fixture.run(0), Err(RunError::Refused(Incomplete::Allowance)));
                })
            }));
            assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
            fixture.assert_idle(0);
            let now = selected(fixture.db, target);
            assert_eq!(now.address, old.address);
            assert_eq!(now.changed_at, old.changed_at);
            assert!(!now.provisional);
            assert_eq!(now.verified_at, fixture.db.zalsa().current_revision());
            assert_eq!(ownership.events.iter().filter(|event| matches!(event, Event::Claim { key, .. } if *key == target)).count(), 1);
            assert!(!ownership.events.iter().any(|event| matches!(event, Event::Execute { key, .. } if *key == target)));
            assert!(trace.iter().any(|event| matches!(event, TraceEvent::Verifier { phase: "shallow", key, .. } if *key == target)));
            assert!(!trace.iter().any(|event| matches!(event, TraceEvent::Verifier { phase: "shallow.updated", key, .. } if *key == target)));
            STATE.with_borrow(|state| {
                let state = state.as_ref().unwrap();
                let entry = state.observations.iter().find(|event| event.stage == "event").unwrap();
                assert!(entry.target_claimed && !entry.selected.provisional);
                assert_eq!(entry.selected.verified_at, old.verified_at);
                let cleanup = state.observations.iter().find(|event| event.stage == "child").unwrap();
                assert_eq!(cleanup.selected.address, old.address);
                assert_eq!(cleanup.selected.verified_at, now.verified_at);
                assert_eq!(state.leaf_bodies, 0);
            });
        },
        move |db, participant| {
            let fixture = Fixture { db: Box::leak(Box::new(db)), path: Path::ClaimedShallow, calls, target, decoy: None, old, expected: 20 };
            let _clear = begin(&fixture);
            fixture.reset(1, Fault::Quiet);
            let ((outcome, ownership), trace) = validation_trace::collect(|| observation::collect(|| {
                participant.run(fixture.db, 100_000, || {
                    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    let selected_before = selected(fixture.db, target);
                    assert_eq!(selected_before.address, old.address);
                    assert_eq!(selected_before.verified_at, old.verified_at);
                    assert!(!selected_before.provisional);
                    assert_eq!(fixture.run(1), Ok(20));
                    let published = selected(fixture.db, target);
                    assert_eq!(published.address, old.address);
                    assert_eq!(published.verified_at, fixture.db.zalsa().current_revision());
                    assert!(published.verified_at > old.verified_at);
                    resume_tx.send(()).unwrap();
                })
            }));
            assert_eq!(outcome, Ok(AttemptOutcome::Complete(())));
            fixture.assert_idle(1);
            assert!(!ownership.events.iter().any(|event| matches!(event, Event::Claim { key, .. } | Event::Execute { key, .. } if *key == target)));
            assert!(trace.iter().any(|event| matches!(event, TraceEvent::Outer { phase: "validation.verification.event", key: Some(key), .. } if *key == target)));
            STATE.with_borrow(|state| assert_eq!(state.as_ref().unwrap().leaf_bodies, 0));
        },
    ).unwrap();
    left.unwrap();
    right.unwrap();
    assert_eq!(selected(fixture.db, target).address, old.address);
    assert_eq!(cyclic(fixture.db, calls[0].target(fixture.db)), 20);
}

fn legacy(path: Path) {
    for fault in [Fault::Quiet, Fault::ChildPanic] {
        if fault == Fault::ChildPanic && cfg!(feature = "shuttle") {
            continue;
        }
        let fixture = Fixture::new(path, false);
        let _clear = begin(&fixture);
        fixture.reset(0, fault);
        let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
        let outcome = catch_unwind(AssertUnwindSafe(|| caller(fixture.db, fixture.calls[0])));
        if fault == Fault::Quiet {
            assert_eq!(outcome.unwrap(), fixture.expected);
            assert_eq!(
                selected(fixture.db, fixture.target).verified_at,
                fixture.db.zalsa().current_revision()
            );
        } else {
            let payload = outcome.unwrap_err();
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<PanicMarker>().unwrap().0,
                &identity
            ));
            assert_eq!(
                selected(fixture.db, fixture.target).verified_at,
                fixture.old.verified_at
            );
        }
        STATE.with_borrow(|state| {
            let state = state.as_ref().unwrap();
            assert_eq!(
                state
                    .events
                    .iter()
                    .filter(|key| **key == fixture.target)
                    .count(),
                1
            );
            assert_eq!(state.observations.len(), 1);
            assert_eq!(
                state.observations[0].selected.verified_at,
                fixture.old.verified_at
            );
            assert_eq!(state.observations[0].target_claimed, path.claimed());
        });
        fixture.assert_idle(0);
    }
}

fn check_path(path: Path) {
    legacy(path);
    for fault in faults() {
        let fixture = Fixture::new(path, false);
        let _clear = begin(&fixture);
        check_run(&fixture, 0, fault, true);
        check_run(&fixture, 1, Fault::Quiet, fault != Fault::Quiet);
        check_run(&fixture, 2, Fault::Quiet, false);
    }
}

#[test]
fn eager_fetch_verification_event_precedes_stamp() {
    check_path(Path::EagerFetch);
}

#[test]
fn eager_validation_event_precedes_stamp() {
    check_path(Path::EagerValidate);
}

#[test]
fn claimed_shallow_event_keeps_the_real_claim() {
    check_path(Path::ClaimedShallow);
}

#[test]
fn deep_verification_event_keeps_the_real_claim() {
    check_path(Path::Deep);
}

#[test]
fn verification_event_matches_exact_key_and_skips_current_memo() {
    let fixture = Fixture::new(Path::EagerFetch, true);
    let _clear = begin(&fixture);
    check_run(&fixture, 0, Fault::ChildOnly, true);
    STATE.with_borrow(|state| {
        assert_eq!(
            state.as_ref().unwrap().events,
            [fixture.decoy.unwrap(), fixture.target]
        )
    });
    check_run(&fixture, 1, Fault::Quiet, true);
    check_run(&fixture, 2, Fault::Quiet, false);
}

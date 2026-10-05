use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::rc::Rc;

use super::super::super::key_run::{key_layout_for_tests, key_preparation_layout_for_tests};
use super::super::super::registration::{
    CallableRoute, CallableRouteProvider, ExecutableRouteProvider, NativeValueOperation,
    NativeValueQuote, PassiveMemoProfile, ProviderBinding, ProviderContext, QueryKeyProfile,
    QueryKeys, RegistryBuilder, RetainedInput, Route, TaskEndpoint,
};
use super::super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::{Collision, KeyEvent, Request, revisions, stored};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::{ClaimResult, Configuration, InternedQueryConfiguration, Reentrancy};
use crate::ingredient::Ingredient;
use crate::plumbing::{AsId, QuoteError, QuoteFuel};
use crate::prepared_source_probe::Stamp;
use crate::table::memo::detached_observation;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id, Setter};

#[derive(Default)]
struct State {
    // Payload tags distinguish setup (0), old storage (1), submitted input (2), and clones (3).
    // Setup and callback payloads use tag 0 to exclude their activity from live/drop counts.
    tag: Cell<usize>,
    in_key: Cell<bool>,
    ordinary: Cell<usize>,
    leaves: Cell<usize>,
    clones: Cell<usize>,
    reentrant_clones: Cell<usize>,
    hashes: [Cell<usize>; 4],
    live: [Cell<usize>; 4],
    drops: [Cell<usize>; 4],
    drop_order: [Cell<usize>; 4],
    drop_reason: [Cell<Option<Incomplete>>; 4],
    detached_at_drop: [Cell<usize>; 4],
    sequence: Cell<usize>,
    child_order: Cell<usize>,
    caller_order: Cell<usize>,
    delivered: RefCell<Vec<Id>>,
    events: RefCell<Vec<(KeyEvent, DatabaseKeyIndex)>>,
    work: RefCell<Vec<ExecutionWork>>,
    refused_work: Cell<Option<ExecutionWork>>,
    request_bytes: Cell<usize>,
    preparation_bytes: Cell<usize>,
    hook: RefCell<Option<Rc<Hook>>>,
}

impl State {
    fn tick(&self) -> usize {
        let next = self.sequence.get() + 1;
        self.sequence.set(next);
        next
    }
}

thread_local! {
    static STATE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

fn state() -> Rc<State> {
    STATE.with_borrow(|slot| slot.as_ref().unwrap().clone())
}

fn fixture() -> (Rc<State>, impl Drop) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            if let Some(state) = STATE.with_borrow_mut(Option::take)
                && let Some(hook) = state.hook.borrow_mut().take()
            {
                hook.endpoint.borrow_mut().take();
            }
        }
    }
    let state = Rc::new(State::default());
    STATE.with_borrow_mut(|slot| assert!(slot.replace(state.clone()).is_none()));
    (state, Reset)
}

#[derive(Debug, crate::SalsaValue)]
struct OwnedBytes {
    bytes: Box<[u8]>,
    tag: usize,
}

impl OwnedBytes {
    fn new(len: usize, tag: usize) -> Self {
        if tag != 0 {
            state().live[tag].set(state().live[tag].get() + 1);
        }
        Self {
            bytes: vec![b'x'; len].into_boxed_slice(),
            tag,
        }
    }
}

impl Clone for OwnedBytes {
    fn clone(&self) -> Self {
        state().clones.set(state().clones.get() + 1);
        state().live[3].set(state().live[3].get() + 1);
        Self {
            bytes: self.bytes.clone(),
            tag: 3,
        }
    }
}

impl PartialEq for OwnedBytes {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for OwnedBytes {}

impl Hash for OwnedBytes {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        state().hashes[self.tag].set(state().hashes[self.tag].get() + 1);
        // Collisions put every payload in one reuse queue while equality still compares bytes.
        hasher.write_u32(0);
    }
}

impl Drop for OwnedBytes {
    fn drop(&mut self) {
        if self.tag == 0 {
            return;
        }
        // Observation neither allocates nor calls into the database during passive cleanup.
        let _ = STATE.try_with(|slot| {
            if let Some(state) = slot.borrow().as_ref() {
                state.live[self.tag].set(state.live[self.tag].get() - 1);
                state.drops[self.tag].set(state.drops[self.tag].get() + 1);
                state.drop_order[self.tag].set(state.tick());
                state.drop_reason[self.tag]
                    .set(attempt_probe::current().and_then(|current| current.reason()));
                if let Some(hook) = state.hook.borrow().as_ref() {
                    state.detached_at_drop[self.tag].set(hook.detached.live.get());
                }
            }
        });
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn owned_value(_db: &dyn Database, _salt: Collision, bytes: OwnedBytes) -> bool {
    state().ordinary.set(state().ordinary.get() + 1);
    !bytes.bytes.is_empty()
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn owned_caller(db: &dyn Database, request: Request) -> bool {
    owned_value(
        db,
        Collision(request.left(db)),
        OwnedBytes::new(request.right(db) as usize, state().tag.get()),
    )
}

struct Profile<const REJECT_OUTPUT: bool>;

impl<C, const REJECT_OUTPUT: bool> PassiveMemoProfile<C> for Profile<REJECT_OUTPUT>
where
    C: for<'db> Configuration<Output<'db> = bool>,
{
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        (!REJECT_OUTPUT).then_some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

// Hash and equality inspect finite native data; field cleanup frees a byte buffer and updates
// scalar observations. The quote covers submitted and retained buffers without semantic calls.
impl<C, const REJECT_OUTPUT: bool> QueryKeyProfile<C> for Profile<REJECT_OUTPUT>
where
    C: InternedQueryConfiguration
        + for<'db> crate::interned::Configuration<Fields<'db> = (Collision, OwnedBytes)>
        + for<'db> Configuration<Output<'db> = bool>,
{
    fn input_work<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        input.1.bytes.len().checked_add(1)
    }

    fn input_work_bounded<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

#[crate::db]
struct OwnedDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for OwnedDb {}

impl OwnedDb {
    fn new() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(|event| {
                let event = match event.kind {
                    EventKind::DidInternValue { key, .. } => (KeyEvent::Intern, key),
                    EventKind::DidValidateInternedValue { key, .. } => (KeyEvent::Validate, key),
                    EventKind::DidReuseInternedValue { key, .. } => (KeyEvent::Reuse, key),
                    EventKind::DidDiscard { key } => (KeyEvent::Discard, key),
                    _ => return,
                };
                state().events.borrow_mut().push(event);
                let hook = state().hook.borrow().clone();
                if let Some(hook) = hook {
                    hook.event(event);
                }
            }))),
        }
    }
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let state = state();
        if !state.in_key.get() {
            return Ok(());
        }
        state.work.borrow_mut().push(work);
        let hook = state.hook.borrow().clone();
        if let Some(hook) = hook
            && !hook.fired.get()
            && match hook.fault {
                Fault::RequestWork => matches!(work, ExecutionWork::Work { .. }),
                Fault::RequestBytes => {
                    matches!(work, ExecutionWork::Resource { requested_bytes } if requested_bytes != 0)
                }
                Fault::PreparationBytes => {
                    matches!(work, ExecutionWork::Resource { requested_bytes } if requested_bytes == state.preparation_bytes.get())
                        && state.hashes[2].get() != 0
                }
                Fault::Retirement => {
                    matches!(work, ExecutionWork::Work { units } if units == OLD_BUFFER_LEN + 1)
                }
                _ => false,
            }
        {
            state.refused_work.set(Some(work));
            hook.fail();
            return Err(RunError::Refused(Incomplete::Interrupted));
        }
        Ok(())
    }
}

static ADMISSION: Admission = Admission;

struct Marker(&'static str);

impl Drop for Marker {
    fn drop(&mut self) {
        let state = state();
        match self.0 {
            "child" => state.child_order.set(state.tick()),
            "caller" => state.caller_order.set(state.tick()),
            _ => {}
        }
        let hook = state.hook.borrow().clone();
        if let Some(hook) = hook {
            hook.note(self.0);
        }
    }
}

struct KeyScope;

impl Drop for KeyScope {
    fn drop(&mut self) {
        state().in_key.set(false);
    }
}

fn native_value_quote<'db, C>(
    operation: NativeValueOperation<'_, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: for<'a> Configuration<Input<'a> = (Collision, OwnedBytes)>,
{
    match operation {
        NativeValueOperation::InputConversion(RetainedInput::Interned((_, bytes))) => {
            // Generated conversion clones the owned buffer; its later drop is prepaid here.
            Ok(NativeValueQuote {
                work: bytes
                    .bytes
                    .len()
                    .checked_add(2)
                    .ok_or(RunError::Contract("owned fixture size overflow"))?,
                requested_bytes: bytes
                    .bytes
                    .len()
                    .checked_add(size_of::<(Collision, OwnedBytes)>())
                    .ok_or(RunError::Contract("owned fixture size overflow"))?,
                cleanup_work: 1,
            })
        }
        NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
            Err(RunError::Contract("owned query requires retained fields"))
        }
        NativeValueOperation::Comparison { .. } => Ok(NativeValueQuote {
            work: 1,
            requested_bytes: 0,
            cleanup_work: 0,
        }),
    }
}

struct Leaf;

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Leaf
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Collision, OwnedBytes),
            Output<'a> = bool,
        >,
{
    async fn native_value<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_value_quote(operation)
    }

    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        input: (Collision, OwnedBytes),
    ) -> RunResult<bool> {
        state().leaves.set(state().leaves.get() + 1);
        Ok(!input.1.bytes.is_empty())
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Collision, OwnedBytes),
    ) -> RunResult<bool> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: (Collision, OwnedBytes),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Leaf
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Collision, OwnedBytes),
            Output<'a> = bool,
        >,
{
    async fn native_value<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_value_quote(operation)
    }

    async fn body<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        input: (Collision, OwnedBytes),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        state().leaves.set(state().leaves.get() + 1);
        Ok(!input.1.bytes.is_empty())
    }

    async fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Collision, OwnedBytes),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: (Collision, OwnedBytes),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

async fn intern_owned<'call, 'run: 'call, 'db: 'run, C, P>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    db: &'db dyn Database,
    input: Request,
    keys: &'call QueryKeys<'db, C, P>,
) -> Id
where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, OwnedBytes)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = bool>,
    P: QueryKeyProfile<C> + 'call,
{
    // Admit the allocation and its passive cleanup before passing ownership to the key API.
    let fields = endpoint
        .local_call(|| {
            let len = input.right(db) as usize;
            let work = len
                .checked_add(2)
                .ok_or(RunError::Contract("owned fixture size overflow"))?;
            endpoint.admit_work(work)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: len
                    .checked_add(size_of::<(Collision, OwnedBytes)>())
                    .ok_or(RunError::Contract("owned fixture size overflow"))?,
            })?;
            Ok((
                Collision(input.left(db)),
                OwnedBytes::new(len, state().tag.get()),
            ))
        })
        .await;
    let clones = state().clones.get() - state().reentrant_clones.get();
    let id = {
        assert!(!state().in_key.replace(true));
        let _scope = KeyScope;
        endpoint.intern_query_key(keys, fields).await
    };
    assert_eq!(
        state().clones.get() - state().reentrant_clones.get(),
        clones
    );
    state().delivered.borrow_mut().push(id);
    id
}

struct Caller<'run, 'db: 'run, C: InternedQueryConfiguration, const REJECT_OUTPUT: bool> {
    target: Route<'db, C>,
    keys: QueryKeys<'db, C, Profile<REJECT_OUTPUT>>,
    leaf: ProviderBinding<'run, Leaf>,
}

impl<'run, 'db: 'run, C, D, const REJECT_OUTPUT: bool> ExecutableRouteProvider<'run, 'db, D>
    for Caller<'run, 'db, C, REJECT_OUTPUT>
where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, OwnedBytes)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = bool>,
    D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = bool>,
{
    // Request conversion constructs a scalar handle; output equality compares bool.
    fixture_native_value!(executable, 'run, 'db, D, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        input: Request,
    ) -> RunResult<bool> {
        let _caller = Marker("caller");
        let endpoint = context.endpoint();
        let id = intern_owned(&endpoint, db, input, &self.keys).await;
        let leaf = endpoint.provider(self.leaf.clone())?;
        Ok(*endpoint
            .child_call(|| async { leaf.fetch_ref(&self.target, id)?.await })
            .await)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Request,
    ) -> RunResult<bool> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: Request,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct CallableCaller<'run, 'db: 'run, C: InternedQueryConfiguration> {
    target: CallableRoute<'run, 'db, C>,
    keys: QueryKeys<'db, C, Profile<false>>,
}

impl<'run, 'db: 'run, C, D> CallableRouteProvider<'run, 'db, D> for CallableCaller<'run, 'db, C>
where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, OwnedBytes)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = bool>,
    D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = bool>,
{
    // Request conversion constructs a scalar handle; output equality compares bool.
    fixture_native_value!(callable, 'run, 'db, D, 1);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Database,
        input: Request,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let _caller = Marker("caller");
        let id = intern_owned(&endpoint, db, input, &self.keys).await;
        Ok(*endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.target, id)?.await })
            .await)
    }

    async fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Request,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: Request,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn record_layout<C: InternedQueryConfiguration>(_ingredient: &crate::function::IngredientImpl<C>) {
    state().request_bytes.set(
        key_layout_for_tests::<C, Profile<false>>()
            .into_iter()
            .find(|(name, _, _)| *name == "KeyRequest")
            .unwrap()
            .1,
    );
    let preparation = key_preparation_layout_for_tests::<C>();
    let size = |name| preparation.iter().find(|entry| entry.0 == name).unwrap().1;
    state().preparation_bytes.set(
        size("Input")
            + size("Result")
            + size("PreparedIntern")
            + size("Option<Input>")
            + 2 * size("Option<PreparedPassiveIntern>")
            + size("PreparedPassiveIntern")
            + size_of::<Id>(),
    );
}

fn run<const REJECT_OUTPUT: bool>(db: &'static OwnedDb, request: Request) -> RunResult<bool> {
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let ingredient = owned_value::fn_ingredient_(db, db.zalsa());
    record_layout(ingredient);
    let target = registry.reserve(db as &dyn Database, ingredient)?;
    let keys = registry.query_keys::<_, Profile<REJECT_OUTPUT>>(&target)?;
    let leaf = registry.provider(&Leaf)?;
    registry.bind_executable(&target, &leaf)?;
    let caller = registry.reserve(
        db as &dyn Database,
        owned_caller::fn_ingredient_(db, db.zalsa()),
    )?;
    let provider: &'static _ = Box::leak(Box::new(Caller { target, keys, leaf }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&caller, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = Marker("root");
        if let Some(hook) = state().hook.borrow().as_ref() {
            *hook.endpoint.borrow_mut() = Some(endpoint.clone());
        }
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&caller, request.as_id())?
            .await?)
    })
}

fn run_callable(db: &'static OwnedDb, request: Request) -> RunResult<bool> {
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let ingredient = owned_value::fn_ingredient_(db, db.zalsa());
    record_layout(ingredient);
    let target = registry.reserve_callable(db as &dyn Database, ingredient)?;
    let keys = registry.callable_query_keys::<_, Profile<false>>(&target)?;
    registry.bind_callable(&target, Leaf)?;
    let caller = registry.reserve_callable(
        db as &dyn Database,
        owned_caller::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.bind_callable(&caller, CallableCaller { target, keys })?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = Marker("root");
        if let Some(hook) = state().hook.borrow().as_ref() {
            *hook.endpoint.borrow_mut() = Some(endpoint.clone());
        }
        Ok(*endpoint.fetch_ref(&caller, request.as_id())?.await?)
    })
}

fn idle(db: &OwnedDb) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(!state().in_key.get());
    assert_eq!(state().live[3].get(), 0);
}

fn complete(db: &'static OwnedDb, request: Request) -> Id {
    complete_with(db, request, run::<false>)
}

fn complete_with(
    db: &'static OwnedDb,
    request: Request,
    run: fn(&'static OwnedDb, Request) -> RunResult<bool>,
) -> Id {
    assert_eq!(
        try_with_attempt(db, 100_000, || run(db, request)),
        Ok(AttemptOutcome::Complete(Ok(true)))
    );
    idle(db);
    *state().delivered.borrow().last().unwrap()
}

fn ids(db: &OwnedDb) -> Vec<Id> {
    owned_value::intern_ingredient_(db.zalsa())
        .entries(db.zalsa())
        .map(|entry| entry.key().key_index())
        .collect()
}

const OLD_BUFFER_LEN: usize = 65;

fn stale() -> (OwnedDb, Request, Id) {
    let mut db = OwnedDb::new();
    let request = Request::new(&db, 0, OLD_BUFFER_LEN as u32, None);
    let count = revisions(owned_value::fn_ingredient_(&db, db.zalsa()));
    let mut old = None;
    for index in 0..count {
        if index != 0 {
            request.set_left(&mut db).to(index as u32);
        }
        state().tag.set(usize::from(index == 0));
        assert!(owned_caller(&db, request));
        if index == 0 {
            old = Some(ids(&db)[0]);
        }
    }
    request.set_left(&mut db).to(count as u32);
    let request = Request::new(&db, count as u32, 1, None);
    (db, request, old.unwrap())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    RequestWork,
    RequestBytes,
    PreparationBytes,
    Intern,
    Validate,
    Retirement,
    Discard,
    Panic,
    Reenter,
    ReenterRefuse,
}

struct Snapshot {
    stage: &'static str,
    live: [usize; 4],
    detached: usize,
    frame: Option<DatabaseKeyIndex>,
    claimed: bool,
    reason: Option<Incomplete>,
}

struct Hook {
    db: &'static OwnedDb,
    request: Request,
    caller: DatabaseKeyIndex,
    old: Option<Id>,
    fault: Fault,
    endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
    fired: Cell<bool>,
    detached: Rc<detached_observation::State>,
    notes: RefCell<Vec<Snapshot>>,
    new_id: Cell<Option<Id>>,
    replacement_memo: Cell<Option<usize>>,
}

impl Hook {
    fn note(&self, stage: &'static str) {
        self.notes.borrow_mut().push(Snapshot {
            stage,
            live: std::array::from_fn(|index| state().live[index].get()),
            detached: self.detached.live.get(),
            frame: self.db.zalsa_local().active_query().map(|(key, _)| key),
            claimed: matches!(
                self.db
                    .zalsa()
                    .lookup_ingredient(self.caller.ingredient_index())
                    .as_function()
                    .unwrap()
                    .sync_table()
                    .peek_claim(self.db.zalsa(), self.caller.key_index(), Reentrancy::Deny),
                ClaimResult::Cycle { .. }
            ),
            reason: attempt_probe::current().and_then(|current| current.reason()),
        });
    }

    fn fail(&self) {
        self.fired.set(true);
        self.note("boundary");
        let endpoint = self.endpoint.borrow_mut().take().unwrap();
        let marker = Marker("child");
        let _reply = endpoint
            .demand::<(), _>(move || async move {
                let _marker = marker;
                panic!("refused owned key request polled its queued child");
            })
            .unwrap();
        if self.fault == Fault::Panic {
            panic_any("owned query key event");
        }
        attempt_probe::report_incomplete(self.db, Incomplete::Interrupted);
    }

    fn event(&self, event: (KeyEvent, DatabaseKeyIndex)) {
        if self.fired.get() {
            return;
        }
        let argument = owned_value::intern_ingredient_(self.db.zalsa());
        let selected = match self.fault {
            Fault::Intern => {
                event.0 == KeyEvent::Intern
                    && event.1.ingredient_index() == argument.ingredient_index()
            }
            Fault::Validate => {
                event.0 == KeyEvent::Validate
                    && self
                        .old
                        .is_some_and(|id| event.1 == argument.database_key_index(id))
            }
            Fault::Discard | Fault::Panic | Fault::Reenter | Fault::ReenterRefuse => {
                event.0 == KeyEvent::Discard
                    && self.old.is_some_and(|id| {
                        event.1
                            == owned_value::fn_ingredient_(self.db, self.db.zalsa())
                                .database_key_index(id)
                    })
            }
            _ => false,
        };
        if !selected {
            return;
        }
        self.fired.set(true);
        self.note("event");
        if matches!(self.fault, Fault::Reenter | Fault::ReenterRefuse) {
            let clones = state().clones.get();
            assert!(owned_value(
                self.db,
                Collision(self.request.left(self.db)),
                OwnedBytes::new(self.request.right(self.db) as usize, 0),
            ));
            state()
                .reentrant_clones
                .set(state().reentrant_clones.get() + state().clones.get() - clones);
            let new = replacement(self.db, self.old.unwrap());
            self.new_id.set(Some(new));
            self.replacement_memo.set(Some(
                std::ptr::from_ref(
                    stored(
                        self.db,
                        owned_value::fn_ingredient_(self.db, self.db.zalsa()),
                        new,
                    )
                    .unwrap(),
                )
                .addr(),
            ));
            if self.fault == Fault::Reenter {
                return;
            }
        }
        self.fail();
    }
}

fn install_hook(
    db: &'static OwnedDb,
    request: Request,
    old: Option<Id>,
    fault: Fault,
) -> (Rc<Hook>, impl Drop) {
    let detached = Rc::new(detached_observation::State::default());
    let observer = detached_observation::install(detached.clone());
    let hook = Rc::new(Hook {
        db,
        request,
        caller: owned_caller::fn_ingredient_(db, db.zalsa()).database_key_index(request.as_id()),
        old,
        fault,
        endpoint: RefCell::new(None),
        fired: Cell::new(false),
        detached,
        notes: RefCell::new(Vec::new()),
        new_id: Cell::new(None),
        replacement_memo: Cell::new(None),
    });
    *state().hook.borrow_mut() = Some(hook.clone());
    state().tag.set(2);
    state().events.borrow_mut().clear();
    (hook, observer)
}

fn reject(hook: &Hook) {
    reject_with(hook, run::<false>);
}

fn reject_with(hook: &Hook, run: fn(&'static OwnedDb, Request) -> RunResult<bool>) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(hook.db, 100_000, || run(hook.db, hook.request))
    }));
    if hook.fault == Fault::Panic {
        assert_eq!(
            *outcome.unwrap_err().downcast::<&str>().unwrap(),
            "owned query key event"
        );
    } else {
        assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
    }
    assert!(hook.fired.get());
    assert!(state().delivered.borrow().is_empty());
    assert!(
        stored(
            hook.db,
            owned_caller::fn_ingredient_(hook.db, hook.db.zalsa()),
            hook.request.as_id()
        )
        .is_none()
    );
    let notes = hook.notes.borrow();
    for stage in ["boundary", "child"] {
        let note = notes.iter().find(|note| note.stage == stage).unwrap();
        assert_eq!(note.frame, Some(hook.caller));
        assert!(note.claimed);
    }
    if hook.fault != Fault::Panic {
        assert_eq!(
            notes
                .iter()
                .find(|note| note.stage == "child")
                .unwrap()
                .reason,
            Some(Incomplete::Interrupted)
        );
    }
    idle(hook.db);
}

fn replacement(db: &OwnedDb, old: Id) -> Id {
    let new = ids(db)
        .into_iter()
        .find(|id| id.index() == old.index())
        .unwrap();
    assert_eq!(new.generation(), old.generation() + 1);
    new
}

fn stop_hook() {
    state().hook.borrow_mut().take();
    state().tag.set(0);
}

#[test]
fn owned_keys_share_canonical_identity_without_cloning_during_interning() {
    for execute in [run::<false> as fn(_, _) -> _, run_callable] {
        let (state, _reset) = fixture();
        let db = Box::leak(Box::new(OwnedDb::new()));
        state.tag.set(1);
        let first = Request::new(db, 0, 3, None);
        let id = complete_with(db, first, execute);
        let ingredient = owned_value::fn_ingredient_(db, db.zalsa());
        let memo = stored(db, ingredient, id).unwrap();
        assert_eq!(memo.value(), Some(&true));
        assert_eq!(state.clones.get(), 1);
        assert_eq!(state.drops[3].get(), 1);
        assert_eq!((state.live[1].get(), state.drops[1].get()), (1, 0));
        state.tag.set(2);
        assert_eq!(complete_with(db, Request::new(db, 0, 3, None), execute), id);
        assert_eq!(state.clones.get(), 1);
        assert_eq!((state.live[2].get(), state.drops[2].get()), (0, 1));
        assert_eq!(state.leaves.get(), 1);
        assert!(state.hashes[1].get() > 0 && state.hashes[2].get() > 0);
        assert_eq!(ids(db), [id]);
        assert!(std::ptr::eq(memo, stored(db, ingredient, id).unwrap()));
        assert!(owned_value(db, Collision(0), OwnedBytes::new(3, 0)));
        assert_eq!(state.ordinary.get(), 0);
        assert_eq!(state.clones.get(), 1);
        let caller_memo = stored(
            db,
            owned_caller::fn_ingredient_(db, db.zalsa()),
            first.as_id(),
        )
        .unwrap();
        let argument = owned_value::intern_ingredient_(db.zalsa()).database_key_index(id);
        assert!(
            caller_memo
                .header
                .origin()
                .inputs()
                .any(|key| key == argument)
        );
        assert!(state.work.borrow().iter().any(|work| matches!(work,
            ExecutionWork::Resource { requested_bytes } if *requested_bytes == state.request_bytes.get()
        )));
    }
}

#[test]
fn admission_refusal_retains_owned_input_before_canonical_mutation() {
    for execute in [run::<false> as fn(_, _) -> _, run_callable] {
        for fault in [
            Fault::RequestWork,
            Fault::RequestBytes,
            Fault::PreparationBytes,
        ] {
            let (state, _reset) = fixture();
            let db = Box::leak(Box::new(OwnedDb::new()));
            let request = Request::new(db, 0, 3, None);
            let (hook, _observer) = install_hook(db, request, None, fault);
            let stamp = Stamp::current(db);
            reject_with(&hook, execute);
            assert!(ids(db).is_empty());
            if fault == Fault::PreparationBytes {
                assert!(state.hashes[2].get() > 0);
            } else {
                assert_eq!(state.hashes[2].get(), 0);
            }
            assert_eq!(state.clones.get(), 0);
            assert_eq!(hook.detached.acquired.get(), 0);
            assert_eq!((state.live[2].get(), state.drops[2].get()), (0, 1));
            assert_eq!(
                hook.notes
                    .borrow()
                    .iter()
                    .find(|note| note.stage == "child")
                    .unwrap()
                    .live[2],
                1
            );
            assert!(state.child_order.get() < state.drop_order[2].get());
            assert!(state.drop_order[2].get() < state.caller_order.get());
            assert!(
                state
                    .work
                    .borrow()
                    .iter()
                    .any(|work| matches!(work, ExecutionWork::Work { units } if *units > 0))
            );
            if fault == Fault::RequestBytes {
                assert!(
                    matches!(state.work.borrow().iter().find(|work| matches!(work, ExecutionWork::Resource { .. })), Some(ExecutionWork::Resource { requested_bytes }) if *requested_bytes == state.request_bytes.get())
                );
            }
            if fault == Fault::PreparationBytes {
                let work = state.work.borrow();
                assert!(matches!(
                    state.refused_work.get(),
                    Some(ExecutionWork::Resource { requested_bytes })
                        if requested_bytes == state.preparation_bytes.get()
                ));
                assert!(
                    work.iter()
                        .filter(|work| matches!(
                            work,
                            ExecutionWork::Resource { requested_bytes } if *requested_bytes != 0
                        ))
                        .count()
                        > 2
                );
            }
            stop_hook();
            complete_with(db, request, execute);
            assert_eq!(Stamp::current(db), stamp);
        }
    }
}

#[test]
fn unused_hit_input_survives_validation_refusal_and_retry() {
    for execute in [run::<false> as fn(_, _) -> _, run_callable] {
        let (state, _reset) = fixture();
        let mut db = OwnedDb::new();
        let old_request = Request::new(&db, 0, 3, None);
        state.tag.set(1);
        assert!(owned_caller(&db, old_request));
        let old = ids(&db)[0];
        old_request.set_left(&mut db).to(1);
        let request = Request::new(&db, 0, 3, None);
        let db = Box::leak(Box::new(db));
        let (hook, _observer) = install_hook(db, request, Some(old), Fault::Validate);
        let stamp = Stamp::current(db);
        let clones = state.clones.get();
        reject_with(&hook, execute);
        assert_eq!(state.clones.get(), clones);
        assert_eq!(ids(db), [old]);
        let notes = hook.notes.borrow();
        let child = notes.iter().find(|note| note.stage == "child").unwrap();
        assert_eq!((child.live[1], child.live[2], child.detached), (1, 1, 0));
        drop(notes);
        assert_eq!((state.live[2].get(), state.drops[2].get()), (0, 1));
        assert_eq!(state.drop_reason[2].get(), Some(Incomplete::Interrupted));
        assert!(state.child_order.get() < state.drop_order[2].get());
        assert!(state.drop_order[2].get() < state.caller_order.get());
        stop_hook();
        assert_eq!(complete_with(db, request, execute), old);
        assert_eq!(Stamp::current(db), stamp);
    }
}

#[test]
fn rejected_reuse_restores_owned_input_and_preserves_the_old_memo() {
    let (state, _reset) = fixture();
    let (db, request, old) = stale();
    let db = Box::leak(Box::new(db));
    let ingredient = owned_value::fn_ingredient_(db, db.zalsa());
    let pointer = std::ptr::from_ref(stored(db, ingredient, old).unwrap()).addr();
    let before = ids(db);
    let stamp = Stamp::current(db);
    let detached = Rc::new(detached_observation::State::default());
    let _observer = detached_observation::install(detached.clone());
    let clones = state.clones.get();
    state.tag.set(2);
    for count in 1..=2 {
        assert_eq!(
            try_with_attempt(db, 100_000, || {
                let result = run::<true>(db, request);
                assert_eq!(
                    result,
                    Err(RunError::Contract("retirement quotation work overflow"))
                );
                result
            }),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!((state.live[2].get(), state.drops[2].get()), (0, count));
        assert_eq!(state.drop_reason[2].get(), Some(Incomplete::Interrupted));
        assert_eq!((state.live[1].get(), state.drops[1].get()), (1, 0));
        assert_eq!(state.clones.get(), clones);
        assert_eq!(ids(db), before);
        assert_eq!(
            std::ptr::from_ref(stored(db, ingredient, old).unwrap()).addr(),
            pointer
        );
        assert_eq!(detached.acquired.get(), 0);
        assert!(state.delivered.borrow().is_empty());
        assert!(
            stored(
                db,
                owned_caller::fn_ingredient_(db, db.zalsa()),
                request.as_id()
            )
            .is_none()
        );
        assert_eq!(Stamp::current(db), stamp);
        idle(db);
    }
    state.tag.set(0);
    let new = complete(db, request);
    assert_eq!(new, replacement(db, old));
    assert_eq!(state.drops[1].get(), 1);
}

#[test]
fn post_commit_refusal_retains_the_canonical_key_for_same_revision_retry() {
    let (state, _reset) = fixture();
    let db = Box::leak(Box::new(OwnedDb::new()));
    let request = Request::new(db, 0, 3, None);
    let (hook, _observer) = install_hook(db, request, None, Fault::Intern);
    let stamp = Stamp::current(db);
    reject(&hook);
    let id = ids(db)[0];
    assert_eq!((state.live[2].get(), state.drops[2].get()), (1, 0));
    assert_eq!(state.clones.get(), 0);
    assert!(stored(db, owned_value::fn_ingredient_(db, db.zalsa()), id).is_none());
    stop_hook();
    state.events.borrow_mut().clear();
    assert_eq!(complete(db, request), id);
    assert!(
        !state
            .events
            .borrow()
            .iter()
            .any(|event| event.0 == KeyEvent::Intern)
    );
    assert_eq!(Stamp::current(db), stamp);
}

#[test]
fn retirement_refusal_preserves_owned_fields_and_memo_for_retry() {
    for execute in [run::<false> as fn(_, _) -> _, run_callable] {
        let (state, _reset) = fixture();
        let (db, request, old) = stale();
        let db = Box::leak(Box::new(db));
        let ingredient = owned_value::fn_ingredient_(db, db.zalsa());
        let pointer = std::ptr::from_ref(stored(db, ingredient, old).unwrap()).addr();
        let before = ids(db);
        let (hook, _observer) = install_hook(db, request, Some(old), Fault::Retirement);
        let stamp = Stamp::current(db);
        let clones = state.clones.get();
        reject_with(&hook, execute);
        assert_eq!(state.clones.get(), clones);
        assert_eq!(ids(db), before);
        assert_eq!(
            std::ptr::from_ref(stored(db, ingredient, old).unwrap()).addr(),
            pointer
        );
        assert!(matches!(
            state.refused_work.get(),
            Some(ExecutionWork::Work { units }) if units == OLD_BUFFER_LEN + 1
        ));
        let notes = hook.notes.borrow();
        for stage in ["boundary", "child"] {
            let note = notes.iter().find(|note| note.stage == stage).unwrap();
            assert_eq!((note.live[1], note.live[2], note.detached), (1, 1, 0));
        }
        drop(notes);
        assert_eq!((state.live[1].get(), state.drops[1].get()), (1, 0));
        assert_eq!((state.live[2].get(), state.drops[2].get()), (0, 1));
        assert_eq!(state.drop_reason[2].get(), Some(Incomplete::Interrupted));
        assert!(state.child_order.get() < state.drop_order[2].get());
        assert!(state.drop_order[2].get() < state.caller_order.get());
        assert_eq!(hook.detached.acquired.get(), 0);
        assert_eq!(hook.detached.retired.get(), 0);
        assert!(
            !state
                .events
                .borrow()
                .iter()
                .any(|event| matches!(event.0, KeyEvent::Reuse | KeyEvent::Discard))
        );
        stop_hook();
        let new = complete_with(db, request, execute);
        assert_eq!(new, replacement(db, old));
        assert_eq!(state.drops[1].get(), 1);
        assert_eq!(Stamp::current(db), stamp);
    }
}

#[test]
fn detached_owned_fields_outlive_children_at_event_boundaries() {
    for fault in [Fault::Discard, Fault::Panic] {
        let (state, _reset) = fixture();
        let (db, request, old) = stale();
        let db = Box::leak(Box::new(db));
        let (hook, _observer) = install_hook(db, request, Some(old), fault);
        let stamp = Stamp::current(db);
        let clones = state.clones.get();
        reject(&hook);
        assert_eq!(state.clones.get(), clones);
        let notes = hook.notes.borrow();
        let child = notes.iter().find(|note| note.stage == "child").unwrap();
        assert_eq!((child.live[1], child.live[2], child.detached), (1, 1, 1));
        drop(notes);
        assert_eq!((state.live[1].get(), state.drops[1].get()), (0, 1));
        assert_eq!((state.live[2].get(), state.drops[2].get()), (1, 0));
        assert_eq!(state.detached_at_drop[1].get(), 0);
        assert!(state.child_order.get() < state.drop_order[1].get());
        assert!(state.drop_order[1].get() < state.caller_order.get());
        assert_eq!(
            (
                hook.detached.live.get(),
                hook.detached.acquired.get(),
                hook.detached.retired.get()
            ),
            (0, 1, 1)
        );
        let new = replacement(db, old);
        assert!(stored(db, owned_value::fn_ingredient_(db, db.zalsa()), new).is_none());
        stop_hook();
        assert_eq!(complete(db, request), new);
        assert_eq!(Stamp::current(db), stamp);
    }
}

#[test]
fn discard_callback_memo_survives_owned_generation_retirement() {
    for fault in [Fault::Reenter, Fault::ReenterRefuse] {
        let (state, _reset) = fixture();
        let (db, request, old) = stale();
        let db = Box::leak(Box::new(db));
        let (hook, _observer) = install_hook(db, request, Some(old), fault);
        let stamp = Stamp::current(db);
        if fault == Fault::Reenter {
            complete(db, request);
        } else {
            reject(&hook);
        }
        let new = hook.new_id.get().unwrap();
        let pointer = hook.replacement_memo.get().unwrap();
        let ingredient = owned_value::fn_ingredient_(db, db.zalsa());
        assert_eq!(
            std::ptr::from_ref(stored(db, ingredient, new).unwrap()).addr(),
            pointer
        );
        assert_eq!(stored(db, ingredient, new).unwrap().value(), Some(&true));
        assert_eq!((state.live[1].get(), state.drops[1].get()), (0, 1));
        assert_eq!(state.detached_at_drop[1].get(), 0);
        assert_eq!(
            (
                hook.detached.live.get(),
                hook.detached.acquired.get(),
                hook.detached.retired.get()
            ),
            (0, 1, 1)
        );
        stop_hook();
        assert_eq!(complete(db, request), new);
        assert_eq!(
            std::ptr::from_ref(stored(db, ingredient, new).unwrap()).addr(),
            pointer
        );
        assert_eq!(Stamp::current(db), stamp);
    }
}

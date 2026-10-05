use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::super::super::registration::{
    ExecutableRouteProvider, NativeValueOperation, NativeValueQuote, PassiveMemoProfile,
    ProviderBinding, ProviderContext, QueryKeyProfile, QueryKeys, RegistryBuilder, RetainedInput,
    Route,
};
use super::super::super::{ExecutionWork, RunError, RunResult};
use super::{Collision, Request, revisions, stored};
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionBudget, ExecutionLimits, Incomplete, charge_observation,
    try_with_metered_execution_budget,
};
use crate::function::{Configuration, InternedQueryConfiguration};
use crate::plumbing::{AsId, QuoteError, QuoteFuel};
use crate::prepared_source_probe::Stamp;
use crate::table::memo::detached_observation;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, Id, Setter};

const OLD_BUFFERS: usize = 16_384;
const BUFFER_BYTES: usize = 8;
const KEY_ALLOWANCE: usize = 10_000;
const LIMITS: ExecutionLimits = ExecutionLimits {
    semantic_work: 1_000_000,
    requested_bytes: 16 * 1024 * 1024,
};

#[derive(Default)]
struct State {
    old_drops: Cell<usize>,
    old_quote: Cell<Option<usize>>,
    preliminary_work: Cell<usize>,
    allowance_before_key: Cell<Option<usize>>,
    delivered: Cell<Option<Id>>,
    leaves: Cell<usize>,
    ordinary: Cell<usize>,
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
            STATE.with_borrow_mut(|slot| *slot = None);
        }
    }

    let state = Rc::new(State::default());
    STATE.with_borrow_mut(|slot| assert!(slot.replace(state.clone()).is_none()));
    (state, Reset)
}

#[derive(Debug, crate::SalsaValue)]
struct Buffer {
    bytes: Box<[u8; BUFFER_BYTES]>,
    old: bool,
}

impl Clone for Buffer {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            old: false,
        }
    }
}

impl PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for Buffer {}

impl Drop for Buffer {
    fn drop(&mut self) {
        // Only the ordinarily seeded canonical buffers are counted, not generated input clones.
        // Dropping a buffer updates one scalar and frees its native allocation without database work.
        if self.old {
            let _ = STATE.try_with(|slot| {
                if let Some(state) = slot.borrow().as_ref() {
                    state.old_drops.set(state.old_drops.get() + 1);
                }
            });
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, crate::SalsaValue)]
struct Buffers(Box<[Buffer]>);

impl Buffers {
    fn new(count: usize, old: bool) -> Self {
        Self(
            (0..count)
                .map(|_| Buffer {
                    bytes: Box::new([b'x'; BUFFER_BYTES]),
                    old,
                })
                .collect(),
        )
    }

    fn work(count: usize) -> Option<usize> {
        count.checked_mul(BUFFER_BYTES + 1)?.checked_add(1)
    }

    fn requested_bytes(count: usize) -> Option<usize> {
        count
            .checked_mul(size_of::<Buffer>() + BUFFER_BYTES)?
            .checked_add(size_of::<(Collision, Self)>())
    }
}

impl Hash for Buffers {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        // Every input uses the same reuse queue; equality still compares the actual bytes.
        hasher.write_u32(0);
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn buffer_count(_db: &dyn Database, _salt: Collision, buffers: Buffers) -> usize {
    state().ordinary.set(state().ordinary.get() + 1);
    buffers.0.len()
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn parent(db: &dyn Database, request: Request) -> usize {
    let salt = request.left(db);
    buffer_count(
        db,
        Collision(salt),
        Buffers::new(request.right(db) as usize, salt == 0),
    )
}

struct Profile;

impl<C> PassiveMemoProfile<C> for Profile
where
    C: for<'db> Configuration<Output<'db> = usize>,
{
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        Some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

// Hash and equality use finite native data. Each owned element has a separate byte allocation;
// its passive cleanup requires one element visit. The quote also covers byte comparisons.
impl<C> QueryKeyProfile<C> for Profile
where
    C: InternedQueryConfiguration
        + for<'db> crate::interned::Configuration<Fields<'db> = (Collision, Buffers)>
        + for<'db> Configuration<Output<'db> = usize>,
{
    fn input_work<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        let work = Buffers::work(input.1.0.len())?;
        if input.0.0 == 0 {
            state().old_quote.set(Some(work));
        }
        Some(work)
    }

    fn input_work_bounded<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

struct Leaf;

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Leaf
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Collision, Buffers),
            Output<'a> = usize,
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
        match operation {
            NativeValueOperation::InputConversion(RetainedInput::Interned((_, buffers))) => {
                let count = buffers.0.len();
                // Generated conversion clones every buffer, and prepays the cloned input's cleanup.
                Ok(NativeValueQuote {
                    work: Buffers::work(count).ok_or(RunError::Contract("buffer work overflow"))?,
                    requested_bytes: Buffers::requested_bytes(count)
                        .ok_or(RunError::Contract("buffer bytes overflow"))?,
                    cleanup_work: count + 1,
                })
            }
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                Err(RunError::Contract("buffer query requires retained fields"))
            }
            NativeValueOperation::Comparison { .. } => Ok(NativeValueQuote {
                work: 1,
                requested_bytes: 0,
                cleanup_work: 0,
            }),
        }
    }

    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        input: (Collision, Buffers),
    ) -> RunResult<usize> {
        state().leaves.set(state().leaves.get() + 1);
        Ok(input.1.0.len())
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Collision, Buffers),
    ) -> RunResult<usize> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call usize,
        _value: usize,
        _input: (Collision, Buffers),
    ) -> RunResult<usize>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct Parent<'run, 'db: 'run, C: InternedQueryConfiguration> {
    target: Route<'db, C>,
    keys: QueryKeys<'db, C, Profile>,
    leaf: ProviderBinding<'run, Leaf>,
    spend_first: bool,
}

impl<'run, 'db: 'run, C, D> ExecutableRouteProvider<'run, 'db, D> for Parent<'run, 'db, C>
where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, Buffers)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = usize>,
    D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = usize>,
{
    // Request conversion copies one handle; output comparison examines one scalar.
    fixture_native_value!(executable, 'run, 'db, D, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        input: Request,
    ) -> RunResult<usize> {
        let endpoint = context.endpoint();
        let fields = endpoint
            .local_call(|| {
                let count = input.right(db) as usize;
                // Pay for construction and destruction of the submitted replacement before it exists.
                endpoint.admit_work(
                    Buffers::work(count)
                        .and_then(|work| work.checked_add(count + 1))
                        .ok_or(RunError::Contract("buffer work overflow"))?,
                )?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: Buffers::requested_bytes(count)
                        .ok_or(RunError::Contract("buffer bytes overflow"))?,
                })?;
                Ok((Collision(input.left(db)), Buffers::new(count, false)))
            })
            .await;
        endpoint
            .local_call(|| {
                if self.spend_first {
                    let remaining = attempt_probe::remaining_allowance_for_diagnostics(db)
                        .ok_or(RunError::Contract("missing work allowance"))?;
                    let work = remaining
                        .checked_sub(KEY_ALLOWANCE)
                        .ok_or(RunError::Contract(
                            "fixture prefix exhausted work allowance",
                        ))?;
                    endpoint.admit_work(work)?;
                    state().preliminary_work.set(work);
                }
                state()
                    .allowance_before_key
                    .set(attempt_probe::remaining_allowance_for_diagnostics(db));
                Ok(())
            })
            .await;
        let id = endpoint.intern_query_key(&self.keys, fields).await;
        state().delivered.set(Some(id));
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
    ) -> RunResult<usize> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call usize,
        _value: usize,
        _input: Request,
    ) -> RunResult<usize>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn run(
    db: &crate::DatabaseImpl,
    budget: &ExecutionBudget<'_>,
    request: Request,
    spend_first: bool,
) -> RunResult<usize> {
    let provider;
    let mut registry = RegistryBuilder::with_budget(db, budget)?;
    let target = registry.reserve(
        db as &dyn Database,
        buffer_count::fn_ingredient_(db, db.zalsa()),
    )?;
    let keys = registry.query_keys::<_, Profile>(&target)?;
    let leaf = registry.provider(&Leaf)?;
    registry.bind_executable(&target, &leaf)?;
    let parent = registry.reserve(db as &dyn Database, parent::fn_ingredient_(db, db.zalsa()))?;
    provider = Parent {
        target,
        keys,
        leaf,
        spend_first,
    };
    let binding = registry.provider(&provider)?;
    registry.bind_executable(&parent, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&parent, request.as_id())?
            .await?)
    })
}

fn ids(db: &crate::DatabaseImpl) -> Vec<Id> {
    buffer_count::intern_ingredient_(db.zalsa())
        .entries(db.zalsa())
        .map(|entry| entry.key().key_index())
        .collect()
}

fn stale() -> (crate::DatabaseImpl, Request, Id) {
    let mut db = crate::DatabaseImpl::default();
    let input = Request::new(&db, 0, OLD_BUFFERS as u32, None);
    assert_eq!(parent(&db, input), OLD_BUFFERS);
    let old = ids(&db)[0];
    // Advance beyond the interner's retention window so the original generation can be reused.
    let count = revisions(buffer_count::fn_ingredient_(&db, db.zalsa()));
    for index in 1..count {
        input.set_left(&mut db).to(index as u32);
        assert_eq!(parent(&db, input), OLD_BUFFERS);
    }
    input.set_left(&mut db).to(count as u32);
    let request = Request::new(&db, count as u32, 1, None);
    (db, request, old)
}

/// Refusing an old generation's cleanup debit preserves its canonical fields and memo.
/// Ordinary execution seeds independently owned buffers without charging the controlled budget.
/// Both attempts use the same limits: the first spends work before the small replacement, and
/// retry in the same revision succeeds when that preliminary work is omitted.
#[test]
fn insufficient_retirement_allowance_preserves_old_generation() {
    let (state, _reset) = fixture();
    let (db, request, old) = stale();
    let argument = buffer_count::intern_ingredient_(db.zalsa());
    let function = buffer_count::fn_ingredient_(&db, db.zalsa());
    let old_memo = std::ptr::from_ref(stored(&db, function, old).unwrap()).addr();
    let old_fields = argument
        .entries(db.zalsa())
        .find(|entry| entry.key().key_index() == old)
        .unwrap()
        .value()
        .fields()
        .1
        .0
        .as_ptr()
        .addr();
    let before = ids(&db);
    let stamp = Stamp::current(&db);
    let ordinary = state.ordinary.get();
    assert_eq!(state.old_drops.get(), 0);
    assert!(state.old_quote.get().is_none());
    let detached = Rc::new(detached_observation::State::default());
    let _detached = detached_observation::install(detached.clone());
    let charges = Rc::new(charge_observation::State::default());
    let _charges = charge_observation::install(charges.clone());

    let rejected =
        try_with_metered_execution_budget(&db, LIMITS, |budget| run(&db, &budget, request, true))
            .unwrap();
    assert_eq!(
        rejected.outcome,
        AttemptOutcome::Incomplete(Incomplete::Allowance)
    );
    let old_quote = state.old_quote.get().expect("old fields were not quoted");
    let refused = charges
        .first
        .get()
        .expect("no numeric work debit was refused");
    assert_eq!(old_quote, Buffers::work(OLD_BUFFERS).unwrap());
    assert_eq!(state.allowance_before_key.get(), Some(KEY_ALLOWANCE));
    assert!(state.preliminary_work.get() > 0);
    assert!(refused.units >= old_quote);
    assert!(refused.remaining < old_quote);
    assert!(refused.remaining <= KEY_ALLOWANCE);
    assert_eq!(
        LIMITS.semantic_work - rejected.usage.semantic_work,
        refused.remaining
    );
    assert!(rejected.usage.requested_bytes < LIMITS.requested_bytes);

    // No old element or detached memo owner may be destroyed after an unpaid retirement debit.
    assert_eq!(state.old_drops.get(), 0);
    assert_eq!(detached.acquired.get(), 0);
    assert_eq!(detached.retired.get(), 0);
    assert_eq!(detached.live.get(), 0);
    assert_eq!(ids(&db), before);
    {
        let entry = argument
            .entries(db.zalsa())
            .find(|entry| entry.key().key_index() == old)
            .unwrap();
        let fields = entry.value().fields();
        assert_eq!(fields.0, Collision(0));
        assert_eq!(fields.1.0.len(), OLD_BUFFERS);
        assert_eq!(fields.1.0.as_ptr().addr(), old_fields);
        assert!(
            fields
                .1
                .0
                .iter()
                .all(|buffer| buffer.old && *buffer.bytes == [b'x'; BUFFER_BYTES])
        );
        let memo = stored(&db, function, old).unwrap();
        assert_eq!(std::ptr::from_ref(memo).addr(), old_memo);
        assert_eq!(memo.value(), Some(&OLD_BUFFERS));
    }
    assert!(
        stored(
            &db,
            parent::fn_ingredient_(&db, db.zalsa()),
            request.as_id()
        )
        .is_none()
    );
    assert_eq!(state.delivered.get(), None);
    assert_eq!(state.leaves.get(), 0);
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(Stamp::current(&db), stamp);

    charges.first.set(None);
    let retry =
        try_with_metered_execution_budget(&db, LIMITS, |budget| run(&db, &budget, request, false))
            .unwrap();
    assert_eq!(retry.outcome, AttemptOutcome::Complete(Ok(1)));
    assert!(charges.first.get().is_none());
    assert!(retry.usage.semantic_work >= old_quote);
    let new = state.delivered.get().unwrap();
    assert_eq!(new.index(), old.index());
    assert_eq!(new.generation(), old.generation() + 1);
    assert_eq!(state.old_drops.get(), OLD_BUFFERS);
    assert_eq!(detached.acquired.get(), 1);
    assert_eq!(detached.retired.get(), 1);
    assert_eq!(detached.live.get(), 0);
    assert_eq!(stored(&db, function, new).unwrap().value(), Some(&1));
    assert_eq!(
        stored(
            &db,
            parent::fn_ingredient_(&db, db.zalsa()),
            request.as_id()
        )
        .unwrap()
        .value(),
        Some(&1)
    );
    assert_eq!(state.leaves.get(), 1);
    assert_eq!(state.ordinary.get(), ordinary);
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(Stamp::current(&db), stamp);
}

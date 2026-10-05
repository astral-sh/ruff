use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::super::super::super::registration::{
    ExecutableRouteProvider, NativeValueOperation, NativeValueQuote, PassiveMemoProfile,
    ProviderContext, QueryKeyProfile, RegistryBuilder,
};
use super::super::super::super::{ExecutionWork, RunError, RunResult};
use super::{Collision, Request, revisions, stored};
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionBudget, ExecutionLimits, Incomplete, charge_observation,
    try_with_metered_execution_budget,
};
use crate::function::{Configuration, IngredientImpl, InternedQueryConfiguration, Memo};
use crate::ingredient::Ingredient;
use crate::plumbing::{QuoteError, QuoteFuel};
use crate::prepared_source_probe::Stamp;
use crate::table::memo::detached_observation;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, EventKind, Id, Setter};

const OLD_LENGTH: usize = 73;
const LARGE_LENGTH: usize = 20_000;
const SMALL_LENGTH: usize = 19;
const KEY_ALLOWANCE: usize = 10_000;
const LIMITS: ExecutionLimits = ExecutionLimits {
    semantic_work: 1_000_000,
    requested_bytes: 16 * 1024 * 1024,
};

#[derive(Default)]
struct State {
    db: RefCell<Option<MemoDb>>,
    replacement: RefCell<Option<Output>>,
    displaced: RefCell<Option<Output>>,
    armed: Cell<bool>,
    pending: Cell<bool>,
    swapped: Cell<bool>,
    in_key: Cell<bool>,
    old_memo: Cell<usize>,
    quotes: Cell<[usize; 16]>,
    quote_count: Cell<usize>,
    drops: [Cell<usize>; 2],
    delivered: Cell<Option<Id>>,
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

#[derive(Debug, Eq, PartialEq, crate::SalsaValue)]
struct Item {
    tag: usize,
}

impl Drop for Item {
    fn drop(&mut self) {
        let _ = STATE.try_with(|slot| {
            if let Some(state) = slot.borrow().as_ref()
                && let Some(drops) = state.drops.get(self.tag)
            {
                drops.set(drops.get() + 1);
            }
        });
    }
}

#[derive(Debug, Eq, PartialEq, crate::SalsaValue)]
struct Output(Box<[Item]>);

impl Output {
    fn new(len: usize, tag: usize) -> Self {
        Self((0..len).map(|_| Item { tag }).collect())
    }

    fn work(&self) -> Option<usize> {
        self.0.len().checked_add(1)
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn output(_db: &dyn Database, salt: Collision, _metadata: u32) -> Output {
    Output::new(
        if salt.0 == 0 { OLD_LENGTH } else { 1 },
        if salt.0 == 0 { 0 } else { 2 },
    )
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn seed(db: &dyn Database, request: Request) -> usize {
    output(db, Collision(request.left(db)), 0).0.len()
}

struct Profile;

impl<C> PassiveMemoProfile<C> for Profile
where
    C: for<'db> Configuration<Output<'db> = Output>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        let work = output.work().ok_or(QuoteError::Overflow)?;
        let state = state();
        if state.in_key.get() {
            let index = state.quote_count.get();
            let mut quotes = state.quotes.get();
            quotes[index] = output.0.len();
            state.quotes.set(quotes);
            state.quote_count.set(index + 1);
            if state.armed.replace(false) {
                assert_eq!(output.0.len(), OLD_LENGTH);
                state.pending.set(true);
            }
        }
        Ok(work)
    }
}

impl<C> QueryKeyProfile<C> for Profile
where
    C: InternedQueryConfiguration
        + for<'db> crate::interned::Configuration<Fields<'db> = (Collision, u32)>
        + for<'db> Configuration<Output<'db> = Output>,
{
    fn input_work<'db>(
        _input: &<C as crate::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        Some(1)
    }

    fn input_work_bounded<'db>(
        _input: &<C as crate::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Ok(1)
    }
}

#[derive(Clone)]
#[crate::db]
struct MemoDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for MemoDb {}

impl MemoDb {
    fn new() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(|event| {
                if matches!(event.kind, EventKind::WillCheckCancellation)
                    && state().pending.replace(false)
                {
                    let state = state();
                    let database = state.db.borrow();
                    let db = database.as_ref().unwrap();
                    replace_output(db, output::fn_ingredient_(db, db.zalsa()));
                }
            }))),
        }
    }
}

fn replace_output<C>(db: &MemoDb, function: &IngredientImpl<C>)
where
    C: InternedQueryConfiguration
        + for<'db> crate::interned::Configuration<Fields<'db> = (Collision, u32)>
        + for<'db> Configuration<DbView = dyn Database, Output<'db> = Output>,
{
    let state = state();
    let argument = C::argument_ingredient(db.zalsa());
    let index = function.query_key_memo_index(db.zalsa(), argument).unwrap();
    // The rejected preparation lends the stale table without validating its key. Swap only
    // the immutable output and retain the displaced payload for ordinary fixture cleanup,
    // so its destruction cannot be mistaken for retirement of the replacement.
    let result = argument.prepare_intern_retaining(
        db.zalsa(),
        db.zalsa_local(),
        (Collision(u32::MAX), 0),
        |_, fields| fields,
        |table| {
            table.map_memo::<Memo<C>>(index, |memo| {
                assert_eq!(std::ptr::from_ref(memo).addr(), state.old_memo.get());
                let replacement = state.replacement.borrow_mut().take().unwrap();
                let displaced = memo.value.replace(replacement).unwrap();
                assert!(state.displaced.borrow_mut().replace(displaced).is_none());
                assert!(!state.swapped.replace(true));
            });
            Err(())
        },
    );
    assert!(matches!(result, Err(((), _))));
    assert!(state.swapped.get());
}

struct UnusedProvider;

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for UnusedProvider
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Collision, u32),
            Output<'a> = Output,
        >,
{
    async fn native_value<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }

    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _input: (Collision, u32),
    ) -> RunResult<Output> {
        Err(RunError::RequiresFetch)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Collision, u32),
    ) -> RunResult<Output> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Output,
        _value: Output,
        _input: (Collision, u32),
    ) -> RunResult<Output>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct KeyScope;

impl Drop for KeyScope {
    fn drop(&mut self) {
        state().in_key.set(false);
    }
}

fn run(db: &MemoDb, budget: &ExecutionBudget<'_>, spend_first: bool) -> RunResult<Id> {
    let provider = UnusedProvider;
    let mut registry = RegistryBuilder::with_budget(db, budget)?;
    let route = registry.reserve(db as &dyn Database, output::fn_ingredient_(db, db.zalsa()))?;
    let keys = registry.query_keys::<_, Profile>(&route)?;
    let provider = registry.provider(&provider)?;
    registry.bind_executable(&route, &provider)?;
    registry.seal()?.run(|endpoint| async move {
        let input = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<(Collision, u32)>(),
                })?;
                if spend_first {
                    let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
                    endpoint.admit_work(remaining.checked_sub(KEY_ALLOWANCE).unwrap())?;
                }
                Ok((Collision(1_000_003), 0))
            })
            .await;
        state().in_key.set(true);
        let _scope = KeyScope;
        let id = endpoint.intern_query_key(&keys, input).await;
        state().delivered.set(Some(id));
        Ok(id)
    })
}

fn ids(db: &MemoDb) -> Vec<Id> {
    output::intern_ingredient_(db.zalsa())
        .entries(db.zalsa())
        .map(|entry| entry.key().key_index())
        .collect()
}

fn stale(changed_length: usize) -> (MemoDb, Id) {
    let mut db = MemoDb::new();
    let request = Request::new(&db, 0, 0, None);
    assert_eq!(seed(&db, request), OLD_LENGTH);
    let old = ids(&db)[0];
    let count = revisions(output::fn_ingredient_(&db, db.zalsa()));
    for revision in 1..count {
        request.set_left(&mut db).to(revision as u32);
        assert_eq!(seed(&db, request), 1);
    }
    request.set_left(&mut db).to(count as u32);
    let state = state();
    state.old_memo.set(
        std::ptr::from_ref(stored(&db, output::fn_ingredient_(&db, db.zalsa()), old).unwrap())
            .addr(),
    );
    *state.replacement.borrow_mut() = Some(Output::new(changed_length, 1));
    *state.db.borrow_mut() = Some(db.clone());
    state.armed.set(true);
    (db, old)
}

fn assert_live_memo(db: &MemoDb, old: Id, length: usize) {
    assert!(ids(db).contains(&old));
    let memo = stored(db, output::fn_ingredient_(db, db.zalsa()), old).unwrap();
    assert_eq!(std::ptr::from_ref(memo).addr(), state().old_memo.get());
    assert_eq!(memo.value().unwrap().0.len(), length);
}

fn complete(db: &MemoDb) -> Id {
    let result =
        try_with_metered_execution_budget(db, LIMITS, |budget| run(db, &budget, false)).unwrap();
    let AttemptOutcome::Complete(Ok(id)) = result.outcome else {
        panic!("{result:?}");
    };
    assert!(result.usage.semantic_work < LIMITS.semantic_work);
    assert!(result.usage.requested_bytes < LIMITS.requested_bytes);
    id
}

/// After the initial cleanup quotation, a callback installs a larger immutable output while
/// preserving the eligible key and memo allocation. Final inspection must quote its actual
/// cleanup and refuse before detachment when the numeric allowance is too small. A fresh attempt
/// in the same revision can then retire the replacement's actual elements.
#[test]
fn increased_output_cleanup_is_requoted_before_detachment_and_retry() {
    let (state, _reset) = fixture();
    let (db, old) = stale(LARGE_LENGTH);
    let before = ids(&db);
    let stamp = Stamp::current(&db);
    let detached = Rc::new(detached_observation::State::default());
    let _observer = detached_observation::install(detached.clone());
    let charges = Rc::new(charge_observation::State::default());
    let _charges = charge_observation::install(charges.clone());
    let result =
        try_with_metered_execution_budget(&db, LIMITS, |budget| run(&db, &budget, true)).unwrap();
    assert_eq!(
        result.outcome,
        AttemptOutcome::Incomplete(Incomplete::Allowance)
    );
    let refusal = charges.first.get().unwrap();
    assert_eq!(refusal.units, LARGE_LENGTH + 2);
    assert!(refusal.remaining < KEY_ALLOWANCE);
    assert_eq!(
        LIMITS.semantic_work - result.usage.semantic_work,
        refusal.remaining
    );
    assert!(result.usage.requested_bytes < LIMITS.requested_bytes);
    assert!(state.swapped.get());
    assert_eq!(state.quote_count.get(), 3);
    assert_eq!(
        &state.quotes.get()[..3],
        &[OLD_LENGTH, LARGE_LENGTH, LARGE_LENGTH]
    );
    assert_eq!(state.delivered.get(), None);
    assert_eq!(ids(&db), before);
    assert_live_memo(&db, old, LARGE_LENGTH);
    assert_eq!((state.drops[0].get(), state.drops[1].get()), (0, 0));
    assert_eq!(
        state.displaced.borrow().as_ref().unwrap().0.len(),
        OLD_LENGTH
    );
    assert_eq!((detached.acquired.get(), detached.retired.get()), (0, 0));
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert_eq!(Stamp::current(&db), stamp);

    let new = complete(&db);
    assert_eq!(new.index(), old.index());
    assert_eq!(new.generation(), old.generation() + 1);
    assert_eq!(
        (state.drops[0].get(), state.drops[1].get()),
        (0, LARGE_LENGTH)
    );
    assert_eq!((detached.acquired.get(), detached.retired.get()), (1, 1));
    assert_eq!(Stamp::current(&db), stamp);
    assert_eq!(complete(&db), new);
    drop(state.displaced.borrow_mut().take());
    assert_eq!(state.drops[0].get(), OLD_LENGTH);
}

/// After the initial cleanup quotation, a callback installs a smaller immutable output while
/// preserving the eligible key and memo allocation. Final inspection finds that it fits the
/// accepted bound and reuses the same slot with an incremented generation, without restarting quotation.
#[test]
fn decreased_output_cleanup_fits_the_prepaid_bound() {
    let (state, _reset) = fixture();
    let (db, old) = stale(SMALL_LENGTH);
    let stamp = Stamp::current(&db);
    let new = complete(&db);
    assert!(state.swapped.get());
    assert_eq!(state.quote_count.get(), 2);
    assert_eq!(&state.quotes.get()[..2], &[OLD_LENGTH, SMALL_LENGTH]);
    assert_eq!(new.index(), old.index());
    assert_eq!(new.generation(), old.generation() + 1);
    assert_eq!(
        (state.drops[0].get(), state.drops[1].get()),
        (0, SMALL_LENGTH)
    );
    assert_eq!(Stamp::current(&db), stamp);
    drop(state.displaced.borrow_mut().take());
    assert_eq!(state.drops[0].get(), OLD_LENGTH);
}

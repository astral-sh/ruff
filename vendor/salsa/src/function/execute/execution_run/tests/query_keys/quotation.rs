use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::super::super::registration::{
    ExecutableRouteProvider, NativeValueOperation, NativeValueQuote, PassiveMemoProfile,
    ProviderContext, QueryKeyProfile, RegistryBuilder,
};
use super::super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::{Collision, Request, revisions, stored};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::{Configuration, InternedQueryConfiguration};
use crate::ingredient::Ingredient;
use crate::interned::FiniteInternedConfiguration;
use crate::plumbing::{AsId, QuoteError, QuoteFuel};
use crate::prepared_source_probe::Stamp;
use crate::table::memo::detached_observation;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, Id, Setter};

mod changed_memo;

const OLD_LENGTH: usize = 16;
const RETIREMENT_WORK: usize = OLD_LENGTH;

#[derive(Default)]
struct State {
    in_key: Cell<bool>,
    refuse_partial: Cell<bool>,
    refuse_quantum: Cell<Option<usize>>,
    pending_refusal: Cell<bool>,
    fired: Cell<bool>,
    old_visits: Cell<usize>,
    old_complete: Cell<usize>,
    old_partial: Cell<usize>,
    visits_at_refusal: Cell<usize>,
    old_drops: Cell<usize>,
    callback_id: Cell<Option<Id>>,
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
struct Payload {
    bytes: Box<[u8]>,
    old: bool,
}

impl Payload {
    fn new(salt: u32) -> Self {
        Self {
            bytes: vec![1; if salt == 0 { OLD_LENGTH } else { 1 }].into_boxed_slice(),
            old: salt == 0,
        }
    }
}

impl Clone for Payload {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            old: false,
        }
    }
}

impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for Payload {}

impl Hash for Payload {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        // Equal hashes keep the stale entry and every replacement in the same reuse queue.
        hasher.write_u32(0);
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        if self.old {
            let _ = STATE.try_with(|slot| {
                if let Some(state) = slot.borrow().as_ref() {
                    state.old_drops.set(state.old_drops.get() + 1);
                }
            });
        }
    }
}

fn field_work_with(
    fields: &(Collision, Payload),
    mut visit: impl FnMut(usize) -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    (0..fields.1.bytes.len()).try_fold(0usize, |total, index| {
        visit(index)?;
        total
            .checked_add(usize::from(fields.1.bytes[index]))
            .ok_or(QuoteError::Overflow)
    })
}

fn field_work(fields: &(Collision, Payload)) -> Option<usize> {
    field_work_with(fields, |_| Ok(())).ok()
}

fn field_work_bounded(
    fields: &(Collision, Payload),
    fuel: &mut QuoteFuel,
) -> Result<usize, QuoteError> {
    let state = state();
    let old = fields.0.0 == 0;
    let total = field_work_with(fields, |index| {
        if let Err(error) = fuel.consume(1) {
            if old && index != 0 {
                state.old_partial.set(state.old_partial.get() + 1);
                if state.refuse_partial.get() {
                    state.pending_refusal.set(true);
                }
            }
            return Err(error);
        }
        if old {
            state.old_visits.set(state.old_visits.get() + 1);
        }
        Ok(())
    })?;
    if old {
        state.old_complete.set(state.old_complete.get() + 1);
    }
    Ok(total)
}

#[crate::interned]
struct Value<'db> {
    #[returns(copy)]
    salt: Collision,
    #[returns(ref)]
    payload: Payload,
}

impl FiniteInternedConfiguration for Value<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        field_work(fields)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        field_work_bounded(fields, fuel)
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn count(_db: &dyn Database, _salt: Collision, payload: Payload) -> usize {
    payload.bytes.len()
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn seed_value(db: &dyn Database, request: Request) -> Id {
    let salt = request.left(db);
    Value::new(db, Collision(salt), Payload::new(salt)).as_id()
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn seed_key(db: &dyn Database, request: Request) -> usize {
    let salt = request.left(db);
    count(db, Collision(salt), Payload::new(salt))
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
        _output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Ok(0)
    }
}

impl<C> QueryKeyProfile<C> for Profile
where
    C: InternedQueryConfiguration
        + for<'db> crate::interned::Configuration<Fields<'db> = (Collision, Payload)>
        + for<'db> Configuration<Output<'db> = usize>,
{
    fn input_work<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        field_work(input)
    }

    fn input_work_bounded<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        field_work_bounded(input, fuel)
    }
}

#[derive(Default)]
#[crate::db]
struct QuoteDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for QuoteDb {}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Query,
    Value,
}

impl Kind {
    fn ids(self, db: &QuoteDb) -> Vec<Id> {
        match self {
            Self::Query => count::intern_ingredient_(db.zalsa())
                .entries(db.zalsa())
                .map(|entry| entry.key().key_index())
                .collect(),
            Self::Value => Value::ingredient(db.zalsa())
                .entries(db.zalsa())
                .map(|entry| entry.key().key_index())
                .collect(),
        }
    }

    fn exhaust(self, db: &QuoteDb, id: Id) -> Id {
        match self {
            Self::Query => {
                count::intern_ingredient_(db.zalsa()).exhaust_generation_for_tests(db.zalsa(), id)
            }
            Self::Value => {
                Value::ingredient(db.zalsa()).exhaust_generation_for_tests(db.zalsa(), id)
            }
        }
        .unwrap()
    }

    fn ordinary_intern(self, db: &QuoteDb, salt: u32) -> Id {
        // Callback inputs do not own the canonical generation's drop observation.
        let mut payload = Payload::new(salt);
        payload.old = false;
        match self {
            Self::Query => count::intern_ingredient_(db.zalsa()).intern_id(
                db.zalsa(),
                db.zalsa_local(),
                (Collision(salt), payload),
                |_, fields| fields,
            ),
            Self::Value => Value::new(db, Collision(salt), payload).as_id(),
        }
    }
}

#[derive(Clone, Copy)]
enum Mutation {
    None,
    Validate,
    Replace,
}

struct Admission<'db> {
    db: &'db QuoteDb,
    kind: Kind,
    mutation: Mutation,
}

impl ExecutionAdmission for Admission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let state = state();
        if !state.in_key.get() {
            return Ok(());
        }
        if let ExecutionWork::Work { units } = work
            && state.refuse_quantum.get() == Some(units)
        {
            state.fired.set(true);
            return Err(RunError::Refused(Incomplete::Interrupted));
        }
        if state.pending_refusal.replace(false) {
            state.fired.set(true);
            state.visits_at_refusal.set(state.old_visits.get());
            return Err(RunError::Refused(Incomplete::Interrupted));
        }
        if matches!(
            work,
            ExecutionWork::Work {
                units: RETIREMENT_WORK
            }
        ) && state.old_complete.get() != 0
            && !state.fired.replace(true)
        {
            let salt = match self.mutation {
                Mutation::None => return Ok(()),
                Mutation::Validate => 0,
                Mutation::Replace => u32::MAX,
            };
            state
                .callback_id
                .set(Some(self.kind.ordinary_intern(self.db, salt)));
        }
        Ok(())
    }
}

struct UnusedProvider;

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for UnusedProvider
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Collision, Payload),
            Output<'a> = usize,
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
        _input: (Collision, Payload),
    ) -> RunResult<usize> {
        Err(RunError::RequiresFetch)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Collision, Payload),
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
        _input: (Collision, Payload),
    ) -> RunResult<usize>
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

fn run(db: &QuoteDb, kind: Kind, mutation: Mutation) -> RunResult<Id> {
    let admission = Admission { db, kind, mutation };
    let provider = UnusedProvider;
    let mut registry = RegistryBuilder::new(db, &admission)?;
    match kind {
        Kind::Query => {
            let route =
                registry.reserve(db as &dyn Database, count::fn_ingredient_(db, db.zalsa()))?;
            let keys = registry.query_keys::<_, Profile>(&route)?;
            let provider = registry.provider(&provider)?;
            registry.bind_executable(&route, &provider)?;
            registry.seal()?.run(|endpoint| async move {
                let fields = endpoint
                    .local_call(|| {
                        endpoint.admit_work(3)?;
                        endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: size_of::<(Collision, Payload)>() + 1,
                        })?;
                        Ok((Collision(100), Payload::new(100)))
                    })
                    .await;
                state().in_key.set(true);
                let _scope = KeyScope;
                Ok(endpoint.intern_query_key(&keys, fields).await)
            })
        }
        Kind::Value => {
            let values =
                registry.finite_interned_values_with_memos(Value::ingredient(db.zalsa()), ())?;
            registry.seal()?.run(|endpoint| async move {
                let fields = endpoint
                    .local_call(|| {
                        endpoint.admit_work(3)?;
                        endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: size_of::<(Collision, Payload)>() + 1,
                        })?;
                        Ok((Collision(100), Payload::new(100)))
                    })
                    .await;
                state().in_key.set(true);
                let _scope = KeyScope;
                Ok(endpoint.intern_value(&values, fields).await.as_id())
            })
        }
    }
}

fn stale(kind: Kind) -> (QuoteDb, Id) {
    let mut db = QuoteDb::default();
    let request = Request::new(&db, 0, 0, None);
    let revisions = match kind {
        Kind::Query => revisions(count::fn_ingredient_(&db, db.zalsa())),
        Kind::Value => <Value<'static> as crate::interned::Configuration>::REVISIONS.get(),
    };
    let mut old = None;
    for revision in 0..revisions {
        if revision != 0 {
            request.set_left(&mut db).to(revision as u32);
        }
        match kind {
            Kind::Query => {
                seed_key(&db, request);
            }
            Kind::Value => {
                seed_value(&db, request);
            }
        }
        if revision == 0 {
            old = Some(kind.ids(&db)[0]);
        }
    }
    request.set_left(&mut db).to(revisions as u32);
    (db, old.unwrap())
}

fn complete(db: &QuoteDb, kind: Kind, mutation: Mutation) -> Id {
    let result = try_with_attempt(db, 100_000, || run(db, kind, mutation)).unwrap();
    let AttemptOutcome::Complete(Ok(id)) = result else {
        panic!("{kind:?}: {result:?}");
    };
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(!state().in_key.get());
    id
}

#[test]
fn exhausted_field_quote_stops_before_the_next_visit_and_preserves_the_old_generation() {
    // Refuse the next admission after a partial quote, before it can restart. Allowing
    // admission on a fresh attempt must complete quotation and reuse the preserved slot.
    for kind in [Kind::Query, Kind::Value] {
        let (state, _reset) = fixture();
        let (db, old) = stale(kind);
        let stamp = Stamp::current(&db);
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        let memo = matches!(kind, Kind::Query).then(|| {
            std::ptr::from_ref(stored(&db, count::fn_ingredient_(&db, db.zalsa()), old).unwrap())
                .addr()
        });
        state.refuse_partial.set(true);
        assert_eq!(
            try_with_attempt(&db, 100_000, || run(&db, kind, Mutation::None)),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted)),
            "{kind:?}"
        );
        assert!(state.fired.get());
        assert!(state.old_partial.get() > 0);
        assert!(state.old_visits.get() < OLD_LENGTH);
        assert_eq!(state.old_visits.get(), state.visits_at_refusal.get());
        assert_eq!(state.old_complete.get(), 0);
        assert_eq!(state.old_drops.get(), 0);
        assert_eq!(detached.acquired.get(), 0);
        assert!(kind.ids(&db).contains(&old));
        if let Some(memo) = memo {
            assert_eq!(
                std::ptr::from_ref(
                    stored(&db, count::fn_ingredient_(&db, db.zalsa()), old).unwrap()
                )
                .addr(),
                memo
            );
        }
        assert!(stamp.belongs_to(&db));
        state.refuse_partial.set(false);
        state.fired.set(false);
        let new = complete(&db, kind, Mutation::None);
        assert_eq!(new.index(), old.index());
        assert_eq!(new.generation(), old.generation() + 1);
        assert!(state.old_complete.get() >= 2);
        assert_eq!(state.old_drops.get(), 1);
        assert_eq!(detached.acquired.get(), 1);
        assert_eq!(detached.retired.get(), 1);
        assert!(stamp.belongs_to(&db));
        assert_eq!(complete(&db, kind, Mutation::None), new);
    }
}

#[test]
fn admission_callback_validation_or_replacement_requires_fresh_selection() {
    // Validation preserves the ID but makes its slot ineligible for reuse. Replacement
    // changes the generation. Either callback invalidates the request's earlier selection.
    for kind in [Kind::Query, Kind::Value] {
        for mutation in [Mutation::Validate, Mutation::Replace] {
            let (state, _reset) = fixture();
            let (db, old) = stale(kind);
            let stamp = Stamp::current(&db);
            let new = complete(&db, kind, mutation);
            assert!(state.fired.get());
            let callback = state.callback_id.get().unwrap();
            assert_ne!(new.index(), old.index());
            if matches!(mutation, Mutation::Validate) {
                assert_eq!(callback, old);
                assert_eq!(state.old_drops.get(), 0);
            } else {
                assert_eq!(callback.index(), old.index());
                assert_eq!(callback.generation(), old.generation() + 1);
                assert_eq!(state.old_drops.get(), 1);
            }
            assert!(kind.ids(&db).contains(&callback));
            assert!(stamp.belongs_to(&db));
            assert_eq!(complete(&db, kind, Mutation::None), new);
            assert!(kind.ids(&db).contains(&callback));
        }
    }
}

#[test]
fn exhausted_lru_visit_retries_without_inserting_a_cold_value() {
    for kind in [Kind::Query, Kind::Value] {
        let (state, _reset) = fixture();
        let (db, old) = stale(kind);
        let old = kind.exhaust(&db, old);
        let count = kind.ids(&db).len();
        let stamp = Stamp::current(&db);
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        // One unit reads the submitted byte. A second removes the maximum-generation slot
        // from the LRU reuse queue, preserving canonical storage. Refuse the next four-unit
        // pass before it can inspect another slot or confirm that fresh storage is needed.
        state.refuse_quantum.set(Some(4));
        assert_eq!(
            try_with_attempt(&db, 100_000, || run(&db, kind, Mutation::None)),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted)),
            "{kind:?}"
        );
        assert!(state.fired.get());
        assert_eq!(state.old_visits.get(), 0);
        assert_eq!(state.old_drops.get(), 0);
        assert_eq!(kind.ids(&db).len(), count);
        assert!(kind.ids(&db).contains(&old));
        assert_eq!(detached.acquired.get(), 0);
        state.refuse_quantum.set(None);
        let new = complete(&db, kind, Mutation::None);
        assert_ne!(new.index(), old.index());
        assert_eq!(kind.ids(&db).len(), count + 1);
        assert!(kind.ids(&db).contains(&old));
        assert_eq!(state.old_drops.get(), 0);
        assert_eq!(detached.acquired.get(), 0);
        assert!(stamp.belongs_to(&db));
        assert_eq!(complete(&db, kind, Mutation::None), new);
    }
}

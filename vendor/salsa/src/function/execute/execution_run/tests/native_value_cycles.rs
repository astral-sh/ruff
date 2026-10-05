use std::cell::{Cell, RefCell};

use super::super::registration::{
    CallableRoute, CallableRouteProvider, RegistryBuilder, TaskEndpoint,
};
use super::super::{ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionLimits, Incomplete, try_with_execution_budget,
};
use crate::execution_probe::{NativeValueOperation, NativeValueQuote, RetainedInput};
use crate::function::memo::FinalSourceMemo;
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::plumbing::CycleRecoveryStrategy;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, Id};

const RESERVE: usize = 1_000_000;
const PAYLOAD: usize = 33;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Body,
    ColdInitial,
    Initial,
    Recovery,
}

#[derive(Debug, Eq, PartialEq)]
enum Event {
    InputQuote(Phase),
    Clone,
    Initial(Phase),
    Recovery,
    ComparisonQuote(u32, u32),
    Equality(u32, u32),
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = RefCell::new(Vec::with_capacity(128));
}

fn record(event: Event) {
    EVENTS.with_borrow_mut(|events| {
        assert!(events.len() < events.capacity());
        events.push(event);
    });
}

fn take_events() -> Vec<Event> {
    EVENTS.with_borrow_mut(|events| events.drain(..).collect())
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct Key(Box<[u8]>);

impl Clone for Key {
    fn clone(&self) -> Self {
        record(Event::Clone);
        Self(self.0.clone())
    }
}

#[derive(Debug)]
struct Value {
    bytes: Box<[u8]>,
    generation: u32,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        record(Event::Equality(self.generation, other.generation));
        self.bytes == other.bytes
    }
}

impl Eq for Value {}

#[crate::input]
struct Input {
    #[returns(copy)]
    seed: u8,
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn fixpoint(db: &dyn Database, input: Input, key: Key) -> Value {
    let previous = fixpoint(db, input, key);
    Value {
        bytes: vec![(previous.bytes[0] + 1).min(2); PAYLOAD].into_boxed_slice(),
        generation: previous.generation + 1,
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_result = initial)]
fn fallback(db: &dyn Database, input: Input, key: Key) -> Value {
    let previous = fallback(db, input, key);
    Value {
        bytes: vec![previous.bytes[0] + 1; PAYLOAD].into_boxed_slice(),
        generation: previous.generation + 1,
    }
}

fn initial(db: &dyn Database, _id: Id, input: Input, _key: Key) -> Value {
    Value {
        bytes: vec![input.seed(db); PAYLOAD].into_boxed_slice(),
        generation: 0,
    }
}

fn recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _last: &Value,
    value: Value,
    _input: Input,
    _key: Key,
) -> Value {
    value
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    None,
    Input(Phase),
    Comparison,
}

struct Provider<'run, 'db: 'run, C: Configuration> {
    route: CallableRoute<'run, 'db, C>,
    id: Id,
    phase: Cell<Phase>,
    fault: Fault,
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = (Input, Key), Output<'a> = Value>,
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
        match operation {
            NativeValueOperation::InputConversion(RetainedInput::Interned((_, key))) => {
                let phase = self.phase.get();
                record(Event::InputQuote(phase));
                Ok(NativeValueQuote {
                    work: key
                        .0
                        .len()
                        .checked_add(1)
                        .ok_or(RunError::Contract("cycle input quote overflow"))?,
                    requested_bytes: if self.fault == Fault::Input(phase) {
                        RESERVE + 1
                    } else {
                        key.0.len()
                    },
                    cleanup_work: 1,
                })
            }
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => Err(
                RunError::Contract("cycle fixture requires retained tuple fields"),
            ),
            NativeValueOperation::Comparison { left, right } => {
                record(Event::ComparisonQuote(left.generation, right.generation));
                Ok(NativeValueQuote {
                    work: if self.fault == Fault::Comparison {
                        RESERVE + 1
                    } else {
                        left.bytes
                            .len()
                            .checked_add(right.bytes.len())
                            .and_then(|work| work.checked_add(1))
                            .ok_or(RunError::Contract("cycle comparison quote overflow"))?
                    },
                    requested_bytes: 0,
                    cleanup_work: 0,
                })
            }
        }
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _input: (Input, Key),
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        self.phase.set(Phase::ColdInitial);
        let previous = endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.route, self.id)?.await })
            .await;
        self.phase.set(
            if C::CYCLE_STRATEGY == CycleRecoveryStrategy::FallbackImmediate {
                Phase::Initial
            } else {
                Phase::Recovery
            },
        );
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(PAYLOAD * 2 + 1)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: PAYLOAD,
                })?;
                Ok(Value {
                    bytes: vec![(previous.bytes[0] + 1).min(2); PAYLOAD].into_boxed_slice(),
                    generation: previous.generation + 1,
                })
            })
            .await)
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Database,
        id: Id,
        input: (Input, Key),
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(PAYLOAD * 2 + 1)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: PAYLOAD,
                })?;
                record(Event::Initial(self.phase.get()));
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        value: Value,
        _input: (Input, Key),
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                record(Event::Recovery);
                self.phase.set(Phase::Body);
                Ok(())
            })
            .await;
        Ok(value)
    }
}

fn run<'db, C>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
    fault: Fault,
) -> Result<AttemptOutcome<RunResult<&'db Value>>, attempt_probe::StartError>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = (Input, Key), Output<'a> = Value>,
{
    try_with_execution_budget(
        db,
        ExecutionLimits {
            semantic_work: RESERVE,
            requested_bytes: RESERVE,
        },
        |budget| {
            let mut registry = RegistryBuilder::with_budget(db, &budget)?;
            let route = registry.reserve_callable(db, ingredient)?;
            registry.bind_callable(
                &route,
                Provider {
                    route: route.clone(),
                    id,
                    phase: Cell::new(Phase::Body),
                    fault,
                },
            )?;
            registry.seal()?.run(move |endpoint| async move {
                Ok(endpoint
                    .child_call(|| async { endpoint.fetch_ref(&route, id)?.await })
                    .await)
            })
        },
    )
}

fn assert_idle<C: Configuration>(db: &dyn Database, ingredient: &IngredientImpl<C>, id: Id) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(matches!(
        ingredient
            .sync_table
            .peek_claim(db.zalsa(), id, Reentrancy::Deny),
        ClaimResult::Claimed(())
    ));
}

fn assert_input_refusal<C>(db: &dyn Database, ingredient: &IngredientImpl<C>, id: Id, phase: Phase)
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = (Input, Key), Output<'a> = Value>,
{
    let stamp = Stamp::current(db);
    take_events();
    let outcome = run(db, ingredient, id, Fault::Input(phase));
    let events = take_events();
    assert!(
        matches!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
        ),
        "{phase:?}: {outcome:?}; {events:?}"
    );
    let refusal = events
        .iter()
        .position(|event| *event == Event::InputQuote(phase))
        .expect("selected conversion must request its profile");
    assert!(
        !events[refusal + 1..]
            .iter()
            .any(|event| matches!(event, Event::Clone | Event::Equality(..)))
    );
    assert_idle(db, ingredient, id);
    assert_eq!(Stamp::current(db), stamp);

    let retry = run(db, ingredient, id, Fault::None);
    assert!(
        matches!(retry, Ok(AttemptOutcome::Complete(Ok(_)))),
        "{retry:?}"
    );
    assert!(FinalSourceMemo::certify(db, ingredient, id).is_ok());
    assert_idle(db, ingredient, id);
    assert_eq!(Stamp::current(db), stamp);
}

#[test]
fn cycle_conversions_refuse_before_clone_and_retry_in_the_same_revision() {
    for phase in [Phase::ColdInitial, Phase::Recovery] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 0);
        let id = fixpoint::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            (input, Key(vec![7; 97].into_boxed_slice())),
            |_, fields| fields,
        );
        assert_input_refusal(&db, fixpoint::fn_ingredient_(&db, db.zalsa()), id, phase);
    }
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 0);
    let id = fallback::intern_ingredient_(db.zalsa()).intern_id(
        db.zalsa(),
        db.zalsa_local(),
        (input, Key(vec![7; 97].into_boxed_slice())),
        |_, fields| fields,
    );
    assert_input_refusal(
        &db,
        fallback::fn_ingredient_(&db, db.zalsa()),
        id,
        Phase::Initial,
    );
    let events = take_events();
    assert!(
        events
            .iter()
            .any(|event| *event == Event::Initial(Phase::Initial))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::ComparisonQuote(..) | Event::Equality(..)))
    );
}

#[test]
fn cycle_comparison_refuses_before_eq_then_preserves_new_previous_operand_order() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 0);
    let id = fixpoint::intern_ingredient_(db.zalsa()).intern_id(
        db.zalsa(),
        db.zalsa_local(),
        (input, Key(vec![7; 97].into_boxed_slice())),
        |_, fields| fields,
    );
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let stamp = Stamp::current(&db);
    take_events();
    let outcome = run(&db, ingredient, id, Fault::Comparison);
    let events = take_events();
    assert!(
        matches!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        ),
        "{outcome:?}; {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| *event == Event::ComparisonQuote(1, 0))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Equality(..)))
    );
    assert_idle(&db, ingredient, id);
    assert_eq!(Stamp::current(&db), stamp);

    let outcome = run(&db, ingredient, id, Fault::None);
    let Ok(AttemptOutcome::Complete(Ok(value))) = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(value.bytes.as_ref(), &[2; PAYLOAD]);
    let events = take_events();
    let quoted: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::ComparisonQuote(left, right) => Some((*left, *right)),
            _ => None,
        })
        .collect();
    let compared: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Equality(left, right) => Some((*left, *right)),
            _ => None,
        })
        .collect();
    assert!(!compared.is_empty());
    assert_eq!(quoted, compared);
    assert!(compared.iter().all(|(left, right)| *left == *right + 1));
    assert!(FinalSourceMemo::certify(&db as &dyn Database, ingredient, id).is_ok());
    assert_idle(&db, ingredient, id);
    assert_eq!(Stamp::current(&db), stamp);
}

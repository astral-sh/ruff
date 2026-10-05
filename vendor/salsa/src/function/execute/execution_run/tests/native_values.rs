use std::cell::RefCell;
use std::future::{Future, ready};

use super::super::registration::{CallableRouteProvider, RegistryBuilder, TaskEndpoint};
use super::super::{ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionLimits, Incomplete, try_with_execution_budget,
};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, Id};

const RESERVE: usize = 1_000_000;

#[derive(Debug, PartialEq)]
enum Event {
    Clone(usize),
    InputDrop(usize),
    Body,
    Equality {
        left: u32,
        right: u32,
        lengths: (usize, usize),
    },
    OutputDrop(u32),
    #[cfg(not(feature = "shuttle"))]
    ChildDrop,
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = RefCell::new(Vec::with_capacity(64));
}

fn record(event: Event) {
    EVENTS.with_borrow_mut(|events| {
        assert!(
            events.len() < events.capacity(),
            "event recording must not allocate during native work"
        );
        events.push(event);
    });
}

fn take_events() -> Vec<Event> {
    EVENTS.with_borrow_mut(|events| events.drain(..).collect())
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct OwnedKey(Box<[u8]>);

impl Clone for OwnedKey {
    fn clone(&self) -> Self {
        record(Event::Clone(self.0.len()));
        Self(self.0.clone())
    }
}

impl Drop for OwnedKey {
    fn drop(&mut self) {
        record(Event::InputDrop(self.0.len()));
    }
}

#[derive(Debug)]
struct Value {
    bytes: Box<[u8]>,
    // The generation identifies operands without changing the value being compared.
    generation: u32,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        record(Event::Equality {
            left: self.generation,
            right: other.generation,
            lengths: (self.bytes.len(), other.bytes.len()),
        });
        self.bytes == other.bytes
    }
}

impl Eq for Value {}

impl Drop for Value {
    fn drop(&mut self) {
        record(Event::OutputDrop(self.generation));
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    generation: u32,
    #[returns(copy)]
    byte: u8,
    #[returns(copy)]
    length: usize,
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn value(db: &dyn Database, input: Input, _key: OwnedKey) -> Value {
    Value {
        bytes: vec![input.byte(db); input.length(db)].into_boxed_slice(),
        generation: input.generation(db),
    }
}

async fn body<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    db: &'db dyn Database,
    (input, _key): (Input, OwnedKey),
) -> RunResult<Value> {
    Ok(endpoint
        .local_call(|| {
            let length = input.length(db);
            // Production prepays the candidate's cleanup; comparison only borrows it.
            endpoint.admit_work(
                length
                    .checked_mul(2)
                    .ok_or(RunError::Contract("fixture size overflow"))?,
            )?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: length,
            })?;
            record(Event::Body);
            Ok(Value {
                bytes: vec![input.byte(db); length].into_boxed_slice(),
                generation: input.generation(db),
            })
        })
        .await)
}

struct Unprofiled;

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Unprofiled
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Input, OwnedKey),
            Output<'a> = Value,
        >,
{
    fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Database,
        input: (Input, OwnedKey),
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        body(endpoint, db, input)
    }

    fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Input, OwnedKey),
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract("acyclic fixture requested initial")))
    }

    fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        _value: Value,
        _input: (Input, OwnedKey),
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract(
            "acyclic fixture requested recovery",
        )))
    }
}

macro_rules! run {
    ($db:expr, $ingredient:expr, $id:expr, $provider:expr) => {{
        let db = $db;
        let ingredient = $ingredient;
        let id = $id;
        let provider = $provider;
        try_with_execution_budget(
            db,
            ExecutionLimits {
                semantic_work: RESERVE,
                requested_bytes: RESERVE,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                let route = registry.reserve_callable(db as &dyn Database, ingredient)?;
                registry.bind_callable(&route, provider)?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, id)?.await })
                        .await)
                })
            },
        )
    }};
}

fn intern(db: &dyn Database, input: Input, length: usize) -> Id {
    value::intern_ingredient_(db.zalsa()).intern_id(
        db.zalsa(),
        db.zalsa_local(),
        (input, OwnedKey(vec![7; length].into_boxed_slice())),
        |_, fields| fields,
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

#[test]
fn missing_profile_refuses_before_generated_owned_input_clone() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 0, 3, 17);
    let id = intern(&db, input, 4097);
    let ingredient = value::fn_ingredient_(&db, db.zalsa());
    let stamp = Stamp::current(&db);
    take_events();
    for _ in 0..2 {
        let outcome = run!(&db, ingredient, id, Unprofiled);
        let events = take_events();
        assert!(
            matches!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            ),
            "unexpected outcome: {outcome:?}; native events: {events:?}"
        );
        assert!(
            events.is_empty(),
            "unprofiled conversion entered Clone or the body"
        );
        assert_idle(&db, ingredient, id);
        assert_eq!(Stamp::current(&db), stamp);
    }
}

mod profiled {
    use super::super::super::native_values::{
        NativeValueOperation, NativeValueQuote, RetainedInput,
    };
    use super::*;
    use crate::function::FunctionIngredient;
    use crate::function::memo::FinalSourceMemo;
    use crate::{Setter, prepared_source_probe};

    #[derive(Clone, Copy, Debug)]
    enum Fault {
        None,
        InputWork,
        InputBytes,
        InputOverflow,
        MissingComparison,
        ComparisonWork,
        ComparisonBytes,
        ComparisonOverflow,
    }

    struct Profiled(Fault);

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Profiled
    where
        C: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = (Input, OwnedKey),
                Output<'a> = Value,
            >,
    {
        fn native_value<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            operation: NativeValueOperation<'call, 'db, C>,
        ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
        where
            'run: 'call,
        {
            let result = (|| match operation {
                NativeValueOperation::InputConversion(RetainedInput::Interned((_, key))) => {
                    let length = key.0.len();
                    Ok(NativeValueQuote {
                        work: if matches!(self.0, Fault::InputWork) {
                            RESERVE + 1
                        } else if matches!(self.0, Fault::InputOverflow) {
                            usize::MAX
                        } else {
                            length
                                .checked_add(1)
                                .ok_or(RunError::Contract("fixture size overflow"))?
                        },
                        requested_bytes: if matches!(self.0, Fault::InputBytes) {
                            RESERVE + 1
                        } else {
                            length
                        },
                        cleanup_work: 1,
                    })
                }
                NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                    Err(RunError::Contract("fixture expected retained tuple fields"))
                }
                NativeValueOperation::Comparison { left, right } => {
                    if matches!(self.0, Fault::MissingComparison) {
                        Err(RunError::Contract("native value operation has no profile"))
                    } else {
                        Ok(NativeValueQuote {
                            work: if matches!(self.0, Fault::ComparisonWork) {
                                RESERVE + 1
                            } else if matches!(self.0, Fault::ComparisonOverflow) {
                                usize::MAX
                            } else {
                                left.bytes
                                    .len()
                                    .checked_add(right.bytes.len())
                                    .and_then(|length| length.checked_add(1))
                                    .ok_or(RunError::Contract("fixture size overflow"))?
                            },
                            requested_bytes: if matches!(self.0, Fault::ComparisonBytes) {
                                RESERVE + 1
                            } else {
                                0
                            },
                            cleanup_work: usize::from(matches!(self.0, Fault::ComparisonOverflow)),
                        })
                    }
                }
            })();
            ready(result)
        }

        fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: (Input, OwnedKey),
        ) -> impl Future<Output = RunResult<Value>> + 'call
        where
            'run: 'call,
        {
            body(endpoint, db, input)
        }

        fn initial<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            _id: Id,
            _input: (Input, OwnedKey),
        ) -> impl Future<Output = RunResult<Value>> + 'call
        where
            'run: 'call,
        {
            ready(Err(RunError::Contract("acyclic fixture requested initial")))
        }

        fn recover<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            _cycle: &'call Cycle<'call>,
            _last: &'call Value,
            _value: Value,
            _input: (Input, OwnedKey),
        ) -> impl Future<Output = RunResult<Value>> + 'call
        where
            'run: 'call,
        {
            ready(Err(RunError::Contract(
                "acyclic fixture requested recovery",
            )))
        }
    }

    fn complete(
        outcome: Result<AttemptOutcome<RunResult<&Value>>, attempt_probe::StartError>,
    ) -> &Value {
        let Ok(AttemptOutcome::Complete(Ok(value))) = outcome else {
            panic!("profiled query did not complete: {outcome:?}");
        };
        value
    }

    #[test]
    fn owned_input_quotes_refuse_before_clone_and_allow_same_revision_retry() {
        for (fault, reason) in [
            (Fault::InputWork, Incomplete::Allowance),
            (Fault::InputBytes, Incomplete::RequestedAllocation),
            (Fault::InputOverflow, Incomplete::Interrupted),
        ] {
            let db = DatabaseImpl::default();
            let input = Input::new(&db, 0, 3, 17);
            let id = intern(&db, input, 4097);
            let ingredient = value::fn_ingredient_(&db, db.zalsa());
            let stamp = Stamp::current(&db);
            take_events();
            assert!(
                matches!(run!(&db, ingredient, id, Profiled(fault)),
                Ok(AttemptOutcome::Incomplete(actual)) if actual == reason),
                "{fault:?}"
            );
            assert!(
                take_events().is_empty(),
                "refused conversion entered Clone or the body"
            );
            assert_idle(&db, ingredient, id);
            let selected = complete(run!(&db, ingredient, id, Profiled(Fault::None)));
            assert_eq!(selected.bytes.as_ref(), &[3; 17]);
            assert_eq!(
                take_events(),
                [Event::Clone(4097), Event::Body, Event::InputDrop(4097)]
            );
            assert_eq!(Stamp::current(&db), stamp);
            assert_idle(&db, ingredient, id);
        }
    }

    #[test]
    fn comparison_refusal_preserves_ordinary_memo_and_drops_only_candidate() {
        for (fault, reason) in [
            (Fault::MissingComparison, Incomplete::Interrupted),
            (Fault::ComparisonWork, Incomplete::Allowance),
            (Fault::ComparisonBytes, Incomplete::RequestedAllocation),
            (Fault::ComparisonOverflow, Incomplete::Interrupted),
        ] {
            for changed in [false, true] {
                let mut db = DatabaseImpl::default();
                let input = Input::new(&db, 0, 3, 4097);
                assert_eq!(
                    value(&db, input, OwnedKey(vec![7; 1025].into_boxed_slice()))
                        .bytes
                        .len(),
                    4097
                );
                let id = intern(&db, input, 1025);
                let original = {
                    let ingredient = value::fn_ingredient_(&db, db.zalsa());
                    std::ptr::from_ref(ingredient.memo(db.zalsa(), id).unwrap().header())
                };
                let previous_changed = value::fn_ingredient_(&db, db.zalsa())
                    .memo(db.zalsa(), id)
                    .unwrap()
                    .header()
                    .revisions
                    .changed_at;
                input.set_generation(&mut db).to(1);
                if changed {
                    input.set_length(&mut db).to(3);
                }
                let ingredient = value::fn_ingredient_(&db, db.zalsa());
                let stamp = Stamp::current(&db);
                take_events();
                assert!(
                    matches!(run!(&db, ingredient, id, Profiled(fault)),
                Ok(AttemptOutcome::Incomplete(actual)) if actual == reason),
                    "{fault:?}"
                );
                assert_eq!(
                    take_events(),
                    [
                        Event::Clone(1025),
                        Event::Body,
                        Event::InputDrop(1025),
                        Event::OutputDrop(1)
                    ]
                );
                assert_eq!(
                    std::ptr::from_ref(ingredient.memo(db.zalsa(), id).unwrap().header()),
                    original
                );
                assert_idle(&db, ingredient, id);
                let captured = prepared_source_probe::capture(&db, || {
                    run!(&db, ingredient, id, Profiled(Fault::None))
                })
                .unwrap();
                captured.check_root_reads().unwrap();
                let selected = complete(captured.value);
                let length = if changed { 3 } else { 4097 };
                assert_eq!(selected.bytes.len(), length);
                let events = take_events();
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, Event::Equality { .. }))
                        .collect::<Vec<_>>(),
                    [&Event::Equality {
                        left: 0,
                        right: 1,
                        lengths: (4097, length)
                    }]
                );
                let memo = ingredient.memo(db.zalsa(), id).unwrap();
                assert_eq!(
                    memo.header().revisions.changed_at == previous_changed,
                    !changed
                );
                assert!(!memo.header().may_be_provisional());
                assert!(FinalSourceMemo::certify(&db as &dyn Database, ingredient, id).is_ok());
                let address = std::ptr::from_ref(memo.header());
                let cached = complete(run!(&db, ingredient, id, Unprofiled));
                assert!(std::ptr::eq(selected, cached));
                assert!(take_events().is_empty());
                assert!(std::ptr::eq(
                    selected,
                    value(&db, input, OwnedKey(vec![7; 1025].into_boxed_slice()))
                ));
                assert_eq!(
                    std::ptr::from_ref(ingredient.memo(db.zalsa(), id).unwrap().header()),
                    address
                );
                assert!(!take_events().iter().any(|event| matches!(
                    event,
                    Event::Body | Event::Equality { .. } | Event::Clone(_)
                )));
                assert_eq!(Stamp::current(&db), stamp);
                assert_idle(&db, ingredient, id);
            }
        }
    }
    #[cfg(not(feature = "shuttle"))]
    mod boundary {
        use std::future::poll_fn;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        use std::task::Poll;

        use super::super::super::super::ExecutionAdmission;
        use super::*;
        use crate::DatabaseKeyIndex;
        use crate::attempt_probe::try_with_attempt;

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Point {
            Input,
            Comparison,
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Failure {
            QueuedChild,
            RevisionCancellation,
        }

        struct Hook {
            point: Point,
            failure: Failure,
            endpoint: Option<TaskEndpoint<'static, 'static>>,
            target: DatabaseKeyIndex,
            fired: bool,
            child_dropped: bool,
        }

        thread_local! {
            static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
        }

        struct ClearHook;

        impl Drop for ClearHook {
            fn drop(&mut self) {
                HOOK.with_borrow_mut(|hook| *hook = None);
            }
        }

        struct Child {
            db: &'static dyn Database,
            target: DatabaseKeyIndex,
            caller: Option<DatabaseKeyIndex>,
            depths: (usize, usize),
        }

        impl Drop for Child {
            fn drop(&mut self) {
                assert_eq!(
                    self.db.zalsa_local().active_query().map(|(key, _)| key),
                    self.caller
                );
                assert_eq!(attempt_probe::stack_depths(), self.depths);
                assert!(
                    self.db
                        .zalsa()
                        .lookup_ingredient(self.target.ingredient_index())
                        .as_function()
                        .is_some_and(|function| matches!(
                            function.sync_table().peek_claim(
                                self.db.zalsa(),
                                self.target.key_index(),
                                Reentrancy::Deny
                            ),
                            ClaimResult::Cycle { .. }
                        ))
                );
                EVENTS.with_borrow(|events| assert!(!events.contains(&Event::OutputDrop(1))));
                record(Event::ChildDrop);
                HOOK.with_borrow_mut(|hook| hook.as_mut().unwrap().child_dropped = true);
            }
        }

        struct Admission;
        static ADMISSION: Admission = Admission;

        impl ExecutionAdmission for Admission {
            fn admit(&self, work: ExecutionWork) -> RunResult<()> {
                if !matches!(work, ExecutionWork::Resource { .. }) {
                    return Ok(());
                }
                let armed = HOOK.with_borrow_mut(|hook| {
                    let hook = hook.as_mut()?;
                    let endpoint = hook.endpoint.take()?;
                    hook.fired = true;
                    Some((endpoint, hook.target, hook.failure))
                });
                let Some((endpoint, target, failure)) = armed else {
                    return Ok(());
                };
                let db = endpoint.inner.context.db;
                let child = Child {
                    db,
                    target,
                    caller: db.zalsa_local().active_query().map(|(key, _)| key),
                    depths: attempt_probe::stack_depths(),
                };
                let _reply = endpoint.demand(move || {
                    poll_fn(move |_| -> Poll<RunResult<()>> {
                        let _child = &child;
                        panic!(
                            "the child queued during native admission must drain without polling"
                        );
                    })
                })?;
                if failure == Failure::RevisionCancellation {
                    db.zalsa().runtime().set_cancellation_flag();
                }
                Ok(())
            }
        }

        struct Provider;

        impl<C> CallableRouteProvider<'static, 'static, C> for Provider
        where
            C: for<'a> Configuration<
                    DbView = dyn Database,
                    Input<'a> = (Input, OwnedKey),
                    Output<'a> = Value,
                >,
        {
            async fn native_value<'call>(
                &'call self,
                endpoint: TaskEndpoint<'static, 'static>,
                _db: &'static dyn Database,
                operation: NativeValueOperation<'call, 'static, C>,
            ) -> RunResult<NativeValueQuote>
            where
                'static: 'call,
            {
                let (point, quote) = match operation {
                    NativeValueOperation::InputConversion(RetainedInput::Interned((_, key))) => (
                        Point::Input,
                        NativeValueQuote {
                            work: key
                                .0
                                .len()
                                .checked_add(1)
                                .ok_or(RunError::Contract("fixture size overflow"))?,
                            requested_bytes: key.0.len(),
                            cleanup_work: 1,
                        },
                    ),
                    NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                        return Err(RunError::Contract("fixture expected retained tuple fields"));
                    }
                    NativeValueOperation::Comparison { left, right } => (
                        Point::Comparison,
                        NativeValueQuote {
                            work: left
                                .bytes
                                .len()
                                .checked_add(right.bytes.len())
                                .and_then(|length| length.checked_add(1))
                                .ok_or(RunError::Contract("fixture size overflow"))?,
                            requested_bytes: 0,
                            cleanup_work: 0,
                        },
                    ),
                };
                HOOK.with_borrow_mut(|hook| {
                    let hook = hook.as_mut().unwrap();
                    if hook.point == point && !hook.fired {
                        assert!(hook.endpoint.replace(endpoint).is_none());
                    }
                });
                Ok(quote)
            }

            fn body<'call>(
                &'call self,
                endpoint: TaskEndpoint<'static, 'static>,
                db: &'static dyn Database,
                input: (Input, OwnedKey),
            ) -> impl Future<Output = RunResult<Value>> + 'call
            where
                'static: 'call,
            {
                body(endpoint, db, input)
            }

            fn initial<'call>(
                &'call self,
                _endpoint: TaskEndpoint<'static, 'static>,
                _db: &'static dyn Database,
                _id: Id,
                _input: (Input, OwnedKey),
            ) -> impl Future<Output = RunResult<Value>> + 'call
            where
                'static: 'call,
            {
                ready(Err(RunError::Contract("acyclic fixture requested initial")))
            }

            fn recover<'call>(
                &'call self,
                _endpoint: TaskEndpoint<'static, 'static>,
                _db: &'static dyn Database,
                _cycle: &'call Cycle<'call>,
                _last: &'call Value,
                _value: Value,
                _input: (Input, OwnedKey),
            ) -> impl Future<Output = RunResult<Value>> + 'call
            where
                'static: 'call,
            {
                ready(Err(RunError::Contract(
                    "acyclic fixture requested recovery",
                )))
            }
        }

        fn run_hooked<C>(
            db: &'static dyn Database,
            ingredient: &'static IngredientImpl<C>,
            id: Id,
        ) -> Result<AttemptOutcome<RunResult<&'static Value>>, attempt_probe::StartError>
        where
            C: for<'a> Configuration<
                    DbView = dyn Database,
                    Input<'a> = (Input, OwnedKey),
                    Output<'a> = Value,
                >,
        {
            try_with_attempt(db, RESERVE, || {
                let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
                let route = registry.reserve_callable(db, ingredient)?;
                registry.bind_callable(&route, Provider)?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, id)?.await })
                        .await)
                })
            })
        }

        #[test]
        fn accepted_quote_keeps_owner_and_candidate_until_queued_child_drains() {
            for point in [Point::Input, Point::Comparison] {
                for failure in [Failure::QueuedChild, Failure::RevisionCancellation] {
                    let mut db = DatabaseImpl::default();
                    let input = Input::new(&db, 0, 3, 4097);
                    let original = if point == Point::Comparison {
                        assert_eq!(
                            value(&db, input, OwnedKey(vec![7; 1025].into_boxed_slice()))
                                .bytes
                                .len(),
                            4097
                        );
                        let id = intern(&db, input, 1025);
                        let original = std::ptr::from_ref(
                            value::fn_ingredient_(&db, db.zalsa())
                                .memo(db.zalsa(), id)
                                .unwrap()
                                .header(),
                        );
                        input.set_generation(&mut db).to(1);
                        Some(original)
                    } else {
                        None
                    };
                    let id = intern(&db, input, 1025);
                    // The admission observer cannot borrow a callback-local endpoint. These TLS
                    // hooks use a dedicated static database, without extending any borrowed lifetime.
                    let db: &'static DatabaseImpl = Box::leak(Box::new(db));
                    let ingredient = value::fn_ingredient_(db, db.zalsa());
                    HOOK.with_borrow_mut(|hook| {
                        assert!(hook.is_none());
                        *hook = Some(Hook {
                            point,
                            failure,
                            endpoint: None,
                            target: ingredient.database_key_index(id),
                            fired: false,
                            child_dropped: false,
                        });
                    });
                    let _clear = ClearHook;
                    let stamp = Stamp::current(db);
                    take_events();
                    let outcome = catch_unwind(AssertUnwindSafe(|| run_hooked(db, ingredient, id)));
                    db.zalsa_local().uncancel();
                    db.zalsa().runtime().reset_cancellation_flag();
                    match failure {
                        Failure::QueuedChild => assert!(matches!(
                            outcome.unwrap(),
                            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                        )),
                        Failure::RevisionCancellation => {
                            let payload =
                                outcome.expect_err("native cancellation must retain its payload");
                            assert!(matches!(
                                payload.downcast_ref::<crate::Cancelled>(),
                                Some(crate::Cancelled::PendingWrite)
                            ));
                        }
                    }
                    HOOK.with_borrow(|hook| {
                        let hook = hook.as_ref().unwrap();
                        assert!(hook.fired && hook.child_dropped && hook.endpoint.is_none());
                    });
                    let expected = match point {
                        Point::Input => vec![Event::ChildDrop],
                        Point::Comparison => vec![
                            Event::Clone(1025),
                            Event::Body,
                            Event::InputDrop(1025),
                            Event::ChildDrop,
                            Event::OutputDrop(1),
                        ],
                    };
                    assert_eq!(take_events(), expected, "{point:?}, {failure:?}");
                    if let Some(original) = original {
                        assert_eq!(
                            std::ptr::from_ref(ingredient.memo(db.zalsa(), id).unwrap().header()),
                            original
                        );
                    }
                    assert_idle(db, ingredient, id);
                    assert_eq!(Stamp::current(db), stamp);
                    assert_eq!(
                        complete(run!(db, ingredient, id, Profiled(Fault::None)))
                            .bytes
                            .len(),
                        4097
                    );
                    assert_idle(db, ingredient, id);
                    assert_eq!(Stamp::current(db), stamp);
                }
            }
        }
    }
}

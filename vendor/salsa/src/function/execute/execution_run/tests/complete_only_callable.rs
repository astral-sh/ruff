use std::cell::{Cell, RefCell};
use std::future::{Future, ready};

use super::super::registration::{
    CallableRoute, CallableRouteProvider, FinalSourceMemo, FinalSourceRoute, NativeValueOperation,
    NativeValueQuote, RegistryBuilder, TaskEndpoint,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
use crate::function::{ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id, Setter};

#[crate::input]
struct Input {
    #[returns(copy)]
    seed: u32,
}

#[derive(Debug, Eq, PartialEq)]
struct Value(Box<[u32]>);

#[derive(Debug, Eq, PartialEq)]
enum DropKind {
    Partial(usize),
    Value(usize),
    Provider,
}

#[derive(Debug)]
struct DropEvent {
    kind: DropKind,
    frame: Option<(DatabaseKeyIndex, bool)>,
    policy: QueryPolicy,
}

thread_local! {
    static DROPS: RefCell<Vec<DropEvent>> = const { RefCell::new(Vec::new()) };
    static NATIVE_BODIES: Cell<usize> = const { Cell::new(0) };
}

fn record_drop(kind: DropKind) {
    let frame = crate::with_attached_database(|db| {
        db.zalsa_local()
            .try_with_query_stack(|stack| {
                stack
                    .last()
                    .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
            })
            .flatten()
    })
    .flatten();
    DROPS.with_borrow_mut(|events| {
        events.push(DropEvent {
            kind,
            frame,
            policy: attempt_probe::current_policy(),
        });
    });
}

impl Drop for Value {
    fn drop(&mut self) {
        record_drop(DropKind::Value(self.0.len()));
    }
}

struct PartialValue(Vec<u32>);

impl Drop for PartialValue {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            record_drop(DropKind::Partial(self.0.len()));
        }
    }
}

#[crate::tracked(returns(ref), attempt = CompleteOnly)]
fn values(db: &dyn Database, input: Input) -> Value {
    NATIVE_BODIES.set(NATIVE_BODIES.get() + 1);
    let seed = input.seed(db);
    Value(Box::new([seed, seed + 1]))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Stop {
    Never,
    PartialWork,
    CompletedValue,
}

#[derive(Default)]
struct Admission {
    reject_resource: Cell<bool>,
    rejected_resource: Cell<bool>,
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if matches!(work, ExecutionWork::Resource { .. }) && self.reject_resource.replace(false) {
            self.rejected_resource.set(true);
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Calls {
    body: Cell<usize>,
    initial: Cell<usize>,
    recover: Cell<usize>,
    dropped: Cell<usize>,
}

struct ValueProvider<'run> {
    calls: &'run Calls,
    admission: &'run Admission,
    stop: Stop,
}

impl Drop for ValueProvider<'_> {
    fn drop(&mut self) {
        self.calls.dropped.set(self.calls.dropped.get() + 1);
        record_drop(DropKind::Provider);
    }
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for ValueProvider<'run>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = Value>,
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
        let work = match operation {
            NativeValueOperation::InputConversion(_) => 1,
            NativeValueOperation::Comparison { left, right } => left.0.len().max(right.0.len()) + 1,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Database,
        input: Input,
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        self.calls.body.set(self.calls.body.get() + 1);
        assert_eq!(attempt_probe::current_policy(), QueryPolicy::CompleteOnly);
        assert!(attempt_probe::current_query().is_some());
        let (mut partial, seed) = endpoint
            .local_call(|| {
                // The producer prepays destruction of either the partial buffer or its result.
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: 2 * size_of::<u32>(),
                })?;
                Ok((PartialValue(Vec::with_capacity(2)), input.seed(db)))
            })
            .await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                partial.0.push(seed);
                Ok(())
            })
            .await;
        endpoint
            .local_call(|| {
                if self.stop == Stop::PartialWork {
                    let remaining = attempt_probe::remaining_allowance_for_diagnostics(db)
                        .ok_or(RunError::Contract("fixture has no shared allowance"))?;
                    endpoint.admit_work(remaining)?;
                }
                endpoint.admit_work(1)?;
                partial.0.push(seed + 1);
                Ok(())
            })
            .await;
        let value = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(Value(std::mem::take(&mut partial.0).into_boxed_slice()))
            })
            .await;
        if self.stop == Stop::CompletedValue {
            // The next resource admission prepares memo storage after the body has returned.
            self.admission.reject_resource.set(true);
        }
        Ok(value)
    }

    fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        self.calls.initial.set(self.calls.initial.get() + 1);
        ready(Err(RunError::Contract("Panic strategy requested a seed")))
    }

    fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        _value: Value,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        self.calls.recover.set(self.calls.recover.get() + 1);
        ready(Err(RunError::Contract("Panic strategy requested recovery")))
    }
}

fn fetch<'db>(
    db: &'db DatabaseImpl,
    input: Input,
    calls: &Calls,
    admission: &Admission,
    stop: Stop,
) -> Result<AttemptOutcome<RunResult<&'db Value>>, attempt_probe::StartError> {
    try_with_attempt(db, 100_000, || {
        let ingredient = values::fn_ingredient_(db, db.zalsa());
        let mut registry = RegistryBuilder::new(db, admission)?;
        let route = registry.reserve_callable(db as &dyn Database, ingredient)?;
        registry.bind_callable(
            &route,
            ValueProvider {
                calls,
                admission,
                stop,
            },
        )?;
        registry.seal()?.run(move |endpoint| async move {
            let first = endpoint
                .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                .await;
            let second = endpoint
                .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                .await;
            assert!(std::ptr::eq(first, second));
            Ok(first)
        })
    })
}

fn inputs<C: Configuration>(
    db: &dyn Database,
    ingredient: &IngredientImpl<C>,
    input: Input,
) -> Vec<&'static str> {
    let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
    assert!(!memo.header().may_be_provisional());
    assert!(memo.header().origin().outputs().next().is_none());
    memo.header()
        .origin()
        .inputs()
        .map(|key| {
            assert_eq!(key.key_index(), input.as_id());
            let ingredient = db.zalsa().lookup_ingredient(key.ingredient_index());
            assert!(ingredient.as_function().is_none());
            ingredient.debug_name()
        })
        .collect()
}

fn assert_idle<C: Configuration>(db: &dyn Database, ingredient: &IngredientImpl<C>, input: Input) {
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(matches!(
        ingredient
            .sync_table
            .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
        ClaimResult::Claimed(())
    ));
}

#[test]
fn complete_only_callable_reuses_the_canonical_owned_result_and_input_dependencies() {
    NATIVE_BODIES.set(0);
    let ordinary = DatabaseImpl::default();
    let ordinary_input = Input::new(&ordinary, 7);
    assert_eq!(&*values(&ordinary, ordinary_input).0, &[7, 8]);
    let expected_inputs = inputs(
        &ordinary,
        values::fn_ingredient_(&ordinary, ordinary.zalsa()),
        ordinary_input,
    );
    assert_eq!(expected_inputs.len(), 1);

    let mut db = DatabaseImpl::default();
    let input = Input::new(&db, 7);
    let calls = Calls::default();
    let admission = Admission::default();
    for _ in 0..2 {
        let result = fetch(&db, input, &calls, &admission, Stop::Never);
        let Ok(AttemptOutcome::Complete(Ok(selected))) = result else {
            panic!("CompleteOnly callable did not complete: {result:?}");
        };
        assert_eq!(&*selected.0, &[7, 8]);
        let ingredient = values::fn_ingredient_(&db, db.zalsa());
        let before =
            std::ptr::from_ref(ingredient.memo(db.zalsa(), input.as_id()).unwrap().header());
        assert!(std::ptr::eq(values(&db, input), selected));
        assert_eq!(
            std::ptr::from_ref(ingredient.memo(db.zalsa(), input.as_id()).unwrap().header()),
            before
        );
        assert_eq!(inputs(&db, ingredient, input), expected_inputs);
        assert_idle(&db, ingredient, input);
    }
    assert_eq!(calls.body.get(), 1);
    assert_eq!(calls.dropped.get(), 2);
    assert_eq!((calls.initial.get(), calls.recover.get()), (0, 0));
    assert_eq!(NATIVE_BODIES.get(), 1);

    input.set_seed(&mut db).to(11);
    assert!(matches!(
        fetch(&db, input, &calls, &admission, Stop::Never),
        Ok(AttemptOutcome::Complete(Ok(Value(values)))) if &**values == [11, 12]
    ));
    assert_eq!(calls.body.get(), 2);
    assert_eq!(NATIVE_BODIES.get(), 1);
}

#[test]
fn complete_only_refusal_drops_owned_values_and_retries_in_the_same_revision() {
    for stop in [Stop::PartialWork, Stop::CompletedValue] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        let ingredient = values::fn_ingredient_(&db, db.zalsa());
        let key = ingredient.database_key_index(input.as_id());
        let calls = Calls::default();
        let admission = Admission::default();
        let stamp = Stamp::current(&db);
        DROPS.with_borrow_mut(Vec::clear);
        assert_eq!(
            fetch(&db, input, &calls, &admission, stop),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(
            admission.rejected_resource.get(),
            stop == Stop::CompletedValue
        );
        let drops = DROPS.take();
        assert_eq!(drops.len(), 2, "unexpected cleanup: {drops:?}");
        assert_eq!(
            drops[0].kind,
            if stop == Stop::PartialWork {
                DropKind::Partial(1)
            } else {
                DropKind::Value(2)
            }
        );
        assert_eq!(drops[0].policy, QueryPolicy::CompleteOnly);
        if stop == Stop::PartialWork {
            assert_eq!(drops[0].frame, Some((key, true)));
        }
        assert_eq!(drops[1].kind, DropKind::Provider);
        assert_eq!(calls.dropped.get(), 1);
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        assert_idle(&db, ingredient, input);
        assert_eq!(Stamp::current(&db), stamp);
        assert!(matches!(
            fetch(&db, input, &calls, &admission, Stop::Never),
            Ok(AttemptOutcome::Complete(Ok(Value(values)))) if &**values == [7, 8]
        ));
        assert_eq!(calls.body.get(), 2);
        assert_eq!((calls.initial.get(), calls.recover.get()), (0, 0));
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db, ingredient, input);
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn semantic(db: &dyn Database, input: Input) -> Value {
    NATIVE_BODIES.set(NATIVE_BODIES.get() + 1);
    Value(Box::new([input.seed(db)]))
}

#[crate::tracked(returns(ref), attempt = CompleteOnly)]
fn semantic_parent(db: &dyn Database, input: Input) -> Value {
    Value(semantic(db, input).0.clone())
}

struct SemanticChild<'run, 'db: 'run, C: Configuration> {
    route: SemanticRoute<'run, 'db, C>,
}

enum SemanticRoute<'run, 'db: 'run, C: Configuration> {
    Callable(CallableRoute<'run, 'db, C>),
    Final(FinalSourceRoute<'db, C>),
}

impl<'run, 'db: 'run, C, Child> CallableRouteProvider<'run, 'db, C>
    for SemanticChild<'run, 'db, Child>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = Value>,
    Child: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = Value>,
{
    // Only input conversion is reachable: the child is rejected before this body returns a value.
    fixture_native_value!(callable, 'run, 'db, C, 1);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        input: Input,
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        match &self.route {
            SemanticRoute::Callable(route) => {
                let _child = endpoint
                    .child_call(|| async { endpoint.fetch_ref(route, input.as_id())?.await })
                    .await;
            }
            SemanticRoute::Final(route) => {
                let _child = endpoint.read_final_source(route, input.as_id()).await;
            }
        }
        Err(RunError::Contract("forbidden child was delivered"))
    }

    fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract("Panic strategy requested a seed")))
    }

    fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        _value: Value,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract("Panic strategy requested recovery")))
    }
}

#[test]
fn complete_only_callable_rejects_hot_cold_and_final_return_only_children() {
    for (hot, final_source) in [(false, false), (true, false), (true, true)] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        if hot {
            semantic(&db, input);
        }
        let native_before = NATIVE_BODIES.get();
        let ingredient = semantic::fn_ingredient_(&db, db.zalsa());
        let previous = ingredient
            .memo(db.zalsa(), input.as_id())
            .map(|memo| std::ptr::from_ref(memo.header()));
        let certificate = if final_source {
            Some(
                FinalSourceMemo::certify(&db as &dyn Database, ingredient, input.as_id())
                    .expect("the ordinary semantic memo is final"),
            )
        } else {
            None
        };
        let calls = Calls::default();
        let admission = Admission::default();
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            let child = if let Some(certificate) = certificate {
                SemanticRoute::Final(registry.register_final_source(
                    &db as &dyn Database,
                    ingredient,
                    &[certificate],
                )?)
            } else {
                let child = registry.reserve_callable(&db as &dyn Database, ingredient)?;
                registry.bind_callable(
                    &child,
                    ValueProvider {
                        calls: &calls,
                        admission: &admission,
                        stop: Stop::Never,
                    },
                )?;
                SemanticRoute::Callable(child)
            };
            let parent = registry.reserve_callable(
                &db as &dyn Database,
                semantic_parent::fn_ingredient_(&db, db.zalsa()),
            )?;
            registry.bind_callable(&parent, SemanticChild { route: child })?;
            let result: RunResult<&Value> = registry.seal()?.run(move |endpoint| async move {
                Ok(endpoint
                    .child_call(|| async { endpoint.fetch_ref(&parent, input.as_id())?.await })
                    .await)
            });
            assert_eq!(
                result,
                Err(RunError::Contract(
                    "complete-only callable requested a non-complete query"
                ))
            );
            result
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(calls.body.get(), 0);
        assert_eq!(NATIVE_BODIES.get(), native_before);
        assert_eq!(
            ingredient
                .memo(db.zalsa(), input.as_id())
                .map(|memo| std::ptr::from_ref(memo.header())),
            previous
        );
        assert!(
            semantic_parent::fn_ingredient_(&db, db.zalsa())
                .memo(db.zalsa(), input.as_id())
                .is_none()
        );
        assert_idle(&db, ingredient, input);
    }
}

#[test]
fn ordinary_complete_only_operation_cannot_enter_a_complete_only_callable() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7);
    let ingredient = values::fn_ingredient_(&db, db.zalsa());
    let calls = Calls::default();
    let admission = Admission::default();
    let outcome = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let route = registry.reserve_callable(&db as &dyn Database, ingredient)?;
        registry.bind_callable(
            &route,
            ValueProvider {
                calls: &calls,
                admission: &admission,
                stop: Stop::Never,
            },
        )?;
        let db = &db;
        registry.seal()?.run(move |endpoint| async move {
            endpoint
                .local_call(|| {
                    let _ordinary = attempt_probe::enter(
                        db.zalsa(),
                        QueryPolicy::CompleteOnly,
                        "ordinary complete-only caller",
                    );
                    assert!(matches!(
                        endpoint.fetch_ref(&route, input.as_id()),
                        Err(RunError::Contract(
                            "controlled callable has an ordinary complete-only ancestor"
                        ))
                    ));
                    Ok(())
                })
                .await;
            Ok(())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(calls.body.get(), 0);
    assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
    assert_idle(&db, ingredient, input);
}
mod terminal_cleanup {
    use std::cell::{Cell, RefCell};
    use std::future::{Future, ready};
    #[cfg(not(feature = "shuttle"))]
    use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
    use std::pin::Pin;
    #[cfg(not(feature = "shuttle"))]
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
    use crate::function::execute::execution_run::registration::{
        CallableRouteProvider, RegistryBuilder, TaskEndpoint,
    };
    use crate::function::execute::execution_run::{RunError, RunResult, Unrestricted};
    use crate::function::{
        ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy,
    };
    use crate::plumbing::AsId;
    use crate::zalsa::ZalsaDatabase;
    use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id};

    #[crate::input]
    struct Input {
        #[returns(copy)]
        value: bool,
    }

    #[crate::tracked(returns(ref), attempt = CompleteOnly)]
    fn structural(db: &dyn Database, input: Input) -> bool {
        input.value(db)
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Stage {
        Child,
        Callback,
        Root,
        Provider,
    }

    #[derive(Debug)]
    struct Snapshot {
        stage: Stage,
        frame: Option<(DatabaseKeyIndex, bool)>,
        operation_depth: usize,
        policy: QueryPolicy,
        reason: Option<Incomplete>,
        claim_held: bool,
        provider_alive: bool,
        callback_alive: bool,
        panicking: bool,
    }

    #[derive(Default)]
    struct Journal {
        drops: RefCell<Vec<Snapshot>>,
        provider_alive: Cell<bool>,
        callback_alive: Cell<bool>,
        body_polls: Cell<usize>,
        child_polled: Cell<bool>,
        initial_calls: Cell<usize>,
        recovery_calls: Cell<usize>,
    }

    struct ObserveDrop<'run, 'db, C: Configuration> {
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
        journal: &'run Journal,
        stage: Stage,
    }

    impl<'run, 'db, C: Configuration> ObserveDrop<'run, 'db, C> {
        fn for_stage(&self, stage: Stage) -> Self {
            Self {
                db: self.db,
                ingredient: self.ingredient,
                input: self.input,
                journal: self.journal,
                stage,
            }
        }
    }

    impl<C: Configuration> Drop for ObserveDrop<'_, '_, C> {
        fn drop(&mut self) {
            self.journal.drops.borrow_mut().push(Snapshot {
                stage: self.stage,
                frame: self
                    .db
                    .zalsa_local()
                    .try_with_query_stack(|stack| {
                        stack
                            .last()
                            .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
                    })
                    .flatten(),
                operation_depth: attempt_probe::stack_depths().0,
                policy: attempt_probe::current_policy(),
                reason: attempt_probe::current().and_then(|support| support.reason()),
                claim_held: matches!(
                    self.ingredient.sync_table.peek_claim(
                        self.db.zalsa(),
                        self.input.as_id(),
                        Reentrancy::Deny,
                    ),
                    ClaimResult::Cycle { .. }
                ),
                provider_alive: self.journal.provider_alive.get(),
                callback_alive: self.journal.callback_alive.get(),
                panicking: std::thread::panicking(),
            });
            match self.stage {
                Stage::Callback => self.journal.callback_alive.set(false),
                Stage::Provider => self.journal.provider_alive.set(false),
                Stage::Child | Stage::Root => {}
            }
        }
    }

    #[cfg(not(feature = "shuttle"))]
    #[derive(Debug)]
    struct NativeFailure(Arc<()>);

    enum Failure {
        Error(RunError),
        #[cfg(not(feature = "shuttle"))]
        Native(Arc<()>),
    }

    struct TerminalProvider<'run, 'db, C: Configuration> {
        owner: ObserveDrop<'run, 'db, C>,
        failure: Failure,
    }

    struct BodyFuture<'call, 'run, 'db: 'run, C: Configuration> {
        provider: &'call TerminalProvider<'run, 'db, C>,
        endpoint: TaskEndpoint<'run, 'db>,
        _owner: ObserveDrop<'run, 'db, C>,
    }

    impl<C: Configuration> Future for BodyFuture<'_, '_, '_, C> {
        type Output = RunResult<bool>;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            let provider = self.provider;
            let journal = provider.owner.journal;
            assert_eq!(journal.body_polls.replace(1), 0);
            let child = provider.owner.for_stage(Stage::Child);
            // Leaving the child queued exercises terminal cleanup while this callback still
            // owns its query frame and claim.
            let _reply = self.endpoint.demand(move || async move {
                let _child = child;
                journal.child_polled.set(true);
                Ok(())
            })?;
            match &provider.failure {
                Failure::Error(error) => Poll::Ready(Err(*error)),
                #[cfg(not(feature = "shuttle"))]
                Failure::Native(identity) => panic_any(NativeFailure(identity.clone())),
            }
        }
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for TerminalProvider<'run, 'db, C>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = bool>,
    {
        // Input conversion constructs a handle; output equality compares one bool.
        fixture_native_value!(callable, 'run, 'db, C, 1);

        fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            input: Input,
        ) -> impl Future<Output = RunResult<bool>> + 'call
        where
            'run: 'call,
        {
            assert_eq!(input.as_id(), self.owner.input.as_id());
            assert!(!self.owner.journal.callback_alive.replace(true));
            BodyFuture {
                provider: self,
                endpoint,
                _owner: self.owner.for_stage(Stage::Callback),
            }
        }

        fn initial<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            _id: Id,
            _input: Input,
        ) -> impl Future<Output = RunResult<bool>> + 'call
        where
            'run: 'call,
        {
            self.owner
                .journal
                .initial_calls
                .set(self.owner.journal.initial_calls.get() + 1);
            ready(Err(RunError::Contract(
                "terminal body entered cycle initialization",
            )))
        }

        fn recover<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            _cycle: &'call Cycle<'call>,
            _last: &'call bool,
            _value: bool,
            _input: Input,
        ) -> impl Future<Output = RunResult<bool>> + 'call
        where
            'run: 'call,
        {
            self.owner
                .journal
                .recovery_calls
                .set(self.owner.journal.recovery_calls.get() + 1);
            ready(Err(RunError::Contract(
                "terminal body entered cycle recovery",
            )))
        }
    }

    fn run_failure<'run, 'db: 'run, C>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
        journal: &'run Journal,
        failure: Failure,
    ) -> RunResult<bool>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = bool>,
    {
        let mut registry = RegistryBuilder::new(db, &Unrestricted)?;
        let route = registry.reserve_callable(db, ingredient)?;
        assert!(!journal.provider_alive.replace(true));
        let owner = ObserveDrop {
            db,
            ingredient,
            input,
            journal,
            stage: Stage::Provider,
        };
        let root = owner.for_stage(Stage::Root);
        registry.bind_callable(&route, TerminalProvider { owner, failure })?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = root;
            Ok(*endpoint.fetch_ref(&route, input.as_id())?.await?)
        })
    }

    fn assert_cleanup(
        journal: &Journal,
        key: DatabaseKeyIndex,
        reason: Option<Incomplete>,
        native: bool,
    ) {
        let snapshots = journal.drops.borrow();
        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.stage)
                .collect::<Vec<_>>(),
            [Stage::Child, Stage::Callback, Stage::Root, Stage::Provider]
        );
        for snapshot in &snapshots[..2] {
            assert_eq!(snapshot.frame, Some((key, reason.is_some())));
            assert_eq!(snapshot.operation_depth, 1);
            assert_eq!(snapshot.policy, QueryPolicy::CompleteOnly);
            assert!(snapshot.claim_held);
            assert!(snapshot.callback_alive);
        }
        for snapshot in &snapshots[2..] {
            assert_eq!(snapshot.frame, None);
            assert_eq!(snapshot.operation_depth, 0);
            assert!(!snapshot.claim_held);
            assert!(!snapshot.callback_alive);
        }
        for snapshot in snapshots.iter() {
            assert!(snapshot.provider_alive);
            assert_eq!(snapshot.reason, reason);
            assert_eq!(snapshot.panicking, native);
        }
        assert_eq!(journal.body_polls.get(), 1);
        assert!(!journal.child_polled.get());
        assert_eq!(journal.initial_calls.get(), 0);
        assert_eq!(journal.recovery_calls.get(), 0);
        assert!(!journal.provider_alive.get());
        assert!(!journal.callback_alive.get());
    }

    fn assert_idle(db: &dyn Database) {
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
    }

    #[test]
    fn terminal_error_retains_complete_only_frame_until_callback_cleanup() {
        for (error, reason) in [
            (
                RunError::Contract("exact complete-only terminal error"),
                Incomplete::Interrupted,
            ),
            (
                RunError::Refused(Incomplete::Allowance),
                Incomplete::Allowance,
            ),
        ] {
            let db = DatabaseImpl::default();
            let input = Input::new(&db, true);
            let ingredient = structural::fn_ingredient_(&db, db.zalsa());
            let journal = Journal::default();
            assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
            let outcome = try_with_attempt(&db, 100_000, || {
                let result = run_failure(&db, ingredient, input, &journal, Failure::Error(error));
                assert_eq!(result, Err(error));
            });
            assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(reason)));
            assert_cleanup(
                &journal,
                ingredient.database_key_index(input.as_id()),
                Some(reason),
                false,
            );
            assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
            assert_idle(&db);
            assert!(*structural(&db, input));
        }
    }

    #[test]
    #[cfg(not(feature = "shuttle"))]
    fn native_panic_retains_complete_only_frame_and_exact_payload_until_cleanup() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, true);
        let ingredient = structural::fn_ingredient_(&db, db.zalsa());
        let journal = Journal::default();
        let identity = Arc::new(());
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || {
                run_failure(
                    &db,
                    ingredient,
                    input,
                    &journal,
                    Failure::Native(identity.clone()),
                )
            })
        }));
        let payload = outcome.expect_err("the provider must preserve its native panic");
        let native = payload
            .downcast_ref::<NativeFailure>()
            .expect("the original native payload");
        assert!(Arc::ptr_eq(&native.0, &identity));
        assert_cleanup(
            &journal,
            ingredient.database_key_index(input.as_id()),
            None,
            true,
        );
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        assert_idle(&db);
    }
}
mod cycles {
    use std::cell::{Cell, RefCell};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use crate::attempt_probe::{
        self, AttemptOutcome, ExecutionLimits, QueryPolicy, try_with_execution_budget,
    };
    use crate::function::execute::execution_run::registration::{
        CallableRoute, CallableRouteProvider, RegistryBuilder, TaskEndpoint,
    };
    use crate::function::execute::execution_run::{RUN_ACTIVE, RunResult};
    use crate::function::memo::FinalSourceMemo;
    use crate::function::{
        ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy,
    };
    use crate::plumbing::{AsId, CycleRecoveryStrategy};
    use crate::prepared_source_probe::Stamp;
    use crate::zalsa::ZalsaDatabase;
    use crate::{Cycle, Database, DatabaseImpl, Id};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Body,
        ProviderInitial,
        Initial,
        ProviderRecovery,
        Recovery { previous: u32, value: u32 },
    }

    thread_local! {
        static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    }

    fn record(event: Event) {
        EVENTS.with_borrow_mut(|events| events.push(event));
    }

    fn take_events() -> Vec<Event> {
        EVENTS.with_borrow_mut(std::mem::take)
    }

    #[crate::input]
    struct Input {
        #[returns(copy)]
        limit: u32,
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn panicking(db: &dyn Database, input: Input) -> u32 {
        record(Event::Body);
        (panicking(db, input) + 1).min(input.limit(db))
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial, cycle_fn = recover)]
    fn fixpoint(db: &dyn Database, input: Input) -> u32 {
        record(Event::Body);
        (fixpoint(db, input) + 1).min(input.limit(db))
    }

    fn initial(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        record(Event::Initial);
        0
    }

    fn recover(
        _db: &dyn Database,
        _cycle: &Cycle<'_>,
        previous: &u32,
        value: u32,
        _input: Input,
    ) -> u32 {
        record(Event::Recovery {
            previous: *previous,
            value,
        });
        value
    }

    struct Provider<'run, 'db: 'run, C: Configuration> {
        route: CallableRoute<'run, 'db, C>,
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        // Input conversion copies an Input handle; output equality compares one u32.
        fixture_native_value!(callable, 'run, 'db, C, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    record(Event::Body);
                    Ok(())
                })
                .await;
            let previous = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                .await;
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok((*previous + 1).min(input.limit(db)))
                })
                .await)
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    record(Event::ProviderInitial);
                    Ok(C::cycle_initial(db, id, input))
                })
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            previous: &'call u32,
            value: u32,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    record(Event::ProviderRecovery);
                    Ok(C::recover_from_cycle(db, cycle, previous, value, input))
                })
                .await)
        }
    }

    fn run<'db, C>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
        strategy: CycleRecoveryStrategy,
    ) -> Result<AttemptOutcome<RunResult<&'db u32>>, attempt_probe::StartError>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        assert_eq!(C::ATTEMPT_POLICY, QueryPolicy::CompleteOnly);
        assert_eq!(C::CYCLE_STRATEGY, strategy);
        try_with_execution_budget(
            db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 100_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                let route = registry.reserve_callable(db, ingredient)?;
                registry.bind_callable(
                    &route,
                    Provider {
                        route: route.clone(),
                    },
                )?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                        .await)
                })
            },
        )
    }

    fn assert_idle<C: Configuration>(
        db: &dyn Database,
        ingredient: &IngredientImpl<C>,
        input: Input,
    ) {
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(!RUN_ACTIVE.with(Cell::get));
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
    }

    #[test]
    fn active_complete_only_panic_key_never_requests_a_cycle_seed() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 2);
        let ingredient = panicking::fn_ingredient_(&db, db.zalsa());
        let stamp = Stamp::current(&db);
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        take_events();

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            run(&db, ingredient, input, CycleRecoveryStrategy::Panic)
        }));
        assert!(matches!(outcome, Err(payload) if
            payload.downcast_ref::<String>()
                .is_some_and(|message| message.contains("dependency graph cycle"))
            || payload.downcast_ref::<&str>()
                .is_some_and(|message| message.contains("dependency graph cycle"))
        ));
        assert_eq!(take_events(), [Event::Body]);
        assert!(
            ingredient
                .memo(db.zalsa(), input.as_id())
                .is_none_or(|memo| !memo.has_value())
        );
        assert_idle(&db, ingredient, input);
        assert_eq!(Stamp::current(&db), stamp);
    }

    #[test]
    fn complete_only_fixpoint_calls_generated_cycle_callbacks_and_publishes_final_memo() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 2);
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        let stamp = Stamp::current(&db);
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        take_events();

        assert_eq!(
            run(&db, ingredient, input, CycleRecoveryStrategy::Fixpoint),
            Ok(AttemptOutcome::Complete(Ok(&2)))
        );
        let events = take_events();
        assert!(events.contains(&Event::ProviderInitial));
        assert!(events.contains(&Event::Initial));
        assert!(events.contains(&Event::ProviderRecovery));
        assert!(events.iter().any(|event| matches!(event,
            Event::Recovery { previous, value } if previous != value
        )));
        assert!(events.iter().filter(|event| **event == Event::Body).count() > 1);
        assert!(FinalSourceMemo::certify(&db as &dyn Database, ingredient, input.as_id()).is_ok());
        assert_idle(&db, ingredient, input);
        assert_eq!(Stamp::current(&db), stamp);

        assert_eq!(
            run(&db, ingredient, input, CycleRecoveryStrategy::Fixpoint),
            Ok(AttemptOutcome::Complete(Ok(&2)))
        );
        assert!(take_events().is_empty());
        assert_idle(&db, ingredient, input);
        assert_eq!(Stamp::current(&db), stamp);
    }
}
mod policy {
    use std::any::Any;
    use std::cell::Cell;
    use std::num::NonZeroUsize;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[cfg(feature = "accumulator")]
    use crate::Accumulator;
    use crate::attempt_probe::{self, AttemptOutcome, Incomplete, MemoReuse, try_with_attempt};
    use crate::function::execute::execution_run::registration::{
        CallableRouteProvider, NativeCallbackLimits, RegistryBuilder, TaskEndpoint,
        with_native_callback,
    };
    use crate::function::execute::execution_run::{
        ExecutionAdmission, ExecutionWork, RunError, RunResult,
    };
    use crate::function::memo::MemoOutputCheck;
    use crate::function::{ClaimResult, Configuration, IngredientImpl, Memo, Reentrancy};
    use crate::plumbing::AsId;
    use crate::zalsa::ZalsaDatabase;
    use crate::{Cycle, Database, DatabaseImpl, Id, Setter};

    #[crate::input]
    struct Input {
        #[returns(copy)]
        value: u32,
    }

    #[crate::tracked]
    struct Product<'db> {
        #[returns(copy)]
        value: u32,
    }

    #[crate::tracked(returns(copy), specify)]
    fn specified<'db>(_db: &'db dyn Database, _product: Product<'db>) -> u32 {
        0
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn product(db: &dyn Database) -> Product<'_> {
        let product = Product::new(db, 7);
        specified::specify(db, product, 11);
        product
    }

    #[crate::tracked(returns(ref), attempt = CompleteOnly)]
    fn complete_only(db: &dyn Database, input: Input) -> u32 {
        input.value(db)
    }

    #[crate::tracked(returns(ref), attempt = CompleteOnly)]
    fn ordinary_output(db: &dyn Database, input: Input) -> u32 {
        let value = input.value(db);
        Product::new(db, value);
        value
    }

    #[cfg(feature = "accumulator")]
    #[crate::accumulator]
    struct Warning(u32);

    #[cfg(feature = "accumulator")]
    #[crate::tracked(returns(ref), attempt = CompleteOnly)]
    fn ordinary_accumulator(db: &dyn Database, input: Input) -> u32 {
        let value = input.value(db);
        Warning(value).accumulate(db);
        value
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn ordinary_child(db: &dyn Database, input: Input) -> u32 {
        let entered = Cell::new(false);
        let result = with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |_| {
            entered.set(true);
            Ok(())
        });
        assert!(matches!(result, Err(RunError::Contract(_))));
        assert!(!entered.get());
        let value = input.value(db);
        Product::new(db, value);
        value
    }

    #[derive(Clone, Copy)]
    enum Action<'db> {
        Read,
        TrackedOutput,
        Specify(Product<'db>),
        #[cfg(feature = "accumulator")]
        Accumulate,
        OrdinaryChild,
    }

    struct Provider<'run, 'db> {
        action: Action<'db>,
        bodies: &'run Cell<usize>,
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        fixture_native_value!(callable, 'run, 'db, C, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            self.bodies.set(self.bodies.get() + 1);
            match self.action {
                Action::Read => {}
                Action::TrackedOutput => {
                    Product::new(db, 99);
                }
                Action::Specify(product) => specified::specify(db, product, 99),
                #[cfg(feature = "accumulator")]
                Action::Accumulate => Warning(99).accumulate(db),
                Action::OrdinaryChild => {
                    return Ok(endpoint.local_call(|| Ok(ordinary_child(db, input))).await);
                }
            }
            Ok(input.value(db))
        }

        async fn initial<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            _id: Id,
            _input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Err(RunError::Contract(
                "acyclic policy fixture requested initial",
            ))
        }

        async fn recover<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            _cycle: &'call Cycle<'call>,
            _last: &'call u32,
            _value: u32,
            _input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Err(RunError::Contract(
                "acyclic policy fixture requested recovery",
            ))
        }
    }

    struct Admission;

    impl ExecutionAdmission for Admission {
        fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
            Ok(())
        }
    }

    fn fetch<'run, 'db: 'run, C>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
        action: Action<'db>,
        bodies: &'run Cell<usize>,
    ) -> RunResult<u32>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        let mut registry = RegistryBuilder::new(db, &Admission)?;
        let route = registry.reserve_callable(db, ingredient)?;
        registry.bind_callable(&route, Provider { action, bodies })?;
        registry.seal()?.run(move |endpoint| async move {
            endpoint.fetch_ref(&route, input.as_id())?.await.copied()
        })
    }

    fn memo<'db, C: Configuration>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
    ) -> Option<&'db Memo<C>> {
        ingredient.get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
    }

    fn assert_idle<C: Configuration>(
        db: &dyn Database,
        ingredient: &IngredientImpl<C>,
        input: Input,
    ) {
        assert!(attempt_probe::current().is_none());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
    }

    fn assert_output_policy(payload: &(dyn Any + Send)) {
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .expect("output policy uses a string panic");
        assert!(
            message.contains("attempted to create a tracked output"),
            "{message}"
        );
    }

    #[test]
    fn callable_complete_only_rejects_output_mutations_before_publication() {
        let db = DatabaseImpl::default();
        let product = product(&db);
        let actions = [
            Action::TrackedOutput,
            Action::Specify(product),
            #[cfg(feature = "accumulator")]
            Action::Accumulate,
        ];
        for action in actions {
            let input = Input::new(&db, 7);
            let ingredient = complete_only::fn_ingredient_(&db, db.zalsa());
            let bodies = Cell::new(0);
            let payload = catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(&db, 100_000, || {
                    fetch(&db, ingredient, input, action, &bodies)
                })
            }))
            .expect_err("the callable output policy rejects this mutation");
            // The policy must reject specify before its separate ownership check can fail.
            assert_output_policy(payload.as_ref());
            assert_eq!(bodies.get(), 1);
            assert_eq!(specified(&db, product), 11);
            assert!(memo(&db, ingredient, input).is_none());
            assert_idle(&db, ingredient, input);
        }
    }

    enum MemoRequest {
        Fetch,
        Validate,
    }

    fn assert_historical_memo_rejected<C>(
        db: &dyn Database,
        ingredient: &IngredientImpl<C>,
        input: Input,
    ) where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        let original = memo(db, ingredient, input).expect("ordinary evaluation produced the memo");
        let verified_at = original.header.verified_at.load();
        assert_eq!(
            original.header.attempt_reuse(db.zalsa()),
            MemoReuse::Ordinary
        );
        for request in [MemoRequest::Fetch, MemoRequest::Validate] {
            let bodies = Cell::new(0);
            let outcome = try_with_attempt(db, 100_000, || {
                let result = (|| {
                    let mut registry = RegistryBuilder::new(db, &Admission)?;
                    let route = registry.reserve_callable(db, ingredient)?;
                    registry.bind_callable(
                        &route,
                        Provider {
                            action: Action::Read,
                            bodies: &bodies,
                        },
                    )?;
                    registry.seal()?.run(move |endpoint| async move {
                        match request {
                            MemoRequest::Fetch => {
                                endpoint.fetch_ref(&route, input.as_id())?.await.map(|_| ())
                            }
                            MemoRequest::Validate => endpoint
                                .validate_callable(
                                    &route,
                                    input.as_id(),
                                    db.zalsa().current_revision(),
                                )?
                                .await
                                .map(|_| ()),
                        }
                    })
                })();
                assert_eq!(
                    result,
                    Err(RunError::Contract(
                        "controlled callable requires a memo without outputs or direct accumulators"
                    ))
                );
            });
            assert_eq!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            );
            assert_eq!(bodies.get(), 0);
            let retained =
                memo(db, ingredient, input).expect("rejection preserves the ordinary memo");
            assert!(std::ptr::eq(original, retained));
            assert_eq!(retained.header.verified_at.load(), verified_at);
            assert_eq!(
                retained.header.attempt_reuse(db.zalsa()),
                MemoReuse::Ordinary
            );
            assert_idle(db, ingredient, input);
        }
    }

    #[test]
    fn callable_complete_only_rejects_hot_and_stale_output_memos() {
        for stale in [false, true] {
            let mut db = DatabaseImpl::default();
            let input = Input::new(&db, 7);
            assert_eq!(*ordinary_output(&db, input), 7);
            if stale {
                input.set_value(&mut db).to(8);
            }
            let ingredient = ordinary_output::fn_ingredient_(&db, db.zalsa());
            let original = memo(&db, ingredient, input).expect("ordinary output memo");
            assert!(!original.header.outputs_are_empty());
            assert_eq!(
                original
                    .header
                    .output_check_work(MemoOutputCheck::Controlled),
                0,
                "tracked IDs make an output-edge scan unnecessary"
            );
            assert_eq!(
                original.header.verified_at.load() != db.zalsa().current_revision(),
                stale
            );
            assert_historical_memo_rejected(&db, ingredient, input);
        }
    }

    #[cfg(feature = "accumulator")]
    #[test]
    fn callable_complete_only_rejects_hot_and_stale_direct_accumulator_memos() {
        for stale in [false, true] {
            let mut db = DatabaseImpl::default();
            let input = Input::new(&db, 7);
            assert_eq!(*ordinary_accumulator(&db, input), 7);
            if stale {
                input.set_value(&mut db).to(8);
            }
            let ingredient = ordinary_accumulator::fn_ingredient_(&db, db.zalsa());
            let original = memo(&db, ingredient, input).expect("ordinary accumulator memo");
            assert!(original.header.outputs_are_empty());
            assert!(original.header.revisions.accumulated().is_some());
            assert_eq!(
                original
                    .header
                    .output_check_work(MemoOutputCheck::Controlled),
                0,
                "direct accumulators make an output-edge scan unnecessary"
            );
            assert_eq!(
                original.header.verified_at.load() != db.zalsa().current_revision(),
                stale
            );
            assert_historical_memo_rejected(&db, ingredient, input);
        }
    }

    #[test]
    fn ordinary_complete_only_child_cannot_reopen_admitted_execution() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        let ingredient = complete_only::fn_ingredient_(&db, db.zalsa());
        let bodies = Cell::new(0);
        // The native child has its own ordinary CompleteOnly operation: it can create outputs,
        // and its completion guarantee forbids reopening interruptible work. The callable's
        // own memo remains output-free.
        assert_eq!(
            try_with_attempt(&db, 100_000, || {
                fetch(&db, ingredient, input, Action::OrdinaryChild, &bodies)
            }),
            Ok(AttemptOutcome::Complete(Ok(7)))
        );
        assert_eq!(bodies.get(), 1);
        assert!(
            memo(&db, ingredient, input)
                .expect("completed callable")
                .header
                .outputs_are_empty()
        );
        let child_ingredient = ordinary_child::fn_ingredient_(&db, db.zalsa());
        assert!(
            !memo(&db, child_ingredient, input)
                .expect("completed ordinary child")
                .header
                .outputs_are_empty()
        );
        assert_idle(&db, ingredient, input);
        assert_idle(&db, child_ingredient, input);
    }
}

mod mixed_native_cycle {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

    use crate::attempt_probe::{
        self, AttemptOutcome, ExecutionLimits, QueryPolicy, try_with_execution_budget,
    };
    use crate::function::execute::execution_run::registration::{
        CallableRoute, CallableRouteProvider, RegistryBuilder, TaskEndpoint,
    };
    use crate::function::execute::execution_run::{RUN_ACTIVE, RunResult};
    use crate::function::memo::FinalSourceMemo;
    use crate::function::{
        ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy,
    };
    use crate::plumbing::AsId;
    use crate::zalsa::ZalsaDatabase;
    use crate::{Cycle, Database, DatabaseImpl, Id};

    #[crate::input]
    struct Input {
        #[returns(copy)]
        limit: u32,
    }

    #[crate::tracked]
    struct Product<'db> {
        #[returns(copy)]
        value: u32,
    }

    thread_local! {
        static INSPECT_BRIDGE: Cell<bool> = const { Cell::new(false) };
        static SAW_SUPPORTED_SEED: Cell<bool> = const { Cell::new(false) };
        static SAW_ORDINARY_B: Cell<bool> = const { Cell::new(false) };
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial, cycle_fn = recover)]
    fn a(db: &dyn Database, input: Input) -> u32 {
        let previous = a(db, input);
        let child = b(db, input);
        (previous.max(child) + 1).min(input.limit(db))
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial, cycle_fn = recover)]
    fn b(db: &dyn Database, input: Input) -> u32 {
        Product::new(db, 0);
        if INSPECT_BRIDGE.get() {
            let ingredient = a::fn_ingredient_(db, db.zalsa());
            let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
            let recipient = b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
            SAW_SUPPORTED_SEED.set(
                SAW_SUPPORTED_SEED.get()
                    || memo.has_value()
                        && memo.header().may_be_provisional()
                        && memo.header().revisions.attempt_support().is_some(),
            );
            SAW_ORDINARY_B.set(
                attempt_probe::current_policy() == QueryPolicy::CompleteOnly
                    && attempt_probe::current_query().is_none()
                    && db.zalsa_local().try_with_query_stack(|stack| {
                        stack.last().is_some_and(|frame| {
                            frame.database_key_index == recipient
                                && !frame.attempt_policy.allows_incomplete()
                        })
                    }) == Some(true),
            );
        }
        a(db, input)
    }

    fn initial(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        0
    }

    fn recover(
        _db: &dyn Database,
        _cycle: &Cycle<'_>,
        _previous: &u32,
        value: u32,
        _input: Input,
    ) -> u32 {
        value
    }

    struct Provider<'run, 'db: 'run, C: Configuration> {
        route: CallableRoute<'run, 'db, C>,
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        fixture_native_value!(callable, 'run, 'db, C, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            let previous = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                .await;
            let child = endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(b(db, input))
                })
                .await;
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(((*previous).max(child) + 1).min(input.limit(db)))
                })
                .await)
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::cycle_initial(db, id, input))
                })
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            previous: &'call u32,
            value: u32,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::recover_from_cycle(db, cycle, previous, value, input))
                })
                .await)
        }
    }

    fn run<'db, C>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
    ) -> Result<AttemptOutcome<RunResult<&'db u32>>, attempt_probe::StartError>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        try_with_execution_budget(
            db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 100_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                let route = registry.reserve_callable(db, ingredient)?;
                registry.bind_callable(
                    &route,
                    Provider {
                        route: route.clone(),
                    },
                )?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                        .await)
                })
            },
        )
    }

    // Both executions must converge to 2 when ordinary B creates a tracked output and reads
    // A's provisional value. The controlled execution keeps B's ordinary CompleteOnly mode.
    // A first reads itself to establish its attempt-supported seed before calling ordinary B.
    #[test]
    fn mixed_callable_and_ordinary_complete_only_cycle_matches_ordinary_completion() {
        let ordinary = DatabaseImpl::default();
        let ordinary_input = Input::new(&ordinary, 2);
        assert_eq!(a(&ordinary, ordinary_input), 2);

        let db = DatabaseImpl::default();
        let input = Input::new(&db, 2);
        let ingredient = a::fn_ingredient_(&db, db.zalsa());
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        INSPECT_BRIDGE.set(true);
        SAW_SUPPORTED_SEED.set(false);
        SAW_ORDINARY_B.set(false);
        let result = catch_unwind(AssertUnwindSafe(|| run(&db, ingredient, input)));
        INSPECT_BRIDGE.set(false);
        assert!(SAW_SUPPORTED_SEED.get());
        assert!(SAW_ORDINARY_B.get());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(!RUN_ACTIVE.with(Cell::get));
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
        let child = b::fn_ingredient_(&db, db.zalsa());
        assert!(matches!(
            child
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(payload) => resume_unwind(payload),
        };
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(&2))));
        assert!(FinalSourceMemo::certify(&db as &dyn Database, ingredient, input.as_id()).is_ok());
        assert!(
            !child
                .memo(db.zalsa(), input.as_id())
                .unwrap()
                .header()
                .outputs_are_empty()
        );
    }
}

mod mixed_native_provenance {
    use std::cell::{Cell, RefCell};
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

    use crate::attempt_probe::{
        self, AttemptOutcome, ExecutionLimits, QueryPolicy, try_with_execution_budget,
    };
    use crate::function::execute::execution_run::registration::{
        CallableRoute, CallableRouteProvider, RegistryBuilder, TaskEndpoint,
    };
    use crate::function::execute::execution_run::{RUN_ACTIVE, RunResult};
    use crate::function::memo::FinalSourceMemo;
    use crate::function::{
        ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy,
    };
    use crate::plumbing::{AsId, FromId};
    use crate::zalsa::ZalsaDatabase;
    use crate::{Cycle, Database, DatabaseImpl, Id};

    #[crate::input]
    struct Input {
        #[returns(copy)]
        limit: u32,
    }

    #[crate::tracked]
    struct Product<'db> {
        #[returns(copy)]
        value: u32,
    }

    thread_local! {
        static INSPECT_BRIDGE: Cell<bool> = const { Cell::new(false) };
        static SAW_NATIVE_SEED: Cell<bool> = const { Cell::new(false) };
        static SAW_ASSIGNED: Cell<bool> = const { Cell::new(false) };
        static SAW_COMPLETED_B: Cell<bool> = const { Cell::new(false) };
        static PRODUCTS: RefCell<Vec<(Id, u32)>> = const { RefCell::new(Vec::new()) };
        static BODIES: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
        static FALLBACKS: Cell<usize> = const { Cell::new(0) };
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial, cycle_fn = recover)]
    fn a(db: &dyn Database, input: Input) -> u32 {
        let (a_count, b_count) = BODIES.get();
        BODIES.set((a_count + 1, b_count));
        let previous = a(db, input);
        let child = b(db, input);
        (previous.max(child) + 1).min(input.limit(db))
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial, cycle_fn = recover)]
    fn b(db: &dyn Database, input: Input) -> u32 {
        let (a_count, b_count) = BODIES.get();
        BODIES.set((a_count, b_count + 1));
        let previous = b(db, input);
        if INSPECT_BRIDGE.get() {
            let a_memo = a::fn_ingredient_(db, db.zalsa())
                .memo(db.zalsa(), input.as_id())
                .unwrap();
            let seed = b::fn_ingredient_(db, db.zalsa())
                .memo(db.zalsa(), input.as_id())
                .unwrap();
            assert!(seed.has_value());
            assert!(seed.header().may_be_provisional());
            assert!(
                seed.header()
                    .revisions
                    .attempt_support()
                    .unwrap()
                    .same_owner(a_memo.header().revisions.attempt_support().unwrap())
            );
            assert_eq!(attempt_probe::current_policy(), QueryPolicy::CompleteOnly);
            assert!(attempt_probe::current_query().is_none());
            SAW_NATIVE_SEED.set(true);
        }
        let value = previous.max(a(db, input));
        let product = Product::new(db, value);
        assigned::specify(db, product, value);
        if INSPECT_BRIDGE.get() {
            let a_memo = a::fn_ingredient_(db, db.zalsa())
                .memo(db.zalsa(), input.as_id())
                .unwrap();
            let assigned_memo = assigned::fn_ingredient_(db, db.zalsa())
                .memo(db.zalsa(), product.as_id())
                .unwrap();
            assert!(assigned_memo.header().may_be_provisional());
            assert!(
                assigned_memo
                    .header()
                    .revisions
                    .attempt_support()
                    .unwrap()
                    .same_owner(a_memo.header().revisions.attempt_support().unwrap())
            );
            assert!(!assigned_memo.header().revisions.cycle_heads().is_empty());
            PRODUCTS.with_borrow_mut(|products| products.push((product.as_id(), value)));
            SAW_ASSIGNED.set(true);
        }
        value
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, specify)]
    fn assigned<'db>(_db: &'db dyn Database, _product: Product<'db>) -> u32 {
        FALLBACKS.set(FALLBACKS.get() + 1);
        u32::MAX
    }

    fn initial(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        0
    }

    fn recover(
        _db: &dyn Database,
        _cycle: &Cycle<'_>,
        _previous: &u32,
        value: u32,
        _input: Input,
    ) -> u32 {
        value
    }

    struct Provider<'run, 'db: 'run, C: Configuration> {
        route: CallableRoute<'run, 'db, C>,
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        fixture_native_value!(callable, 'run, 'db, C, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            let previous = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                .await;
            let child = endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    let value = b(db, input);
                    let memo = b::fn_ingredient_(db, db.zalsa())
                        .memo(db.zalsa(), input.as_id())
                        .unwrap();
                    if memo.header().may_be_provisional() {
                        assert!(
                            memo.header()
                                .revisions
                                .attempt_support()
                                .unwrap()
                                .same_owner(&attempt_probe::current_query().unwrap())
                        );
                        SAW_COMPLETED_B.set(true);
                    }
                    Ok(value)
                })
                .await;
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(((*previous).max(child) + 1).min(input.limit(db)))
                })
                .await)
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::cycle_initial(db, id, input))
                })
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            previous: &'call u32,
            value: u32,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::recover_from_cycle(db, cycle, previous, value, input))
                })
                .await)
        }
    }

    fn run<'db, C>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        input: Input,
    ) -> Result<AttemptOutcome<RunResult<&'db u32>>, attempt_probe::StartError>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        try_with_execution_budget(
            db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 100_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                let route = registry.reserve_callable(db, ingredient)?;
                registry.bind_callable(
                    &route,
                    Provider {
                        route: route.clone(),
                    },
                )?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                        .await)
                })
            },
        )
    }

    // A uses the registered callable route; ordinary B first creates its own seed, then reads A
    // and specifies a real output. All three provisional producers must retain A's evaluation
    // owner while B keeps its ordinary CompleteOnly policy and cannot return an allowance refusal.
    #[test]
    fn native_seed_and_assigned_output_preserve_mixed_cycle_provenance() {
        let ordinary = DatabaseImpl::default();
        let ordinary_input = Input::new(&ordinary, 2);
        assert_eq!(a(&ordinary, ordinary_input), 2);

        let db = DatabaseImpl::default();
        let input = Input::new(&db, 2);
        let ingredient = a::fn_ingredient_(&db, db.zalsa());
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        INSPECT_BRIDGE.set(true);
        SAW_NATIVE_SEED.set(false);
        SAW_ASSIGNED.set(false);
        SAW_COMPLETED_B.set(false);
        PRODUCTS.with_borrow_mut(Vec::clear);
        FALLBACKS.set(0);
        let result = catch_unwind(AssertUnwindSafe(|| run(&db, ingredient, input)));
        INSPECT_BRIDGE.set(false);
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(payload) => resume_unwind(payload),
        };
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(&2))));
        assert!(SAW_NATIVE_SEED.get());
        assert!(SAW_ASSIGNED.get());
        assert!(SAW_COMPLETED_B.get());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(!RUN_ACTIVE.with(Cell::get));
        assert!(FinalSourceMemo::certify(&db as &dyn Database, ingredient, input.as_id()).is_ok());
        let child = b::fn_ingredient_(&db, db.zalsa());
        for key in [
            ingredient.database_key_index(input.as_id()),
            child.database_key_index(input.as_id()),
        ] {
            let ingredient = db
                .zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap();
            assert!(matches!(
                ingredient
                    .sync_table()
                    .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
                ClaimResult::Claimed(())
            ));
        }
        let bodies = BODIES.get();
        assert_eq!(a(&db, input), 2);
        assert_eq!(b(&db, input), 2);
        assert_eq!(BODIES.get(), bodies);
        let (id, value) = PRODUCTS.take().pop().unwrap();
        // The ID came from this database's actual constructor in the current revision.
        let product = Product::from_id(id);
        assert_eq!(product.value(&db), value);
        assert_eq!(assigned(&db, product), value);
        let memo = assigned::fn_ingredient_(&db, db.zalsa())
            .memo(db.zalsa(), id)
            .unwrap();
        assert!(!memo.header().may_be_provisional());
        assert!(
            matches!(memo.header().origin(), crate::zalsa_local::QueryOriginRef::Assigned(key) if key == child.database_key_index(input.as_id()))
        );
        assert_eq!(FALLBACKS.get(), 0);
        assert_eq!(BODIES.get(), bodies);
    }
}
mod mixed_output_lifecycle {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex};

    use super::super::validation_trace::{self, TraceEvent};
    use crate::attempt_probe::{
        self, AttemptOutcome, AttemptSupport, ExecutionLimits, Incomplete, MemoReuse,
        try_with_execution_budget,
    };
    use crate::function::execute::execution_run::registration::{
        CallableRoute, CallableRouteProvider, RegistryBuilder, TaskEndpoint,
    };
    use crate::function::execute::execution_run::{RUN_ACTIVE, RunResult};
    use crate::function::memo::{FinalSourceMemo, MemoHeader};
    use crate::function::{ClaimResult, Configuration, FunctionIngredient, Reentrancy};
    use crate::plumbing::{AsId, FromId};
    use crate::prepared_source_probe::Stamp;
    use crate::tracked_struct::Identity;
    use crate::zalsa::ZalsaDatabase;
    use crate::zalsa_local::OutputOrder;
    use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id, Setter};

    const AMPLE_WORK: usize = 100_000;

    #[crate::db]
    #[derive(Clone)]
    struct TestDb {
        storage: crate::Storage<Self>,
        discards: Arc<Mutex<Vec<(DatabaseKeyIndex, DatabaseKeyIndex)>>>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let discards = Arc::new(Mutex::new(Vec::new()));
            let events = discards.clone();
            Self {
                storage: crate::Storage::new(Some(Box::new(move |event| {
                    if let EventKind::WillDiscardStaleOutput {
                        execute_key,
                        output_key,
                    } = event.kind
                    {
                        events.lock().unwrap().push((execute_key, output_key));
                    }
                }))),
                discards,
            }
        }
    }

    #[crate::db]
    impl Database for TestDb {}

    #[crate::input]
    struct Input {
        #[returns(copy)]
        limit: u32,
    }

    // Every Product has the same identity hash. Retried construction must reserve old
    // disambiguators so that previously read values cannot be overwritten.
    #[crate::tracked]
    struct Product<'db> {
        #[tracked]
        #[returns(copy)]
        value: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum NativeEvent {
        ABody,
        AInitial(u32),
        BValue(u32),
        Created { id: Id, value: u32 },
        SpecifiedFallback,
    }

    thread_local! {
        static NATIVE_EVENTS: RefCell<Vec<NativeEvent>> = const { RefCell::new(Vec::new()) };
    }

    fn record(event: NativeEvent) {
        NATIVE_EVENTS.with_borrow_mut(|events| events.push(event));
    }

    fn take_events() -> Vec<NativeEvent> {
        NATIVE_EVENTS.with_borrow_mut(std::mem::take)
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial_a, cycle_fn = recover)]
    fn a(db: &dyn Database, input: Input) -> u32 {
        record(NativeEvent::ABody);
        let previous = a(db, input);
        let child = b(db, input);
        (previous.max(child) + 1).min(input.limit(db))
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial_b, cycle_fn = recover)]
    fn b(db: &dyn Database, input: Input) -> u32 {
        validation_trace::record(TraceEvent::Body {
            key: b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        });
        let value = a(db, input);
        record(NativeEvent::BValue(value));
        if value < input.limit(db) {
            let product = Product::new(db, value);
            specified::specify(db, product, value + 10);
            record(NativeEvent::Created {
                id: product.as_id(),
                value,
            });
            product.value(db)
        } else {
            value
        }
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, specify)]
    fn specified<'db>(_db: &'db dyn Database, _product: Product<'db>) -> u32 {
        record(NativeEvent::SpecifiedFallback);
        0
    }

    fn initial_a(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        record(NativeEvent::AInitial(0));
        0
    }

    fn initial_b(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        0
    }

    fn recover(
        _db: &dyn Database,
        _cycle: &Cycle<'_>,
        _previous: &u32,
        value: u32,
        _input: Input,
    ) -> u32 {
        value
    }

    #[derive(Clone, Debug)]
    struct Ownership {
        ids: Vec<(Identity, Id)>,
        assigned: Vec<DatabaseKeyIndex>,
    }

    impl Ownership {
        fn from_header(header: &MemoHeader) -> Self {
            Self {
                ids: header.revisions.tracked_struct_ids().to_vec(),
                assigned: header.origin().outputs().collect(),
            }
        }

        fn keys(&self) -> Vec<DatabaseKeyIndex> {
            self.ids
                .iter()
                .map(|(identity, id)| DatabaseKeyIndex::new(identity.ingredient_index(), *id))
                .chain(self.assigned.iter().copied())
                .collect()
        }

        fn assert_contains(&self, earlier: &Self) {
            for id in &earlier.ids {
                assert!(
                    self.ids.contains(id),
                    "lost earlier tracked identity: {id:?}"
                );
            }
            for key in &earlier.assigned {
                assert!(
                    self.assigned.contains(key),
                    "lost earlier assigned output: {key:?}"
                );
            }
        }
    }

    #[derive(Debug)]
    struct Boundary {
        work_before: usize,
        child: u32,
        owner: AttemptSupport,
        outputs: Ownership,
        provisional: bool,
    }

    #[derive(Default)]
    struct Observations {
        boundaries: RefCell<Vec<Boundary>>,
    }

    impl Observations {
        fn refusal_prefix(&self) -> usize {
            let boundaries = self.boundaries.borrow();
            let boundary = boundaries
                .iter()
                .find(|boundary| boundary.child == 1)
                .expect("the finite cycle reaches a nonzero provisional B value");
            assert!(boundary.provisional);
            assert!(!boundary.outputs.ids.is_empty());
            assert!(!boundary.outputs.assigned.is_empty());
            boundary.work_before
        }
    }

    struct Provider<'run, 'db: 'run, C: Configuration> {
        route: CallableRoute<'run, 'db, C>,
        work_limit: usize,
        observations: &'run Observations,
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        // Input conversion copies an Input handle; output equality compares one u32.
        fixture_native_value!(callable, 'run, 'db, C, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            let previous = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                .await;
            let child = endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(b(db, input))
                })
                .await;
            Ok(endpoint
                .local_call(|| {
                    let ingredient = b::fn_ingredient_(db, db.zalsa());
                    let memo = ingredient
                        .memo(db.zalsa(), input.as_id())
                        .expect("native B returned with its canonical memo");
                    let header = memo.header();
                    let owner = header
                        .revisions
                        .attempt_support()
                        .expect("B read the admitted cycle's provisional A value")
                        .clone();
                    assert!(owner.same_owner(&attempt_probe::current_query().unwrap()));
                    let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
                    self.observations.boundaries.borrow_mut().push(Boundary {
                        work_before: self.work_limit - remaining,
                        child,
                        owner,
                        outputs: Ownership::from_header(header),
                        provisional: header.may_be_provisional(),
                    });
                    // This real work request precedes the next semantic calculation. Observation
                    // only records its consumed prefix; the shared budget decides whether it runs.
                    endpoint.admit_work(1)?;
                    Ok(((*previous).max(child) + 1).min(input.limit(db)))
                })
                .await)
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::cycle_initial(db, id, input))
                })
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            previous: &'call u32,
            value: u32,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::recover_from_cycle(db, cycle, previous, value, input))
                })
                .await)
        }
    }

    fn run(
        db: &dyn Database,
        input: Input,
        work_limit: usize,
        observations: &Observations,
    ) -> Result<AttemptOutcome<RunResult<u32>>, attempt_probe::StartError> {
        try_with_execution_budget(
            db,
            ExecutionLimits {
                semantic_work: work_limit,
                requested_bytes: AMPLE_WORK,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                let route = registry.reserve_callable(db, a::fn_ingredient_(db, db.zalsa()))?;
                registry.bind_callable(
                    &route,
                    Provider {
                        route: route.clone(),
                        work_limit,
                        observations,
                    },
                )?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(*endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                        .await)
                })
            },
        )
    }

    fn assert_idle(db: &dyn Database, input: Input) {
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(!RUN_ACTIVE.with(std::cell::Cell::get));
        for ingredient in [
            a::fn_ingredient_(db, db.zalsa()) as &dyn FunctionIngredient,
            b::fn_ingredient_(db, db.zalsa()),
        ] {
            assert!(matches!(
                ingredient
                    .sync_table()
                    .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
                ClaimResult::Claimed(())
            ));
        }
    }

    fn assert_fresh_seed(events: &[NativeEvent]) {
        assert_eq!(
            events.iter().find_map(|event| match event {
                NativeEvent::AInitial(value) => Some(*value),
                _ => None,
            }),
            Some(0)
        );
        assert_eq!(
            events.iter().find_map(|event| match event {
                NativeEvent::BValue(value) => Some(*value),
                _ => None,
            }),
            Some(0)
        );
        assert!(!events.contains(&NativeEvent::ABody));
        assert!(!events.contains(&NativeEvent::SpecifiedFallback));
    }

    fn created(events: &[NativeEvent]) -> Vec<(Id, u32)> {
        events
            .iter()
            .filter_map(|event| match event {
                NativeEvent::Created { id, value } => Some((*id, *value)),
                _ => None,
            })
            .collect()
    }

    fn assert_fields(db: &dyn Database, products: &[(Id, u32)]) {
        for (id, expected) in products {
            assert_eq!(Product::from_id(*id).value(db), *expected);
        }
    }

    fn assert_stopped(db: &dyn Database, input: Input, observations: &Observations, prefix: usize) {
        assert_eq!(observations.refusal_prefix(), prefix);
        let boundaries = observations.boundaries.borrow();
        let last = boundaries.last().unwrap();
        assert_eq!(last.work_before, prefix);
        assert_eq!(last.child, 1);
        assert_eq!(last.owner.reason(), Some(Incomplete::Allowance));
        let ingredient = b::fn_ingredient_(db, db.zalsa());
        let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
        let header = memo.header();
        assert!(memo.has_value());
        assert!(header.may_be_provisional());
        assert_eq!(header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
        assert!(!header.can_seed_attempt(db.zalsa()));
        assert!(
            header
                .revisions
                .attempt_support()
                .unwrap()
                .same_owner(&last.owner)
        );
        assert!(
            FinalSourceMemo::certify(db, a::fn_ingredient_(db, db.zalsa()), input.as_id()).is_err()
        );
        assert_idle(db, input);
    }

    // Measure work limits that stop the first and second attempts just after B returns 1.
    // Each limit is the work consumed before the next checkpoint, so that checkpoint refuses.
    // Refusing at B = 1 abandons a noninitial approximation. The second limit is measured
    // after replaying the first real refusal on an independent database, because retained
    // output metadata can change the work needed to reach that boundary again.
    fn calibration_prefixes() -> [usize; 2] {
        let first = {
            let db = TestDb::default();
            let input = Input::new(&db, 2);
            let observed = Observations::default();
            assert_eq!(
                run(&db, input, AMPLE_WORK, &observed),
                Ok(AttemptOutcome::Complete(Ok(2)))
            );
            observed.refusal_prefix()
        };
        let second = {
            let db = TestDb::default();
            let input = Input::new(&db, 2);
            let stopped = Observations::default();
            assert_eq!(
                run(&db, input, first, &stopped),
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            );
            assert_stopped(&db, input, &stopped, first);
            let observed = Observations::default();
            assert_eq!(
                run(&db, input, AMPLE_WORK, &observed),
                Ok(AttemptOutcome::Complete(Ok(2)))
            );
            observed.refusal_prefix()
        };
        println!("MIXED_OUTPUT_REFUSAL_PREFIXES\t{first}\t{second}");
        [first, second]
    }

    fn assert_reexecutes_before_edge_replay(trace: &[TraceEvent], key: DatabaseKeyIndex) {
        let body = trace
            .iter()
            .position(|event| matches!(event, TraceEvent::Body { key: body } if *body == key))
            .expect("B must reexecute after its input changes");
        assert!(trace[..body].iter().all(|event| !matches!(
            event,
            TraceEvent::Request { owner, .. } | TraceEvent::Output { owner, .. } if *owner == key
        )), "inherited output order was replayed before B reexecuted: {trace:?}");
    }

    // Refusal preserves the output history of native B without accepting its abandoned value.
    // Same-revision retries retain old fields and reserve new identities. Only a later revision
    // can retire identities and specifications that the real body no longer produces. Warming
    // B in both databases makes both memos final; the traces then verify that B reexecutes
    // before replaying any edge after the input changes.
    #[test]
    fn repeated_mixed_refusals_retain_outputs_until_canonical_revision_cleanup() {
        let prefixes = calibration_prefixes();
        let mut ordinary = TestDb::default();
        let ordinary_input = Input::new(&ordinary, 2);
        let expected = a(&ordinary, ordinary_input);
        assert_eq!(expected, 2);
        assert_eq!(b(&ordinary, ordinary_input), expected);
        let ordinary_key = {
            let ingredient = b::fn_ingredient_(&ordinary, ordinary.zalsa());
            let header = ingredient
                .memo(ordinary.zalsa(), ordinary_input.as_id())
                .unwrap()
                .header();
            assert!(!header.may_be_provisional());
            assert_eq!(header.revisions.output_order(), OutputOrder::Inherited);
            ingredient.database_key_index(ordinary_input.as_id())
        };
        let mut db = TestDb::default();
        let input = Input::new(&db, 2);
        let stamp = Stamp::current(&db);
        let mut all_products = Vec::new();
        let retained;
        let controlled_key;
        {
            let ingredient = b::fn_ingredient_(&db, db.zalsa());
            let mut stopped_headers = Vec::new();
            let mut earlier_outputs = None;
            let mut earlier_owner: Option<AttemptSupport> = None;
            for prefix in prefixes {
                take_events();
                let observations = Observations::default();
                assert_eq!(
                    run(&db, input, prefix, &observations),
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                );
                assert_stopped(&db, input, &observations, prefix);
                let events = take_events();
                assert_fresh_seed(&events);
                let products = created(&events);
                assert!(!products.is_empty());
                for (id, _) in &products {
                    assert!(!all_products.iter().any(|(old, _)| old == id));
                }
                all_products.extend(products);
                assert_fields(&db, &all_products);
                let header = ingredient.memo(db.zalsa(), input.as_id()).unwrap().header();
                let outputs = Ownership::from_header(header);
                if let Some(earlier) = &earlier_outputs {
                    outputs.assert_contains(earlier);
                }
                let owner = header.revisions.attempt_support().unwrap().clone();
                if let Some(earlier) = &earlier_owner {
                    assert!(!owner.same_owner(earlier));
                }
                earlier_outputs = Some(outputs);
                earlier_owner = Some(owner);
                stopped_headers.push(header);
                assert!(db.discards.lock().unwrap().is_empty());
                assert_eq!(Stamp::current(&db), stamp);
            }

            take_events();
            let observations = Observations::default();
            assert_eq!(
                run(&db, input, AMPLE_WORK, &observations),
                Ok(AttemptOutcome::Complete(Ok(expected)))
            );
            let events = take_events();
            assert_fresh_seed(&events);
            all_products.extend(created(&events));
            assert_fields(&db, &all_products);
            let header = ingredient.memo(db.zalsa(), input.as_id()).unwrap().header();
            retained = Ownership::from_header(header);
            retained.assert_contains(earlier_outputs.as_ref().unwrap());
            for old in stopped_headers {
                assert!(old.may_be_provisional());
                assert!(old.has_incomplete_attempt());
                assert_eq!(old.attempt_reuse(db.zalsa()), MemoReuse::Stale);
                assert!(!old.same_attempt_owner(header));
            }
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Database,
                    a::fn_ingredient_(&db, db.zalsa()),
                    input.as_id()
                )
                .is_ok()
            );
            assert_eq!(a(&db, input), expected);
            assert_eq!(b(&db, input), expected);
            assert!(take_events().is_empty());
            assert!(
                !ingredient
                    .memo(db.zalsa(), input.as_id())
                    .unwrap()
                    .header()
                    .may_be_provisional()
            );
            assert!(db.discards.lock().unwrap().is_empty());
            assert_eq!(
                ingredient
                    .memo(db.zalsa(), input.as_id())
                    .unwrap()
                    .header()
                    .revisions
                    .output_order(),
                OutputOrder::Inherited
            );
            controlled_key = ingredient.database_key_index(input.as_id());
            assert_idle(&db, input);
        }

        let expected_discards = retained.keys();
        assert!(!expected_discards.is_empty());
        assert_eq!(retained.ids.len(), all_products.len());
        for (id, _) in &all_products {
            assert!(retained.ids.iter().any(|(_, retained)| retained == id));
        }
        // Freeze keys before the setter: reading old Product fields in the new revision would
        // take read locks and prevent the deletion this control needs to observe.
        input.set_limit(&mut db).to(0);
        ordinary_input.set_limit(&mut ordinary).to(0);
        let (expected, ordinary_trace) = validation_trace::collect(|| a(&ordinary, ordinary_input));
        assert_eq!(expected, 0);
        assert_reexecutes_before_edge_replay(&ordinary_trace, ordinary_key);
        take_events();
        let observations = Observations::default();
        let (outcome, controlled_trace) =
            validation_trace::collect(|| run(&db, input, AMPLE_WORK, &observations));
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(expected))));
        assert_reexecutes_before_edge_replay(&controlled_trace, controlled_key);
        assert!(created(&take_events()).is_empty());
        let ingredient = b::fn_ingredient_(&db, db.zalsa());
        let header = ingredient.memo(db.zalsa(), input.as_id()).unwrap().header();
        assert!(header.revisions.tracked_struct_ids().is_empty());
        assert!(header.origin().outputs().next().is_none());
        let key = ingredient.database_key_index(input.as_id());
        let events = db.discards.lock().unwrap();
        let actual: Vec<_> = events
            .iter()
            .filter_map(|(owner, output)| (*owner == key).then_some(*output))
            .collect();
        assert_eq!(
            actual.len(),
            expected_discards.len(),
            "{actual:?} != {expected_discards:?}"
        );
        for output in &expected_discards {
            assert!(
                actual.contains(output),
                "missing canonical retirement: {output:?}"
            );
        }
        drop(events);
        assert_idle(&db, input);
    }

    mod cache_controls {
        use super::*;
        use crate::function::maybe_changed_after::ShallowUpdate;
        use crate::function::memo::ErasedMemo;
        use crate::zalsa_local::{OutputOrder, QueryOriginRef};
        use crate::{Durability, Revision};

        // B specifies an output even in the final iteration. The cache checks read `specified`
        // for that iteration's Product; earlier specifications remain historical ownership.
        #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial_a, cycle_fn = recover)]
        fn a(db: &dyn Database, input: Input) -> u32 {
            record(NativeEvent::ABody);
            let previous = a(db, input);
            let child = b(db, input);
            (previous.max(child) + 1).min(input.limit(db))
        }

        #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial_b, cycle_fn = recover)]
        fn b(db: &dyn Database, input: Input) -> u32 {
            let value = a(db, input);
            record(NativeEvent::BValue(value));
            let limit = input.limit(db);
            let product = Product::new(db, value);
            specified::specify(db, product, value + 10);
            record(NativeEvent::Created {
                id: product.as_id(),
                value,
            });
            if value < limit {
                product.value(db)
            } else {
                value
            }
        }

        struct CacheProvider<'run, 'db: 'run, C: Configuration> {
            route: CallableRoute<'run, 'db, C>,
        }

        impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for CacheProvider<'run, 'db, C>
        where
            C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
        {
            fixture_native_value!(callable, 'run, 'db, C, 1);

            async fn body<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                db: &'db dyn Database,
                input: Input,
            ) -> RunResult<u32>
            where
                'run: 'call,
            {
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        record(NativeEvent::ABody);
                        Ok(())
                    })
                    .await;
                let previous = endpoint
                    .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                    .await;
                let child = endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        Ok(b(db, input))
                    })
                    .await;
                Ok(endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        Ok(((*previous).max(child) + 1).min(input.limit(db)))
                    })
                    .await)
            }

            async fn initial<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                db: &'db dyn Database,
                id: Id,
                input: Input,
            ) -> RunResult<u32>
            where
                'run: 'call,
            {
                Ok(endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        Ok(C::cycle_initial(db, id, input))
                    })
                    .await)
            }

            async fn recover<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                db: &'db dyn Database,
                cycle: &'call Cycle<'call>,
                previous: &'call u32,
                value: u32,
                input: Input,
            ) -> RunResult<u32>
            where
                'run: 'call,
            {
                Ok(endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        Ok(C::recover_from_cycle(db, cycle, previous, value, input))
                    })
                    .await)
            }
        }

        #[derive(Clone, Copy, Debug)]
        enum Entry {
            Ordinary,
            Callable,
        }

        impl Entry {
            fn read(self, db: &dyn Database, input: Input) {
                match self {
                    Self::Ordinary => assert_eq!(a(db, input), 2),
                    Self::Callable => {
                        let result = try_with_execution_budget(
                            db,
                            ExecutionLimits {
                                semantic_work: AMPLE_WORK,
                                requested_bytes: AMPLE_WORK,
                            },
                            |budget| {
                                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                                let route = registry
                                    .reserve_callable(db, a::fn_ingredient_(db, db.zalsa()))?;
                                registry.bind_callable(
                                    &route,
                                    CacheProvider {
                                        route: route.clone(),
                                    },
                                )?;
                                registry.seal()?.run(move |endpoint| async move {
                                    Ok(*endpoint
                                        .child_call(|| async {
                                            endpoint.fetch_ref(&route, input.as_id())?.await
                                        })
                                        .await)
                                })
                            },
                        );
                        assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(2))));
                    }
                }
                assert_eq!(b(db, input), 2);
            }
        }

        fn query_memos(db: &dyn Database, input: Input) -> [(DatabaseKeyIndex, ErasedMemo<'_>); 2] {
            let a = a::fn_ingredient_(db, db.zalsa());
            let b = b::fn_ingredient_(db, db.zalsa());
            [
                (
                    a.database_key_index(input.as_id()),
                    a.memo(db.zalsa(), input.as_id()).unwrap(),
                ),
                (
                    b.database_key_index(input.as_id()),
                    b.memo(db.zalsa(), input.as_id()).unwrap(),
                ),
            ]
        }

        fn assert_shallow(db: &dyn Database, input: Input, expected: ShallowUpdate) {
            for (key, memo) in query_memos(db, input) {
                let header = memo.header();
                assert!(memo.has_value());
                assert!(!header.may_be_provisional());
                assert!(!header.has_incomplete_attempt());
                assert_eq!(header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
                assert_eq!(header.revisions.durability, Durability::HIGH);
                assert!(
                    header.shallow_verify_memo(
                        db.zalsa(),
                        key,
                        #[cfg(feature = "detailed-trace")]
                        memo.has_value(),
                    ) == expected
                );
            }
        }

        struct CacheSnapshot {
            query_addresses: [usize; 2],
            outputs: Ownership,
            products: Vec<(Id, u32)>,
            specified_id: Id,
            specified_value: u32,
            specified_address: usize,
            revision: Revision,
        }

        impl CacheSnapshot {
            fn capture(db: &dyn Database, input: Input, events: &[NativeEvent]) -> Self {
                let products = created(events);
                let (specified_id, value) =
                    *products.last().expect("B creates a final-iteration output");
                assert_eq!(value, 2);
                let specified_value = value + 10;
                assert_eq!(
                    specified(db, Product::from_id(specified_id)),
                    specified_value
                );
                assert!(
                    take_events().is_empty(),
                    "specification must not execute its fallback"
                );
                let memos = query_memos(db, input);
                let specified_memo = specified::fn_ingredient_(db, db.zalsa())
                    .memo(db.zalsa(), specified_id)
                    .unwrap();
                let specified_header = specified_memo.header();
                assert!(!specified_header.may_be_provisional());
                assert!(!specified_header.has_incomplete_attempt());
                assert_eq!(
                    specified_header.attempt_reuse(db.zalsa()),
                    MemoReuse::Ordinary
                );
                assert!(
                    matches!(specified_header.origin(), QueryOriginRef::Assigned(owner) if owner == memos[1].0)
                );
                let outputs = Ownership::from_header(memos[1].1.header());
                assert!(!outputs.ids.is_empty());
                assert!(outputs.ids.iter().any(|(_, id)| *id == specified_id));
                assert!(outputs.assigned.contains(
                    &specified::fn_ingredient_(db, db.zalsa()).database_key_index(specified_id)
                ));
                assert_fields(db, &products);
                Self {
                    query_addresses: memos
                        .map(|(_, memo)| std::ptr::from_ref(memo.header()).addr()),
                    outputs,
                    products,
                    specified_id,
                    specified_value,
                    specified_address: std::ptr::from_ref(specified_header).addr(),
                    revision: db.zalsa().current_revision(),
                }
            }

            fn assert_reused(&self, db: &dyn Database, input: Input) {
                let memos = query_memos(db, input);
                assert_eq!(
                    memos.map(|(_, memo)| std::ptr::from_ref(memo.header()).addr()),
                    self.query_addresses,
                );
                let outputs = Ownership::from_header(memos[1].1.header());
                assert_eq!(outputs.ids, self.outputs.ids);
                assert_eq!(outputs.assigned, self.outputs.assigned);
                assert_eq!(
                    memos[1].1.header().revisions.output_order(),
                    OutputOrder::Inherited
                );
                assert_fields(db, &self.products);
                assert_eq!(
                    specified(db, Product::from_id(self.specified_id)),
                    self.specified_value
                );
                let memo = specified::fn_ingredient_(db, db.zalsa())
                    .memo(db.zalsa(), self.specified_id)
                    .unwrap();
                assert_eq!(
                    std::ptr::from_ref(memo.header()).addr(),
                    self.specified_address
                );
                assert!(!memo.header().may_be_provisional());
                assert!(!memo.header().has_incomplete_attempt());
                assert_eq!(
                    memo.header().verified_at.load(),
                    db.zalsa().current_revision()
                );
                assert_eq!(memo.header().attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
                assert!(
                    matches!(memo.header().origin(), QueryOriginRef::Assigned(owner) if owner == memos[1].0)
                );
                assert_shallow(db, input, ShallowUpdate::Verified);
            }
        }

        fn assert_cache_idle(db: &dyn Database, input: Input) {
            assert!(attempt_probe::current().is_none());
            assert_eq!(attempt_probe::stack_depths(), (0, 0));
            assert!(db.zalsa_local().active_query().is_none());
            assert!(!RUN_ACTIVE.with(std::cell::Cell::get));
            for ingredient in [
                a::fn_ingredient_(db, db.zalsa()) as &dyn FunctionIngredient,
                b::fn_ingredient_(db, db.zalsa()),
            ] {
                assert!(matches!(
                    ingredient
                        .sync_table()
                        .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny,),
                    ClaimResult::Claimed(())
                ));
            }
        }

        /// Final inherited-output memos reuse their values and identities in the same revision
        /// and after an unrelated lower-durability edit, without executing A or B or retiring outputs.
        #[test]
        fn inherited_outputs_keep_same_revision_and_higher_durability_reuse() {
            for entry in [Entry::Ordinary, Entry::Callable] {
                let mut db = TestDb::default();
                let input = Input::builder(2).durability(Durability::HIGH).new(&db);
                let unrelated = Input::builder(0).durability(Durability::LOW).new(&db);
                take_events();
                entry.read(&db, input);
                let snapshot = CacheSnapshot::capture(&db, input, &take_events());
                assert_eq!(
                    query_memos(&db, input)[1]
                        .1
                        .header()
                        .revisions
                        .output_order(),
                    OutputOrder::Inherited
                );
                assert_shallow(&db, input, ShallowUpdate::Verified);
                for _ in 0..2 {
                    entry.read(&db, input);
                    snapshot.assert_reused(&db, input);
                    assert!(
                        take_events().is_empty(),
                        "same-revision read executed a body"
                    );
                }
                assert!(db.discards.lock().unwrap().is_empty());

                unrelated.set_limit(&mut db).to(1);
                assert!(db.zalsa().current_revision() > snapshot.revision);
                assert!(db.zalsa().last_changed_revision(Durability::HIGH) <= snapshot.revision);
                assert_shallow(&db, input, ShallowUpdate::HigherDurability);
                entry.read(&db, input);
                snapshot.assert_reused(&db, input);
                assert!(
                    take_events().is_empty(),
                    "higher-durability read executed a body"
                );
                assert!(db.discards.lock().unwrap().is_empty());
                assert_cache_idle(&db, input);
            }
        }

        /// An unrelated edit at the query's durability makes inherited-output B execute again,
        /// preserving canonical values and reconciling omitted outputs through both entry paths.
        /// Shallow verification no longer proves reuse, and inherited output positions do not
        /// establish the input prerequisites needed to validate outputs through the stored edge order.
        #[test]
        fn inherited_outputs_reexecute_after_a_same_durability_irrelevant_edit() {
            for entry in [Entry::Ordinary, Entry::Callable] {
                let mut db = TestDb::default();
                let input = Input::builder(2).durability(Durability::HIGH).new(&db);
                let unrelated = Input::builder(0).durability(Durability::HIGH).new(&db);
                take_events();
                entry.read(&db, input);
                let before = CacheSnapshot::capture(&db, input, &take_events());
                assert_eq!(
                    query_memos(&db, input)[1]
                        .1
                        .header()
                        .revisions
                        .output_order(),
                    OutputOrder::Inherited
                );
                assert!(db.discards.lock().unwrap().is_empty());

                // Freeze ownership before the setter; old fields must not be read until the
                // canonical body has reconciled outputs in the new revision.
                let old_keys = before.outputs.keys();
                unrelated.set_limit(&mut db).to(1);
                assert!(db.zalsa().last_changed_revision(Durability::HIGH) > before.revision);
                assert_shallow(&db, input, ShallowUpdate::No);
                entry.read(&db, input);
                let events = take_events();
                let a_bodies = events
                    .iter()
                    .filter(|event| **event == NativeEvent::ABody)
                    .count();
                let b_bodies = events
                    .iter()
                    .filter(|event| matches!(event, NativeEvent::BValue(_)))
                    .count();
                assert!(
                    b_bodies > 0,
                    "inherited output order bypassed canonical B execution"
                );
                assert!(!events.contains(&NativeEvent::SpecifiedFallback));
                println!("INHERITED_OUTPUT_REEXECUTION\t{entry:?}\ta={a_bodies}\tb={b_bodies}");
                let after = CacheSnapshot::capture(&db, input, &events);
                assert_shallow(&db, input, ShallowUpdate::Verified);
                let new_keys = after.outputs.keys();
                let expected: Vec<_> = old_keys
                    .into_iter()
                    .filter(|key| !new_keys.contains(key))
                    .collect();
                let owner = b::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
                let discarded: Vec<_> = db
                    .discards
                    .lock()
                    .unwrap()
                    .iter()
                    .filter_map(|(executor, key)| (*executor == owner).then_some(*key))
                    .collect();
                assert_eq!(discarded.len(), expected.len());
                for key in expected {
                    assert!(
                        discarded.contains(&key),
                        "missing canonical retirement: {key:?}"
                    );
                }
                entry.read(&db, input);
                assert!(
                    take_events().is_empty(),
                    "reconciled result should be cached"
                );
                assert_cache_idle(&db, input);
            }
        }
    }

    mod remaining {
        use std::cell::Cell;
        use std::panic::{AssertUnwindSafe, catch_unwind, panic_any, resume_unwind};

        use super::*;
        use crate::Cancelled;
        use crate::function::execute::execution_run::RunError;

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum NativeStop {
            PendingWrite,
            Panic,
        }

        #[derive(Debug)]
        struct NativeFailure(Arc<()>);

        struct StoppingProvider<'run, 'db: 'run, C: Configuration> {
            inner: Provider<'run, 'db, C>,
            stop: NativeStop,
            identity: &'run Arc<()>,
        }

        impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for StoppingProvider<'run, 'db, C>
        where
            C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
        {
            // The delegated provider only converts an Input handle and compares u32 values.
            fixture_native_value!(callable, 'run, 'db, C, 1);

            async fn body<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                db: &'db dyn Database,
                input: Input,
            ) -> RunResult<u32>
            where
                'run: 'call,
            {
                let value = self.inner.body(endpoint.clone(), db, input).await?;
                let stop = self
                    .inner
                    .observations
                    .boundaries
                    .borrow()
                    .last()
                    .is_some_and(|boundary| boundary.child == 1);
                if stop {
                    endpoint
                        .local_call(|| -> RunResult<()> {
                            match self.stop {
                                NativeStop::PendingWrite => {
                                    // Fixed-point execution masks local cancellation. A pending write
                                    // still uses the real runtime cancellation check at this boundary.
                                    db.zalsa().runtime().set_cancellation_flag();
                                    endpoint.check_completion().expect(
                                        "a pending write must preserve its native cancellation",
                                    );
                                    panic!("pending-write cancellation returned");
                                }
                                NativeStop::Panic => {
                                    panic_any(NativeFailure(Arc::clone(self.identity)))
                                }
                            }
                        })
                        .await;
                }
                Ok(value)
            }

            async fn initial<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                db: &'db dyn Database,
                id: Id,
                input: Input,
            ) -> RunResult<u32>
            where
                'run: 'call,
            {
                self.inner.initial(endpoint, db, id, input).await
            }

            async fn recover<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                db: &'db dyn Database,
                cycle: &'call Cycle<'call>,
                previous: &'call u32,
                value: u32,
                input: Input,
            ) -> RunResult<u32>
            where
                'run: 'call,
            {
                self.inner
                    .recover(endpoint, db, cycle, previous, value, input)
                    .await
            }
        }

        fn run_native_stop(
            db: &dyn Database,
            input: Input,
            observations: &Observations,
            stop: NativeStop,
            identity: &Arc<()>,
        ) -> Result<AttemptOutcome<RunResult<u32>>, attempt_probe::StartError> {
            try_with_execution_budget(
                db,
                ExecutionLimits {
                    semantic_work: AMPLE_WORK,
                    requested_bytes: AMPLE_WORK,
                },
                |budget| {
                    let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                    let route = registry.reserve_callable(db, a::fn_ingredient_(db, db.zalsa()))?;
                    registry.bind_callable(
                        &route,
                        StoppingProvider {
                            inner: Provider {
                                route: route.clone(),
                                work_limit: AMPLE_WORK,
                                observations,
                            },
                            stop,
                            identity,
                        },
                    )?;
                    let registry = registry.seal()?;
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        registry.run(move |endpoint| async move {
                            Ok(*endpoint
                                .child_call(|| async {
                                    endpoint.fetch_ref(&route, input.as_id())?.await
                                })
                                .await)
                        })
                    }));
                    match outcome {
                        Ok(result) => result,
                        Err(payload) => {
                            // The driver records cancellation as Interrupted. Rethrowing through the
                            // outer attempt scope replaces the owner state with ABANDONED for either
                            // native failure, so inspect B's captured support before that scope unwinds.
                            let boundaries = observations.boundaries.borrow();
                            let before = boundaries
                                .last()
                                .expect("B returned before the native stop");
                            assert_eq!(
                                before.owner.reason(),
                                match stop {
                                    NativeStop::PendingWrite => Some(Incomplete::Interrupted),
                                    NativeStop::Panic => None,
                                }
                            );
                            resume_unwind(payload)
                        }
                    }
                },
            )
        }

        // Native failure after B publishes must leave B's output ownership intact. Cancellation
        // aborts the admitted head without panic poison; a genuine panic retains that poison.
        // Both cases retire omitted outputs only after an input setter starts a new revision.
        #[test]
        #[cfg(not(feature = "shuttle"))]
        fn native_stop_after_b_preserves_outputs_and_the_native_poison_contract() {
            for stop in [NativeStop::PendingWrite, NativeStop::Panic] {
                let mut db = TestDb::default();
                let input = Input::new(&db, 2);
                let revision = db.zalsa().current_revision();
                let observations = Observations::default();
                let identity = Arc::new(());
                take_events();
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    run_native_stop(&db, input, &observations, stop, &identity)
                }));
                db.zalsa().runtime().reset_cancellation_flag();
                let payload =
                    outcome.expect_err("the native stop cannot become an ordinary result");
                match stop {
                    NativeStop::PendingWrite => assert!(matches!(
                        payload.downcast_ref::<Cancelled>(),
                        Some(Cancelled::PendingWrite)
                    )),
                    NativeStop::Panic => assert!(Arc::ptr_eq(
                        &payload
                            .downcast_ref::<NativeFailure>()
                            .expect("the original native payload")
                            .0,
                        &identity,
                    )),
                }
                let products = created(&take_events());
                assert!(!products.is_empty());
                assert_fields(&db, &products);
                let retained;
                {
                    let boundaries = observations.boundaries.borrow();
                    let before = boundaries
                        .last()
                        .expect("B returned before the native stop");
                    assert_eq!(before.child, 1);
                    assert!(before.provisional);
                    assert_eq!(before.owner.reason(), None);
                    assert!(before.owner.incomplete(true));
                    let child = b::fn_ingredient_(&db, db.zalsa());
                    let memo = child.memo(db.zalsa(), input.as_id()).unwrap();
                    let header = memo.header();
                    assert!(memo.has_value());
                    assert!(header.may_be_provisional());
                    assert_eq!(header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
                    assert!(!header.can_seed_attempt(db.zalsa()));
                    retained = Ownership::from_header(header);
                    assert_eq!(retained.ids, before.outputs.ids);
                    assert_eq!(retained.assigned, before.outputs.assigned);
                    assert!(
                        header
                            .revisions
                            .attempt_support()
                            .unwrap()
                            .same_owner(&before.owner)
                    );
                    let head = a::fn_ingredient_(&db, db.zalsa());
                    assert_eq!(
                        head.memo(db.zalsa(), input.as_id()).unwrap().has_value(),
                        stop == NativeStop::PendingWrite
                    );
                    assert_idle(&db, input);
                    assert!(db.discards.lock().unwrap().is_empty());
                    assert_eq!(db.zalsa().current_revision(), revision);

                    if stop == NativeStop::Panic {
                        let retry = Observations::default();
                        let outcome = Cancelled::catch(AssertUnwindSafe(|| {
                            run(&db, input, AMPLE_WORK, &retry)
                        }));
                        assert!(matches!(outcome, Err(Cancelled::PropagatedPanic)));
                        assert!(retry.boundaries.borrow().is_empty());
                        assert!(take_events().is_empty());
                        let after = child.memo(db.zalsa(), input.as_id()).unwrap().header();
                        assert!(std::ptr::eq(header, after));
                        assert_eq!(Ownership::from_header(after).ids, retained.ids);
                        assert_eq!(Ownership::from_header(after).assigned, retained.assigned);
                        assert_idle(&db, input);
                    }
                }

                let expected_discards = retained.keys();
                input.set_limit(&mut db).to(0);
                let ordinary = TestDb::default();
                let ordinary_input = Input::new(&ordinary, 0);
                let expected = a(&ordinary, ordinary_input);
                assert_eq!(expected, 0);
                take_events();
                let complete = Observations::default();
                assert_eq!(
                    run(&db, input, AMPLE_WORK, &complete),
                    Ok(AttemptOutcome::Complete(Ok(expected)))
                );
                assert!(created(&take_events()).is_empty());
                let child = b::fn_ingredient_(&db, db.zalsa());
                let header = child.memo(db.zalsa(), input.as_id()).unwrap().header();
                assert!(header.revisions.tracked_struct_ids().is_empty());
                assert!(header.origin().outputs().next().is_none());
                let key = child.database_key_index(input.as_id());
                let discarded: Vec<_> = db
                    .discards
                    .lock()
                    .unwrap()
                    .iter()
                    .filter_map(|(owner, output)| (*owner == key).then_some(*output))
                    .collect();
                assert_eq!(discarded.len(), expected_discards.len());
                for output in expected_discards {
                    assert!(
                        discarded.contains(&output),
                        "missing canonical retirement: {output:?}"
                    );
                }
                assert_idle(&db, input);
            }
        }

        thread_local! {
            static ORDINARY_INITIAL: Cell<bool> = const { Cell::new(false) };
            static ORDINARY_PROVISIONAL: Cell<bool> = const { Cell::new(false) };
        }

        #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = ordinary_initial, cycle_fn = recover)]
        fn independent(db: &dyn Database, input: Input) -> u32 {
            assert!(attempt_probe::current_query().is_none());
            assert!(attempt_probe::current_cycle_support(db.zalsa()).is_none());
            let previous = independent(db, input);
            let ingredient = independent::fn_ingredient_(db, db.zalsa());
            let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
            if memo.header().may_be_provisional() {
                assert!(memo.has_value());
                assert!(memo.header().revisions.attempt_support().is_none());
                ORDINARY_PROVISIONAL.set(true);
            }
            (previous + 1).min(input.limit(db))
        }

        fn ordinary_initial(db: &dyn Database, _id: Id, _input: Input) -> u32 {
            assert!(attempt_probe::current_query().is_none());
            assert!(attempt_probe::current_cycle_support(db.zalsa()).is_none());
            ORDINARY_INITIAL.set(true);
            0
        }

        fn assert_independent(db: &dyn Database, input: Input) {
            ORDINARY_INITIAL.set(false);
            ORDINARY_PROVISIONAL.set(false);
            assert_eq!(independent(db, input), 2);
            assert!(ORDINARY_INITIAL.get());
            assert!(ORDINARY_PROVISIONAL.get());
            let ingredient = independent::fn_ingredient_(db, db.zalsa());
            let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
            assert!(!memo.header().may_be_provisional());
            assert!(memo.header().revisions.attempt_support().is_none());
            assert!(matches!(
                ingredient
                    .sync_table()
                    .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
                ClaimResult::Claimed(())
            ));
            assert!(attempt_probe::current_cycle_support(db.zalsa()).is_none());
        }

        // A budget alone does not give an ordinary cycle an admitted ancestor. Run fresh keys
        // after a completed registered cycle and after an unrelated Work refusal in that same
        // budget, and inspect their real provisional seeds for absent attempt ownership.
        #[test]
        fn independent_ordinary_cycle_stays_ownerless_after_success_and_refusal() {
            let db = TestDb::default();
            let controlled = Input::new(&db, 2);
            let after_success = Input::new(&db, 2);
            let after_refusal = Input::new(&db, 2);
            let observations = Observations::default();
            let result = try_with_execution_budget(
                &db,
                ExecutionLimits {
                    semantic_work: AMPLE_WORK,
                    requested_bytes: AMPLE_WORK,
                },
                |budget| {
                    let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                    let route = registry.reserve_callable(
                        &db as &dyn Database,
                        a::fn_ingredient_(&db, db.zalsa()),
                    )?;
                    registry.bind_callable(
                        &route,
                        Provider {
                            route: route.clone(),
                            work_limit: AMPLE_WORK,
                            observations: &observations,
                        },
                    )?;
                    assert_eq!(
                        registry.seal()?.run(move |endpoint| async move {
                            Ok(*endpoint
                                .child_call(|| async {
                                    endpoint.fetch_ref(&route, controlled.as_id())?.await
                                })
                                .await)
                        }),
                        Ok(2)
                    );
                    assert_idle(&db, controlled);
                    assert_independent(&db, after_success);

                    let db_ref = &db;
                    let refused = RegistryBuilder::with_budget(&db, &budget)?.seal()?.run(
                        |endpoint| async move {
                            endpoint
                                .local_call(|| {
                                    let remaining =
                                        attempt_probe::remaining_allowance_for_diagnostics(db_ref)
                                            .unwrap();
                                    endpoint.admit_work(remaining + 1)
                                })
                                .await;
                            Ok(())
                        },
                    );
                    assert_eq!(refused, Err(RunError::Refused(Incomplete::Allowance)));
                    assert_idle(&db, controlled);
                    assert_independent(&db, after_refusal);
                    Ok::<_, RunError>(())
                },
            );
            assert_eq!(
                result,
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            );
            assert_idle(&db, controlled);
        }
    }
}
#[cfg(feature = "persistence")]
mod persistent_mixed_output_lifecycle {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex};

    use crate::attempt_probe::{
        self, AttemptOutcome, AttemptSupport, ExecutionLimits, Incomplete, MemoReuse,
        try_with_execution_budget,
    };
    use crate::function::execute::execution_run::registration::{
        CallableRoute, CallableRouteProvider, RegistryBuilder, TaskEndpoint,
    };
    use crate::function::execute::execution_run::{RUN_ACTIVE, RunResult};
    use crate::function::memo::{FinalSourceMemo, MemoHeader};
    use crate::function::{
        ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Memo, Reentrancy,
    };
    use crate::plumbing::{AsId, FromId, SalsaStructInDb};
    use crate::prepared_source_probe::Stamp;
    use crate::tracked_struct::Identity;
    use crate::zalsa::ZalsaDatabase;
    use crate::zalsa_local::{OutputOrder, QueryOriginRef};
    use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id, Setter};

    const AMPLE_WORK: usize = 100_000;

    #[crate::db]
    #[derive(Clone)]
    struct TestDb {
        storage: crate::Storage<Self>,
        discards: Arc<Mutex<Vec<(DatabaseKeyIndex, DatabaseKeyIndex)>>>,
        validations: Arc<Mutex<Vec<DatabaseKeyIndex>>>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let discards = Arc::new(Mutex::new(Vec::new()));
            let events = discards.clone();
            let validations = Arc::new(Mutex::new(Vec::new()));
            let validated = validations.clone();
            Self {
                storage: crate::Storage::new(Some(Box::new(move |event| match event.kind {
                    EventKind::WillDiscardStaleOutput {
                        execute_key,
                        output_key,
                    } => {
                        events.lock().unwrap().push((execute_key, output_key));
                    }
                    EventKind::DidValidateMemoizedValue { database_key } => {
                        validated.lock().unwrap().push(database_key);
                    }
                    _ => {}
                }))),
                discards,
                validations,
            }
        }
    }

    #[crate::db]
    impl Database for TestDb {}

    #[crate::input(persist)]
    struct Input {
        #[returns(copy)]
        limit: u32,
    }

    // Every Product has the same identity hash. Retried construction must reserve old
    // disambiguators so that previously read values cannot be overwritten.
    #[crate::tracked(persist)]
    struct Product<'db> {
        #[tracked]
        #[returns(copy)]
        value: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum NativeEvent {
        ABody,
        AInitial(u32),
        BValue(u32),
        Created { id: Id, value: u32 },
        SpecifiedFallback,
    }

    thread_local! {
        static NATIVE_EVENTS: RefCell<Vec<NativeEvent>> = const { RefCell::new(Vec::new()) };
    }

    fn record(event: NativeEvent) {
        NATIVE_EVENTS.with_borrow_mut(|events| events.push(event));
    }

    fn take_events() -> Vec<NativeEvent> {
        NATIVE_EVENTS.with_borrow_mut(std::mem::take)
    }

    #[crate::tracked(returns(copy), persist, attempt = CompleteOnly, cycle_initial = initial_a, cycle_fn = recover)]
    fn a(db: &dyn Database, input: Input) -> u32 {
        record(NativeEvent::ABody);
        let previous = a(db, input);
        let child = b(db, input);
        (previous.max(child) + 1).min(input.limit(db))
    }

    #[crate::tracked(returns(copy), persist, attempt = CompleteOnly, cycle_initial = initial_b, cycle_fn = recover)]
    fn b(db: &dyn Database, input: Input) -> u32 {
        let value = a(db, input);
        record(NativeEvent::BValue(value));
        if value < input.limit(db) {
            let product = Product::new(db, value);
            specified::specify(db, product, value + 10);
            record(NativeEvent::Created {
                id: product.as_id(),
                value,
            });
            product.value(db)
        } else {
            value
        }
    }

    #[crate::tracked(returns(copy), persist, attempt = CompleteOnly, specify)]
    fn specified<'db>(_db: &'db dyn Database, _product: Product<'db>) -> u32 {
        record(NativeEvent::SpecifiedFallback);
        0
    }

    fn initial_a(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        record(NativeEvent::AInitial(0));
        0
    }

    fn initial_b(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        0
    }

    fn recover(
        _db: &dyn Database,
        _cycle: &Cycle<'_>,
        _previous: &u32,
        value: u32,
        _input: Input,
    ) -> u32 {
        value
    }

    #[derive(Clone, Debug)]
    struct Ownership {
        ids: Vec<(Identity, Id)>,
        assigned: Vec<DatabaseKeyIndex>,
    }

    impl Ownership {
        fn from_header(header: &MemoHeader) -> Self {
            Self {
                ids: header.revisions.tracked_struct_ids().to_vec(),
                assigned: header.origin().outputs().collect(),
            }
        }

        fn keys(&self) -> Vec<DatabaseKeyIndex> {
            self.ids
                .iter()
                .map(|(identity, id)| DatabaseKeyIndex::new(identity.ingredient_index(), *id))
                .chain(self.assigned.iter().copied())
                .collect()
        }

        fn assert_contains(&self, earlier: &Self) {
            for id in &earlier.ids {
                assert!(
                    self.ids.contains(id),
                    "lost earlier tracked identity: {id:?}"
                );
            }
            for key in &earlier.assigned {
                assert!(
                    self.assigned.contains(key),
                    "lost earlier assigned output: {key:?}"
                );
            }
        }
    }

    #[derive(Debug)]
    struct Boundary {
        work_before: usize,
        child: u32,
        owner: AttemptSupport,
        outputs: Ownership,
        provisional: bool,
    }

    #[derive(Default)]
    struct Observations {
        boundaries: RefCell<Vec<Boundary>>,
    }

    impl Observations {
        fn refusal_prefix(&self) -> usize {
            let boundaries = self.boundaries.borrow();
            let boundary = boundaries
                .iter()
                .find(|boundary| boundary.child == 1)
                .expect("the finite cycle reaches a nonzero provisional B value");
            assert!(boundary.provisional);
            assert!(!boundary.outputs.ids.is_empty());
            assert!(!boundary.outputs.assigned.is_empty());
            boundary.work_before
        }
    }

    struct Provider<'run, 'db: 'run, C: Configuration> {
        route: CallableRoute<'run, 'db, C>,
        work_limit: usize,
        observations: &'run Observations,
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    {
        // Input conversion copies an Input handle; output equality compares one u32.
        fixture_native_value!(callable, 'run, 'db, C, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            let previous = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                .await;
            let child = endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(b(db, input))
                })
                .await;
            Ok(endpoint
                .local_call(|| {
                    let ingredient = b::fn_ingredient_(db, db.zalsa());
                    let memo = ingredient
                        .memo(db.zalsa(), input.as_id())
                        .expect("native B returned with its canonical memo");
                    let header = memo.header();
                    let owner = header
                        .revisions
                        .attempt_support()
                        .expect("B read the admitted cycle's provisional A value")
                        .clone();
                    assert!(owner.same_owner(&attempt_probe::current_query().unwrap()));
                    let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
                    self.observations.boundaries.borrow_mut().push(Boundary {
                        work_before: self.work_limit - remaining,
                        child,
                        owner,
                        outputs: Ownership::from_header(header),
                        provisional: header.may_be_provisional(),
                    });
                    // This real work request precedes the next semantic calculation. Observation
                    // only records its consumed prefix; the shared budget decides whether it runs.
                    endpoint.admit_work(1)?;
                    Ok(((*previous).max(child) + 1).min(input.limit(db)))
                })
                .await)
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::cycle_initial(db, id, input))
                })
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            previous: &'call u32,
            value: u32,
            input: Input,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(C::recover_from_cycle(db, cycle, previous, value, input))
                })
                .await)
        }
    }

    fn run(
        db: &dyn Database,
        input: Input,
        work_limit: usize,
        observations: &Observations,
    ) -> Result<AttemptOutcome<RunResult<u32>>, attempt_probe::StartError> {
        try_with_execution_budget(
            db,
            ExecutionLimits {
                semantic_work: work_limit,
                requested_bytes: AMPLE_WORK,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(db, &budget)?;
                let route = registry.reserve_callable(db, a::fn_ingredient_(db, db.zalsa()))?;
                registry.bind_callable(
                    &route,
                    Provider {
                        route: route.clone(),
                        work_limit,
                        observations,
                    },
                )?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(*endpoint
                        .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                        .await)
                })
            },
        )
    }

    fn assert_idle(db: &dyn Database, input: Input) {
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(!RUN_ACTIVE.with(std::cell::Cell::get));
        for ingredient in [
            a::fn_ingredient_(db, db.zalsa()) as &dyn FunctionIngredient,
            b::fn_ingredient_(db, db.zalsa()),
        ] {
            assert!(matches!(
                ingredient
                    .sync_table()
                    .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
                ClaimResult::Claimed(())
            ));
        }
    }

    fn assert_fresh_seed(events: &[NativeEvent]) {
        assert_eq!(
            events.iter().find_map(|event| match event {
                NativeEvent::AInitial(value) => Some(*value),
                _ => None,
            }),
            Some(0)
        );
        assert_eq!(
            events.iter().find_map(|event| match event {
                NativeEvent::BValue(value) => Some(*value),
                _ => None,
            }),
            Some(0)
        );
        assert!(!events.contains(&NativeEvent::ABody));
        assert!(!events.contains(&NativeEvent::SpecifiedFallback));
    }

    fn created(events: &[NativeEvent]) -> Vec<(Id, u32)> {
        events
            .iter()
            .filter_map(|event| match event {
                NativeEvent::Created { id, value } => Some((*id, *value)),
                _ => None,
            })
            .collect()
    }

    fn assert_fields(db: &dyn Database, products: &[(Id, u32)]) {
        for (id, expected) in products {
            assert_eq!(Product::from_id(*id).value(db), *expected);
        }
    }

    fn assert_stopped(db: &dyn Database, input: Input, observations: &Observations, prefix: usize) {
        assert_eq!(observations.refusal_prefix(), prefix);
        let boundaries = observations.boundaries.borrow();
        let last = boundaries.last().unwrap();
        assert_eq!(last.work_before, prefix);
        assert_eq!(last.child, 1);
        assert_eq!(last.owner.reason(), Some(Incomplete::Allowance));
        let ingredient = b::fn_ingredient_(db, db.zalsa());
        let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
        let header = memo.header();
        assert!(memo.has_value());
        assert!(header.may_be_provisional());
        assert_eq!(header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
        assert!(!header.can_seed_attempt(db.zalsa()));
        assert!(
            header
                .revisions
                .attempt_support()
                .unwrap()
                .same_owner(&last.owner)
        );
        assert!(
            FinalSourceMemo::certify(db, a::fn_ingredient_(db, db.zalsa()), input.as_id()).is_err()
        );
        assert_idle(db, input);
    }

    fn stored<'db, C: Configuration>(
        db: &'db dyn Database,
        ingredient: &'db IngredientImpl<C>,
        id: Id,
    ) -> Option<&'db Memo<C>> {
        ingredient.get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
    }

    #[crate::tracked(returns(copy), persist)]
    fn acyclic(db: &dyn Database, input: Input) -> u32 {
        input.limit(db)
    }

    // Measure the work consumed just before the checkpoint after B returns 1. Replaying that
    // limit on a fresh database lets the shared budget stop a real noninitial approximation.
    fn calibration_prefix() -> usize {
        let db = TestDb::default();
        let input = Input::new(&db, 2);
        let observed = Observations::default();
        assert_eq!(
            run(&db, input, AMPLE_WORK, &observed),
            Ok(AttemptOutcome::Complete(Ok(2)))
        );
        let prefix = observed.refusal_prefix();
        println!("PERSISTENT_MIXED_OUTPUT_REFUSAL_PREFIX\t{prefix}");
        prefix
    }

    // The driver runs A through its callable route while B uses ordinary native cycle handling.
    // A completed owner can retain Products and assigned-output keys from an earlier stopped
    // attempt. Persistence preserves those entities and cleanup keys, but the abandoned Assigned
    // values remain ineligible for serialization. A later input edit must reexecute the owner
    // before validating any inherited output, then retire every omitted entity and assigned key.
    // Preserving Inherited order prevents copied output positions from authorizing that validation;
    // the acyclic query checks that Execution order also survives the round trip.
    #[test]
    fn round_trip_preserves_inherited_ownership_without_abandoned_values() {
        let prefix = calibration_prefix();
        let mut db = TestDb::default();
        let input = Input::new(&db, 2);
        let stamp = Stamp::current(&db);
        take_events();
        let stopped = Observations::default();
        assert_eq!(
            run(&db, input, prefix, &stopped),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_stopped(&db, input, &stopped, prefix);
        let events = take_events();
        assert_fresh_seed(&events);
        let stopped_products = created(&events);
        assert!(!stopped_products.is_empty());
        assert_fields(&db, &stopped_products);
        let stopped_header = b::fn_ingredient_(&db, db.zalsa())
            .memo(db.zalsa(), input.as_id())
            .unwrap()
            .header();
        let stopped_outputs = Ownership::from_header(stopped_header);
        let stopped_owner = stopped_header.revisions.attempt_support().unwrap().clone();
        assert_eq!(
            stopped_header.revisions.output_order(),
            OutputOrder::Inherited
        );
        assert_eq!(stopped_outputs.ids.len(), stopped_products.len());
        assert_eq!(stopped_outputs.assigned.len(), stopped_products.len());
        let specified_ingredient = specified::fn_ingredient_(&db, db.zalsa());
        for (id, value) in &stopped_products {
            assert!(stopped_outputs.ids.iter().any(|(_, tracked)| tracked == id));
            assert!(
                stopped_outputs
                    .assigned
                    .contains(&specified_ingredient.database_key_index(*id))
            );
            let assigned = stored(&db, specified_ingredient, *id).unwrap();
            assert_eq!(assigned.value(), Some(&(value + 10)));
            assert!(assigned.header.may_be_provisional());
            assert!(assigned.header.has_incomplete_attempt());
            assert!(!assigned.should_serialize());
            assert!(
                assigned
                    .header
                    .revisions
                    .attempt_support()
                    .unwrap()
                    .same_owner(&stopped_owner)
            );
        }

        let completed = Observations::default();
        assert_eq!(
            run(&db, input, AMPLE_WORK, &completed),
            Ok(AttemptOutcome::Complete(Ok(2)))
        );
        let events = take_events();
        assert_fresh_seed(&events);
        let mut products = stopped_products.clone();
        for (id, value) in created(&events) {
            assert!(!products.iter().any(|(earlier, _)| *earlier == id));
            products.push((id, value));
        }
        assert_fields(&db, &products);
        assert_eq!(a(&db, input), 2);
        // Completing the cycle head A does not itself certify B's retained memo as final.
        // Reading B verifies its completed head before we require B's memo to be serializable.
        assert_eq!(b(&db, input), 2);
        assert!(take_events().is_empty());
        let owner = stored(&db, b::fn_ingredient_(&db, db.zalsa()), input.as_id()).unwrap();
        assert!(!owner.header.may_be_provisional());
        assert!(owner.should_serialize());
        assert!(
            !owner
                .header
                .revisions
                .attempt_support()
                .unwrap()
                .same_owner(&stopped_owner)
        );
        assert_eq!(
            owner.header.revisions.output_order(),
            OutputOrder::Inherited
        );
        let retained = Ownership::from_header(&owner.header);
        retained.assert_contains(&stopped_outputs);
        assert_eq!(retained.ids.len(), products.len());
        for (id, _) in &stopped_products {
            let assigned = stored(&db, specified_ingredient, *id).unwrap();
            assert!(assigned.header.may_be_provisional());
            assert!(assigned.header.has_incomplete_attempt());
            assert_eq!(assigned.header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
            assert!(!assigned.should_serialize());
            assert!(
                matches!(assigned.header.origin(), QueryOriginRef::Assigned(key)
                if key == b::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()))
            );
        }
        assert_eq!(acyclic(&db, input), 2);
        let acyclic_memo =
            stored(&db, acyclic::fn_ingredient_(&db, db.zalsa()), input.as_id()).unwrap();
        assert_eq!(
            acyclic_memo.header.revisions.output_order(),
            OutputOrder::Execution
        );
        assert!(acyclic_memo.should_serialize());
        assert!(db.discards.lock().unwrap().is_empty());
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db, input);

        let serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
        assert!(take_events().is_empty());
        let mut restored = TestDb::default();
        <dyn Database>::deserialize(
            &mut restored,
            &mut serde_json::Deserializer::from_str(&serialized),
        )
        .unwrap();
        assert!(take_events().is_empty());
        assert_eq!(
            restored.zalsa().current_revision(),
            db.zalsa().current_revision()
        );
        assert_eq!(input.limit(&restored), 2);
        let restored_owner = stored(
            &restored,
            b::fn_ingredient_(&restored, restored.zalsa()),
            input.as_id(),
        )
        .unwrap();
        assert_eq!(restored_owner.value(), Some(&2));
        assert!(!restored_owner.header.may_be_provisional());
        assert!(restored_owner.header.revisions.attempt_support().is_none());
        assert_eq!(
            restored_owner.header.revisions.output_order(),
            OutputOrder::Inherited
        );
        let restored_outputs = Ownership::from_header(&restored_owner.header);
        assert_eq!(restored_outputs.ids, retained.ids);
        assert_eq!(restored_outputs.assigned, retained.assigned);
        let entities: Vec<_> = Product::entries(restored.zalsa()).collect();
        assert_eq!(entities.len(), products.len());
        for (id, _) in &products {
            assert!(entities.iter().any(|key| key.key_index() == *id));
        }
        assert_fields(&restored, &products);
        let restored_specified = specified::fn_ingredient_(&restored, restored.zalsa());
        for (id, _) in &stopped_products {
            assert!(stored(&restored, restored_specified, *id).is_none());
        }
        let acyclic_memo = stored(
            &restored,
            acyclic::fn_ingredient_(&restored, restored.zalsa()),
            input.as_id(),
        )
        .unwrap();
        assert_eq!(acyclic_memo.value(), Some(&2));
        assert_eq!(
            acyclic_memo.header.revisions.output_order(),
            OutputOrder::Execution
        );
        assert!(restored.discards.lock().unwrap().is_empty());
        assert!(take_events().is_empty());

        // Copy the retirement keys before the edit. Reading old fields or their memo slots in
        // the new revision would acquire locks and interfere with the deletion being tested.
        let expected_discards = restored_outputs.keys();
        let owner_key =
            b::fn_ingredient_(&restored, restored.zalsa()).database_key_index(input.as_id());
        restored.validations.lock().unwrap().clear();
        input.set_limit(&mut restored).to(0);
        let completed = Observations::default();
        assert_eq!(
            run(&restored, input, AMPLE_WORK, &completed),
            Ok(AttemptOutcome::Complete(Ok(0)))
        );
        let events = take_events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, NativeEvent::BValue(_)))
        );
        assert!(created(&events).is_empty());
        assert!(!events.contains(&NativeEvent::SpecifiedFallback));
        let owner = stored(
            &restored,
            b::fn_ingredient_(&restored, restored.zalsa()),
            input.as_id(),
        )
        .unwrap();
        assert!(owner.header.revisions.tracked_struct_ids().is_empty());
        assert!(owner.header.origin().outputs().next().is_none());
        assert!(Product::entries(restored.zalsa()).next().is_none());
        let validations = restored.validations.lock().unwrap();
        for key in &restored_outputs.assigned {
            assert!(
                !validations.contains(key),
                "inherited output validated before retirement: {key:?}"
            );
        }
        drop(validations);
        let discards = restored.discards.lock().unwrap();
        assert_eq!(discards.len(), expected_discards.len());
        for output in expected_discards {
            assert_eq!(
                discards
                    .iter()
                    .filter(|event| **event == (owner_key, output))
                    .count(),
                1
            );
        }
        drop(discards);
        assert_idle(&restored, input);
    }
}

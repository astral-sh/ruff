use std::cell::{Cell, RefCell};

use super::{Caller, Refresh, RunOperation, complete_selected, select};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
use crate::function::execute::execution_run::registration::{
    ExecutableRouteProvider, NativeValueOperation, NativeValueQuote, ProviderContext,
    RegistryBuilder, RetainedInput, Route, TaskEndpoint,
};
use crate::function::execute::execution_run::tests::observation;
use crate::function::execute::execution_run::{
    Endpoint, ExecutionAdmission, ExecutionProvider, ExecutionWork, RunError, RunResult,
    execute_owned,
};
use crate::function::fetch::completion_observation::{self as completion, Stage};
use crate::function::sync::ClaimResult;
use crate::function::{Configuration, IngredientImpl, Memo, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::{self, Status};
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::QueryEdgeKind;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id};

mod selected_delivery;

#[crate::input(debug)]
struct Number {
    #[returns(copy)]
    value: u32,
}

#[derive(Debug, Eq, PartialEq)]
struct Value {
    input: Number,
    value: u32,
}

impl Clone for Value {
    fn clone(&self) -> Self {
        crate::with_attached_database(|db| {
            let ingredient = leaf::fn_ingredient_(db, db.zalsa());
            completion::record(
                Stage::Converted,
                db.zalsa_local(),
                ingredient.database_key_index(self.input.as_id()),
                // Conversion receives a value reference, not its memo handle.
                0,
            );
        });
        Self {
            input: self.input,
            value: self.value,
        }
    }
}

fn leaf_value(db: &dyn Database, input: Number) -> u32 {
    input.value(db)
}

fn parent_value(value: u32) -> u32 {
    value + 1
}

fn cycle_value(value: u32) -> u32 {
    (value + 1).min(2)
}

#[crate::tracked(returns(clone), attempt = ReturnOnly)]
fn leaf(db: &dyn Database, input: Number) -> Value {
    Value {
        input,
        value: leaf_value(db, input),
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn parent(db: &dyn Database, input: Number) -> u32 {
    parent_value(leaf(db, input).value)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn cyclic(db: &dyn Database, input: Number) -> u32 {
    cycle_value(cyclic(db, input))
}

fn initial(_db: &dyn Database, _id: Id, _input: Number) -> u32 {
    0
}

fn recover(_db: &dyn Database, _cycle: &Cycle<'_>, _old: &u32, value: u32, _input: Number) -> u32 {
    value
}

trait Output {
    const LEAF: bool;
    const COMPARISON_WORK: usize;
    fn new(input: Number, value: u32) -> Self;
}

impl Output for Value {
    const LEAF: bool = true;
    const COMPARISON_WORK: usize = 2;
    fn new(input: Number, value: u32) -> Self {
        Self { input, value }
    }
}

impl Output for u32 {
    const LEAF: bool = false;
    const COMPARISON_WORK: usize = 1;
    fn new(_input: Number, value: u32) -> Self {
        value
    }
}

fn native_value_quote<'call, 'db, C>(
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: Configuration<Input<'db> = Number>,
    C::Output<'db>: Output,
{
    let work = match operation {
        // Number conversion constructs its generated identity. Value's derived equality
        // compares that identity and a u32; its custom delivery Clone is a separate operation.
        NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => 1,
        NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
            return Err(RunError::Contract(
                "fetch fixture requires a generated Number handle",
            ));
        }
        NativeValueOperation::Comparison { .. } => <C::Output<'db> as Output>::COMPARISON_WORK,
    };
    Ok(NativeValueQuote {
        work,
        requested_bytes: 0,
        cleanup_work: 0,
    })
}

#[derive(Clone, Copy, Default)]
enum Mode {
    #[default]
    Normal,
    Replace,
    Cycle,
}

#[derive(Default)]
struct Script {
    mode: Mode,
    leaf_bodies: Cell<usize>,
    leaf_depth: Cell<usize>,
    selected_address: Cell<Option<usize>>,
    replacement_address: Cell<Option<usize>>,
    parent_inputs: RefCell<Option<Vec<DatabaseKeyIndex>>>,
}

impl Script {
    fn leaf(&self, db: &dyn Database, input: Number) -> Value {
        self.leaf_bodies.set(self.leaf_bodies.get() + 1);
        self.leaf_depth.set(attempt_probe::stack_depths().0);
        Value {
            input,
            value: leaf_value(db, input),
        }
    }
}

fn inputs(db: &dyn Database) -> Vec<DatabaseKeyIndex> {
    db.zalsa_local()
        .try_with_query_stack(|stack| {
            stack.last().map_or_else(Vec::new, |frame| {
                frame
                    .completion_state()
                    .0
                    .iter()
                    .filter_map(|edge| (edge.kind() == QueryEdgeKind::Input).then_some(edge.key()))
                    .collect()
            })
        })
        .expect("caller query stack is available")
}

struct ParentSnapshot<'a> {
    db: &'a dyn Database,
    script: &'a Script,
}

impl Drop for ParentSnapshot<'_> {
    fn drop(&mut self) {
        *self.script.parent_inputs.borrow_mut() = Some(inputs(self.db));
    }
}

struct Providers<'a, 'db, L: Configuration, P: Configuration> {
    leaf: Route<'db, L>,
    parent: Route<'db, P>,
    script: &'a Script,
}

impl<'run, 'db: 'run, L, P, C> ExecutableRouteProvider<'run, 'db, C> for Providers<'run, 'db, L, P>
where
    L: Configuration<DbView = dyn Database, Input<'db> = Number, Output<'db> = Value>,
    P: Configuration<DbView = dyn Database, Input<'db> = Number, Output<'db> = u32>,
    C: Configuration<DbView = dyn Database, Input<'db> = Number>,
    C::Output<'db>: Output,
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
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        input: Number,
    ) -> RunResult<C::Output<'db>> {
        let value = if <C::Output<'db> as Output>::LEAF {
            self.script.leaf(db, input).value
        } else {
            let _snapshot = ParentSnapshot {
                db,
                script: self.script,
            };
            match self.script.mode {
                Mode::Cycle => cycle_value(*context.fetch_ref(&self.parent, input.as_id())?.await?),
                Mode::Normal | Mode::Replace => {
                    let value = match self.script.mode {
                        Mode::Replace => {
                            let endpoint = context.endpoint().clone();
                            let ingredient = leaf::fn_ingredient_(db, db.zalsa());
                            context
                                .endpoint()
                                .demand(move || {
                                    replace_selected(endpoint, db, ingredient, input, self.script)
                                })?
                                .await?
                        }
                        Mode::Normal | Mode::Cycle => {
                            context.fetch_ref(&self.leaf, input.as_id())?.await?
                        }
                    };
                    parent_value(value.clone().value)
                }
            }
        };
        Ok(<C::Output<'db> as Output>::new(input, value))
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        id: Id,
        input: Number,
    ) -> RunResult<C::Output<'db>> {
        Ok(<C::Output<'db> as Output>::new(
            input,
            initial(db, id, input),
        ))
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call C::Output<'db>,
        value: C::Output<'db>,
        _input: Number,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        Ok(value)
    }
}

struct LeafCallbacks<'a>(&'a Script);

impl<'run, 'db: 'run, C> ExecutionProvider<'run, 'db, C> for LeafCallbacks<'_>
where
    C: Configuration<DbView = dyn Database, Input<'db> = Number, Output<'db> = Value>,
{
    async fn native_value<'call>(
        &'call self,
        _db: &'db dyn Database,
        operation: NativeValueOperation<'call, 'db, C>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_value_quote(operation)
    }

    async fn body<'call>(
        &'call self,
        db: &'db dyn Database,
        input: Number,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        Ok(self.0.leaf(db, input))
    }
    async fn initial<'call>(
        &'call self,
        _db: &'db dyn Database,
        _id: Id,
        _input: Number,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        Err(RunError::Contract("finite leaf has no cycle initializer"))
    }
    async fn recover<'call>(
        &'call self,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        _value: Value,
        _input: Number,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<Value>
    where
        'run: 'call,
    {
        Err(RunError::Contract("finite leaf has no cycle recovery"))
    }
}

async fn replace_selected<'run, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    input: Number,
    script: &'run Script,
) -> RunResult<&'db Value>
where
    C: Configuration<DbView = dyn Database, Input<'db> = Number, Output<'db> = Value>,
{
    endpoint.admit(ExecutionWork::Resource {
        requested_bytes: 4 * size_of::<usize>(),
    })?;
    let operation = RunOperation::enter::<C>(endpoint.inner.context.clone())?;
    let caller = Caller::capture(&operation)?;
    let refresh = Refresh::new(
        ingredient,
        db,
        db.zalsa(),
        db.zalsa_local(),
        input.as_id(),
        ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
    );
    let callbacks = LeafCallbacks(script);
    let mut selected = select(&endpoint, &operation, caller, refresh, &callbacks).await?;
    let original = selected.admitted_mut()?.memo();
    let original_value = selected.admitted_mut()?.value();
    script
        .selected_address
        .set(Some(std::ptr::from_ref(original).addr()));
    let child_endpoint = endpoint.inner.clone();
    let replacement = endpoint
        .demand(move || async move {
            let operation = RunOperation::enter::<C>(child_endpoint.context.clone())?;
            let claim = match ingredient.sync_table.try_claim(
                db.zalsa(),
                db.zalsa_local(),
                input.as_id(),
                Reentrancy::Deny,
            ) {
                ClaimResult::Claimed(claim) => claim,
                _ => return Err(RunError::Contract("completed selection retained its claim")),
            };
            execute_owned(
                child_endpoint,
                &operation,
                ingredient,
                db,
                claim,
                Some(original),
                crate::function::execute::participant::Consumer::capture(db.zalsa_local()),
                &LeafCallbacks(script),
            )
            .await?
            .ok_or(RunError::Contract("finite replacement did not complete"))
        })?
        .await?;
    assert!(!std::ptr::eq(original, replacement));
    assert_eq!(original.value(), replacement.value());
    assert_eq!(
        original.header.revisions.changed_at,
        replacement.header.revisions.changed_at
    );
    assert!(std::ptr::eq(replacement, memo(db, ingredient, input)));
    script
        .replacement_address
        .set(Some(std::ptr::from_ref(replacement).addr()));
    let result = complete_selected(
        &endpoint,
        &operation,
        ingredient,
        db,
        input.as_id(),
        selected,
    )
    .await?;
    assert!(std::ptr::eq(result, original_value));
    assert!(std::ptr::eq(replacement, memo(db, ingredient, input)));
    Ok(result)
}

struct Unlimited;
impl ExecutionAdmission for Unlimited {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn run<'db, P>(
    db: &'db DatabaseImpl,
    ingredient: &'db IngredientImpl<P>,
    input: Number,
    script: &Script,
    admission: &dyn ExecutionAdmission,
) -> Result<AttemptOutcome<RunResult<&'db u32>>, attempt_probe::StartError>
where
    P: Configuration<DbView = dyn Database, Input<'db> = Number, Output<'db> = u32>,
{
    try_with_attempt(db, 100_000, || {
        let db: &dyn Database = db;
        let mut registry = RegistryBuilder::new(db, admission)?;
        let leaf = registry.reserve(db, leaf::fn_ingredient_(db, db.zalsa()))?;
        let parent = registry.reserve(db, ingredient)?;
        let providers = Providers {
            leaf: leaf.clone(),
            parent: parent.clone(),
            script,
        };
        let mut registry = registry;
        let binding = registry.provider(&providers)?;
        registry.bind_executable(&leaf, &binding)?;
        registry.bind_executable(&parent, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            endpoint
                .provider(binding)?
                .fetch_ref(&parent, input.as_id())?
                .await
        })
    })
}

fn memo<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &IngredientImpl<C>,
    input: Number,
) -> &'db Memo<C> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
        .expect("completed fixture memo")
}

fn value(result: Result<AttemptOutcome<RunResult<&u32>>, attempt_probe::StartError>) -> u32 {
    match result {
        Ok(AttemptOutcome::Complete(Ok(value))) => *value,
        other => panic!("fetch did not complete: {other:?}"),
    }
}

fn assert_idle(db: &DatabaseImpl) {
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert_eq!(
        db.zalsa()
            .attempt_operations
            .load(crate::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[test]
fn selected_handle_survives_equal_value_reexecution() {
    let db = DatabaseImpl::default();
    let input = Number::new(&db, 7);
    let script = Script {
        mode: Mode::Replace,
        ..Script::default()
    };
    let ((captured, polls), events) = completion::collect(|| {
        observation::collect(|| {
            prepared_source_probe::capture(&db, || {
                run(
                    &db,
                    parent::fn_ingredient_(&db, db.zalsa()),
                    input,
                    &script,
                    &Unlimited,
                )
            })
            .unwrap()
        })
    });
    assert_eq!(value(captured.value), 8);
    assert_eq!(script.leaf_bodies.get(), 2);
    assert_eq!(polls.max_active_polls, 1);
    let key = leaf::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let parent = parent::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let reads: Vec<_> = captured
        .reads
        .iter()
        .filter(|read| read.key == key)
        .collect();
    assert_eq!(reads.len(), 1);
    assert_eq!(Some(reads[0].memo_address), script.selected_address.get());
    assert_ne!(
        script.selected_address.get(),
        script.replacement_address.get()
    );
    assert_eq!(reads[0].parent, Some(parent));
    assert_eq!(reads[0].status, Status::Final);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.key == key && event.stage == Stage::PreparedSource)
            .count(),
        1
    );
    assert_eq!(
        script
            .parent_inputs
            .borrow()
            .as_ref()
            .unwrap()
            .iter()
            .filter(|input| **input == key)
            .count(),
        1
    );
    assert_idle(&db);
}

struct RefuseCompletion<'a> {
    db: &'a DatabaseImpl,
    input: Number,
    script: &'a Script,
    fired: Cell<bool>,
}

impl ExecutionAdmission for RefuseCompletion<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.fired.get()
            || self.script.leaf_bodies.get() == 0
            || !matches!(work, ExecutionWork::Work { units: 1 })
        {
            return Ok(());
        }
        let parent =
            parent::fn_ingredient_(self.db, self.db.zalsa()).database_key_index(self.input.as_id());
        if self.db.zalsa_local().active_query().map(|(key, _)| key) != Some(parent)
            || attempt_probe::stack_depths().0 != self.script.leaf_depth.get()
        {
            return Ok(());
        }
        let ingredient = leaf::fn_ingredient_(self.db, self.db.zalsa());
        let Some(memo) = ingredient.get_memo_from_table_for(
            self.db.zalsa(),
            self.input.as_id(),
            ingredient.memo_ingredient_index(self.db.zalsa(), self.input.as_id()),
        ) else {
            return Ok(());
        };
        assert!(memo.value().is_some());
        assert!(!memo.header.may_be_provisional());
        assert!(!inputs(self.db).contains(&ingredient.database_key_index(self.input.as_id())));
        self.script
            .selected_address
            .set(Some(std::ptr::from_ref(memo).addr()));
        self.fired.set(true);
        Err(RunError::Refused(Incomplete::Allowance))
    }
}

#[test]
fn refused_completion_records_no_read_and_reuses_final_memo() {
    let db = DatabaseImpl::default();
    let input = Number::new(&db, 7);
    let script = Script::default();
    let admission = RefuseCompletion {
        db: &db,
        input,
        script: &script,
        fired: Cell::new(false),
    };
    let (captured, events) = completion::collect(|| {
        prepared_source_probe::capture(&db, || {
            run(
                &db,
                parent::fn_ingredient_(&db, db.zalsa()),
                input,
                &script,
                &admission,
            )
        })
        .unwrap()
    });
    assert!(matches!(
        captured.value,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    ));
    assert!(admission.fired.get());
    let leaf = leaf::fn_ingredient_(&db, db.zalsa());
    let key = leaf.database_key_index(input.as_id());
    assert!(!events.iter().any(|event| event.key == key));
    assert!(!captured.reads.iter().any(|read| read.key == key));
    assert!(
        !script
            .parent_inputs
            .borrow()
            .as_ref()
            .unwrap()
            .contains(&key)
    );
    let completed = memo(&db, leaf, input);
    assert_eq!(
        Some(std::ptr::from_ref(completed).addr()),
        script.selected_address.get()
    );
    assert!(!completed.header.has_incomplete_attempt());
    assert_idle(&db);
    let retry = Script::default();
    assert_eq!(
        value(run(
            &db,
            parent::fn_ingredient_(&db, db.zalsa()),
            input,
            &retry,
            &Unlimited
        )),
        8
    );
    assert_eq!(retry.leaf_bodies.get(), 0);
    assert!(std::ptr::eq(completed, memo(&db, leaf, input)));
    assert_idle(&db);
}

fn assert_epilogues(events: &[completion::Event]) {
    let mut index = 0;
    while index < events.len() {
        let event = &events[index];
        if event.stage == Stage::Converted {
            assert!(index > 0);
            let previous = &events[index - 1];
            assert_eq!(previous.stage, Stage::PreparedSource);
            assert_eq!(previous.key, event.key);
            assert_eq!(previous.caller, event.caller);
            assert_eq!(previous.depths.0, event.depths.0 + 1);
            assert_eq!(event.policy, QueryPolicy::ReturnOnly);
            index += 1;
            continue;
        }
        assert_eq!(event.stage, Stage::Eviction);
        let mut next = index + 1;
        if events
            .get(next)
            .is_some_and(|event| event.stage == Stage::Support)
        {
            assert!(events[next].has_support);
            next += 1;
        }
        assert_eq!(events[next].stage, Stage::TrackedRead);
        assert_eq!(events[next + 1].stage, Stage::PreparedSource);
        for completed in &events[index + 1..=next + 1] {
            assert_eq!(completed.key, event.key);
            assert_eq!(completed.memo_address, event.memo_address);
            assert_eq!(completed.caller, event.caller);
            assert_eq!(completed.depths, event.depths);
            if completed.caller.is_some()
                && matches!(completed.stage, Stage::TrackedRead | Stage::PreparedSource)
            {
                assert!(completed.inputs.contains(&completed.key));
            }
        }
        index = next + 2;
    }
}

#[test]
fn completion_order_and_conversion_match_ordinary_cold_hot_and_cycle_reads() {
    for controlled in [false, true] {
        let db = DatabaseImpl::default();
        let input = Number::new(&db, 7);
        for hot in [false, true] {
            let script = Script::default();
            let (captured, events) = completion::collect(|| {
                prepared_source_probe::capture(&db, || {
                    if controlled {
                        value(run(
                            &db,
                            parent::fn_ingredient_(&db, db.zalsa()),
                            input,
                            &script,
                            &Unlimited,
                        ))
                    } else {
                        parent(&db, input)
                    }
                })
                .unwrap()
            });
            assert_eq!(captured.value, 8);
            assert_epilogues(&events);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event.stage == Stage::Converted)
                    .count(),
                usize::from(!hot)
            );
            assert_eq!(captured.reads.len(), if hot { 1 } else { 2 });
            for read in &captured.reads {
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.stage == Stage::PreparedSource
                            && event.key == read.key
                            && event.memo_address == read.memo_address
                            && event.caller == read.parent)
                        .count(),
                    1
                );
            }
            assert_idle(&db);
        }
        let cycle_script = Script {
            mode: Mode::Cycle,
            ..Script::default()
        };
        let (captured, events) = completion::collect(|| {
            prepared_source_probe::capture(&db, || {
                if controlled {
                    value(run(
                        &db,
                        cyclic::fn_ingredient_(&db, db.zalsa()),
                        input,
                        &cycle_script,
                        &Unlimited,
                    ))
                } else {
                    cyclic(&db, input)
                }
            })
            .unwrap()
        });
        assert_eq!(captured.value, 2);
        assert_epilogues(&events);
        assert!(
            captured
                .reads
                .iter()
                .any(|read| read.status == Status::Provisional)
        );
        if controlled {
            assert!(events.iter().any(|event| event.stage == Stage::Support));
        }
        assert_eq!(captured.reads.last().unwrap().status, Status::Final);
        assert_idle(&db);
    }
}

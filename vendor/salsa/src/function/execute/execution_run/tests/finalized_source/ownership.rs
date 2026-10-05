use std::cell::Cell;
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::task::Poll;

use super::*;
use crate::attempt_probe::{Incomplete, QueryPolicy};
use crate::function::execute::execution_run::registration::{Route, TaskEndpoint};
use crate::function::execute::execution_run::tests::validation_trace;
use crate::function::{ClaimResult, Reentrancy};
use crate::prepared_source_probe::{self, Stamp};
use crate::zalsa_local::QueryEdgeKind;

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn outer(db: &dyn Db, input: Number) -> u32 {
    consumer(db, input)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Fault {
    Quiet,
    Refuse,
    Panic,
    Validation,
}

#[derive(Debug)]
struct Payload(Arc<()>);

#[derive(Debug)]
struct Snapshot {
    stage: &'static str,
    memo: usize,
    value: usize,
    verified: Revision,
    changed: Revision,
    caller: Option<DatabaseKeyIndex>,
    operations: Vec<usize>,
    depths: (usize, usize),
    policy: QueryPolicy,
    source_claimed: bool,
    consumer_claimed: bool,
    outer_live: bool,
    consumer_live: bool,
    storage_free: bool,
    inputs: Vec<DatabaseKeyIndex>,
    pending: Option<DatabaseKeyIndex>,
    resumes: usize,
    reason: Option<Incomplete>,
}

struct Hook {
    db: &'static TestDb,
    input: Number,
    source: DatabaseKeyIndex,
    consumer: DatabaseKeyIndex,
    outer: DatabaseKeyIndex,
    endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
    fault: Fault,
    fired: Cell<bool>,
    delivered: Cell<bool>,
    outer_live: Cell<bool>,
    consumer_live: Cell<bool>,
    storage: RefCell<()>,
    notes: RefCell<Vec<Snapshot>>,
    payload: Arc<()>,
    validation_task: RefCell<Option<Rc<Cell<bool>>>>,
    skip_admission_observation: Cell<bool>,
}

thread_local! { static HOOK: RefCell<Option<Rc<Hook>>> = const { RefCell::new(None) }; }

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

impl Hook {
    fn note(&self, stage: &'static str) {
        let memo = stored(
            self.db,
            source::fn_ingredient_(self.db, self.db.zalsa()),
            self.input.as_id(),
        );
        let inputs = self
            .db
            .zalsa_local()
            .try_with_query_stack(|stack| {
                stack.last().map_or_else(Vec::new, |query| {
                    query
                        .completion_state()
                        .0
                        .iter()
                        .filter_map(|edge| {
                            (edge.kind() == QueryEdgeKind::Input).then_some(edge.key())
                        })
                        .collect()
                })
            })
            .unwrap();
        self.notes.borrow_mut().push(Snapshot {
            stage,
            memo: std::ptr::from_ref(memo).addr(),
            value: std::ptr::from_ref(memo.value().unwrap()).addr(),
            verified: memo.header.verified_at.load(),
            changed: memo.header.revisions.changed_at,
            caller: self.db.zalsa_local().active_query().map(|(key, _)| key),
            operations: self
                .endpoint
                .borrow()
                .as_ref()
                .map_or_else(Vec::new, |endpoint| {
                    endpoint.inner.context.operations.borrow().clone()
                }),
            depths: attempt_probe::stack_depths(),
            policy: attempt_probe::current_policy(),
            source_claimed: claimed(self.db, self.source),
            consumer_claimed: claimed(self.db, self.consumer),
            outer_live: self.outer_live.get(),
            consumer_live: self.consumer_live.get(),
            storage_free: self.storage.try_borrow_mut().is_ok(),
            inputs,
            pending: validation_trace::pending_dependency(self.consumer),
            resumes: validation_trace::dependency_resumes(self.consumer),
            reason: attempt_probe::current().and_then(|support| support.reason()),
        });
    }

    fn fire(self: &Rc<Self>) {
        assert!(!self.fired.replace(true));
        self.note("fault");
        let endpoint = self.endpoint.borrow().as_ref().unwrap().clone();
        let child = Marker {
            hook: self.clone(),
            stage: "child",
        };
        let _reply = endpoint
            .demand(move || {
                poll_fn(move |_| -> Poll<RunResult<()>> {
                    let _held = &child;
                    panic!("rejected source callback polled its child");
                })
            })
            .unwrap();
        if self.fault == Fault::Panic {
            panic_any(Payload(self.payload.clone()));
        }
        attempt_probe::report_incomplete(self.db, Incomplete::Allowance);
    }
}

struct Marker {
    hook: Rc<Hook>,
    stage: &'static str,
}
impl Drop for Marker {
    fn drop(&mut self) {
        self.hook.note(self.stage);
        match self.stage {
            "outer.drop" => self.hook.outer_live.set(false),
            "consumer.drop" => self.hook.consumer_live.set(false),
            _ => {}
        }
    }
}

pub(super) fn on_cancellation() {
    let Some(hook) = HOOK.with_borrow(Clone::clone) else {
        return;
    };
    if hook.fault != Fault::Validation || hook.fired.get() {
        return;
    }
    let Some(task) = hook.validation_task.borrow().clone() else {
        return;
    };
    let Some(endpoint) = hook.endpoint.borrow().clone() else {
        return;
    };
    let current = endpoint
        .inner
        .queue
        .active_poll
        .borrow()
        .as_ref()
        .map(|poll| poll.identity.wanted.clone());
    if !current.is_some_and(|current| Rc::ptr_eq(&current, &task)) {
        return;
    }
    assert_eq!(
        validation_trace::pending_dependency(hook.consumer),
        Some(hook.source)
    );
    // The admission's own observe(Ok) check precedes selection. The following check belongs
    // to SourceReadOwner, after the exact token has been selected and retained.
    if hook.skip_admission_observation.replace(false) {
        return;
    }
    hook.fire();
}

struct HookAdmission {
    hook: Rc<Hook>,
    work: RefCell<Vec<ExecutionWork>>,
}
impl ExecutionAdmission for HookAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.work.borrow_mut().push(work);
        if self.hook.fault == Fault::Validation
            && work == (ExecutionWork::Work { units: 33 })
            && validation_trace::pending_dependency(self.hook.consumer) == Some(self.hook.source)
        {
            let endpoint = self.hook.endpoint.borrow();
            *self.hook.validation_task.borrow_mut() = Some(
                endpoint
                    .as_ref()
                    .unwrap()
                    .inner
                    .queue
                    .active_poll
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .identity
                    .wanted
                    .clone(),
            );
            self.hook.skip_admission_observation.set(true);
        }
        Ok(())
    }
}

struct Hooks<C: Configuration, D: Configuration> {
    source: FinalSourceRoute<'static, C>,
    consumer: Route<'static, D>,
    hook: Rc<Hook>,
}

impl<C, D, E> ExecutableRouteProvider<'static, 'static, E> for Hooks<C, D>
where
    C: Configuration<DbView = dyn Db, Output<'static> = u32>,
    D: Configuration<DbView = dyn Db, Input<'static> = Number, Output<'static> = u32>,
    E: Configuration<DbView = dyn Db, Input<'static> = Number, Output<'static> = u32>,
{
    // Number conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'static, 'static, E, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Db,
        input: Number,
    ) -> RunResult<u32> {
        let endpoint = context.endpoint();
        *self.hook.endpoint.borrow_mut() = Some(endpoint.clone());
        let key = db.zalsa_local().active_query().unwrap().0;
        if key == self.hook.consumer {
            assert!(!self.hook.consumer_live.replace(true));
            let _owner = Marker {
                hook: self.hook.clone(),
                stage: "consumer.drop",
            };
            db.counts().consumer.fetch_add(1, Ordering::Relaxed);
            let value = endpoint
                .read_final_source(&self.source, input.as_id())
                .await;
            self.hook.delivered.set(true);
            self.hook.note("delivered");
            Ok(*value + 1)
        } else {
            assert_eq!(key, self.hook.outer);
            assert!(!self.hook.outer_live.replace(true));
            let _owner = Marker {
                hook: self.hook.clone(),
                stage: "outer.drop",
            };
            let value = endpoint
                .child_call(|| async { context.fetch_ref(&self.consumer, input.as_id())?.await })
                .await;
            Ok(*value)
        }
    }
    async fn initial(
        &'static self,
        _: ProviderContext<'static, 'static, Self>,
        _: &'static dyn Db,
        _: Id,
        _: Number,
    ) -> RunResult<u32> {
        Err(RunError::RequiresFetch)
    }
    async fn recover<'call>(
        &'static self,
        _: ProviderContext<'static, 'static, Self>,
        _: &'static dyn Db,
        _: &'call Cycle<'call>,
        _: &'call u32,
        _: u32,
        _: Number,
    ) -> RunResult<u32>
    where
        'static: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct Clear(Rc<Hook>);
impl Drop for Clear {
    fn drop(&mut self) {
        EVICTION.with_borrow_mut(|slot| *slot = None);
        HOOK.with_borrow_mut(|slot| *slot = None);
        self.0.endpoint.borrow_mut().take();
        self.0.validation_task.borrow_mut().take();
    }
}

fn run<C>(
    db: &'static TestDb,
    source_ingredient: &'static IngredientImpl<C>,
    certificate: FinalSourceMemo<'static, C>,
    hook: Rc<Hook>,
    admission: &'static HookAdmission,
) -> RunResult<u32>
where
    C: Configuration<DbView = dyn Db, Output<'static> = u32>,
{
    let mut registry = RegistryBuilder::new(db, admission)?;
    let source =
        registry.register_final_source(db as &dyn Db, source_ingredient, &[certificate])?;
    let consumer = registry.reserve(db as &dyn Db, consumer::fn_ingredient_(db, db.zalsa()))?;
    let outer = registry.reserve(db as &dyn Db, outer::fn_ingredient_(db, db.zalsa()))?;
    let input = hook.input;
    let provider: &'static _ = Box::leak(Box::new(Hooks {
        source,
        consumer,
        hook,
    }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&provider.consumer, &binding)?;
    registry.bind_executable(&outer, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&outer, input.as_id())?
            .await?)
    })
}

fn exercise(fault: Fault) {
    let mut db = TestDb::default();
    let input = Number::new(&db, 7);
    let unrelated = Number::new(&db, 50);
    assert_eq!(source(&db, input), 7);
    let mut old_consumer_revision = None;
    if fault == Fault::Validation {
        assert_eq!(consumer(&db, input), 8);
        old_consumer_revision = Some(
            stored(
                &db,
                consumer::fn_ingredient_(&db, db.zalsa()),
                input.as_id(),
            )
            .header
            .verified_at
            .load(),
        );
        unrelated.set_value(&mut db).to(51);
        assert_eq!(source(&db, input), 7);
    }
    let db: &'static TestDb = Box::leak(Box::new(db));
    let source_ingredient = source::fn_ingredient_(db, db.zalsa());
    let consumer_ingredient = consumer::fn_ingredient_(db, db.zalsa());
    let outer_ingredient = outer::fn_ingredient_(db, db.zalsa());
    let certificate =
        FinalSourceMemo::certify(db as &dyn Db, source_ingredient, input.as_id()).unwrap();
    let hook = Rc::new(Hook {
        db,
        input,
        source: certificate.database_key(),
        consumer: consumer_ingredient.database_key_index(input.as_id()),
        outer: outer_ingredient.database_key_index(input.as_id()),
        endpoint: RefCell::new(None),
        fault,
        fired: Cell::new(false),
        delivered: Cell::new(false),
        outer_live: Cell::new(false),
        consumer_live: Cell::new(false),
        storage: RefCell::new(()),
        notes: RefCell::new(Vec::new()),
        payload: Arc::new(()),
        validation_task: RefCell::new(None),
        skip_admission_observation: Cell::new(false),
    });
    let clear = Clear(hook.clone());
    HOOK.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(hook.clone());
    });
    let policy = hook.clone();
    EVICTION.with_borrow_mut(|slot| {
        *slot = Some(Rc::new(move |id| {
            if id != policy.input.as_id()
                || policy.db.zalsa_local().active_query().map(|(key, _)| key)
                    != Some(policy.consumer)
            {
                return;
            }
            policy.note("eviction");
            if matches!(policy.fault, Fault::Refuse | Fault::Panic) && !policy.fired.get() {
                policy.fire();
            }
        }))
    });
    let admission: &'static HookAdmission = Box::leak(Box::new(HookAdmission {
        hook: hook.clone(),
        work: RefCell::new(Vec::new()),
    }));
    let stamp = Stamp::current(db);
    let mut returned = None;
    let (captured, _) = validation_trace::collect(|| {
        prepared_source_probe::capture(db, || {
            catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(db, 100_000, || {
                    let result = run(db, source_ingredient, certificate, hook.clone(), admission);
                    returned = Some(result);
                    result
                })
            }))
        })
        .unwrap()
    });
    assert_eq!(db.counts.bodies(), (1, 1));
    if fault == Fault::Quiet {
        assert_eq!(returned, Some(Ok(8)));
        assert_eq!(captured.value.unwrap(), Ok(AttemptOutcome::Complete(Ok(8))));
        assert!(hook.delivered.get() && !hook.fired.get());
        assert!(
            captured
                .reads
                .iter()
                .any(|read| read.key == hook.source && read.parent == Some(hook.consumer))
        );
    } else {
        assert!(hook.fired.get() && !hook.delivered.get());
        let notes = hook.notes.borrow();
        let start = notes.iter().find(|note| note.stage == "fault").unwrap();
        let child = notes.iter().find(|note| note.stage == "child").unwrap();
        assert_eq!(child.memo, start.memo);
        assert_eq!(child.value, start.value);
        assert_eq!(child.verified, start.verified);
        assert_eq!(child.changed, start.changed);
        assert_eq!(child.caller, start.caller);
        assert_eq!(child.operations, start.operations);
        assert_eq!(child.depths, start.depths);
        assert_eq!(child.policy, QueryPolicy::ReturnOnly);
        assert!(
            !child.source_claimed
                && child.consumer_claimed
                && child.outer_live
                && child.storage_free
        );
        assert_eq!(child.consumer_live, fault != Fault::Validation);
        assert_eq!(
            child.caller,
            Some(if fault == Fault::Validation {
                hook.outer
            } else {
                hook.consumer
            })
        );
        assert_eq!(
            child.reason,
            if fault == Fault::Panic {
                None
            } else {
                Some(Incomplete::Allowance)
            }
        );
        let position = |stage| notes.iter().position(|note| note.stage == stage).unwrap();
        assert!(position("child") < position("outer.drop"));
        assert!(!child.inputs.contains(&hook.source));
        assert!(!captured.reads.iter().any(|read| read.key == hook.source));
        if fault == Fault::Validation {
            assert_eq!(child.pending, Some(hook.source));
            assert_eq!(child.resumes, start.resumes);
            assert_eq!(
                stored(db, consumer_ingredient, input.as_id())
                    .header
                    .verified_at
                    .load(),
                old_consumer_revision.unwrap()
            );
        } else {
            assert!(position("child") < position("consumer.drop"));
            let consumer_memo = consumer_ingredient.get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                consumer_ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            );
            // Cold Panic-strategy queries have no provisional seed to poison during unwind.
            assert!(consumer_memo.is_none());
        }
        if fault == Fault::Panic {
            assert!(returned.is_none());
            let payload = captured.value.unwrap_err();
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<Payload>().unwrap().0,
                &hook.payload
            ));
        } else {
            assert_eq!(
                returned,
                Some(Err(RunError::Refused(Incomplete::Allowance)))
            );
            assert_eq!(
                captured.value.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            );
        }
    }
    assert!(!hook.outer_live.get() && !hook.consumer_live.get());
    drop(clear);
    assert_idle(db);
    assert!(stamp.belongs_to(db));
    assert!(FinalSourceMemo::certify(db as &dyn Db, source_ingredient, input.as_id()).is_ok());
    for _ in 0..2 {
        assert_eq!(outer(db, input), 8);
    }
    assert_eq!(db.counts.source.load(Ordering::Relaxed), 1);
    assert!(stamp.belongs_to(db));
    assert_idle(db);
}

#[test]
fn finalized_source_eviction_retains_the_selected_value_and_real_caller() {
    exercise(Fault::Quiet);
    exercise(Fault::Refuse);
    #[cfg(not(feature = "shuttle"))]
    exercise(Fault::Panic);
}

#[test]
fn finalized_source_validation_retains_the_pending_dependency_and_source_owner() {
    exercise(Fault::Validation);
}

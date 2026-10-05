use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::rc::Rc;
use std::sync::Arc;

use super::super::field_run::{BorrowOrCopy, FieldRequest};
use super::super::native_source::{self, NativeTrace};
use super::super::registration::{CallableRouteProvider, RegistryBuilder, TaskEndpoint};
use super::super::{RunError, RunResult};
#[cfg(feature = "accumulator")]
use crate::Accumulator;
use crate::active_query::read_storage::ReadState;
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionBudget, ExecutionLimits, Incomplete, try_with_execution_budget,
};
use crate::execution_probe::{OrdinaryReadViolation, with_explicit_reads};
use crate::function::{ClaimResult, Configuration, FunctionIngredient, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, FieldReads, Id};

thread_local! {
    static CLONES: Cell<usize> = const { Cell::new(0) };
    static HASHES: Cell<usize> = const { Cell::new(0) };
    static BODIES: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug, Eq, PartialEq, crate::SalsaValue)]
struct Counted(u32);

impl Clone for Counted {
    fn clone(&self) -> Self {
        CLONES.set(CLONES.get() + 1);
        Self(self.0)
    }
}

impl Hash for Counted {
    fn hash<H: Hasher>(&self, state: &mut H) {
        HASHES.set(HASHES.get() + 1);
        self.0.hash(state);
    }
}

#[crate::input(field_requests = read_fields)]
struct Input {
    #[returns(copy)]
    number: u32,
    #[returns(clone)]
    owned: Counted,
}

#[crate::tracked(field_requests = read_fields)]
struct Tracked<'db> {
    #[returns(clone)]
    identity: Counted,
    #[tracked]
    #[returns(copy)]
    number: u32,
    #[tracked]
    #[returns(clone)]
    owned: Counted,
}

#[crate::interned(field_view = fields, field_requests = read_fields)]
struct Interned<'db> {
    #[returns(copy)]
    number: u32,
    #[returns(clone)]
    owned: Counted,
}

#[crate::tracked(returns(copy))]
fn make_tracked(db: &dyn Database, input: Input) -> Tracked<'_> {
    Tracked::new(db, Counted(1), input.number(db), input.owned(db))
}

#[crate::tracked(returns(copy))]
fn keyed(_db: &dyn Database, key: Counted, _unit: ()) -> u32 {
    BODIES.set(BODIES.get() + 1);
    #[cfg(feature = "accumulator")]
    Note(key.0).accumulate(_db);
    key.0
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn canonical(db: &dyn Database, input: Input) -> u32 {
    BODIES.set(BODIES.get() + 1);
    input.number(db)
}

#[cfg(feature = "accumulator")]
#[crate::accumulator]
#[derive(Debug)]
struct Note(u32);

fn limits() -> ExecutionLimits {
    ExecutionLimits {
        semantic_work: 1_000_000,
        requested_bytes: 1_000_000,
    }
}

fn reset_counts() {
    CLONES.set(0);
    HASHES.set(0);
    BODIES.set(0);
}

fn assert_idle(db: &dyn Database) {
    assert!(attempt_probe::current().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(!super::super::RUN_ACTIVE.with(Cell::get));
}

fn rejects(db: &dyn Database, action: impl FnOnce()) -> OrdinaryReadViolation {
    let ingredients = db.zalsa().ingredients().count();
    let stamp = Stamp::current(db);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        try_with_execution_budget(db, limits(), |budget| {
            with_explicit_reads(db, &budget, || {
                action();
                Ok(())
            })
        })
    }))
    .expect_err("ordinary access must preserve its native violation payload");
    let violation = *panic
        .downcast_ref::<OrdinaryReadViolation>()
        .expect("ordinary access has a distinct contract-violation payload");
    assert!(!violation.operation.is_empty());
    assert_eq!(db.zalsa().ingredients().count(), ingredients);
    assert_eq!(Stamp::current(db), stamp);
    assert_idle(db);
    violation
}

#[test]
fn ordinary_query_entry_rejects_before_key_work_or_ingredient_discovery() {
    let db = DatabaseImpl::default();
    reset_counts();
    rejects(&db, || {
        keyed(&db, Counted(7), ());
    });
    assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));

    assert_eq!(keyed(&db, Counted(7), ()), 7);
    assert!(HASHES.get() > 0);
    assert_eq!(BODIES.get(), 1);
    reset_counts();
    rejects(&db, || {
        keyed(&db, Counted(7), ());
    });
    assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));
    assert_eq!(keyed(&db, Counted(7), ()), 7);
    assert_eq!(BODIES.get(), 0);
}

#[cfg(feature = "accumulator")]
#[test]
fn ordinary_accumulated_entry_rejects_before_key_work() {
    let db = DatabaseImpl::default();
    reset_counts();
    rejects(&db, || {
        keyed::accumulated::<Note>(&db, Counted(7), ());
    });
    assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));
    assert_eq!(
        keyed::accumulated::<Note>(&db, Counted(7), ())
            .iter()
            .map(|note| note.0)
            .collect::<Vec<_>>(),
        [7]
    );
    assert_eq!(BODIES.get(), 1);
}

#[test]
fn ordinary_fields_requests_and_views_reject_before_conversion() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7, Counted(11));
    let tracked = make_tracked(&db, input);
    let interned = Interned::new(&db, 13, Counted(17));
    let fields = FieldReads::new(&db);
    let actions: [&dyn Fn(); 8] = [
        &|| {
            input.owned(&db);
        },
        &|| {
            tracked.identity(&db);
        },
        &|| {
            tracked.owned(&db);
        },
        &|| {
            interned.owned(&db);
        },
        &|| {
            input.read_fields(&db).owned().read_ordinary();
        },
        &|| {
            tracked.read_fields(&db).owned().read_ordinary();
        },
        &|| {
            interned.read_fields(&db).owned().read_ordinary();
        },
        &|| {
            interned.fields(fields).owned();
        },
    ];
    for action in actions {
        reset_counts();
        rejects(&db, action);
        assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));
    }
    assert_eq!(input.owned(&db), Counted(11));
    assert_eq!(tracked.identity(&db), Counted(1));
    assert_eq!(tracked.owned(&db), Counted(11));
    assert_eq!(interned.owned(&db), Counted(17));
    assert_eq!(input.read_fields(&db).owned().read_ordinary(), Counted(11));
    assert_eq!(interned.fields(fields).owned(), &Counted(17));
}

#[test]
fn ordinary_constructors_and_direct_interning_reject_before_storage_changes() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7, Counted(11));
    let tracked = make_tracked(&db, input);
    let interned = Interned::new(&db, 13, Counted(17));
    let input_ingredient = Input::ingredient(&db);
    let tracked_ingredient = Tracked::ingredient(&db);
    let interned_ingredient = Interned::ingredient(db.zalsa());
    let before = (
        input_ingredient.entries(db.zalsa()).count(),
        tracked_ingredient.entries(db.zalsa()).count(),
        interned_ingredient.entries(db.zalsa()).count(),
    );
    let actions: [&dyn Fn(); 4] = [
        &|| {
            Input::new(&db, 19, Counted(23));
        },
        &|| {
            Tracked::new(&db, Counted(29), 31, Counted(37));
        },
        &|| {
            Interned::new(&db, 41, Counted(43));
        },
        &|| {
            interned_ingredient.intern_id(
                db.zalsa(),
                db.zalsa_local(),
                (47, Counted(53)),
                |_, fields| fields,
            );
        },
    ];
    for action in actions {
        reset_counts();
        rejects(&db, action);
        assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));
        assert_eq!(
            (
                input_ingredient.entries(db.zalsa()).count(),
                tracked_ingredient.entries(db.zalsa()).count(),
                interned_ingredient.entries(db.zalsa()).count(),
            ),
            before,
        );
    }
    assert_eq!(input.number(&db), 7);
    assert_eq!(tracked.number(&db), 7);
    assert_eq!(interned.number(&db), 13);
    assert!(Interned::new(&db, 13, Counted(17)) == interned);
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    SwallowField,
    SwallowUntracked,
}

struct DropCounter(Rc<Cell<usize>>);

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

struct ReadProvider {
    fault: Fault,
    caught: Rc<Cell<Option<OrdinaryReadViolation>>>,
    drops: Rc<Cell<usize>>,
}

fn read_state(db: &dyn Database) -> Option<ReadState> {
    db.zalsa_local()
        .try_with_query_stack(|stack| stack.last().map(|query| query.read_state()))
        .flatten()
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for ReadProvider
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
{
    // Input reconstruction copies one handle, and output equality compares one u32.
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
        let _owner = DropCounter(self.drops.clone());
        if !matches!(self.fault, Fault::None) {
            let before = read_state(db);
            assert!(before.is_some());
            let panic = catch_unwind(AssertUnwindSafe(|| match self.fault {
                Fault::SwallowField => {
                    input.owned(db);
                }
                Fault::SwallowUntracked => db.report_untracked_read(),
                Fault::None => {}
            }))
            .expect_err("the provider deliberately catches an ordinary-read violation");
            self.caught.set(Some(
                *panic.downcast_ref::<OrdinaryReadViolation>().unwrap(),
            ));
            assert_eq!(read_state(db), before);
            // A provider can suppress the unwind, but its value must never become a successful memo.
            return Ok(7);
        }
        let fields = input.read_fields(endpoint.field_request_context());
        Ok(endpoint.read_field(fields.number(), &BorrowOrCopy).await)
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
        Err(RunError::RequiresFetch)
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
        Err(RunError::RequiresFetch)
    }
}

fn run_canonical(
    db: &dyn Database,
    input: Input,
    budget: &ExecutionBudget<'_>,
    provider: ReadProvider,
    ingredient: &crate::function::IngredientImpl<
        impl for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
    >,
) -> RunResult<u32> {
    with_explicit_reads(db, budget, || {
        let mut registry = RegistryBuilder::with_budget(db, budget)?;
        let route = registry.reserve_callable(db, ingredient)?;
        registry.bind_callable(&route, provider)?;
        registry.seal()?.run(move |endpoint| async move {
            let value = endpoint
                .child_call(|| async { endpoint.fetch_ref(&route, input.as_id())?.await })
                .await;
            Ok(*value)
        })
    })
}

#[test]
fn caught_violations_cannot_publish_and_admitted_retry_keeps_canonical_dependencies() {
    for fault in [Fault::SwallowField, Fault::SwallowUntracked] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7, Counted(11));
        let ingredient = canonical::fn_ingredient_(&db, db.zalsa());
        let caught = Rc::new(Cell::new(None));
        let drops = Rc::new(Cell::new(0));
        let stamp = Stamp::current(&db);
        reset_counts();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            try_with_execution_budget(&db, limits(), |budget| {
                run_canonical(
                    &db,
                    input,
                    &budget,
                    ReadProvider {
                        fault,
                        caught: caught.clone(),
                        drops: drops.clone(),
                    },
                    ingredient,
                )
            })
        }))
        .expect_err("acceptance must rethrow a provider's first ordinary-read violation");
        assert_eq!(
            panic.downcast_ref::<OrdinaryReadViolation>().copied(),
            caught.get()
        );
        assert!(caught.get().is_some());
        assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));
        assert_eq!(drops.get(), 1);
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
        assert_idle(&db);

        assert_eq!(
            try_with_execution_budget(&db, limits(), |budget| {
                run_canonical(
                    &db,
                    input,
                    &budget,
                    ReadProvider {
                        fault: Fault::None,
                        caught: caught.clone(),
                        drops: drops.clone(),
                    },
                    ingredient,
                )
            }),
            Ok(AttemptOutcome::Complete(Ok(7))),
        );
        assert_eq!(drops.get(), 2);
        let memo = ingredient.memo(db.zalsa(), input.as_id()).unwrap();
        let owner = Input::ingredient(&db).database_key_index(input.as_id());
        let number = DatabaseKeyIndex::new(owner.ingredient_index().successor(0), input.as_id());
        assert_eq!(
            memo.header().origin().inputs().collect::<Vec<_>>(),
            [number]
        );
        assert_eq!(canonical(&db, input), 7);
        assert_eq!(BODIES.get(), 0);
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db);
    }
}

#[test]
fn admitted_fields_select_input_tracked_and_interned_values() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7, Counted(11));
    let tracked = make_tracked(&db, input);
    let interned = Interned::new(&db, 13, Counted(17));
    assert_eq!(
        try_with_execution_budget(&db, limits(), |budget| {
            with_explicit_reads(&db, &budget, || {
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        let context = endpoint.field_request_context();
                        let a = endpoint
                            .read_field(input.read_fields(context).number(), &BorrowOrCopy)
                            .await;
                        let b = endpoint
                            .read_field(tracked.read_fields(context).number(), &BorrowOrCopy)
                            .await;
                        let c = endpoint
                            .read_field(interned.read_fields(context).number(), &BorrowOrCopy)
                            .await;
                        Ok((a, b, c))
                    })
            })
        }),
        Ok(AttemptOutcome::Complete(Ok((7, 7, 13))))
    );
    assert_idle(&db);
}

#[test]
fn native_source_reads_and_validation_refuse_before_native_entry() {
    let _trace = native_source::trace();
    for validate in [false, true] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7, Counted(11));
        let ingredient = canonical::fn_ingredient_(&db, db.zalsa());
        let key = ingredient.database_key_index(input.as_id());
        let stamp = Stamp::current(&db);
        let revision = db.zalsa().current_revision();
        let delivered = Cell::new(false);
        reset_counts();
        native_source::take_trace();
        assert_eq!(
            try_with_execution_budget(&db, limits(), |budget| {
                let result = with_explicit_reads(&db, &budget, || {
                    let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                    let route =
                        registry.register_native_source(&db as &dyn Database, ingredient)?;
                    let delivered = &delivered;
                    registry.seal()?.run(|endpoint| async move {
                        if validate {
                            endpoint.validate(key, revision)?.await?;
                        } else {
                            endpoint.read_native_source(&route, input.as_id()).await;
                        }
                        delivered.set(true);
                        Ok(())
                    })
                });
                assert_eq!(result, Err(RunError::RequiresFetch));
                result
            }),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert!(!delivered.get());
        assert_eq!((CLONES.get(), HASHES.get(), BODIES.get()), (0, 0, 0));
        assert!(ingredient.memo(db.zalsa(), input.as_id()).is_none());
        assert!(!native_source::take_trace().iter().any(|event| matches!(
            event,
            NativeTrace::SourceEnter { .. }
                | NativeTrace::Read { .. }
                | NativeTrace::Validation { .. }
        )));
        assert!(native_source::source_permit_is_clear());
        assert_idle(&db);

        assert_eq!(
            try_with_execution_budget(&db, limits(), |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let route = registry.register_native_source(&db as &dyn Database, ingredient)?;
                registry.seal()?.run(|endpoint| async move {
                    Ok(*endpoint.read_native_source(&route, input.as_id()).await)
                })
            }),
            Ok(AttemptOutcome::Complete(Ok(7)))
        );
        assert_eq!(BODIES.get(), 1);
        assert!(
            native_source::take_trace()
                .iter()
                .any(|event| matches!(event, NativeTrace::Read { key: read } if *read == key))
        );
        assert_eq!(canonical(&db, input), 7);
        assert_eq!(BODIES.get(), 1);
        assert_eq!(Stamp::current(&db), stamp);
        assert!(native_source::source_permit_is_clear());
        assert_idle(&db);
    }
}

#[test]
fn registered_runs_cannot_escape_their_lexical_boundary() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7, Counted(11));
    let entered = Cell::new(false);
    let escaped = RunError::Contract("execution run escaped its explicit read scope");
    assert_eq!(
        try_with_execution_budget(&db, limits(), |budget| {
            let (outside, later) = with_explicit_reads(&db, &budget, || {
                Ok((
                    RegistryBuilder::with_budget(&db, &budget)?.seal()?,
                    RegistryBuilder::with_budget(&db, &budget)?.seal()?,
                ))
            })?;
            assert_eq!(
                outside.run(|_| {
                    entered.set(true);
                    std::future::ready(Ok(()))
                }),
                Err(escaped)
            );
            with_explicit_reads(&db, &budget, || {
                assert_eq!(
                    later.run(|_| {
                        entered.set(true);
                        std::future::ready(Ok(()))
                    }),
                    Err(escaped)
                );
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        let request = input.read_fields(endpoint.field_request_context()).number();
                        Ok(endpoint.read_field(request, &BorrowOrCopy).await)
                    })
            })
        }),
        Ok(AttemptOutcome::Complete(Ok(7)))
    );
    assert!(!entered.get());
    assert_eq!(input.number(&db), 7);
    assert_idle(&db);
}

#[derive(Debug)]
struct NativePanic(Arc<()>);

#[test]
fn explicit_boundary_preserves_unrelated_native_payloads_and_releases_owners() {
    for catch_violation in [false, true] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7, Counted(11));
        let stamp = Stamp::current(&db);
        let identity = Arc::new(());
        let drops = Rc::new(Cell::new(0));
        let identity_ref = &identity;
        let drops_ref = &drops;
        let db_ref = &db;
        let panic = catch_unwind(AssertUnwindSafe(|| {
            try_with_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    RegistryBuilder::with_budget(&db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            let _owner = DropCounter(drops_ref.clone());
                            endpoint
                                .local_call(|| -> RunResult<()> {
                                    if catch_violation {
                                        let violation = catch_unwind(AssertUnwindSafe(|| {
                                            input.owned(db_ref);
                                        }))
                                        .expect_err("ordinary access must be rejected");
                                        assert!(violation.is::<OrdinaryReadViolation>());
                                    }
                                    panic_any(NativePanic(identity_ref.clone()))
                                })
                                .await;
                            Ok(())
                        })
                })
            })
        }))
        .expect_err("the callback's native panic must escape unchanged");
        let payload = panic.downcast_ref::<NativePanic>().unwrap();
        assert!(Arc::ptr_eq(&payload.0, &identity));
        assert_eq!(drops.get(), 1);
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db);
        assert_eq!(input.number(&db), 7);
        assert_eq!(
            try_with_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    RegistryBuilder::with_budget(&db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            let request =
                                input.read_fields(endpoint.field_request_context()).number();
                            Ok(endpoint.read_field(request, &BorrowOrCopy).await)
                        })
                })
            }),
            Ok(AttemptOutcome::Complete(Ok(7)))
        );
        assert_idle(&db);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn native_cancellation_clears_the_boundary_before_same_revision_retry() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7, Counted(11));
    let stamp = Stamp::current(&db);
    let drops = Rc::new(Cell::new(0));
    let db_ref = &db;
    let drops_ref = &drops;
    let panic = catch_unwind(AssertUnwindSafe(|| {
        try_with_execution_budget(&db, limits(), |budget| {
            with_explicit_reads(&db, &budget, || {
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        let _owner = DropCounter(drops_ref.clone());
                        endpoint
                            .local_call(|| {
                                db_ref.cancellation_token().cancel();
                                db_ref.unwind_if_revision_cancelled();
                                Ok(())
                            })
                            .await;
                        Ok(())
                    })
            })
        })
    }))
    .expect_err("local cancellation must retain its native payload");
    assert!(matches!(
        panic.downcast_ref::<crate::Cancelled>(),
        Some(crate::Cancelled::Local)
    ));
    db.zalsa_local().uncancel();
    assert_eq!(drops.get(), 1);
    assert_eq!(Stamp::current(&db), stamp);
    assert_idle(&db);
    assert_eq!(input.number(&db), 7);

    assert_eq!(
        try_with_execution_budget(&db, limits(), |budget| {
            with_explicit_reads(&db, &budget, || {
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        let request = input.read_fields(endpoint.field_request_context()).number();
                        Ok(endpoint.read_field(request, &BorrowOrCopy).await)
                    })
            })
        }),
        Ok(AttemptOutcome::Complete(Ok(7))),
    );
    assert_idle(&db);
}

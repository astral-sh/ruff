use std::cell::Cell;
#[cfg(not(feature = "shuttle"))]
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::super::registration::{ExecutableRouteProvider, ProviderContext, RegistryBuilder};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::{observation, validation_trace};
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionLimits, Incomplete, MemoReuse, try_with_attempt,
    try_with_metered_execution_budget,
};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Memo, Reentrancy, VerifyResult};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, Durability, Id, Revision, Setter};

#[crate::db]
trait Db: Database {
    fn executions(&self) -> &AtomicUsize;
}

#[crate::db]
#[derive(Clone, Default)]
struct TestDb {
    storage: crate::Storage<Self>,
    executions: Arc<AtomicUsize>,
}

#[crate::db]
impl Database for TestDb {}

#[crate::db]
impl Db for TestDb {
    fn executions(&self) -> &AtomicUsize {
        &self.executions
    }
}

#[crate::input]
struct Number {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked]
struct Product<'db> {
    value: u32,
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn structural(db: &dyn Db, input: Number) -> u32 {
    assert!(attempt_probe::current().is_none());
    db.executions().fetch_add(1, Ordering::Relaxed);
    let value = input.value(db) % 2;
    Product::new(db, value);
    #[cfg(not(feature = "shuttle"))]
    validation_lifecycle::during_structural(db, input);
    value
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn semantic(db: &dyn Db, input: Number) -> u32 {
    input.value(db)
}

#[crate::tracked(returns(copy), attempt = CompleteOnly, lru = 1)]
fn evictable(db: &dyn Db, input: Number) -> u32 {
    db.executions().fetch_add(1, Ordering::Relaxed);
    input.value(db)
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn optional_product(db: &dyn Db, input: Number) -> Option<Product<'_>> {
    (input.value(db) != 0).then(|| Product::new(db, input.value(db)))
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn product_value<'db>(db: &'db dyn Db, product: Product<'db>) -> u32 {
    *product.value(db)
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn optional_product_value(db: &dyn Db, input: Number) -> u32 {
    optional_product(db, input).map_or(0, |product| product_value(db, product))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn ancestor(db: &dyn Db, input: Number) -> u32 {
    structural(db, input)
}

struct UnchangedAncestor;

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for UnchangedAncestor
where
    C: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    // Number conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _input: Number,
    ) -> RunResult<u32> {
        panic!("unchanged structural dependencies do not reexecute their ancestor")
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _id: Id,
        _input: Number,
    ) -> RunResult<u32> {
        panic!("the ancestor is acyclic")
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Number,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        panic!("the ancestor is acyclic")
    }
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, _: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn validate(
    db: &dyn Database,
    key: DatabaseKeyIndex,
    revision: Revision,
) -> RunResult<VerifyResult> {
    let mut registry = RegistryBuilder::new(db, &Admission)?;
    registry.enable_structural_dependency_validation()?;
    registry
        .seal()?
        .run(move |endpoint| async move { endpoint.validate(key, revision)?.await })
}

fn assert_validation(
    db: &dyn Database,
    key: DatabaseKeyIndex,
    revision: Revision,
    unchanged: bool,
) {
    assert_eq!(
        try_with_attempt(db, 10_000, || {
            validate(db, key, revision).map(|result| result.is_unchanged())
        }),
        Ok(AttemptOutcome::Complete(Ok(unchanged)))
    );
    assert_idle(db);
}

fn assert_idle(db: &dyn Database) {
    assert!(attempt_probe::current().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
}

fn memo<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
) -> &'db Memo<C> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .expect("the fixture evaluated this query")
}

#[test]
fn current_output_memo_validates_without_structural_execution() {
    let mut db = TestDb::default();
    let input = Number::new(&db, 4);
    assert_eq!(structural(&db, input), 0);
    let first_revision = db.zalsa().current_revision();
    input.set_value(&mut db).to(5);
    assert_eq!(structural(&db, input), 1);
    let ingredient = structural::fn_ingredient_(&db, db.zalsa());
    let key = ingredient.database_key_index(input.as_id());
    let original = memo(&db, ingredient, input.as_id());
    assert!(!original.header.outputs_are_empty());
    assert_eq!(
        original.header.attempt_reuse(db.zalsa()),
        MemoReuse::Ordinary
    );
    let (_, observed) = observation::collect(|| {
        assert_validation(&db, key, first_revision, false);
        assert_validation(&db, key, db.zalsa().current_revision(), true);
    });
    assert!(observed.events.is_empty());
    assert_eq!(db.executions.load(Ordering::Relaxed), 2);
    assert!(std::ptr::eq(original, memo(&db, ingredient, input.as_id())));
}

#[test]
fn stale_output_memos_validate_in_one_attempt() {
    for changed in [false, true] {
        let mut db = TestDb::default();
        let input = Number::new(&db, 4);
        assert_eq!(structural(&db, input), 0);
        let revision = db.zalsa().current_revision();
        input.set_value(&mut db).to(if changed { 5 } else { 6 });
        let ingredient = structural::fn_ingredient_(&db, db.zalsa());
        let key = ingredient.database_key_index(input.as_id());
        assert_eq!(db.executions.load(Ordering::Relaxed), 1);
        assert_validation(&db, key, revision, !changed);
        assert_eq!(db.executions.load(Ordering::Relaxed), 2);
        let prepared_memo = memo(&db, ingredient, input.as_id());
        assert!(!prepared_memo.header.outputs_are_empty());
        assert_eq!(
            prepared_memo.header.verified_at.load(),
            db.zalsa().current_revision()
        );
        assert_eq!(
            prepared_memo.header.attempt_reuse(db.zalsa()),
            MemoReuse::Ordinary
        );
        assert_validation(&db, key, revision, !changed);
        assert_validation(&db, key, revision, !changed);
        assert_eq!(db.executions.load(Ordering::Relaxed), 2);
        #[cfg(not(feature = "shuttle"))]
        assert!(
            ingredient
                .sync_table
                .test_transfer_state(input.as_id())
                .is_none()
        );
    }
}

#[test]
fn missing_memo_is_changed_without_structural_execution() {
    let db = TestDb::default();
    let input = Number::new(&db, 4);
    let ingredient = structural::fn_ingredient_(&db, db.zalsa());
    let key = ingredient.database_key_index(input.as_id());
    let revision = db.zalsa().current_revision();
    assert_validation(&db, key, revision, false);
    assert_eq!(db.executions.load(Ordering::Relaxed), 0);
    assert!(
        ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .is_none()
    );
}

#[test]
fn evicted_values_retain_their_canonical_validation_proof() {
    for changed in [false, true] {
        let mut db = TestDb::default();
        let input = Number::new(&db, 4);
        let retained = Number::new(&db, 7);
        assert_eq!(evictable(&db, input), 4);
        assert_eq!(evictable(&db, retained), 7);
        let revision = db.zalsa().current_revision();
        if changed {
            input.set_value(&mut db).to(5);
        } else {
            db.synthetic_write(Durability::LOW);
        }
        let ingredient = evictable::fn_ingredient_(&db, db.zalsa());
        let key = ingredient.database_key_index(input.as_id());
        let evicted = memo(&db, ingredient, input.as_id());
        assert!(evicted.value().is_none());
        assert!(memo(&db, ingredient, retained.as_id()).value().is_some());

        // LRU removes the value but preserves the dependency edges needed for validation.
        assert_validation(&db, key, revision, !changed);
        assert_eq!(db.executions.load(Ordering::Relaxed), 2);
        assert!(std::ptr::eq(evicted, memo(&db, ingredient, input.as_id())));
        assert!(evicted.value().is_none());
    }
}

#[test]
fn structural_validation_discards_memos_of_deleted_outputs() {
    let discarded = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&discarded);
    let mut db = TestDb {
        storage: crate::Storage::builder()
            .event_callback(Box::new(move |event| {
                if let crate::EventKind::DidDiscard { key } = event.kind {
                    observed.lock().unwrap().push(key);
                }
            }))
            .build(),
        ..TestDb::default()
    };
    let input = Number::new(&db, 4);
    assert_eq!(optional_product_value(&db, input), 4);
    let discarded_key = {
        let product = optional_product(&db, input).unwrap();
        let ingredient = product_value::fn_ingredient_(&db, db.zalsa());
        assert!(memo(&db, ingredient, product.as_id()).value().is_some());
        ingredient.database_key_index(product.as_id())
    };
    let revision = db.zalsa().current_revision();
    input.set_value(&mut db).to(0);
    let ingredient = optional_product_value::fn_ingredient_(&db, db.zalsa());
    let key = ingredient.database_key_index(input.as_id());
    assert!(discarded.lock().unwrap().is_empty());

    // Validation reaches the producer before its former output. Reexecuting the producer
    // deletes that output and its memo; the changed dependency then invalidates the consumer.
    assert_validation(&db, key, revision, false);
    assert!(discarded.lock().unwrap().contains(&discarded_key));
    assert!(optional_product(&db, input).is_none());
    assert_eq!(optional_product_value(&db, input), 0);
    assert_validation(&db, key, revision, false);
    assert_validation(&db, key, revision, false);
}

fn validate_ancestor<'run, 'db: 'run>(
    db: &'db TestDb,
    input: Number,
    revision: Revision,
    observed: &'run RootObservations,
    mut registry: RegistryBuilder<'run, 'db>,
) -> RunResult<VerifyResult> {
    let route = registry.reserve(db as &dyn Db, ancestor::fn_ingredient_(db, db.zalsa()))?;
    let binding = registry.provider(&UnchangedAncestor)?;
    registry.bind_executable(&route, &binding)?;
    registry.enable_structural_dependency_validation()?;
    registry.seal()?.run(move |endpoint| async move {
        observed.entries.set(observed.entries.get() + 1);
        let _root = ValidationRoot {
            db,
            input,
            support: attempt_probe::current().unwrap(),
            observed,
        };
        endpoint
            .provider(binding)?
            .validate(&route, input.as_id(), revision)?
            .await
    })
}

#[derive(Default)]
struct RootObservations {
    entries: Cell<usize>,
    drops: Cell<usize>,
}

struct ValidationRoot<'a> {
    db: &'a TestDb,
    input: Number,
    support: crate::attempt_probe::AttemptSupport,
    observed: &'a RootObservations,
}

impl Drop for ValidationRoot<'_> {
    fn drop(&mut self) {
        assert!(self.support.same_owner(&attempt_probe::current().unwrap()));
        assert!(self.db.zalsa_local().active_query().is_none());
        assert!(matches!(
            ancestor::fn_ingredient_(self.db, self.db.zalsa())
                .sync_table
                .peek_claim(self.db.zalsa(), self.input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
        self.observed.drops.set(self.observed.drops.get() + 1);
    }
}

fn ancestor_fixture() -> (TestDb, Number, Revision) {
    let mut db = TestDb::default();
    let input = Number::new(&db, 4);
    assert_eq!(ancestor(&db, input), 0);
    let revision = db.zalsa().current_revision();
    input.set_value(&mut db).to(6);
    (db, input, revision)
}

fn assert_ancestor_idle(db: &TestDb, input: Number) {
    assert_idle(db);
    let ingredient = ancestor::fn_ingredient_(db, db.zalsa());
    match ingredient.sync_table.try_claim(
        db.zalsa(),
        db.zalsa_local(),
        input.as_id(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Claimed(claim) => claim.abort(),
        _ => panic!("the interrupted ancestor retained its claim"),
    }
}

#[test]
fn semantic_ancestor_retains_its_claim_and_backdates_without_root_reentry() {
    let (db, input, revision) = ancestor_fixture();
    let ingredient = ancestor::fn_ingredient_(&db, db.zalsa());
    let key = ingredient.database_key_index(input.as_id());
    let original = memo(&db, ingredient, input.as_id());
    let root = RootObservations::default();
    let (outcome, observed) = observation::collect(|| {
        try_with_attempt(&db, 100_000, || {
            validate_ancestor(
                &db,
                input,
                revision,
                &root,
                RegistryBuilder::new(&db, &Admission)?,
            )
            .map(|result| result.is_unchanged())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(true))));
    assert_eq!((root.entries.get(), root.drops.get()), (1, 1));
    assert_eq!(
        observed.events.iter().filter(|event| matches!(event, observation::Event::Claim { key: claimed, .. } if *claimed == key)).count(),
        1
    );
    assert!(std::ptr::eq(original, memo(&db, ingredient, input.as_id())));
    assert_eq!(
        original.header.verified_at.load(),
        db.zalsa().current_revision()
    );
    assert_eq!(original.header.revisions.changed_at, revision);
    assert_eq!(db.executions.load(Ordering::Relaxed), 2);
    assert_ancestor_idle(&db, input);
    assert_eq!(ancestor(&db, input), 0);
    assert_eq!(db.executions.load(Ordering::Relaxed), 2);
}

struct RefuseDependency {
    parent: DatabaseKeyIndex,
    dependency: DatabaseKeyIndex,
    reason: Incomplete,
    fired: Cell<bool>,
}

impl ExecutionAdmission for RefuseDependency {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let selected = match self.reason {
            Incomplete::Allowance => matches!(work, ExecutionWork::Work { .. }),
            Incomplete::RequestedAllocation => matches!(work, ExecutionWork::Resource { .. }),
            _ => false,
        };
        if selected
            && validation_trace::pending_dependency(self.parent) == Some(self.dependency)
            && !self.fired.replace(true)
        {
            return Err(RunError::Refused(self.reason));
        }
        Ok(())
    }
}

#[test]
fn refused_structural_validation_releases_its_parent_before_same_revision_retry() {
    for reason in [Incomplete::Allowance, Incomplete::RequestedAllocation] {
        let (db, input, revision) = ancestor_fixture();
        let current_revision = db.zalsa().current_revision();
        let ingredient = ancestor::fn_ingredient_(&db, db.zalsa());
        let original = memo(&db, ingredient, input.as_id());
        let admission = RefuseDependency {
            parent: ingredient.database_key_index(input.as_id()),
            dependency: structural::fn_ingredient_(&db, db.zalsa())
                .database_key_index(input.as_id()),
            reason,
            fired: Cell::new(false),
        };
        let root = RootObservations::default();
        let (outcome, _) = validation_trace::collect(|| {
            try_with_attempt(&db, 100_000, || {
                validate_ancestor(
                    &db,
                    input,
                    revision,
                    &root,
                    RegistryBuilder::new(&db, &admission)?,
                )
            })
        });
        assert!(matches!(outcome, Ok(AttemptOutcome::Incomplete(actual)) if actual == reason));
        assert!(admission.fired.get());
        assert_eq!((root.entries.get(), root.drops.get()), (1, 1));
        assert_eq!(original.header.verified_at.load(), revision);
        assert_eq!(db.executions.load(Ordering::Relaxed), 1);
        assert_ancestor_idle(&db, input);

        let retry = try_with_attempt(&db, 100_000, || {
            validate_ancestor(
                &db,
                input,
                revision,
                &root,
                RegistryBuilder::new(&db, &Admission)?,
            )
            .map(|result| result.is_unchanged())
        });
        assert_eq!(retry, Ok(AttemptOutcome::Complete(Ok(true))));
        assert_eq!((root.entries.get(), root.drops.get()), (2, 2));
        assert_eq!(db.zalsa().current_revision(), current_revision);
        assert_eq!(db.executions.load(Ordering::Relaxed), 2);
        assert!(std::ptr::eq(original, memo(&db, ingredient, input.as_id())));
        assert_ancestor_idle(&db, input);
    }
}

struct RefuseCompletedStructural<'db> {
    db: &'db TestDb,
    input: Number,
    fired: Cell<bool>,
}

impl ExecutionAdmission for RefuseCompletedStructural<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if work != ExecutionWork::Poll || self.fired.get() {
            return Ok(());
        }
        let parent = ancestor::fn_ingredient_(self.db, self.db.zalsa());
        let dependency = structural::fn_ingredient_(self.db, self.db.zalsa());
        let id = self.input.as_id();
        let current = self.db.zalsa().current_revision();
        if validation_trace::pending_dependency(parent.database_key_index(id))
            != Some(dependency.database_key_index(id))
            || memo(self.db, dependency, id).header.verified_at.load() != current
            || memo(self.db, parent, id).header.verified_at.load() == current
        {
            return Ok(());
        }
        assert!(matches!(
            parent
                .sync_table
                .peek_claim(self.db.zalsa(), id, Reentrancy::Deny),
            ClaimResult::Cycle { .. }
        ));
        assert!(matches!(
            dependency
                .sync_table
                .peek_claim(self.db.zalsa(), id, Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
        self.fired.set(true);
        Err(RunError::Refused(Incomplete::Allowance))
    }
}

#[test]
fn completed_structural_dependency_survives_refusal_before_parent_verification() {
    let (db, input, revision) = ancestor_fixture();
    let current_revision = db.zalsa().current_revision();
    let parent = ancestor::fn_ingredient_(&db, db.zalsa());
    let dependency = structural::fn_ingredient_(&db, db.zalsa());
    let original = memo(&db, parent, input.as_id());
    let admission = RefuseCompletedStructural {
        db: &db,
        input,
        fired: Cell::new(false),
    };
    let root = RootObservations::default();
    let (outcome, _) = validation_trace::collect(|| {
        try_with_attempt(&db, 100_000, || {
            validate_ancestor(
                &db,
                input,
                revision,
                &root,
                RegistryBuilder::new(&db, &admission)?,
            )
        })
    });
    assert!(matches!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    ));
    assert!(admission.fired.get());
    assert_eq!((root.entries.get(), root.drops.get()), (1, 1));
    assert_eq!(original.header.verified_at.load(), revision);
    assert_eq!(db.executions.load(Ordering::Relaxed), 2);
    assert_ancestor_idle(&db, input);
    assert!(matches!(
        dependency
            .sync_table
            .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
        ClaimResult::Claimed(())
    ));
    let completed = memo(&db, dependency, input.as_id());
    assert_eq!(completed.header.verified_at.load(), current_revision);
    assert_eq!(
        completed.header.attempt_reuse(db.zalsa()),
        MemoReuse::Ordinary
    );
    assert!(!completed.header.may_be_provisional());
    assert!(!completed.header.outputs_are_empty());

    // The semantic parent is still stale, but its completed structural dependency is reusable.
    let retry = try_with_attempt(&db, 100_000, || {
        validate_ancestor(
            &db,
            input,
            revision,
            &root,
            RegistryBuilder::new(&db, &Admission)?,
        )
        .map(|result| result.is_unchanged())
    });
    assert_eq!(retry, Ok(AttemptOutcome::Complete(Ok(true))));
    assert_eq!((root.entries.get(), root.drops.get()), (2, 2));
    assert_eq!(db.zalsa().current_revision(), current_revision);
    assert_eq!(db.executions.load(Ordering::Relaxed), 2);
    assert!(std::ptr::eq(
        completed,
        memo(&db, dependency, input.as_id())
    ));
    assert!(std::ptr::eq(original, memo(&db, parent, input.as_id())));
    assert_eq!(original.header.verified_at.load(), current_revision);
    assert_eq!(ancestor(&db, input), 0);
    assert_eq!(db.executions.load(Ordering::Relaxed), 2);
    assert_ancestor_idle(&db, input);
}

#[test]
fn structural_validation_preserves_missing_semantic_route_errors() {
    let db = TestDb::default();
    let input = Number::new(&db, 4);
    assert_eq!(semantic(&db, input), 4);
    let key = semantic::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    assert!(matches!(
        try_with_attempt(&db, 10_000, || {
            let result = validate(&db, key, db.zalsa().current_revision());
            assert!(matches!(
                result,
                Err(RunError::Contract("validation route is not registered"))
            ));
            result
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    ));
    assert_idle(&db);
}

#[cfg(not(feature = "shuttle"))]
mod validation_lifecycle {
    use std::panic::{catch_unwind, panic_any};

    use super::*;

    #[derive(Clone, Copy)]
    enum Stop {
        Panic,
        LocalCancellation,
        PendingWriteCancellation,
    }

    #[derive(Debug)]
    struct ValidationPanic;

    thread_local! {
        static STOP: Cell<Option<Stop>> = const { Cell::new(None) };
        static ENTERED: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn during_structural(db: &dyn Db, input: Number) {
        let Some(stop) = STOP.take() else {
            return;
        };
        ENTERED.set(true);
        assert!(attempt_probe::current().is_none());
        assert_eq!(
            attempt_probe::current_policy(),
            crate::attempt_probe::QueryPolicy::CompleteOnly,
        );
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(structural::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id())),
        );
        assert!(matches!(
            ancestor::fn_ingredient_(db, db.zalsa())
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Cycle { .. }
        ));
        match stop {
            Stop::Panic => panic_any(ValidationPanic),
            Stop::LocalCancellation => db.cancellation_token().cancel(),
            Stop::PendingWriteCancellation => db.zalsa().runtime().set_cancellation_flag(),
        }
        db.unwind_if_revision_cancelled();
    }

    #[test]
    fn native_validation_failures_release_children_before_the_root_and_retry() {
        for stop in [
            Stop::Panic,
            Stop::LocalCancellation,
            Stop::PendingWriteCancellation,
        ] {
            let (db, input, revision) = ancestor_fixture();
            let current_revision = db.zalsa().current_revision();
            let ingredient = ancestor::fn_ingredient_(&db, db.zalsa());
            let original = memo(&db, ingredient, input.as_id());
            let root = RootObservations::default();
            STOP.set(Some(stop));
            ENTERED.set(false);
            let result = catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(&db, 100_000, || {
                    validate_ancestor(
                        &db,
                        input,
                        revision,
                        &root,
                        RegistryBuilder::new(&db, &Admission)?,
                    )
                })
            }));
            STOP.set(None);
            db.zalsa().runtime().reset_cancellation_flag();
            db.zalsa_local().uncancel();
            let failure = result.unwrap_err();
            match stop {
                Stop::Panic => assert!(failure.is::<ValidationPanic>()),
                Stop::LocalCancellation => assert!(matches!(
                    failure.downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::Local)
                )),
                Stop::PendingWriteCancellation => assert!(matches!(
                    failure.downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::PendingWrite)
                )),
            }
            assert!(ENTERED.get());
            assert_eq!((root.entries.get(), root.drops.get()), (1, 1));
            assert_eq!(original.header.verified_at.load(), revision);
            assert_ancestor_idle(&db, input);
            assert!(matches!(
                structural::fn_ingredient_(&db, db.zalsa())
                    .sync_table
                    .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
                ClaimResult::Claimed(())
            ));

            let retry = try_with_attempt(&db, 100_000, || {
                validate_ancestor(
                    &db,
                    input,
                    revision,
                    &root,
                    RegistryBuilder::new(&db, &Admission)?,
                )
                .map(|result| result.is_unchanged())
            });
            assert_eq!(retry, Ok(AttemptOutcome::Complete(Ok(true))));
            assert_eq!((root.entries.get(), root.drops.get()), (2, 2));
            assert_eq!(db.zalsa().current_revision(), current_revision);
            assert!(std::ptr::eq(original, memo(&db, ingredient, input.as_id())));
            assert_eq!(ancestor(&db, input), 0);
            assert_ancestor_idle(&db, input);
        }
    }
}

mod registration_boundaries {
    use std::num::NonZeroUsize;

    use super::super::super::registration::{NativeCallbackLimits, with_native_callback};
    use super::*;

    #[test]
    fn disabled_structural_validation_rejects_current_and_stale_memos() {
        for current in [false, true] {
            let mut db = TestDb::default();
            let input = Number::new(&db, 4);
            assert_eq!(structural(&db, input), 0);
            let revision = db.zalsa().current_revision();
            if !current {
                input.set_value(&mut db).to(6);
            }
            let key = structural::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
            let outcome = try_with_attempt(&db, 10_000, || {
                let result = RegistryBuilder::new(&db, &Admission)?
                    .seal()?
                    .run(|endpoint| async move { endpoint.validate(key, revision)?.await });
                assert!(matches!(
                    result,
                    Err(RunError::Contract("validation route is not registered"))
                ));
                result
            });
            assert!(matches!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            ));
            assert_eq!(db.executions.load(Ordering::Relaxed), 1);
            assert_idle(&db);
        }
    }

    #[test]
    fn duplicate_enabling_preserves_the_enabled_service() {
        let (db, input, revision) = ancestor_fixture();
        let key = structural::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
        let outcome = try_with_attempt(&db, 10_000, || {
            let mut registry = RegistryBuilder::new(&db, &Admission)?;
            registry.enable_structural_dependency_validation()?;
            assert_eq!(
                registry.enable_structural_dependency_validation(),
                Err(RunError::Contract(
                    "structural dependency validation already enabled"
                ))
            );
            registry.seal()?.run(|endpoint| async move {
                Ok(endpoint.validate(key, revision)?.await?.is_unchanged())
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(true))));
        assert_eq!(db.executions.load(Ordering::Relaxed), 2);
        assert_idle(&db);
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn native_validation(db: &dyn Db, input: Number) -> RunResult<bool> {
        with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
            let mut registry = RegistryBuilder::for_native_callback_with_budget(db, &entry)?;
            registry.enable_structural_dependency_validation()?;
            let key = structural::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
            let revision = db.zalsa().current_revision();
            registry.seal()?.run(|endpoint| async move {
                Ok(endpoint.validate(key, revision)?.await?.is_unchanged())
            })
        })
    }

    #[test]
    fn native_driver_accepts_current_proofs_but_cannot_prepare_stale_dependencies() {
        let limits = ExecutionLimits {
            semantic_work: 100_000,
            requested_bytes: 1_000_000,
        };
        for current in [false, true] {
            let (db, input, revision) = ancestor_fixture();
            if current {
                assert_eq!(structural(&db, input), 0);
            }
            let ingredient = structural::fn_ingredient_(&db, db.zalsa());
            let key = ingredient.database_key_index(input.as_id());
            let actual = Cell::new(None);
            let receipt = try_with_metered_execution_budget(&db, limits, |_| {
                let result = native_validation(&db, input);
                actual.set(Some(result));
                result
            })
            .unwrap();
            if current {
                assert_eq!(actual.get(), Some(Ok(true)));
                assert_eq!(receipt.outcome, AttemptOutcome::Complete(Ok(true)));
            } else {
                assert_eq!(
                    actual.get(),
                    Some(Err(RunError::Contract(
                        "structural preparation requires a root execution driver"
                    )))
                );
                assert_eq!(
                    receipt.outcome,
                    AttemptOutcome::Incomplete(Incomplete::Interrupted)
                );
                assert_eq!(
                    memo(&db, ingredient, input.as_id())
                        .header
                        .verified_at
                        .load(),
                    revision
                );
            }
            assert_eq!(
                db.executions.load(Ordering::Relaxed),
                if current { 2 } else { 1 }
            );
            assert_idle(&db);

            // Root validation can prepare the dependency without changing the revision. The
            // previously refused native query can then use its current canonical proof.
            assert_validation(&db, key, revision, true);
            let retry =
                try_with_metered_execution_budget(&db, limits, |_| native_validation(&db, input))
                    .unwrap();
            assert_eq!(retry.outcome, AttemptOutcome::Complete(Ok(true)));
            assert_eq!(db.executions.load(Ordering::Relaxed), 2);
            assert_idle(&db);
        }
    }
}

#[cfg(not(feature = "shuttle"))]
mod retained_preparation {
    use std::panic::{catch_unwind, panic_any};

    use super::*;
    use crate::attempt_probe::{AttemptSupport, StartError};
    use crate::execution_probe::{OrdinaryReadViolation, with_explicit_reads};
    use crate::function::execute::DisableLocalCancellationGuard;
    use crate::function::execute::execution_run::explicit_reads::strict_is_active;
    use crate::prepared_source_probe::{Stamp, try_with_preparation};

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn prepared_ancestor(db: &dyn Db, input: Number) -> u32 {
        optional_product_value(db, input)
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = prepared_initial, cycle_fn = prepared_recover)]
    fn prepared_fixpoint(db: &dyn Db, input: Number) -> u32 {
        optional_product_value(db, input)
    }

    fn prepared_initial(_db: &dyn Db, _id: Id, _input: Number) -> u32 {
        0
    }

    fn prepared_recover(
        _db: &dyn Db,
        _cycle: &Cycle<'_>,
        _last: &u32,
        value: u32,
        _input: Number,
    ) -> u32 {
        value
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn forbidden_semantic(db: &dyn Db, input: Number) -> u32 {
        db.executions().fetch_add(1, Ordering::Relaxed);
        input.value(db)
    }

    #[derive(Clone, Copy, Debug)]
    enum Stop {
        None,
        Error,
        Panic,
        LocalCancellation,
        MaskedLocalCancellation,
        PendingWriteCancellation,
        OrdinaryReadAfterResume,
    }

    #[derive(Debug)]
    struct PreparationPanic;

    struct PreparingAncestor {
        stop: Cell<Stop>,
        entries: Cell<usize>,
        phases: Cell<usize>,
        resumes: Cell<usize>,
        drops: Cell<usize>,
        foreign: TestDb,
    }

    impl PreparingAncestor {
        fn new(stop: Stop) -> Self {
            Self {
                stop: Cell::new(stop),
                entries: Cell::new(0),
                phases: Cell::new(0),
                resumes: Cell::new(0),
                drops: Cell::new(0),
                foreign: TestDb::default(),
            }
        }
    }

    struct ParentOwner<'a> {
        support: AttemptSupport,
        drops: &'a Cell<usize>,
    }

    impl Drop for ParentOwner<'_> {
        fn drop(&mut self) {
            assert!(self.support.same_owner(&attempt_probe::current().unwrap()));
            self.drops.set(self.drops.get() + 1);
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ReturnedState {
        same_attempt: bool,
        strict_reads: bool,
        same_caller: bool,
    }

    struct ReturnedProbe<'a> {
        db: &'a dyn Database,
        support: AttemptSupport,
        caller: Option<DatabaseKeyIndex>,
        observed: &'a Cell<Option<ReturnedState>>,
        drops: &'a Cell<usize>,
    }

    impl Drop for ReturnedProbe<'_> {
        fn drop(&mut self) {
            self.observed.set(Some(ReturnedState {
                same_attempt: attempt_probe::current()
                    .is_some_and(|current| self.support.same_owner(&current)),
                strict_reads: strict_is_active(),
                same_caller: self.db.zalsa_local().try_with_query_stack(|stack| {
                    stack.last().map(|query| query.database_key_index) == self.caller
                }) == Some(true),
            }));
            self.drops.set(self.drops.get() + 1);
        }
    }

    impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for PreparingAncestor
    where
        C: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
    {
        // Number conversion constructs a handle; output equality compares u32.
        fixture_native_value!(executable, 'run, 'db, C, 1);

        async fn body(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Db,
            input: Number,
        ) -> RunResult<u32> {
            self.entries.set(self.entries.get() + 1);
            let support = attempt_probe::current().unwrap();
            let _owner = ParentOwner {
                support: support.clone(),
                drops: &self.drops,
            };
            let endpoint = context.endpoint();
            let _cancellation_mask = matches!(self.stop.get(), Stop::MaskedLocalCancellation)
                .then(|| DisableLocalCancellationGuard::new(db.zalsa_local()));
            if matches!(self.stop.get(), Stop::MaskedLocalCancellation) {
                db.cancellation_token().cancel();
                assert!(!db.zalsa_local().should_trigger_local_cancellation());
            }
            let parent = db.zalsa_local().active_query().map(|(key, _)| key);
            let depths = attempt_probe::stack_depths();
            let ingredient = db
                .zalsa()
                .lookup_ingredient(parent.unwrap().ingredient_index())
                .as_function()
                .unwrap();
            let mut previous = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
            let mut value = 0;

            for _ in 0..2 {
                let certificate = endpoint
                    .prepare_structural(|| {
                        self.phases.set(self.phases.get() + 1);
                        assert!(attempt_probe::current().is_none());
                        assert_eq!(attempt_probe::stack_depths(), (0, 0));
                        assert!(db.zalsa_local().active_query().is_none());
                        assert!(matches!(
                            ingredient.sync_table().peek_claim(
                                db.zalsa(),
                                input.as_id(),
                                Reentrancy::Deny,
                            ),
                            ClaimResult::Cycle { .. }
                        ));
                        for nested_db in [db as &dyn Database, &self.foreign] {
                            let entered = Cell::new(false);
                            assert_eq!(
                                try_with_attempt(nested_db, 100, || entered.set(true)),
                                Err(StartError::NestedAttempt)
                            );
                            assert!(!entered.get());
                        }
                        try_with_preparation(db, || optional_product_value(db, input)).unwrap();
                        match self.stop.get() {
                            Stop::Error => {
                                return Err(RunError::Contract(
                                    "structural preparation fixture failed",
                                ));
                            }
                            Stop::Panic => panic_any(PreparationPanic),
                            Stop::LocalCancellation => db.cancellation_token().cancel(),
                            Stop::PendingWriteCancellation => {
                                db.zalsa().runtime().set_cancellation_flag();
                            }
                            Stop::None
                            | Stop::MaskedLocalCancellation
                            | Stop::OrdinaryReadAfterResume => {}
                        }
                        Ok(optional_product_value::prepare_memo(db, input).unwrap())
                    })
                    .await;
                self.resumes.set(self.resumes.get() + 1);
                assert!(support.same_owner(&attempt_probe::current().unwrap()));
                assert_eq!(db.zalsa_local().active_query().map(|(key, _)| key), parent);
                assert_eq!(attempt_probe::stack_depths(), depths);
                let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
                assert!(remaining < previous);
                previous = remaining;
                if matches!(self.stop.get(), Stop::OrdinaryReadAfterResume) {
                    semantic(db, input);
                }
                value = *endpoint
                    .read_prepared_source(optional_product_value::prepared_read(&certificate))
                    .await;
            }
            Ok(value)
        }

        async fn initial(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Db,
            _id: Id,
            _input: Number,
        ) -> RunResult<u32> {
            Err(RunError::RequiresFetch)
        }

        async fn recover<'call>(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Db,
            _cycle: &'call Cycle<'call>,
            _last: &'call u32,
            _value: u32,
            _input: Number,
        ) -> RunResult<u32>
        where
            'run: 'call,
        {
            Err(RunError::RequiresFetch)
        }
    }

    fn limits() -> ExecutionLimits {
        ExecutionLimits {
            semantic_work: 100_000,
            requested_bytes: 1_000_000,
        }
    }

    fn run<'run, 'db: 'run>(
        db: &'db TestDb,
        input: Number,
        provider: &'run PreparingAncestor,
        mut registry: RegistryBuilder<'run, 'db>,
    ) -> RunResult<u32> {
        let route = registry.reserve(
            db as &dyn Db,
            prepared_ancestor::fn_ingredient_(db, db.zalsa()),
        )?;
        let binding = registry.provider(provider)?;
        registry.bind_executable(&route, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            Ok(*endpoint
                .provider(binding)?
                .fetch_ref(&route, input.as_id())?
                .await?)
        })
    }

    fn assert_released(db: &TestDb, input: Number) {
        assert_idle(db);
        let ingredient = prepared_ancestor::fn_ingredient_(db, db.zalsa());
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
    }

    #[test]
    fn structural_preparation_retains_canonical_parent_and_tracked_outputs() {
        let oracle = TestDb::default();
        let expected = prepared_ancestor(&oracle, Number::new(&oracle, 7));
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        let stamp = Stamp::current(&db);
        let provider = PreparingAncestor::new(Stop::None);
        let (receipt, observed) = observation::collect(|| {
            try_with_metered_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    run(
                        &db,
                        input,
                        &provider,
                        RegistryBuilder::with_budget(&db, &budget)?,
                    )
                })
            })
        });
        assert_eq!(
            receipt.unwrap().outcome,
            AttemptOutcome::Complete(Ok(expected))
        );
        assert_eq!(
            (
                provider.entries.get(),
                provider.phases.get(),
                provider.resumes.get(),
                provider.drops.get()
            ),
            (1, 2, 2, 1)
        );
        assert!(stamp.belongs_to(&db));
        assert_released(&db, input);
        let ingredient = prepared_ancestor::fn_ingredient_(&db, db.zalsa());
        let key = ingredient.database_key_index(input.as_id());
        assert_eq!(observed.events.iter().filter(|event| matches!(event, observation::Event::Claim { key: claimed, .. } if *claimed == key)).count(), 1);
        let parent = memo(&db, ingredient, input.as_id());
        let structural = optional_product_value::prepare_memo(&db, input).unwrap();
        assert_eq!(
            parent.header.origin().inputs().collect::<Vec<_>>(),
            [structural.database_key()]
        );
        assert!(parent.header.outputs_are_empty());
        let product = memo(
            &db,
            optional_product::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        );
        assert!(!product.header.outputs_are_empty());
        assert_eq!(
            product.header.attempt_reuse(db.zalsa()),
            MemoReuse::Ordinary
        );
        assert_eq!(parent.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
        assert_eq!(prepared_ancestor(&db, input), 7);
        assert!(std::ptr::eq(parent, memo(&db, ingredient, input.as_id())));
    }

    #[test]
    fn structural_preparation_failures_restore_owners_and_allow_same_revision_retry() {
        for stop in [
            Stop::Error,
            Stop::Panic,
            Stop::LocalCancellation,
            Stop::MaskedLocalCancellation,
            Stop::PendingWriteCancellation,
            Stop::OrdinaryReadAfterResume,
        ] {
            let db = TestDb::default();
            let input = Number::new(&db, 7);
            let revision = db.zalsa().current_revision();
            let provider = PreparingAncestor::new(stop);
            let actual = Cell::new(None);
            let result = catch_unwind(AssertUnwindSafe(|| {
                try_with_metered_execution_budget(&db, limits(), |budget| {
                    with_explicit_reads(&db, &budget, || {
                        let result = run(
                            &db,
                            input,
                            &provider,
                            RegistryBuilder::with_budget(&db, &budget)?,
                        );
                        actual.set(result.as_ref().err().copied());
                        result
                    })
                })
            }));
            match stop {
                Stop::Error => {
                    assert_eq!(
                        actual.get(),
                        Some(RunError::Contract("structural preparation fixture failed"))
                    );
                    assert!(matches!(
                        result.unwrap().unwrap().outcome,
                        AttemptOutcome::Incomplete(Incomplete::Interrupted)
                    ));
                }
                Stop::Panic => assert!(result.unwrap_err().is::<PreparationPanic>()),
                Stop::LocalCancellation | Stop::MaskedLocalCancellation => assert!(matches!(
                    result.unwrap_err().downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::Local)
                )),
                Stop::PendingWriteCancellation => assert!(matches!(
                    result.unwrap_err().downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::PendingWrite)
                )),
                Stop::OrdinaryReadAfterResume => {
                    assert!(result.unwrap_err().is::<OrdinaryReadViolation>())
                }
                Stop::None => assert!(result.is_ok()),
            }
            db.zalsa().runtime().reset_cancellation_flag();
            db.zalsa_local().uncancel();
            assert_eq!(provider.drops.get(), 1);
            assert_eq!(
                provider.phases.get(),
                usize::from(!matches!(stop, Stop::MaskedLocalCancellation))
            );
            assert_eq!(
                provider.resumes.get(),
                usize::from(matches!(stop, Stop::OrdinaryReadAfterResume))
            );
            assert_released(&db, input);
            assert_eq!(db.zalsa().current_revision(), revision);
            let ingredient = prepared_ancestor::fn_ingredient_(&db, db.zalsa());
            assert!(
                ingredient
                    .get_memo_from_table_for(
                        db.zalsa(),
                        input.as_id(),
                        ingredient.memo_ingredient_index(db.zalsa(), input.as_id())
                    )
                    .is_none()
            );
            provider.stop.set(Stop::None);
            let retry = try_with_metered_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    run(
                        &db,
                        input,
                        &provider,
                        RegistryBuilder::with_budget(&db, &budget)?,
                    )
                })
            })
            .unwrap();
            assert_eq!(retry.outcome, AttemptOutcome::Complete(Ok(7)));
            assert_eq!(provider.entries.get(), 2);
            assert_eq!(provider.drops.get(), 2);
            assert_eq!(db.zalsa().current_revision(), revision);
            assert_released(&db, input);
        }
    }

    #[test]
    fn structural_cancellation_aborts_fixpoint_owner_before_same_revision_retry() {
        for stop in [
            Stop::LocalCancellation,
            Stop::MaskedLocalCancellation,
            Stop::PendingWriteCancellation,
        ] {
            let db = TestDb::default();
            let input = Number::new(&db, 7);
            let stamp = Stamp::current(&db);
            let provider = PreparingAncestor::new(stop);
            let ingredient = prepared_fixpoint::fn_ingredient_(&db, db.zalsa());
            let execute = || {
                try_with_metered_execution_budget(&db, limits(), |budget| {
                    with_explicit_reads(&db, &budget, || {
                        let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                        let route = registry.reserve(&db as &dyn Db, ingredient)?;
                        let binding = registry.provider(&provider)?;
                        registry.bind_executable(&route, &binding)?;
                        registry.seal()?.run(move |endpoint| async move {
                            Ok(*endpoint
                                .provider(binding)?
                                .fetch_ref(&route, input.as_id())?
                                .await?)
                        })
                    })
                })
            };
            let payload = catch_unwind(AssertUnwindSafe(execute))
                .expect_err("structural preparation delivers native cancellation");
            assert!(matches!(
                (stop, payload.downcast_ref::<crate::Cancelled>()),
                (
                    Stop::LocalCancellation | Stop::MaskedLocalCancellation,
                    Some(crate::Cancelled::Local)
                ) | (
                    Stop::PendingWriteCancellation,
                    Some(crate::Cancelled::PendingWrite)
                )
            ));
            db.zalsa_local().uncancel();
            db.zalsa().runtime().reset_cancellation_flag();
            assert_eq!(provider.entries.get(), 1);
            assert_eq!(provider.drops.get(), 1);
            assert_eq!(provider.resumes.get(), 0);
            assert_idle(&db);
            assert!(matches!(
                ingredient
                    .sync_table
                    .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
                ClaimResult::Claimed(())
            ));
            assert!(stamp.belongs_to(&db));

            provider.stop.set(Stop::None);
            assert_eq!(execute().unwrap().outcome, AttemptOutcome::Complete(Ok(7)));
            assert_eq!(provider.entries.get(), 2);
            assert_eq!(provider.drops.get(), 2);
            assert_eq!(prepared_fixpoint(&db, input), 7);
            assert!(stamp.belongs_to(&db));
            assert_idle(&db);
        }
    }

    #[test]
    fn structural_phase_rejects_warm_and_cold_semantic_queries_through_certification() {
        for warm in [false, true] {
            for producer in [false, true] {
                for caught in [false, true] {
                    let db = TestDb::default();
                    let input = Number::new(&db, 7);
                    if warm {
                        assert_eq!(forbidden_semantic(&db, input), 7);
                    }
                    let executions = db.executions.load(Ordering::Relaxed);
                    let revision = db.zalsa().current_revision();
                    let delivered = Cell::new(false);
                    let returned = Cell::new(false);
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        try_with_metered_execution_budget(&db, limits(), |budget| {
                            with_explicit_reads(&db, &budget, || {
                                let db = &db;
                                let delivered = &delivered;
                                let returned = &returned;
                                RegistryBuilder::with_budget(db, &budget)?.seal()?.run(
                                    |endpoint| async move {
                                        endpoint
                                            .prepare_structural(|| {
                                                let forbidden = || {
                                                    if caught {
                                                        assert!(
                                                            catch_unwind(AssertUnwindSafe(|| {
                                                                forbidden_semantic(db, input)
                                                            }))
                                                            .is_err()
                                                        );
                                                    } else {
                                                        forbidden_semantic(db, input);
                                                    }
                                                };
                                                try_with_preparation(db, || {
                                                    optional_product_value(db, input);
                                                    if producer {
                                                        forbidden();
                                                    }
                                                })
                                                .unwrap();
                                                if !producer {
                                                    optional_product_value::prepare_memo(db, input)
                                                        .unwrap();
                                                    forbidden();
                                                }
                                                returned.set(true);
                                                Ok(())
                                            })
                                            .await;
                                        delivered.set(true);
                                        Ok(())
                                    },
                                )
                            })
                        })
                    }));
                    if caught {
                        assert!(matches!(
                            result.unwrap().unwrap().outcome,
                            AttemptOutcome::Incomplete(Incomplete::Interrupted)
                        ));
                    } else {
                        assert!(result.is_err());
                    }
                    assert_eq!(returned.get(), caught);
                    assert!(!delivered.get());
                    assert_eq!(db.executions.load(Ordering::Relaxed), executions);
                    assert_eq!(db.zalsa().current_revision(), revision);
                    assert_idle(&db);
                    assert_eq!(
                        try_with_attempt(&db, 100, || ()),
                        Ok(AttemptOutcome::Complete(()))
                    );
                }
            }
        }
    }

    #[test]
    fn refused_structural_transition_does_not_enter_preparation() {
        let db = TestDb::default();
        let entered = Cell::new(false);
        let receipt = try_with_metered_execution_budget(&db, limits(), |budget| {
            with_explicit_reads(&db, &budget, || {
                let db = &db;
                let entered = &entered;
                RegistryBuilder::with_budget(db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        endpoint
                            .local_call(|| {
                                let remaining =
                                    attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
                                endpoint.admit_work(remaining)
                            })
                            .await;
                        endpoint
                            .prepare_structural(|| {
                                entered.set(true);
                                Ok(())
                            })
                            .await;
                        Ok(())
                    })
            })
        })
        .unwrap();
        assert_eq!(
            receipt.outcome,
            AttemptOutcome::Incomplete(Incomplete::Allowance)
        );
        assert_eq!(receipt.usage.semantic_work, limits().semantic_work);
        assert!(!entered.get());
        assert_idle(&db);
        assert_eq!(
            try_with_attempt(&db, 100, || ()),
            Ok(AttemptOutcome::Complete(()))
        );
    }

    #[test]
    fn returned_structural_value_drops_after_semantic_state_is_restored() {
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        let revision = db.zalsa().current_revision();
        let observed = Cell::new(None);
        let drops = Cell::new(0);
        let delivered = Cell::new(false);
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_metered_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    let db = &db;
                    let observed = &observed;
                    let drops = &drops;
                    let delivered = &delivered;
                    RegistryBuilder::with_budget(db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            let support = attempt_probe::current().unwrap();
                            let caller = db.zalsa_local().active_query().map(|(key, _)| key);
                            let value = endpoint
                                .prepare_structural(|| {
                                    try_with_preparation(db, || optional_product_value(db, input))
                                        .unwrap();
                                    optional_product_value::prepare_memo(db, input).unwrap();
                                    let value = ReturnedProbe {
                                        db,
                                        support,
                                        caller,
                                        observed,
                                        drops,
                                    };
                                    db.cancellation_token().cancel();
                                    Ok(value)
                                })
                                .await;
                            delivered.set(true);
                            drop(value);
                            Ok(())
                        })
                })
            })
        }));
        assert!(matches!(
            result.unwrap_err().downcast_ref::<crate::Cancelled>(),
            Some(crate::Cancelled::Local)
        ));
        db.zalsa_local().uncancel();
        assert_idle(&db);
        assert_eq!(db.zalsa().current_revision(), revision);
        assert!(!delivered.get());
        assert_eq!(drops.get(), 1);
        assert_eq!(
            observed.get(),
            Some(ReturnedState {
                same_attempt: true,
                strict_reads: true,
                same_caller: true,
            })
        );

        let provider = PreparingAncestor::new(Stop::None);
        let retry = try_with_metered_execution_budget(&db, limits(), |budget| {
            with_explicit_reads(&db, &budget, || {
                run(
                    &db,
                    input,
                    &provider,
                    RegistryBuilder::with_budget(&db, &budget)?,
                )
            })
        })
        .unwrap();
        assert_eq!(retry.outcome, AttemptOutcome::Complete(Ok(7)));
        assert_released(&db, input);
        assert_eq!(db.zalsa().current_revision(), revision);
    }

    #[cfg(feature = "accumulator")]
    #[crate::accumulator]
    struct PreparationNote(u32);

    #[cfg(feature = "accumulator")]
    #[test]
    fn structural_accumulator_reads_reject_a_cloned_database_worker() {
        for cloned_worker in [false, true] {
            let db = TestDb::default();
            let input = Number::new(&db, 7);
            assert_eq!(optional_product_value(&db, input), 7);
            let cloned = db.clone();
            assert!(std::ptr::eq(db.zalsa(), cloned.zalsa()));
            assert!(!std::ptr::eq(db.zalsa_local(), cloned.zalsa_local()));
            let target = if cloned_worker { &cloned } else { &db };
            let stamp = Stamp::current(&db);
            let rejected = Cell::new(false);
            let returned = Cell::new(false);
            let delivered = Cell::new(false);
            let receipt = try_with_metered_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    let rejected = &rejected;
                    let returned = &returned;
                    let delivered = &delivered;
                    RegistryBuilder::with_budget(&db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            endpoint
                                .prepare_structural(|| {
                                    let result = catch_unwind(AssertUnwindSafe(|| {
                                        optional_product_value::accumulated::<PreparationNote>(
                                            target, input,
                                        )
                                    }));
                                    match result {
                                        Ok(notes) => {
                                            assert_eq!(notes.first().map(|note| note.0), None)
                                        }
                                        Err(_) => rejected.set(true),
                                    }
                                    returned.set(true);
                                    Ok(())
                                })
                                .await;
                            delivered.set(true);
                            Ok(())
                        })
                })
            })
            .unwrap();
            assert_eq!(rejected.get(), cloned_worker);
            assert!(returned.get());
            assert_eq!(delivered.get(), !cloned_worker);
            if cloned_worker {
                assert_eq!(
                    receipt.outcome,
                    AttemptOutcome::Incomplete(Incomplete::Interrupted)
                );
            } else {
                assert_eq!(receipt.outcome, AttemptOutcome::Complete(Ok(())));
            }
            assert!(stamp.belongs_to(&db));
            assert_idle(&db);
            assert_idle(&cloned);
            assert_eq!(
                try_with_attempt(&db, 100, || ()),
                Ok(AttemptOutcome::Complete(()))
            );
        }
    }

    #[test]
    fn structural_input_reads_reject_a_foreign_database() {
        for foreign_database in [false, true] {
            let db = TestDb::default();
            let foreign = TestDb::default();
            let target = if foreign_database { &foreign } else { &db };
            let input = Number::new(target, 7);
            let stamp = Stamp::current(&db);
            let rejected = Cell::new(false);
            let value = Cell::new(None);
            let returned = Cell::new(false);
            let delivered = Cell::new(false);
            let receipt = try_with_metered_execution_budget(&db, limits(), |budget| {
                with_explicit_reads(&db, &budget, || {
                    let rejected = &rejected;
                    let value = &value;
                    let returned = &returned;
                    let delivered = &delivered;
                    RegistryBuilder::with_budget(&db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            endpoint
                                .prepare_structural(|| {
                                    match catch_unwind(AssertUnwindSafe(|| input.value(target))) {
                                        Ok(read) => value.set(Some(read)),
                                        Err(_) => rejected.set(true),
                                    }
                                    returned.set(true);
                                    Ok(())
                                })
                                .await;
                            delivered.set(true);
                            Ok(())
                        })
                })
            })
            .unwrap();
            assert_eq!(rejected.get(), foreign_database);
            assert_eq!(value.get(), (!foreign_database).then_some(7));
            assert!(returned.get());
            assert_eq!(delivered.get(), !foreign_database);
            if foreign_database {
                assert_eq!(
                    receipt.outcome,
                    AttemptOutcome::Incomplete(Incomplete::Interrupted)
                );
            } else {
                assert_eq!(receipt.outcome, AttemptOutcome::Complete(Ok(())));
            }
            assert!(stamp.belongs_to(&db));
            assert_idle(&db);
            assert_idle(&foreign);
            assert_eq!(input.value(target), 7);
            assert_eq!(
                try_with_attempt(&db, 100, || ()),
                Ok(AttemptOutcome::Complete(()))
            );
        }
    }
}

#[cfg(feature = "accumulator")]
mod accumulated {
    use super::*;
    use crate::Accumulator;

    #[crate::accumulator]
    struct Warning(u32);

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn direct(db: &dyn Db, input: Number) -> u32 {
        if input.value(db) != 0 {
            Warning(7).accumulate(db);
        }
        0
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn indirect(db: &dyn Db, input: Number) -> u32 {
        direct(db, input)
    }

    #[test]
    fn live_validation_preserves_direct_and_indirect_accumulated_inputs() {
        for through_indirect in [false, true] {
            let mut db = TestDb::default();
            let input = Number::new(&db, 0);
            assert_eq!(indirect(&db, input), 0);
            let revision = db.zalsa().current_revision();
            input.set_value(&mut db).to(1);
            let key = if through_indirect {
                indirect::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id())
            } else {
                direct::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id())
            };
            assert_eq!(
                try_with_attempt(&db, 10_000, || {
                    validate(&db, key, revision).map(|result| {
                        matches!(result, VerifyResult::Unchanged { accumulated } if accumulated.is_any())
                    })
                }),
                Ok(AttemptOutcome::Complete(Ok(true)))
            );
            assert_eq!(
                direct::accumulated::<Warning>(&db, input)
                    .iter()
                    .map(|warning| warning.0)
                    .collect::<Vec<_>>(),
                [7]
            );
            assert_idle(&db);
        }
    }
}

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{Node, Phase, memo};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
use crate::function::execute::execution_run::{Driver, Endpoint, Provider, RunError, RunResult};
use crate::function::memo::SelectedMemo;
use crate::function::{Configuration, IngredientImpl};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id, Setter};

#[derive(Debug, Eq, PartialEq)]
struct DropValue {
    value: u32,
    computed: bool,
}

#[derive(Debug)]
struct DropEvent {
    query: Option<DatabaseKeyIndex>,
    policy: QueryPolicy,
    operation_depth: usize,
}

thread_local! {
    static EVENTS: RefCell<Vec<DropEvent>> = const { RefCell::new(Vec::new()) };
    static PANIC_ON_DROP: Cell<bool> = const { Cell::new(false) };
}

impl Drop for DropValue {
    fn drop(&mut self) {
        if !self.computed {
            return;
        }
        let query =
            crate::with_attached_database(|db| db.zalsa_local().active_query().map(|(key, _)| key))
                .flatten();
        EVENTS.with_borrow_mut(|events| {
            events.push(DropEvent {
                query,
                policy: attempt_probe::current_policy(),
                operation_depth: attempt_probe::stack_depths().0,
            })
        });
        if PANIC_ON_DROP.replace(false) {
            panic!("computed query output destructor panic");
        }
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_result = initial)]
fn fallback(db: &dyn Database, node: Node) -> DropValue {
    DropValue {
        value: node.next(db).map_or(0, |next| fallback(db, next).value) + 1,
        computed: true,
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn fixpoint(db: &dyn Database, node: Node) -> DropValue {
    DropValue {
        value: (node.next(db).map_or(0, |next| fixpoint(db, next).value) + 1).min(3),
        computed: true,
    }
}

fn initial(db: &dyn Database, _id: Id, node: Node) -> DropValue {
    DropValue {
        value: node.seed(db),
        computed: false,
    }
}

fn recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _old: &DropValue,
    value: DropValue,
    _node: Node,
) -> DropValue {
    value
}

struct DropProvider<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    node: Node,
    stop: Option<Phase>,
}

async fn checkpoint<'run, 'db: 'run>(
    endpoint: Endpoint<'run, 'db>,
    stop: Option<Phase>,
    phase: Phase,
) -> RunResult<()> {
    endpoint
        .demand(move || async move {
            if stop == Some(phase) {
                Err(RunError::Refused(Incomplete::Allowance))
            } else {
                Ok(())
            }
        })?
        .await
}

impl<'run, 'db: 'run, C> Provider<'run, 'db, C> for DropProvider<'db, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = DropValue>,
{
    // Node conversion constructs a handle; DropValue equality checks u32 and bool.
    fixture_native_value!(provider, 'run, 'db, C, 2);

    type Output = u32;

    async fn body(
        &self,
        db: &'db dyn Database,
        input: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<DropValue> {
        let value = if let Some(next) = input.next(db) {
            if next == input {
                // The active target supplies a query-free seed; cold fetch is outside this control.
                self.ingredient
                    .fetch(db, db.zalsa(), db.zalsa_local(), next.as_id())
                    .value
            } else {
                endpoint
                    .execute(
                        self.ingredient,
                        db,
                        next.as_id(),
                        memo(db, self.ingredient, next),
                        Self {
                            ingredient: self.ingredient,
                            node: next,
                            stop: self.stop,
                        },
                    )?
                    .await?
            }
        } else {
            0
        };
        Ok(DropValue {
            value: if C::CYCLE_STRATEGY == crate::cycle::CycleRecoveryStrategy::Fixpoint {
                (value + 1).min(3)
            } else {
                value + 1
            },
            computed: true,
        })
    }

    async fn initial(
        &self,
        db: &'db dyn Database,
        id: Id,
        input: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<DropValue> {
        checkpoint(endpoint, self.stop, Phase::Initial).await?;
        Ok(initial(db, id, input))
    }

    async fn recover(
        &self,
        _db: &'db dyn Database,
        _cycle: &Cycle<'_>,
        _last: &DropValue,
        value: DropValue,
        _input: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<DropValue> {
        checkpoint(endpoint, self.stop, Phase::Recovery).await?;
        Ok(value)
    }

    async fn complete(
        &self,
        db: &'db dyn Database,
        selected: Option<SelectedMemo<'db, C>>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        let selected = selected.ok_or(RunError::RequiresFetch)?;
        Ok(self
            .ingredient
            .record_memo_read(db.zalsa(), db.zalsa_local(), self.node.as_id(), &selected)
            .value)
    }
}

fn run<C>(
    db: &dyn Database,
    ingredient: &IngredientImpl<C>,
    node: Node,
    stop: Option<Phase>,
) -> RunResult<u32>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = DropValue>,
{
    crate::attach(db, || {
        Driver::run(db, |endpoint| async move {
            endpoint
                .execute(
                    ingredient,
                    db,
                    node.as_id(),
                    memo(db, ingredient, node),
                    DropProvider {
                        ingredient,
                        node,
                        stop,
                    },
                )?
                .await
        })
    })
}

fn fixture() -> (DatabaseImpl, Node, Node) {
    let mut db = DatabaseImpl::default();
    let leaf = Node::new(&db, None, 0);
    leaf.set_next(&mut db).to(Some(leaf));
    let root = Node::new(&db, Some(leaf), 0);
    (db, root, leaf)
}

fn assert_restored(db: &dyn Database) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(
        db.zalsa()
            .attempt_operations
            .load(crate::sync::atomic::Ordering::SeqCst),
        0
    );
}

fn exercise<C>(
    db: &dyn Database,
    ingredient: &IngredientImpl<C>,
    root: Node,
    leaf: Node,
    phase: Phase,
    panic: bool,
) where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = DropValue>,
{
    EVENTS.with_borrow_mut(Vec::clear);
    PANIC_ON_DROP.set(panic);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100, || run(db, ingredient, root, Some(phase)))
    }));
    PANIC_ON_DROP.set(false);
    if panic {
        let payload = outcome.expect_err("the computed output destructor panics");
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"computed query output destructor panic")
        );
        for node in [root, leaf] {
            assert!(memo(db, ingredient, node).is_some_and(|memo| memo.value().is_none()));
        }
    } else {
        assert_eq!(
            outcome.expect("cooperative refusal does not unwind"),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
    }
    assert_restored(db);
    EVENTS.with_borrow(|events| {
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].query,
            Some(ingredient.database_key_index(leaf.as_id()))
        );
        assert_eq!(events[0].policy, QueryPolicy::ReturnOnly);
        assert_eq!(events[0].operation_depth, 2);
    });
    if !panic {
        EVENTS.with_borrow_mut(Vec::clear);
        let expected = if C::CYCLE_STRATEGY == crate::cycle::CycleRecoveryStrategy::Fixpoint {
            3
        } else {
            1
        };
        // The retry allowances include native quotations, comparisons and memo publication.
        const INITIAL_RETRY_WORK: usize = 67;
        const RECOVERY_RETRY_WORK: usize = 141;
        let retry_work = if phase == Phase::Recovery {
            RECOVERY_RETRY_WORK
        } else {
            INITIAL_RETRY_WORK
        };
        assert_eq!(
            try_with_attempt(db, retry_work, || {
                let result = run(db, ingredient, root, None);
                assert_eq!(
                    attempt_probe::remaining_allowance_for_diagnostics(db),
                    Some(0)
                );
                result
            }),
            Ok(AttemptOutcome::Complete(Ok(expected)))
        );
        assert_restored(db);
        if phase == Phase::Initial {
            // Fallback can repeat while dependency metadata converges. Every replacement
            // discards the computed output under the same owner.
            EVENTS.with_borrow(|events| {
                assert!(!events.is_empty());
                for event in events {
                    assert_eq!(
                        event.query,
                        Some(ingredient.database_key_index(leaf.as_id()))
                    );
                    assert_eq!(event.policy, QueryPolicy::ReturnOnly);
                    assert_eq!(event.operation_depth, 2);
                }
            });
        }
    } else {
        assert_eq!(
            try_with_attempt(db, 10, || Driver::run(db, |_| async { Ok(()) })),
            Ok(AttemptOutcome::Complete(Ok(())))
        );
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn initial_computed_output_drop_retains_owner_and_drains_on_panic() {
    for panic in [false, true] {
        let (db, root, leaf) = fixture();
        let ingredient = fallback::fn_ingredient_(&db, db.zalsa());
        exercise(&db, ingredient, root, leaf, Phase::Initial, panic);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn recovery_owned_output_drop_retains_owner_and_drains_on_panic() {
    for panic in [false, true] {
        let (db, root, leaf) = fixture();
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        exercise(&db, ingredient, root, leaf, Phase::Recovery, panic);
    }
}

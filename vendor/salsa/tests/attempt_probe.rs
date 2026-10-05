#![cfg(feature = "inventory")]

#[path = "attempt_probe/registration.rs"]
mod registration;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::attempt_probe::{
    AttemptOutcome, Incomplete, StartError, charge, remaining_allowance_for_diagnostics,
    try_with_attempt, try_with_operation,
};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult,
};
use salsa::{Database, DatabaseImpl, Durability, Setter, Storage};

thread_local! {
    static OUTPUT_PARENT_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static SYNTAX_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static SEMANTIC_ENTRIES: Cell<usize> = const { Cell::new(0) };
}

#[salsa::tracked]
struct Syntax<'db> {
    #[tracked]
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn unclassified_output_parent(db: &dyn Database) -> u32 {
    OUTPUT_PARENT_ENTRIES.set(OUTPUT_PARENT_ENTRIES.get() + 1);
    Syntax::new(db, 7).value(db)
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn syntax(db: &dyn Database) -> Syntax<'_> {
    SYNTAX_ENTRIES.set(SYNTAX_ENTRIES.get() + 1);
    Syntax::new(db, 7)
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn semantic(db: &dyn Database) -> u32 {
    SEMANTIC_ENTRIES.set(SEMANTIC_ENTRIES.get() + 1);
    let value = syntax(db).value(db);
    if charge(db, 1).is_err() {
        return 0;
    }
    value
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn mislabeled_output_parent(db: &dyn Database) -> u32 {
    Syntax::new(db, 9).value(db)
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn forbidden_callback(db: &dyn Database) -> u32 {
    semantic(db)
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn forbidden_direct_charge(db: &dyn Database) -> u32 {
    let _syntax = Syntax::new(db, 4);
    let _ = charge(db, 1);
    4
}

#[salsa::tracked(returns(copy))]
fn nested_start(db: &dyn Database) -> bool {
    try_with_attempt(db, 10, || panic!("rejected body executed")) == Err(StartError::ActiveQuery)
}

#[test]
fn unclassified_query_is_rejected_before_cold_and_warm_reads() {
    OUTPUT_PARENT_ENTRIES.set(0);
    let db = DatabaseImpl::default();
    let cold = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 0, || unclassified_output_parent(&db))
    }));
    assert!(cold.is_err());
    assert_eq!(OUTPUT_PARENT_ENTRIES.get(), 0);
    assert_eq!(unclassified_output_parent(&db), 7);
    assert_eq!(OUTPUT_PARENT_ENTRIES.get(), 1);
    let warm = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 0, || unclassified_output_parent(&db))
    }));
    assert!(warm.is_err());
    assert_eq!(OUTPUT_PARENT_ENTRIES.get(), 1);
}

#[test]
fn return_only_parent_can_read_completed_syntax_output() {
    SYNTAX_ENTRIES.set(0);
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_attempt(&db, 1, || semantic(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert_eq!(
        try_with_attempt(&db, 0, || semantic(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert_eq!(SYNTAX_ENTRIES.get(), 1);
}

#[test]
fn exhaustion_is_a_normal_incomplete_return() {
    SYNTAX_ENTRIES.set(0);
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_attempt(&db, 0, || semantic(&db)),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(syntax(&db).value(&db), 7);
    assert_eq!(SYNTAX_ENTRIES.get(), 1);
}

#[test]
fn retry_recomputes_incomplete_semantics_and_reuses_complete_syntax() {
    SYNTAX_ENTRIES.set(0);
    SEMANTIC_ENTRIES.set(0);
    let db = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    assert_eq!(
        try_with_attempt(&db, 0, || semantic(&db)),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_attempt(&db, 1, || semantic(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert_eq!(SEMANTIC_ENTRIES.get(), 2);
    assert_eq!(SYNTAX_ENTRIES.get(), 1);
    assert!(stamp.belongs_to(&db));
}

#[test]
fn return_only_output_creation_is_a_contract_error() {
    let db = DatabaseImpl::default();
    let payload = catch_unwind(AssertUnwindSafe(|| mislabeled_output_parent(&db)))
        .expect_err("ReturnOnly forbids tracked construction outside an attempt too");
    assert_return_only_output_contract(payload.as_ref());
}

fn assert_return_only_output_contract(payload: &(dyn Any + Send)) {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("string contract panic");
    assert!(
        message.contains("return-only query attempted to create a tracked output"),
        "{message}"
    );
}

#[salsa::tracked(returns(copy), specify)]
fn specified_value<'db>(_db: &'db dyn Database, _syntax: Syntax<'db>) -> u32 {
    5
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn specified_syntax(db: &dyn Database) -> Syntax<'_> {
    let syntax = Syntax::new(db, 7);
    specified_value::specify(db, syntax, 11);
    syntax
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn mislabeled_specifier<'db>(db: &'db dyn Database, syntax: Syntax<'db>) -> u32 {
    specified_value::specify(db, syntax, 99);
    0
}

#[test]
fn return_only_specification_is_a_contract_error() {
    let db = DatabaseImpl::default();
    let syntax = specified_syntax(&db);
    assert_eq!(specified_value(&db, syntax), 11);
    let payload = catch_unwind(AssertUnwindSafe(|| mislabeled_specifier(&db, syntax)))
        .expect_err("ReturnOnly forbids specification outside an attempt too");
    // This key belongs to the producer; a generic ownership panic would miss the policy guard.
    assert_return_only_output_contract(payload.as_ref());
    assert_eq!(specified_value(&db, syntax), 11);
}

#[test]
fn complete_only_cannot_hide_a_warm_semantic_callback() {
    SEMANTIC_ENTRIES.set(0);
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_attempt(&db, 1, || semantic(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert!(catch_unwind(AssertUnwindSafe(|| forbidden_callback(&db))).is_err());
    assert_eq!(SEMANTIC_ENTRIES.get(), 1);
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_nested_child(db: &dyn Database, request: Work) -> u32 {
    if request.value(db) != 0 {
        unclassified_output_parent(db)
    } else {
        semantic(db)
    }
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_nested_parent(db: &dyn Database, request: Work) -> u32 {
    complete_nested_child(db, request)
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn nested_output_creation(db: &dyn Database) -> u32 {
    mislabeled_output_parent(db)
}

#[test]
fn identical_policy_chains_keep_cold_and_hot_read_restrictions() {
    for unclassified in [false, true] {
        for warm in [false, true] {
            SEMANTIC_ENTRIES.set(0);
            OUTPUT_PARENT_ENTRIES.set(0);
            let db = DatabaseImpl::default();
            if warm {
                if unclassified {
                    assert_eq!(unclassified_output_parent(&db), 7);
                } else {
                    assert_eq!(
                        try_with_attempt(&db, 1, || semantic(&db)),
                        Ok(AttemptOutcome::Complete(7))
                    );
                }
            }
            let request = Work::new(&db, u32::from(unclassified));
            assert!(
                catch_unwind(AssertUnwindSafe(|| complete_nested_parent(&db, request))).is_err()
            );
            assert_eq!(
                SEMANTIC_ENTRIES.get() + OUTPUT_PARENT_ENTRIES.get(),
                usize::from(warm)
            );
        }
    }
    let db = DatabaseImpl::default();
    assert!(catch_unwind(AssertUnwindSafe(|| nested_output_creation(&db))).is_err());
}

#[derive(Clone, Copy)]
enum CallbackKind {
    Cancellation,
    Validation,
}

thread_local! {
    static CALLBACK_DATABASE: RefCell<Option<CallbackDatabase>> = const { RefCell::new(None) };
    static CALLBACK_KIND: Cell<CallbackKind> = const { Cell::new(CallbackKind::Cancellation) };
    static CALLBACK_ARMED: Cell<bool> = const { Cell::new(false) };
    static CALLBACK_REJECTED: Cell<bool> = const { Cell::new(false) };
    static CALLBACK_CHILD_ENTRIES: Cell<usize> = const { Cell::new(0) };
}

#[salsa::db]
#[derive(Clone)]
struct CallbackDatabase {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for CallbackDatabase {}

impl Default for CallbackDatabase {
    fn default() -> Self {
        Self {
            storage: Storage::new(Some(Box::new(|event| {
                let matches = match CALLBACK_KIND.get() {
                    CallbackKind::Cancellation => {
                        matches!(event.kind, salsa::EventKind::WillCheckCancellation)
                    }
                    CallbackKind::Validation => matches!(
                        event.kind,
                        salsa::EventKind::DidValidateMemoizedValue { .. }
                    ),
                };
                if matches && CALLBACK_ARMED.replace(false) {
                    CALLBACK_DATABASE.with_borrow(|db| {
                        let db = db.as_ref().expect("callback database installed");
                        CALLBACK_REJECTED.set(
                            catch_unwind(AssertUnwindSafe(|| unclassified_output_parent(db)))
                                .is_err(),
                        );
                    });
                }
            }))),
        }
    }
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn callback_child(db: &dyn Database, work: Work) -> u32 {
    CALLBACK_CHILD_ENTRIES.set(CALLBACK_CHILD_ENTRIES.get() + 1);
    work.value(db)
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn callback_parent(db: &dyn Database, work: Work) -> u32 {
    CALLBACK_ARMED.set(true);
    callback_child(db, work)
}

#[test]
fn reentrant_callbacks_keep_policy_on_cold_hot_and_validated_children() {
    for (kind, warm) in [
        (CallbackKind::Cancellation, false),
        (CallbackKind::Cancellation, true),
        (CallbackKind::Validation, true),
    ] {
        CALLBACK_KIND.set(kind);
        CALLBACK_ARMED.set(false);
        CALLBACK_REJECTED.set(false);
        CALLBACK_CHILD_ENTRIES.set(0);
        OUTPUT_PARENT_ENTRIES.set(0);
        let mut db = CallbackDatabase::default();
        let work = Work::builder(7).value_durability(Durability::HIGH).new(&db);
        let unrelated = Work::new(&db, 1);
        assert_eq!(unclassified_output_parent(&db), 7);
        if warm {
            assert_eq!(callback_child(&db, work), 7);
        }
        if matches!(kind, CallbackKind::Validation) {
            unrelated.set_value(&mut db).to(2);
        }
        CALLBACK_DATABASE.with_borrow_mut(|installed| *installed = Some(db.clone()));
        let value = callback_parent(&db, work);
        CALLBACK_DATABASE.with_borrow_mut(Option::take);
        assert_eq!(value, 7);
        assert!(CALLBACK_REJECTED.get());
        assert!(!CALLBACK_ARMED.get());
        assert_eq!(CALLBACK_CHILD_ENTRIES.get(), 1);
        assert_eq!(OUTPUT_PARENT_ENTRIES.get(), 1);
    }
}

#[test]
fn complete_only_cannot_charge_after_creating_output() {
    let db = DatabaseImpl::default();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 0, || forbidden_direct_charge(&db))
        }))
        .is_err()
    );
}

#[test]
fn active_source_cannot_install_an_attempt() {
    let db = DatabaseImpl::default();
    assert!(nested_start(&db));
}

thread_local! {
    static LEAF_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static CONSTRUCTOR_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static DEFINITION_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static PARENT_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static CYCLE_SEEDS: Cell<usize> = const { Cell::new(0) };
    static CYCLE_RECOVERIES: Cell<usize> = const { Cell::new(0) };
    static REFUSED: Cell<bool> = const { Cell::new(false) };
    static RECOVERIES_AFTER_REFUSAL: Cell<usize> = const { Cell::new(0) };
}

#[salsa::input]
struct Work {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn leaf(db: &dyn Database, work: Work) -> u32 {
    LEAF_ENTRIES.set(LEAF_ENTRIES.get() + 1);
    work.value(db)
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn constructor(db: &dyn Database, work: Work) -> u32 {
    CONSTRUCTOR_ENTRIES.set(CONSTRUCTOR_ENTRIES.get() + 1);
    let value = leaf(db, work);
    if charge(db, 1).is_err() {
        return 0;
    }
    value
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn definition(db: &dyn Database, work: Work) -> u32 {
    DEFINITION_ENTRIES.set(DEFINITION_ENTRIES.get() + 1);
    constructor(db, work) + 10
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn parent(db: &dyn Database, work: Work) -> u32 {
    PARENT_ENTRIES.set(PARENT_ENTRIES.get() + 1);
    definition(db, work) + 100
}

fn reset_entries() {
    LEAF_ENTRIES.set(0);
    CONSTRUCTOR_ENTRIES.set(0);
    DEFINITION_ENTRIES.set(0);
    PARENT_ENTRIES.set(0);
}

fn entries() -> [usize; 4] {
    [
        LEAF_ENTRIES.get(),
        CONSTRUCTOR_ENTRIES.get(),
        DEFINITION_ENTRIES.get(),
        PARENT_ENTRIES.get(),
    ]
}

#[test]
fn incomplete_reads_propagate_and_two_fresh_retries_reuse_leaves() {
    reset_entries();
    let db = DatabaseImpl::default();
    let first = Work::new(&db, 1);
    let second = Work::new(&db, 2);
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    for (round, work) in [first, second].into_iter().enumerate() {
        assert_eq!(
            try_with_attempt(&db, 0, || {
                assert_eq!(parent(&db, work), 110);
                let before = entries();
                assert_eq!(parent(&db, work), 110);
                assert_eq!(definition(&db, work), 10);
                assert_eq!(constructor(&db, work), 0);
                assert_eq!(entries(), before);
                // A parent may discard the fallback value entirely; its completion still depends on it.
                999
            }),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(
            try_with_attempt(&db, 1, || parent(&db, work)),
            Ok(AttemptOutcome::Complete(111 + round as u32))
        );
        let completed = entries();
        assert_eq!(
            try_with_attempt(&db, 0, || parent(&db, work)),
            Ok(AttemptOutcome::Complete(111 + round as u32))
        );
        assert_eq!(entries(), completed);
        assert!(stamp.belongs_to(&db));
    }
    assert_eq!(entries(), [2, 4, 4, 4]);
}

#[test]
fn source_callbacks_share_one_allowance() {
    reset_entries();
    let db = DatabaseImpl::default();
    let first = Work::new(&db, 1);
    let second = Work::new(&db, 2);
    assert_eq!(
        try_with_attempt(&db, 1, || (parent(&db, first), parent(&db, second))),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(entries(), [2, 2, 2, 2]);
    assert_eq!(
        try_with_attempt(&db, 1, || (parent(&db, first), parent(&db, second))),
        Ok(AttemptOutcome::Complete((111, 112)))
    );
    assert_eq!(entries(), [2, 3, 3, 3]);
}

#[test]
fn deep_validation_cannot_backdate_an_equal_incomplete_result() {
    reset_entries();
    let mut db = DatabaseImpl::default();
    let relevant = Work::new(&db, 0);
    let unrelated = Work::new(&db, 0);
    assert_eq!(
        try_with_attempt(&db, 1, || parent(&db, relevant)),
        Ok(AttemptOutcome::Complete(110))
    );
    unrelated.set_value(&mut db).to(1);
    assert_eq!(
        try_with_attempt(&db, 0, || parent(&db, relevant)),
        Ok(AttemptOutcome::Complete(110))
    );
    assert_eq!(entries(), [1, 1, 1, 1]);
    relevant.set_value(&mut db).to(1);
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    assert_eq!(
        try_with_attempt(&db, 0, || parent(&db, relevant)),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_attempt(&db, 1, || parent(&db, relevant)),
        Ok(AttemptOutcome::Complete(111))
    );
    assert!(stamp.belongs_to(&db));
    assert_eq!(LEAF_ENTRIES.get(), 2);
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = cycle_initial, cycle_fn = cycle_recover)]
fn cycle_a(db: &dyn Database) -> u32 {
    let previous = cycle_b(db);
    if charge(db, 1).is_err() {
        REFUSED.set(true);
        return 0;
    }
    (previous + 1).min(3)
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = cycle_initial, cycle_fn = cycle_recover)]
fn cycle_b(db: &dyn Database) -> u32 {
    cycle_a(db)
}

fn cycle_initial(_db: &dyn Database, _id: salsa::Id) -> u32 {
    CYCLE_SEEDS.set(CYCLE_SEEDS.get() + 1);
    0
}

fn cycle_recover(_db: &dyn Database, _cycle: &salsa::Cycle, _old: &u32, value: u32) -> u32 {
    CYCLE_RECOVERIES.set(CYCLE_RECOVERIES.get() + 1);
    if REFUSED.get() {
        RECOVERIES_AFTER_REFUSAL.set(RECOVERIES_AFTER_REFUSAL.get() + 1);
    }
    value
}

fn reset_cycle_counters() {
    CYCLE_SEEDS.set(0);
    CYCLE_RECOVERIES.set(0);
    REFUSED.set(false);
    RECOVERIES_AFTER_REFUSAL.set(0);
}

#[test]
fn interrupted_provisional_participant_restarts_from_canonical_seed() {
    reset_cycle_counters();
    let db = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    assert_eq!(
        try_with_attempt(&db, 1, || {
            cycle_a(&db);
            let callbacks = (CYCLE_SEEDS.get(), CYCLE_RECOVERIES.get());
            cycle_b(&db);
            cycle_a(&db);
            assert_eq!((CYCLE_SEEDS.get(), CYCLE_RECOVERIES.get()), callbacks);
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert!(CYCLE_RECOVERIES.get() > 0);
    assert_eq!(RECOVERIES_AFTER_REFUSAL.get(), 0);
    let initial_seeds = CYCLE_SEEDS.get();
    REFUSED.set(false);
    assert_eq!(
        try_with_attempt(&db, 20, || cycle_b(&db)),
        Ok(AttemptOutcome::Complete(3))
    );
    assert!(CYCLE_SEEDS.get() > initial_seeds);
    assert_eq!(RECOVERIES_AFTER_REFUSAL.get(), 0);
    assert!(stamp.belongs_to(&db));
}

#[test]
fn verified_cycle_and_independent_leaf_survive_later_refusal() {
    reset_cycle_counters();
    reset_entries();
    let db = DatabaseImpl::default();
    let work = Work::new(&db, 5);
    assert_eq!(
        try_with_attempt(&db, 20, || {
            assert_eq!(cycle_a(&db), 3);
            assert_eq!(leaf(&db, work), 5);
            let _ = charge(&db, 20);
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    let completed = (
        CYCLE_SEEDS.get(),
        CYCLE_RECOVERIES.get(),
        LEAF_ENTRIES.get(),
    );
    assert_eq!(
        try_with_attempt(&db, 0, || (cycle_a(&db), leaf(&db, work))),
        Ok(AttemptOutcome::Complete((3, 5)))
    );
    assert_eq!(
        (
            CYCLE_SEEDS.get(),
            CYCLE_RECOVERIES.get(),
            LEAF_ENTRIES.get()
        ),
        completed
    );
}

#[salsa::tracked(returns(ref), attempt = ReturnOnly)]
fn borrowed(db: &dyn Database) -> String {
    if charge(db, 1).is_err() {
        return "incomplete".to_owned();
    }
    "complete".to_owned()
}

#[test]
fn replaced_incomplete_memo_keeps_outstanding_borrow_alive() {
    let db = DatabaseImpl::default();
    let mut original = None;
    assert_eq!(
        try_with_attempt(&db, 0, || {
            original = Some(borrowed(&db));
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_attempt(&db, 0, || borrowed(&db)),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_attempt(&db, 1, || borrowed(&db)),
        Ok(AttemptOutcome::Complete(&"complete".to_owned()))
    );
    assert_eq!(original.map(String::as_str), Some("incomplete"));
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = callback_initial, cycle_fn = callback_recover)]
fn callback_cycle(db: &dyn Database, work: Work) -> u32 {
    (callback_cycle(db, work) + 1).min(3)
}

fn callback_initial(db: &dyn Database, _id: salsa::Id, work: Work) -> u32 {
    if work.value(db) != 0 && charge(db, 1).is_err() {
        REFUSED.set(true);
    }
    0
}

fn callback_recover(
    db: &dyn Database,
    _cycle: &salsa::Cycle,
    _old: &u32,
    value: u32,
    work: Work,
) -> u32 {
    if REFUSED.get() {
        RECOVERIES_AFTER_REFUSAL.set(RECOVERIES_AFTER_REFUSAL.get() + 1);
    }
    if work.value(db) == 0 && charge(db, 1).is_err() {
        REFUSED.set(true);
        return 0;
    }
    value
}

#[test]
fn refusal_in_cycle_callbacks_does_not_become_convergence() {
    for refuse_at_initial in [true, false] {
        reset_cycle_counters();
        let db = DatabaseImpl::default();
        let work = Work::new(&db, u32::from(refuse_at_initial));
        let stamp = salsa::prepared_source_probe::Stamp::current(&db);
        assert_eq!(
            try_with_attempt(&db, 0, || callback_cycle(&db, work)),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(RECOVERIES_AFTER_REFUSAL.get(), 0);
        REFUSED.set(false);
        assert_eq!(
            try_with_attempt(&db, 20, || callback_cycle(&db, work)),
            Ok(AttemptOutcome::Complete(3))
        );
        assert_eq!(RECOVERIES_AFTER_REFUSAL.get(), 0);
        assert!(stamp.belongs_to(&db));
    }
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = forbidden_initial)]
fn complete_cycle_with_semantic_initial(db: &dyn Database) -> u32 {
    complete_cycle_with_semantic_initial(db)
}

fn forbidden_initial(db: &dyn Database, _id: salsa::Id) -> u32 {
    semantic(db)
}

#[test]
fn complete_only_cycle_initial_cannot_call_semantic_queries() {
    SEMANTIC_ENTRIES.set(0);
    let db = DatabaseImpl::default();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 10, || complete_cycle_with_semantic_initial(&db))
        }))
        .is_err()
    );
    assert_eq!(SEMANTIC_ENTRIES.get(), 0);
}

#[test]
fn nested_outer_attempt_does_not_renew_allowance() {
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_attempt(&db, 0, || {
            assert_eq!(
                try_with_attempt(&db, 10, || 7),
                Err(StartError::NestedAttempt)
            );
            semantic(&db)
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
}

thread_local! {
    static RETAINED_DROPS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug, Eq, PartialEq)]
struct Retained(bool);

impl Drop for Retained {
    fn drop(&mut self) {
        RETAINED_DROPS.set(RETAINED_DROPS.get() + 1);
    }
}

#[salsa::tracked(returns(ref), attempt = ReturnOnly)]
fn retained(db: &dyn Database) -> Retained {
    Retained(charge(db, 1).is_ok())
}

#[test]
fn replaced_values_are_retained_until_the_next_revision() {
    RETAINED_DROPS.set(0);
    let mut db = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    for _ in 0..20 {
        assert_eq!(
            try_with_attempt(&db, 0, || {
                retained(&db);
            }),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert!(stamp.belongs_to(&db));
    }
    assert_eq!(
        try_with_attempt(&db, 1, || retained(&db).0),
        Ok(AttemptOutcome::Complete(true))
    );
    assert_eq!(RETAINED_DROPS.get(), 0);
    db.synthetic_write(salsa::Durability::LOW);
    assert_eq!(RETAINED_DROPS.get(), 20);
    drop(db);
    assert_eq!(RETAINED_DROPS.get(), 21);
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = cycle_initial)]
fn nested_a(db: &dyn Database) -> u32 {
    let value = (nested_b(db) + nested_d(db)).min(3);
    if charge(db, 1).is_err() {
        return 0;
    }
    value
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = cycle_initial)]
fn nested_b(db: &dyn Database) -> u32 {
    (nested_c(db) + 1).min(3)
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = cycle_initial)]
fn nested_c(db: &dyn Database) -> u32 {
    nested_b(db).max(nested_d(db))
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = cycle_initial)]
fn nested_d(db: &dyn Database) -> u32 {
    nested_a(db).max(nested_c(db))
}

#[test]
fn interrupted_nested_cycle_releases_every_participant_for_retry() {
    for entry in [nested_a, nested_b, nested_c, nested_d] {
        let db = DatabaseImpl::default();
        let stamp = salsa::prepared_source_probe::Stamp::current(&db);
        assert_eq!(
            try_with_attempt(&db, 0, || nested_a(&db)),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(
            try_with_attempt(&db, 100, || entry(&db)),
            Ok(AttemptOutcome::Complete(3))
        );
        assert!(stamp.belongs_to(&db));
    }
}

#[test]
fn independent_cold_leaf_after_refusal_remains_complete() {
    reset_entries();
    let db = DatabaseImpl::default();
    let refused = Work::new(&db, 1);
    let independent = Work::new(&db, 2);
    assert_eq!(
        try_with_attempt(&db, 0, || {
            constructor(&db, refused);
            assert_eq!(leaf(&db, independent), 2);
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(LEAF_ENTRIES.get(), 2);
    assert_eq!(
        try_with_attempt(&db, 0, || leaf(&db, independent)),
        Ok(AttemptOutcome::Complete(2))
    );
    assert_eq!(LEAF_ENTRIES.get(), 2);
}

#[test]
fn other_worker_enters_independently_during_an_attempt() {
    let db = DatabaseImpl::default();
    let worker_db = db.clone();
    assert_eq!(
        try_with_attempt(&db, 1, || {
            std::thread::spawn(move || {
                assert_eq!(
                    try_with_attempt(&worker_db, 1, || 7),
                    Ok(AttemptOutcome::Complete(7))
                );
                let scope_entered = Cell::new(false);
                assert_eq!(
                    try_with_operation(&worker_db, || scope_entered.set(true)),
                    Ok(())
                );
                assert!(scope_entered.get());
                assert_eq!(syntax(&worker_db).value(&worker_db), 7);
                assert!(!salsa::attempt_probe::is_incomplete(&worker_db));
            })
            .join()
            .unwrap();
            semantic(&db)
        }),
        Ok(AttemptOutcome::Complete(7))
    );
}

thread_local! {
    static BLOCK_QUERY: std::cell::RefCell<Option<(std::sync::Arc<std::sync::Barrier>, std::sync::Arc<std::sync::Barrier>)>> = const { std::cell::RefCell::new(None) };
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn held_complete(db: &dyn Database) -> u32 {
    BLOCK_QUERY.with_borrow(|block| {
        if let Some((entered, release)) = block {
            entered.wait();
            release.wait();
        }
    });
    syntax(db).value(db)
}

#[test]
fn attempt_start_overlaps_a_query_running_on_another_worker() {
    let db = DatabaseImpl::default();
    let worker_db = db.clone();
    let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
    let release = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_entered = entered.clone();
    let worker_release = release.clone();
    let worker = std::thread::spawn(move || {
        BLOCK_QUERY.with_borrow_mut(|block| *block = Some((worker_entered, worker_release)));
        held_complete(&worker_db)
    });
    entered.wait();
    let attempt = try_with_attempt(&db, 10, || 7);
    release.wait();
    assert_eq!(worker.join().unwrap(), 7);
    assert_eq!(attempt, Ok(AttemptOutcome::Complete(7)));
    assert_eq!(
        try_with_attempt(&db, 0, || held_complete(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
}

#[derive(Default)]
struct DiagnosticAdmission(Cell<usize>);

impl ExecutionAdmission for DiagnosticAdmission {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

#[test]
fn remaining_allowance_is_passive_and_local() {
    let events = Arc::new(AtomicUsize::new(0));
    let event_count = Arc::clone(&events);
    let db = CallbackDatabase {
        storage: Storage::new(Some(Box::new(move |_| {
            event_count.fetch_add(1, Ordering::SeqCst);
        }))),
    };
    let foreign = DatabaseImpl::default();
    let admission = DiagnosticAdmission::default();
    let delivered = Cell::new(false);
    let check = |expected| {
        let before = (events.load(Ordering::SeqCst), admission.0.get());
        assert_eq!(remaining_allowance_for_diagnostics(&db), expected);
        assert_eq!(remaining_allowance_for_diagnostics(&db), expected);
        assert_eq!(remaining_allowance_for_diagnostics(&foreign), None);
        assert_eq!(
            (events.load(Ordering::SeqCst), admission.0.get()),
            before,
            "diagnostic reads must not report events or admissions",
        );
    };
    check(None);
    let outcome = try_with_attempt(&db, 7, || {
        check(Some(7));
        let registry = RegistryBuilder::new(&db, &admission)?;
        let check = &check;
        let delivered = &delivered;
        registry.seal()?.run(|endpoint| async move {
            endpoint
                .local_call(|| {
                    check(Some(7));
                    endpoint.admit(ExecutionWork::Work { units: 100 })?;
                    check(Some(7));
                    endpoint.admit_work(3)?;
                    check(Some(4));
                    endpoint.admit_work(4)?;
                    check(Some(0));
                    let failure = endpoint.admit_work(1);
                    assert_eq!(failure, Err(RunError::Refused(Incomplete::Allowance)));
                    check(None);
                    failure
                })
                .await;
            delivered.set(true);
            Ok(())
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert!(!delivered.get());
    check(None);
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn explicit_refusal(db: &dyn Database) -> u32 {
    assert_eq!(
        salsa::attempt_probe::report_incomplete(db, Incomplete::Interrupted),
        Incomplete::Interrupted
    );
    assert!(salsa::attempt_probe::is_incomplete(db));
    assert_eq!(charge(db, 1), Err(Incomplete::Interrupted));
    assert_eq!(
        salsa::attempt_probe::report_incomplete(db, Incomplete::Allowance),
        Incomplete::Interrupted
    );
    0
}

#[test]
fn explicit_refusal_reason_survives_cached_reads() {
    let db = DatabaseImpl::default();
    assert!(!salsa::attempt_probe::is_incomplete(&db));
    assert_eq!(
        try_with_attempt(&db, 10, || {
            assert_eq!(explicit_refusal(&db), 0);
            assert_eq!(explicit_refusal(&db), 0);
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert!(!salsa::attempt_probe::is_incomplete(&db));
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn guarded_query(db: &dyn Database) -> u32 {
    if salsa::attempt_probe::is_incomplete(db) {
        return 0;
    }
    7
}

#[test]
fn checking_incomplete_status_also_records_the_dependency() {
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_attempt(&db, 1, || {
            salsa::attempt_probe::report_incomplete(&db, Incomplete::Interrupted);
            guarded_query(&db)
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_eq!(
        try_with_attempt(&db, 1, || guarded_query(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
}

#[salsa::tracked]
impl Work {
    #[salsa::tracked(returns(copy), attempt = ReturnOnly)]
    fn semantic_method(self, db: &dyn Database) -> u32 {
        constructor(db, self)
    }
}

#[test]
fn tracked_method_preserves_policy_when_expanding_macro_options() {
    let db = DatabaseImpl::default();
    let work = Work::new(&db, 9);
    assert_eq!(
        try_with_attempt(&db, 0, || work.semantic_method(&db)),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_attempt(&db, 1, || work.semantic_method(&db)),
        Ok(AttemptOutcome::Complete(9))
    );
}

#[test]
fn nested_operation_scopes_preserve_ordinary_queries_and_block_attempts_between_them() {
    let db = DatabaseImpl::default();
    let other = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    let other_stamp = salsa::prepared_source_probe::Stamp::current(&other);
    assert_eq!(
        try_with_operation(&db, || {
            assert_eq!(unclassified_output_parent(&db), 7);
            assert_eq!(
                try_with_attempt(&db, 1, || 9),
                Err(StartError::ActiveOperation)
            );
            assert_eq!(
                try_with_attempt(&other, 1, || 9),
                Err(StartError::ActiveOperation)
            );
            assert_eq!(
                try_with_operation(&db, || {
                    try_with_operation(&other, || {
                        assert_eq!(unclassified_output_parent(&other), 7);
                        try_with_operation(&db, || syntax(&db).value(&db))
                    })
                }),
                Ok(Ok(Ok(7)))
            );
            assert_eq!(unclassified_output_parent(&db), 7);
        }),
        Ok(())
    );
    assert_eq!(
        try_with_attempt(&db, 0, || 9),
        Ok(AttemptOutcome::Complete(9))
    );
    assert_eq!(
        try_with_attempt(&other, 0, || 9),
        Ok(AttemptOutcome::Complete(9))
    );
    assert!(stamp.belongs_to(&db));
    assert!(other_stamp.belongs_to(&other));
}

#[test]
fn ordinary_scope_allows_another_workers_attempt_between_queries() {
    let db = DatabaseImpl::default();
    let worker_db = db.clone();
    assert_eq!(
        try_with_operation(&db, || {
            assert_eq!(syntax(&db).value(&db), 7);
            let attempt = std::thread::spawn(move || {
                let entered = Cell::new(false);
                let result = try_with_attempt(&worker_db, 0, || entered.set(true));
                (result, entered.get())
            })
            .join()
            .unwrap();
            assert_eq!(attempt, (Ok(AttemptOutcome::Complete(())), true));
            assert_eq!(
                try_with_attempt(&db, 0, || ()),
                Err(StartError::ActiveOperation)
            );
            assert_eq!(syntax(&db).value(&db), 7);
        }),
        Ok(())
    );
    assert_eq!(
        try_with_attempt(&db, 0, || 9),
        Ok(AttemptOutcome::Complete(9))
    );
}

#[test]
fn unwinding_nested_scopes_releases_each_database_registration() {
    let db = DatabaseImpl::default();
    let other = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    let other_stamp = salsa::prepared_source_probe::Stamp::current(&other);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            try_with_operation(&db, || {
                syntax(&db);
                try_with_operation(&other, || {
                    syntax(&other);
                    try_with_operation(&db, || panic!("scope body unwinds"))
                })
            })
        }))
        .is_err()
    );
    assert_eq!(
        try_with_attempt(&db, 0, || syntax(&db).value(&db)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert_eq!(
        try_with_attempt(&other, 0, || syntax(&other).value(&other)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert!(stamp.belongs_to(&db));
    assert!(other_stamp.belongs_to(&other));
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn scoped_forbidden_callback(db: &dyn Database) -> u32 {
    try_with_operation(db, || semantic(db)).unwrap()
}

#[test]
fn operation_scope_preserves_complete_only_ancestor_for_cold_and_warm_reads() {
    for warm in [false, true] {
        SEMANTIC_ENTRIES.set(0);
        let db = DatabaseImpl::default();
        if warm {
            assert_eq!(
                try_with_attempt(&db, 1, || semantic(&db)),
                Ok(AttemptOutcome::Complete(7))
            );
        }
        assert!(catch_unwind(AssertUnwindSafe(|| scoped_forbidden_callback(&db))).is_err());
        assert_eq!(SEMANTIC_ENTRIES.get(), usize::from(warm));
    }
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn scoped_mislabeled_output_parent(db: &dyn Database) -> u32 {
    try_with_operation(db, || Syntax::new(db, 9).value(db)).unwrap()
}

#[test]
fn operation_scope_preserves_return_only_output_contract() {
    let db = DatabaseImpl::default();
    assert!(catch_unwind(AssertUnwindSafe(|| scoped_mislabeled_output_parent(&db))).is_err());
}

#[test]
fn operation_scopes_share_the_installed_attempt_allowance_and_retry() {
    reset_entries();
    let db = DatabaseImpl::default();
    let first = Work::new(&db, 1);
    let second = Work::new(&db, 2);
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    assert_eq!(
        try_with_attempt(&db, 1, || {
            try_with_operation(&db, || {
                assert_eq!(constructor(&db, first), 1);
                assert_eq!(try_with_operation(&db, || constructor(&db, second)), Ok(0));
                assert_eq!(charge(&db, 0), Err(Incomplete::Allowance));
            })
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(entries(), [2, 2, 0, 0]);
    assert_eq!(
        try_with_attempt(&db, 1, || {
            try_with_operation(&db, || (constructor(&db, first), constructor(&db, second)))
        }),
        Ok(AttemptOutcome::Complete(Ok((1, 2))))
    );
    assert_eq!(entries(), [2, 3, 0, 0]);
    assert!(stamp.belongs_to(&db));
}

#[test]
fn operation_scope_preserves_borrowed_values_across_same_revision_attempts() {
    let db = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    let mut original = None;
    assert_eq!(
        try_with_attempt(&db, 0, || {
            try_with_operation(&db, || {
                original = Some(borrowed(&db));
            })
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_attempt(&db, 1, || try_with_operation(&db, || borrowed(&db))),
        Ok(AttemptOutcome::Complete(Ok(&"complete".to_owned())))
    );
    assert_eq!(original.map(String::as_str), Some("incomplete"));
    assert!(stamp.belongs_to(&db));
}

#[test]
fn operation_scope_rejects_a_foreign_database_inside_an_attempt() {
    let db = DatabaseImpl::default();
    let other = DatabaseImpl::default();
    let entered = Cell::new(false);
    assert_eq!(
        try_with_attempt(&db, 0, || {
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    try_with_operation(&other, || entered.set(true))
                }))
                .is_err()
            );
            assert!(!entered.get());
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert_eq!(try_with_operation(&other, || 7), Ok(7));
    assert_eq!(
        try_with_attempt(&other, 0, || 9),
        Ok(AttemptOutcome::Complete(9))
    );
}

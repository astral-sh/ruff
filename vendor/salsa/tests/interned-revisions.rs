#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.

mod common;
use common::LogDatabase;
use expect_test::expect;
use salsa::{Database, Durability, FieldReads, HashEqLike, Lookup, Setter};
use test_log::test;

#[salsa::input]
struct Input {
    #[returns(copy)]
    field1: usize,
}

#[salsa::interned(revisions = 3, field_view = read_fields)]
#[derive(Debug)]
struct Interned<'db> {
    field1: BadHash,
}
// Use a consistent hash value to ensure that interned value sharding
// does not interefere with garbage collection.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, salsa::SalsaValue)]
struct BadHash(usize);

impl std::hash::Hash for BadHash {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_i16(0);
    }
}

#[derive(Debug)]
struct PanickingLookup(usize);

impl std::hash::Hash for PanickingLookup {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_i16(0);
    }
}

impl Lookup<BadHash> for PanickingLookup {
    fn into_owned(self) -> BadHash {
        assert_ne!(self.0, 2, "lookup panic");
        BadHash(self.0)
    }
}

impl HashEqLike<PanickingLookup> for BadHash {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_i16(0);
    }

    fn eq(&self, data: &PanickingLookup) -> bool {
        self.0 == data.0
    }
}

#[salsa::interned(revisions = 1)]
struct PanickingInterned<'db> {
    value: BadHash,
}

#[salsa::tracked(returns(copy))]
fn intern_panicking(db: &dyn Database, input: Input) -> PanickingInterned<'_> {
    PanickingInterned::new(db, PanickingLookup(input.field1(db)))
}

#[test]
fn panic_during_reuse_does_not_orphan_slot() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use salsa::plumbing::AsId;

    let mut db = common::LoggerDatabase::default();
    let input = Input::new(&db, 0);

    let first_id = intern_panicking(&db, input).as_id();
    input.set_field1(&mut db).to(1);
    let second_id = intern_panicking(&db, input).as_id();
    assert_eq!(second_id, first_id.next_generation().unwrap());

    input.set_field1(&mut db).to(2);
    let result = catch_unwind(AssertUnwindSafe(|| intern_panicking(&db, input)));
    assert!(result.is_err());

    input.set_field1(&mut db).to(3);
    let recovered_id = intern_panicking(&db, input).as_id();
    assert_eq!(recovered_id, second_id.next_generation().unwrap());
}

#[salsa::interned]
#[derive(Debug)]
struct NestedInterned<'db> {
    #[returns(copy)]
    interned: Interned<'db>,
}

#[test]
fn test_intern_new() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(input.field1(db)))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    let result_in_rev_1 = function(&db, input);
    assert_eq!(result_in_rev_1.field1(&db).0, 0);

    // Modify the input to force a new value to be created.
    input.set_field1(&mut db).to(1);

    let result_in_rev_2 = function(&db, input);
    assert_eq!(result_in_rev_2.field1(&db).0, 1);

    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(80)), revision: R1 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(81)), revision: R2 }",
        ]"#]]);
}

#[test]
fn test_reintern() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> Interned<'_> {
        let _ = input.field1(db);
        Interned::new(db, BadHash(0))
    }

    let mut db = common::EventLoggerDatabase::default();

    let input = Input::new(&db, 0);
    let result_in_rev_1 = function(&db, input);
    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(80)), revision: R1 }",
        ]"#]]);

    assert_eq!(result_in_rev_1.field1(&db).0, 0);

    // Modify the input to force the value to be re-interned.
    input.set_field1(&mut db).to(1);

    let result_in_rev_2 = function(&db, input);
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidValidateInternedValue { key: Interned(Id(80)), revision: R2 }",
        ]"#]]);

    assert_eq!(result_in_rev_2.field1(&db).0, 0);
}

#[test]
fn test_durability() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, _input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(0))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    let result_in_rev_1 = function(&db, input);
    assert_eq!(result_in_rev_1.field1(&db).0, 0);

    // Modify the input to bump the revision without re-interning the value, as there
    // is no read dependency.
    input.set_field1(&mut db).to(1);

    let result_in_rev_2 = function(&db, input);
    assert_eq!(result_in_rev_2.field1(&db).0, 0);

    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(80)), revision: R1 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "DidValidateMemoizedValue { database_key: function(Id(0)) }",
        ]"#]]);
}

#[test]
fn test_non_reusable_new_value_does_not_record_dependency() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> Interned<'_> {
        let _ = input.field1(db);
        Interned::new(db, BadHash(0))
    }

    for durability in [Durability::MEDIUM, Durability::HIGH] {
        let mut db = common::EventLoggerDatabase::default();
        let input = Input::builder(0).durability(durability).new(&db);

        let _ = function(&db, input);
        db.clear_logs();

        db.synthetic_write(durability);

        let _ = function(&db, input);
        db.assert_logs(expect![[r#"
            [
                "DidSetCancellationFlag",
                "WillCheckCancellation",
                "DidValidateMemoizedValue { database_key: function(Id(0)) }",
            ]"#]]);
    }
}

#[test]
fn test_non_reusable_existing_value_does_not_record_dependency() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> Interned<'_> {
        let _ = input.field1(db);
        Interned::new(db, BadHash(0))
    }

    for durability in [Durability::MEDIUM, Durability::HIGH] {
        let mut db = common::EventLoggerDatabase::default();
        let input0 = Input::builder(0).durability(durability).new(&db);
        let input1 = Input::builder(0).durability(durability).new(&db);

        let _ = function(&db, input0);
        let _ = function(&db, input1);
        db.clear_logs();

        db.synthetic_write(durability);

        let _ = function(&db, input1);
        db.assert_logs(expect![[r#"
            [
                "DidSetCancellationFlag",
                "WillCheckCancellation",
                "DidValidateMemoizedValue { database_key: function(Id(1)) }",
            ]"#]]);
    }
}

#[test]
fn test_non_reusable_value_still_updates_query_stamp() {
    #[salsa::tracked(returns(copy))]
    fn outer(db: &dyn Database, input: Input) -> bool {
        let _ = input.field1(db);
        let _ = Interned::new(db, BadHash(0));
        true
    }

    #[salsa::tracked(returns(copy))]
    fn intern_from_input(db: &dyn Database, input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(input.field1(db)))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    // Create an interned value with low durability in a revision after the outer query's other
    // inputs last changed. This makes the outer query's `changed_at` depend on the interned value.
    db.synthetic_write(Durability::LOW);
    assert!(outer(&db, input));

    // Collect the original interned value, then recreate it outside an active query. This gives the
    // new generation high durability, making it non-reusable.
    for key in 1..10 {
        db.synthetic_write(Durability::LOW);
        let _ = intern_from_input(&db, Input::new(&db, key));
    }
    let _ = Interned::new(&db, BadHash(0));

    // Revalidating `outer` observes that the old interned value was collected and executes it
    // again. Even though the replacement is non-reusable and needs no dependency edge, its revision
    // must still contribute to the new query stamp.
    assert!(outer(&db, input));
}

#[salsa::interned(revisions = usize::MAX)]
#[derive(Debug)]
struct Immortal<'db> {
    field1: BadHash,
}

#[salsa::interned(revisions = 4)]
#[derive(Debug)]
struct SpilledInterned<'db> {
    field1: BadHash,
}

#[test]
fn test_revisions_above_inline_capacity() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> SpilledInterned<'_> {
        SpilledInterned::new(db, BadHash(input.field1(db)))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    let result = function(&db, input);
    assert_eq!(result.field1(&db).0, 0);
    assert_eq!(salsa::plumbing::AsId::as_id(&result).generation(), 0);

    for i in 1..4 {
        input.set_field1(&mut db).to(i);

        let result = function(&db, input);
        assert_eq!(result.field1(&db).0, i);
        assert_eq!(salsa::plumbing::AsId::as_id(&result).generation(), 0);
    }

    input.set_field1(&mut db).to(4);
    let result = function(&db, input);
    assert_eq!(result.field1(&db).0, 4);
    assert_eq!(salsa::plumbing::AsId::as_id(&result).generation(), 1);
}

#[test]
fn test_immortal() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> Immortal<'_> {
        Immortal::new(db, BadHash(input.field1(db)))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    let result = function(&db, input);
    assert_eq!(result.field1(&db).0, 0);

    // Modify the input to bump the revision and intern a new value.
    //
    // No values should ever be reused with `revisions = usize::MAX`.
    for i in 1..if cfg!(miri) { 50 } else { 1000 } {
        input.set_field1(&mut db).to(i);
        let result = function(&db, input);
        assert_eq!(result.field1(&db).0, i);
        assert_eq!(salsa::plumbing::AsId::as_id(&result).generation(), 0);
    }
}

#[test]
fn test_reuse() {
    #[salsa::tracked(returns(copy))]
    fn function(db: &dyn Database, input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(input.field1(db)))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    let result = function(&db, input);
    assert_eq!(result.field1(&db).0, 0);

    // Modify the input to bump the revision and intern a new value.
    //
    // The slot will not be reused for the first few revisions, but after
    // that we should not allocate any more slots.
    for i in 1..10 {
        input.set_field1(&mut db).to(i);

        let result = function(&db, input);
        assert_eq!(result.field1(&db).0, i);
    }

    // Values that have been reused should be re-interned.
    for i in 1..10 {
        let result = function(&db, Input::new(&db, i));
        assert_eq!(result.field1(&db).0, i);
    }

    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(80)), revision: R1 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(81)), revision: R2 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidInternValue { key: Interned(Id(82)), revision: R3 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(80g1)), revision: R4 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(81g1)), revision: R5 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(82g1)), revision: R6 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(80g2)), revision: R7 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(81g2)), revision: R8 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(82g2)), revision: R9 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "DidReuseInternedValue { key: Interned(Id(80g3)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(1)) }",
            "DidInternValue { key: Interned(Id(83)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(2)) }",
            "DidInternValue { key: Interned(Id(84)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(3)) }",
            "DidInternValue { key: Interned(Id(85)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(4)) }",
            "DidInternValue { key: Interned(Id(86)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(5)) }",
            "DidInternValue { key: Interned(Id(87)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(6)) }",
            "DidInternValue { key: Interned(Id(88)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(7)) }",
            "DidValidateInternedValue { key: Interned(Id(81g2)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(8)) }",
            "DidValidateInternedValue { key: Interned(Id(82g2)), revision: R10 }",
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(9)) }",
        ]"#]]);
}

#[test]
fn reuse_discards_memos_for_old_generation() {
    use salsa::plumbing::AsId;

    #[salsa::tracked(returns(copy))]
    fn intern(db: &dyn Database, input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(input.field1(db)))
    }

    #[salsa::tracked(returns(copy))]
    fn read(db: &dyn Database, interned: Interned<'_>) -> usize {
        interned.field1(db).0
    }

    let mut db = common::DiscardLoggerDatabase::default();
    let input = Input::new(&db, 0);
    let first = intern(&db, input);
    let first_id = first.as_id();
    assert_eq!(read(&db, first), 0);
    db.clear_logs();

    let mut reused = false;
    for value in 1..10 {
        input.set_field1(&mut db).to(value);
        if intern(&db, input).as_id().index() == first_id.index() {
            reused = true;
            break;
        }
    }
    assert!(reused);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidDiscard { key: read(Id(80)) })",
        ]"#]]);
}

#[test]
fn test_reuse_indirect() {
    #[salsa::tracked(returns(copy))]
    fn intern(db: &dyn Database, input: Input, value: usize) -> Interned<'_> {
        intern_inner(db, input, value)
    }

    #[salsa::tracked(returns(copy))]
    fn intern_inner(db: &dyn Database, input: Input, value: usize) -> Interned<'_> {
        let _i = input.field1(db); // Only low durability interned values are garbage collected.
        Interned::new(db, BadHash(value))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::builder(0).durability(Durability::LOW).new(&db);

    // Intern `i0`.
    let i0 = intern(&db, input, 0);
    let i0_id = salsa::plumbing::AsId::as_id(&i0);
    assert_eq!(i0.field1(&db).0, 0);

    // Get the garbage collector to consider `i0` stale.
    for x in 1.. {
        db.synthetic_write(Durability::LOW);

        let ix = intern(&db, input, x);
        let ix_id = salsa::plumbing::AsId::as_id(&ix);

        // We reused the slot of `i0`.
        if ix_id.index() == i0_id.index() {
            assert_eq!(ix.field1(&db).0, x);

            // Re-intern and read `i0` from a new slot.
            //
            // Note that the only writes have been synthetic, so none of the query dependencies
            // have changed directly. The interned value dependency should be enough to force
            // the inner query to update.
            let i0 = intern(&db, input, 0);
            assert_eq!(i0.field1(&db).0, 0);

            break;
        }
    }
}

#[test]
fn raw_views_read_revalidated_reusable_handles() {
    #[salsa::tracked(returns(copy))]
    fn intern(db: &dyn Database, input: Input, value: usize) -> Interned<'_> {
        intern_inner(db, input, value)
    }

    #[salsa::tracked(returns(copy))]
    fn intern_inner(db: &dyn Database, input: Input, value: usize) -> Interned<'_> {
        let _ = input.field1(db);
        Interned::new(db, BadHash(value))
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::builder(0).durability(Durability::LOW).new(&db);
    let first_id = {
        let first = intern(&db, input, 0);
        db.clear_logs();
        let raw = first.read_fields(FieldReads::new(&db)).field1();
        assert_eq!(raw.0, 0);
        assert!(std::ptr::eq(raw, first.field1(&db)));
        db.assert_logs_len(0);
        salsa::plumbing::AsId::as_id(&first)
    };

    let mut replacement = None;
    for value in 1..=10 {
        db.synthetic_write(Durability::LOW);
        let current = intern(&db, input, value);
        let id = salsa::plumbing::AsId::as_id(&current);
        db.clear_logs();
        let raw = current.read_fields(FieldReads::new(&db)).field1();
        assert_eq!(raw.0, value);
        assert!(std::ptr::eq(raw, current.field1(&db)));
        db.assert_logs_len(0);
        if id.index() == first_id.index() {
            assert!(id.generation() > first_id.generation());
            replacement = Some(id);
            break;
        }
    }
    let replacement = replacement.expect("the constant-hash slot is reused within ten revisions");

    // Re-fetch through the creator: only the copied ID survives the database writes.
    let restored = intern(&db, input, 0);
    let restored_id = salsa::plumbing::AsId::as_id(&restored);
    assert_ne!(restored_id, first_id);
    assert_ne!(restored_id, replacement);
    db.clear_logs();
    let raw = restored.read_fields(FieldReads::new(&db)).field1();
    assert_eq!(raw.0, 0);
    assert!(std::ptr::eq(raw, restored.field1(&db)));
    db.assert_logs_len(0);
}

#[test]
fn test_reuse_interned_input() {
    // A query that creates an interned value.
    #[salsa::tracked(returns(copy))]
    fn create_interned(db: &dyn Database, input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(input.field1(db)))
    }

    #[salsa::tracked(returns(copy))]
    fn use_interned<'db>(db: &'db dyn Database, interned: Interned<'db>) -> usize {
        interned.field1(db).0
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    // Create and use I0 in R0.
    let interned = create_interned(&db, input);
    let result = use_interned(&db, interned);
    assert_eq!(result, 0);

    // Create and use I1 in a number of revisions, marking I0 as stale.
    input.set_field1(&mut db).to(1);
    for _ in 0..10 {
        let interned = create_interned(&db, input);
        let result = use_interned(&db, interned);
        assert_eq!(result, 1);

        // Trigger a new revision.
        input.set_field1(&mut db).to(1);
    }

    // Create I2, reusing the stale slot of I0.
    input.set_field1(&mut db).to(2);
    let interned = create_interned(&db, input);

    // Use I2. The function should not be memoized with the value of I0, despite I2 and I0
    // sharing the same slot.
    let result = use_interned(&db, interned);
    assert_eq!(result, 2);
}

#[test]
fn test_reuse_multiple_interned_input() {
    // A query that creates an interned value.
    #[salsa::tracked(returns(copy))]
    fn create_interned(db: &dyn Database, input: Input) -> Interned<'_> {
        Interned::new(db, BadHash(input.field1(db)))
    }

    // A query that creates an interned value.
    #[salsa::tracked(returns(copy))]
    fn create_nested_interned<'db>(
        db: &'db dyn Database,
        interned: Interned<'db>,
    ) -> NestedInterned<'db> {
        NestedInterned::new(db, interned)
    }

    #[salsa::tracked(returns(copy))]
    fn use_interned<'db>(db: &'db dyn Database, interned: Interned<'db>) -> usize {
        interned.field1(db).0
    }

    // A query that reads an interned value.
    #[salsa::tracked(returns(copy))]
    fn use_nested_interned<'db>(
        db: &'db dyn Database,
        nested_interned: NestedInterned<'db>,
    ) -> usize {
        nested_interned.interned(db).field1(db).0
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = Input::new(&db, 0);

    // Create and use NI0, which wraps I0, in R0.
    let interned = create_interned(&db, input);
    let i0_id = salsa::plumbing::AsId::as_id(&interned);
    let nested_interned = create_nested_interned(&db, interned);
    let result = use_nested_interned(&db, nested_interned);
    assert_eq!(result, 0);

    // Create and use I1 in a number of revisions, marking I0 as stale.
    input.set_field1(&mut db).to(1);
    for _ in 0..10 {
        let interned = create_interned(&db, input);
        let result = use_interned(&db, interned);
        assert_eq!(result, 1);

        // Trigger a new revision.
        input.set_field1(&mut db).to(1);
    }

    // Create I2, reusing the stale slot of I0.
    input.set_field1(&mut db).to(2);
    let interned = create_interned(&db, input);

    let i2_id = salsa::plumbing::AsId::as_id(&interned);
    assert_ne!(i0_id, i2_id);

    // Create NI1 wrapping I2 instead of I0.
    let nested_interned = create_nested_interned(&db, interned);

    // Use NI1. The function should not be memoized with the value of NI0,
    // despite I2 and I0 sharing the same ID.
    let result = use_nested_interned(&db, nested_interned);
    assert_eq!(result, 2);
}

#[test]
fn test_durability_increase() {
    #[salsa::tracked(returns(copy))]
    fn intern(db: &dyn Database, input: Input, value: usize) -> Interned<'_> {
        let _f = input.field1(db);
        Interned::new(db, BadHash(value))
    }

    let mut db = common::EventLoggerDatabase::default();

    let high_durability = Input::builder(0).durability(Durability::HIGH).new(&db);
    let low_durability = Input::builder(1).durability(Durability::LOW).new(&db);

    // Intern `i0`.
    let _i0 = intern(&db, low_durability, 0);
    // Re-intern `i0`, this time using a high-durability.
    let _i0 = intern(&db, high_durability, 0);

    // Get the garbage collector to consider `i0` stale.
    for _ in 0..100 {
        let _dummy = intern(&db, low_durability, 1000).field1(&db);
        db.synthetic_write(Durability::LOW);
    }

    // Intern `i1`.
    //
    // The slot of `i0` should not be reused as it is high-durability, and there
    // were no high-durability writes.
    let _i1 = intern(&db, low_durability, 1);

    // Re-intern and read `i0`.
    //
    // If the slot was reused, the memo would be shallow-verified and we would
    // read `i1` incorrectly.
    let value = intern(&db, high_durability, 0);
    assert_eq!(value.field1(&db).0, 0);

    db.synthetic_write(Durability::LOW);

    // We should have the same issue even after a low-durability write.
    let value = intern(&db, high_durability, 0);
    assert_eq!(value.field1(&db).0, 0);
}

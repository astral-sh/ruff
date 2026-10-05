#![cfg(feature = "inventory")]

//! Tests for incremental validation for queries involved in a cycle.
mod common;

use crate::common::EventLoggerDatabase;
use salsa::{Database, Durability, Setter};

#[salsa::input(debug)]
struct Input {
    #[returns(copy)]
    value: u32,
    #[returns(copy)]
    max: u32,
}

#[salsa::interned(debug)]
struct Output<'db> {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy), cycle_initial=query_a_initial)]
fn query_c(db: &dyn salsa::Database, input: Input) -> u32 {
    query_d(db, input)
}

#[salsa::tracked(returns(copy))]
fn query_d(db: &dyn salsa::Database, input: Input) -> u32 {
    let value = query_c(db, input);
    if value < input.max(db) * 2 {
        // Only the first iteration depends on value but the entire
        // cycle must re-run if input changes.
        let result = value + input.value(db);
        Output::new(db, result);
        result
    } else {
        value
    }
}

fn query_a_initial(_db: &dyn Database, _id: salsa::Id, _input: Input) -> u32 {
    0
}

/// Only the first iteration depends on `input.value`. It's important that the entire query
/// reruns if `input.value` changes. That's why salsa has to carry-over the inputs and outputs
/// from the previous iteration.
#[test_log::test]
fn first_iteration_input_only() {
    #[salsa::tracked(returns(copy), cycle_initial=query_a_initial)]
    fn query_a(db: &dyn salsa::Database, input: Input) -> u32 {
        query_b(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_b(db: &dyn salsa::Database, input: Input) -> u32 {
        let value = query_a(db, input);

        if value < input.max(db) {
            // Only the first iteration depends on value but the entire
            // cycle must re-run if input changes.
            value + input.value(db)
        } else {
            value
        }
    }

    let mut db = EventLoggerDatabase::default();

    let input = Input::builder(4, 5).durability(Durability::MEDIUM).new(&db);

    {
        let result = query_a(&db, input);

        assert_eq!(result, 8);
    }

    {
        input.set_value(&mut db).to(3);

        let result = query_a(&db, input);
        assert_eq!(result, 6);
    }
}

/// Very similar to the previous test, but the difference is that the called function
/// isn't the cycle head and that `cycle_participant` is called from
/// both the `cycle_head` and the `entry` function.
#[test_log::test]
fn nested_cycle_fewer_dependencies_in_first_iteration() {
    #[salsa::interned(debug)]
    struct ClassLiteral<'db> {
        #[returns(copy)]
        scope: Scope<'db>,
    }

    #[salsa::tracked]
    impl<'db> ClassLiteral<'db> {
        #[salsa::tracked(returns(copy))]
        fn context(self, db: &'db dyn salsa::Database) -> u32 {
            let scope = self.scope(db);

            // Access a field on `scope` that changed in the new revision.
            scope.field(db)
        }
    }

    #[salsa::tracked(debug)]
    struct Scope<'db> {
        #[returns(copy)]
        field: u32,
    }

    #[salsa::tracked(returns(copy))]
    fn create_interned<'db>(db: &'db dyn salsa::Database, scope: Scope<'db>) -> ClassLiteral<'db> {
        ClassLiteral::new(db, scope)
    }

    #[derive(Eq, PartialEq, Debug, salsa::SalsaValue)]
    struct Index<'db> {
        scope: Scope<'db>,
    }

    #[salsa::tracked(returns(copy), cycle_initial=head_initial)]
    fn cycle_head(db: &dyn salsa::Database, input: Input) -> Option<ClassLiteral<'_>> {
        let b = cycle_outer(db, input);
        tracing::info!("query_b = {b:?}");

        b.or_else(|| {
            let index = index(db, input);
            Some(create_interned(db, index.scope))
        })
    }

    fn head_initial(_db: &dyn Database, _id: salsa::Id, _input: Input) -> Option<ClassLiteral<'_>> {
        None
    }

    #[salsa::tracked(returns(copy))]
    fn cycle_outer(db: &dyn salsa::Database, input: Input) -> Option<ClassLiteral<'_>> {
        cycle_participant(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn cycle_participant(db: &dyn salsa::Database, input: Input) -> Option<ClassLiteral<'_>> {
        let value = cycle_head(db, input);
        tracing::info!("cycle_head = {value:?}");

        if let Some(value) = value {
            value.context(db);
            Some(value)
        } else {
            None
        }
    }

    #[salsa::tracked(returns(ref))]
    fn index(db: &dyn salsa::Database, input: Input) -> Index<'_> {
        Index {
            scope: Scope::new(db, input.value(db) * 2),
        }
    }

    #[salsa::tracked(returns(copy))]
    fn entry(db: &dyn salsa::Database, input: Input) -> u32 {
        let _ = input.value(db);
        let head = cycle_head(db, input);

        let participant = cycle_participant(db, input);
        tracing::debug!("head: {head:?}, participant: {participant:?}");

        head.or(participant)
            .map(|class| class.scope(db).field(db))
            .unwrap_or(0)
    }

    let mut db = EventLoggerDatabase::default();

    let input = Input::builder(3, 5)
        .max_durability(Durability::HIGH)
        .value_durability(Durability::LOW)
        .new(&db);

    {
        let result = entry(&db, input);

        assert_eq!(result, 6);
    }

    db.synthetic_write(Durability::MEDIUM);

    {
        input.set_value(&mut db).to(4);
        let result = entry(&db, input);
        assert_eq!(result, 8);
    }
}

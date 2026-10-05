#![cfg(feature = "inventory")]

use expect_test::expect;
use salsa::{Backtrace, Database, DatabaseImpl};
use test_log::test;

#[salsa::input(debug)]
struct Thing {
    #[returns(copy)]
    detailed: bool,
}

#[salsa::tracked(returns(clone))]
fn query_a(db: &dyn Database, thing: Thing) -> String {
    query_b(db, thing)
}

#[salsa::tracked(returns(clone))]
fn query_b(db: &dyn Database, thing: Thing) -> String {
    query_c(db, thing)
}

#[salsa::tracked(returns(clone))]
fn query_c(db: &dyn Database, thing: Thing) -> String {
    query_d(db, thing)
}

#[salsa::tracked(returns(clone))]
fn query_d(db: &dyn Database, thing: Thing) -> String {
    query_e(db, thing)
}

#[salsa::tracked(returns(clone))]
fn query_e(db: &dyn Database, thing: Thing) -> String {
    if thing.detailed(db) {
        format!("{:#}", Backtrace::capture().unwrap())
    } else {
        format!("{}", Backtrace::capture().unwrap())
    }
}
#[salsa::tracked(returns(clone))]
fn query_f(db: &dyn Database, thing: Thing) -> String {
    query_cycle(db, thing)
}

#[salsa::tracked(returns(clone), cycle_initial=cycle_initial)]
fn query_cycle(db: &dyn Database, thing: Thing) -> String {
    let backtrace = query_cycle(db, thing);
    if backtrace.is_empty() {
        query_e(db, thing)
    } else {
        backtrace
    }
}

fn cycle_initial(_db: &dyn salsa::Database, _id: salsa::Id, _thing: Thing) -> String {
    String::new()
}

#[test]
fn backtrace_works() {
    let db = DatabaseImpl::default();

    let backtrace = query_a(&db, Thing::new(&db, false)).replace("\\", "/");
    expect![[r#"
        query stacktrace:
           0: query_e(Id(0))
                     at tests/backtrace.rs:33
           1: query_d(Id(0))
                     at tests/backtrace.rs:28
           2: query_c(Id(0))
                     at tests/backtrace.rs:23
           3: query_b(Id(0))
                     at tests/backtrace.rs:18
           4: query_a(Id(0))
                     at tests/backtrace.rs:13
    "#]]
    .assert_eq(&backtrace);

    let backtrace = query_a(&db, Thing::new(&db, true)).replace("\\", "/");
    expect![[r#"
        query stacktrace:
           0: query_e(Id(1)) -> (R1, Durability::LOW)
                     at tests/backtrace.rs:33
           1: query_d(Id(1)) -> (R1, Durability::NEVER_CHANGE)
                     at tests/backtrace.rs:28
           2: query_c(Id(1)) -> (R1, Durability::NEVER_CHANGE)
                     at tests/backtrace.rs:23
           3: query_b(Id(1)) -> (R1, Durability::NEVER_CHANGE)
                     at tests/backtrace.rs:18
           4: query_a(Id(1)) -> (R1, Durability::NEVER_CHANGE)
                     at tests/backtrace.rs:13
    "#]]
    .assert_eq(&backtrace);

    let backtrace = query_f(&db, Thing::new(&db, false)).replace("\\", "/");
    expect![[r#"
        query stacktrace:
           0: query_e(Id(2))
                     at tests/backtrace.rs:33
           1: query_cycle(Id(2))
                     at tests/backtrace.rs:46
                     cycle heads: query_cycle(Id(2)) -> iteration = 0
           2: query_f(Id(2))
                     at tests/backtrace.rs:41
    "#]]
    .assert_eq(&backtrace);

    let backtrace = query_f(&db, Thing::new(&db, true)).replace("\\", "/");
    expect![[r#"
        query stacktrace:
           0: query_e(Id(3)) -> (R1, Durability::LOW)
                     at tests/backtrace.rs:33
           1: query_cycle(Id(3)) -> (R1, Durability::NEVER_CHANGE)
                     at tests/backtrace.rs:46
                     cycle heads: query_cycle(Id(3)) -> iteration = 0
           2: query_f(Id(3)) -> (R1, Durability::NEVER_CHANGE)
                     at tests/backtrace.rs:41
    "#]]
    .assert_eq(&backtrace);
}

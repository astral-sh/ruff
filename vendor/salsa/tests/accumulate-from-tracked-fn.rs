#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Accumulate values from within a tracked function.
//! Then mutate the values so that the tracked function re-executes.
//! Check that we accumulate the appropriate, new values.

use expect_test::expect;
use salsa::{Accumulator, Setter};
use test_log::test;

#[salsa::input(debug)]
struct List {
    #[returns(copy)]
    value: u32,
    #[returns(copy)]
    next: Option<List>,
}

#[salsa::accumulator]
#[derive(Copy, Clone, Debug)]
struct Integers(u32);

#[salsa::tracked(returns(copy))]
fn compute(db: &dyn salsa::Database, input: List) {
    eprintln!(
        "{:?}(value={:?}, next={:?})",
        input,
        input.value(db),
        input.next(db)
    );
    let result = if let Some(next) = input.next(db) {
        let next_integers = compute::accumulated::<Integers>(db, next);
        eprintln!("{next_integers:?}");
        let v = input.value(db) + next_integers.iter().map(|a| a.0).sum::<u32>();
        eprintln!("input={:?} v={:?}", input.value(db), v);
        v
    } else {
        input.value(db)
    };
    Integers(result).accumulate(db);
    eprintln!("pushed result {result:?}");
}

#[test]
fn test1() {
    let mut db = salsa::DatabaseImpl::new();

    let l0 = List::new(&db, 1, None);
    let l1 = List::new(&db, 10, Some(l0));

    compute(&db, l1);
    expect![[r#"
        [
            Integers(
                11,
            ),
            Integers(
                1,
            ),
        ]
    "#]]
    .assert_debug_eq(&compute::accumulated::<Integers>(&db, l1));

    l0.set_value(&mut db).to(2);
    compute(&db, l1);
    expect![[r#"
        [
            Integers(
                12,
            ),
            Integers(
                2,
            ),
        ]
    "#]]
    .assert_debug_eq(&compute::accumulated::<Integers>(&db, l1));
}

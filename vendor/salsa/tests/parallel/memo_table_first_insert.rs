use crate::sync::thread;
use crate::{Knobs, KnobsDatabase};

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn query_a(db: &dyn KnobsDatabase, input: Input) -> u32 {
    db.signal(1);
    db.wait_for(2);
    input.value(db)
}

#[salsa::tracked(returns(copy))]
fn query_b(db: &dyn KnobsDatabase, input: Input) -> u32 {
    db.wait_for(1);
    db.signal(2);
    input.value(db)
}

#[test_log::test]
fn concurrent_first_insert() {
    crate::sync::check(|| {
        let db_a = Knobs::default();
        let input = Input::new(&db_a, 42);
        let db_b = db_a.clone();

        let a = thread::spawn(move || query_a(&db_a, input));
        let b = thread::spawn(move || query_b(&db_b, input));

        assert_eq!(a.join().unwrap(), 42);
        assert_eq!(b.join().unwrap(), 42);
    });
}

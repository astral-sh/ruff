#![cfg(feature = "inventory")]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use salsa::{Database, Setter, Storage};

#[salsa::db]
struct TestDatabase {
    storage: Storage<Self>,
}

impl Default for TestDatabase {
    fn default() -> Self {
        let did_panic = Arc::new(AtomicBool::new(false));
        Self {
            storage: Storage::new(Some(Box::new(move |event| {
                if matches!(event.kind, salsa::EventKind::DidSetCancellationFlag)
                    && !did_panic.swap(true, Ordering::Relaxed)
                {
                    panic!("event callback panic");
                }
            }))),
        }
    }
}

#[salsa::db]
impl Database for TestDatabase {}

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn read(db: &dyn Database, input: Input) -> u32 {
    input.value(db)
}

#[test]
fn event_panic_does_not_leave_database_cancelled() {
    let mut db = TestDatabase::default();
    let input = Input::new(&db, 1);

    let result = catch_unwind(AssertUnwindSafe(|| input.set_value(&mut db).to(2)));
    assert!(result.is_err());

    assert_eq!(read(&db, input), 1);
}

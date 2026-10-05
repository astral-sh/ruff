#![cfg(feature = "inventory")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::{Database, EventKind, Setter, Storage};

#[salsa::db]
struct EventDatabase {
    storage: Storage<Self>,
    will_discard: Arc<AtomicUsize>,
    did_discard: Arc<AtomicUsize>,
}

impl Default for EventDatabase {
    fn default() -> Self {
        let will_discard = Arc::new(AtomicUsize::new(0));
        let did_discard = Arc::new(AtomicUsize::new(0));
        Self {
            storage: Storage::new(Some(Box::new({
                let will_discard = Arc::clone(&will_discard);
                let did_discard = Arc::clone(&did_discard);
                move |event| match event.kind {
                    EventKind::WillDiscardStaleOutput { .. } => {
                        will_discard.fetch_add(1, Ordering::Relaxed);
                    }
                    EventKind::DidDiscard { .. } => {
                        did_discard.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }))),
            will_discard,
            did_discard,
        }
    }
}

#[salsa::db]
impl Database for EventDatabase {}

#[salsa::tracked]
struct Owner<'db> {
    #[returns(copy)]
    value: u32,
}

#[salsa::input]
struct Input {
    #[returns(copy)]
    specify: bool,
}

#[salsa::tracked]
struct Child<'db> {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy), specify)]
fn overridable<'db>(db: &'db dyn Database, owner: Owner<'db>) -> Child<'db> {
    Child::new(db, owner.value(db) + 10)
}

#[salsa::tracked(returns(copy))]
fn specify_over_derived_memo(db: &dyn Database, input: Input) -> Child<'_> {
    let owner = Owner::new(db, 1);
    if input.specify(db) {
        let replacement = Child::new(db, 99);
        overridable::specify(db, owner, replacement);
        replacement
    } else {
        overridable(db, owner)
    }
}

#[test]
fn specify_discards_tracked_outputs_of_derived_memo() {
    let mut db = EventDatabase::default();
    let input = Input::new(&db, false);

    assert_eq!(specify_over_derived_memo(&db, input).value(&db), 11);
    assert_eq!(db.will_discard.load(Ordering::Relaxed), 0);
    assert_eq!(db.did_discard.load(Ordering::Relaxed), 0);

    input.set_specify(&mut db).to(true);

    assert_eq!(specify_over_derived_memo(&db, input).value(&db), 99);
    assert_eq!(db.will_discard.load(Ordering::Relaxed), 1);
    assert_eq!(db.did_discard.load(Ordering::Relaxed), 1);
}

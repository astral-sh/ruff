#![cfg(feature = "inventory")]

use salsa::Setter;
use salsa::plumbing::ZalsaDatabase;

mod deleted {
    use super::*;

    #[salsa::input]
    struct Input {
        #[returns(copy)]
        enabled: bool,
    }

    #[salsa::tracked]
    struct Entity<'db> {
        #[returns(copy)]
        value: u32,
    }

    #[salsa::tracked(returns(copy))]
    fn maybe_entity(db: &dyn salsa::Database, input: Input) -> Option<Entity<'_>> {
        input.enabled(db).then(|| Entity::new(db, 22))
    }

    #[test]
    fn deleted_tracked_structs_are_not_enumerated() {
        let mut db = salsa::DatabaseImpl::default();
        let input = Input::new(&db, true);
        assert!(maybe_entity(&db, input).is_some());

        input.set_enabled(&mut db).to(false);
        assert!(maybe_entity(&db, input).is_none());

        assert_eq!(Entity::ingredient(&db).entries(db.zalsa()).count(), 0);
    }
}

mod stale {
    use super::*;

    #[salsa::input]
    struct Input {
        #[returns(copy)]
        value: u32,
    }

    #[salsa::tracked]
    struct Entity<'db> {
        #[returns(copy)]
        value: u32,
    }

    #[salsa::tracked(returns(copy))]
    fn make_entity(db: &dyn salsa::Database, input: Input) -> Entity<'_> {
        Entity::new(db, input.value(db))
    }

    #[test]
    fn stale_tracked_structs_remain_enumerated() {
        let mut db = salsa::DatabaseImpl::default();
        let input = Input::new(&db, 1);
        assert_eq!(make_entity(&db, input).value(&db), 1);

        input.set_value(&mut db).to(2);

        assert_eq!(Entity::ingredient(&db).entries(db.zalsa()).count(), 1);
        assert_eq!(make_entity(&db, input).value(&db), 2);
    }
}

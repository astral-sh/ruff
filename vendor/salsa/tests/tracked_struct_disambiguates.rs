#![cfg(feature = "inventory")]

//! Test that disambiguation works, that is when we have a revision where we track multiple structs
//! that have the same hash, we can still differentiate between them.
#![allow(warnings)]

use std::hash::Hash;

use rayon::iter::Either;
use salsa::Setter;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::input]
struct MyInputs {
    #[returns(clone)]
    field: Vec<MyInput>,
}

#[salsa::tracked]
struct TrackedStruct<'db> {
    field: DumbHashable,
}

#[salsa::tracked]
struct TrackedStruct2<'db> {
    field: DumbHashable,
}

#[derive(Debug, Clone, salsa::SalsaValue)]
pub struct DumbHashable {
    field: u32,
}

impl Eq for DumbHashable {}
impl PartialEq for DumbHashable {
    fn eq(&self, other: &Self) -> bool {
        self.field == other.field
    }
}

// Force collisions, note that this is still a correct implementation wrt. PartialEq / Eq above
// as keep the property that k1 == k2 -> hash(k1) == hash(k2)
impl Hash for DumbHashable {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.field % 3).hash(state);
    }
}

fn alternate(
    db: &dyn salsa::Database,
    input: MyInput,
) -> Either<TrackedStruct<'_>, TrackedStruct2<'_>> {
    if input.field(db) % 2 == 0 {
        Either::Left(TrackedStruct::new(
            db,
            DumbHashable {
                field: input.field(db),
            },
        ))
    } else {
        Either::Right(TrackedStruct2::new(
            db,
            DumbHashable {
                field: input.field(db),
            },
        ))
    }
}

#[salsa::tracked(returns(clone))]
fn batch(
    db: &dyn salsa::Database,
    inputs: MyInputs,
) -> Vec<Either<TrackedStruct<'_>, TrackedStruct2<'_>>> {
    inputs
        .field(db)
        .iter()
        .map(|input| alternate(db, input.clone()))
        .collect()
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let inputs = MyInputs::new(
        &db,
        (0..64).into_iter().map(|i| MyInput::new(&db, i)).collect(),
    );
    let trackeds = batch(&db, inputs);
    for (id, tracked) in trackeds.into_iter().enumerate() {
        assert_eq!(id % 2 == 0, tracked.is_left());
        assert_eq!(id % 2 != 0, tracked.is_right());
    }
    for input in inputs.field(&db) {
        let prev = input.field(&db);
        input.set_field(&mut db).to(prev);
    }
    let trackeds = batch(&db, inputs);
    for (id, tracked) in trackeds.into_iter().enumerate() {
        assert_eq!(id % 2 == 0, tracked.is_left());
        assert_eq!(id % 2 != 0, tracked.is_right());
    }
}

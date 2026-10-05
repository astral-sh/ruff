//! Direct timing of ordinary memo validation, with route checks outside timed intervals.

use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Database, DatabaseImpl, Durability, Setter};

thread_local! {
    static BODIES: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

#[crate::input]
struct Leaf {
    value: u32,
}

#[crate::input]
struct Root {
    #[returns(ref)]
    leaves: Vec<Leaf>,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn leaf(db: &dyn Database, input: Leaf) -> u32 {
    BODIES.with(|count| {
        let (leaves, parents) = count.get();
        count.set((leaves + 1, parents));
    });
    input.value(db) % 2
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn parent(db: &dyn Database, input: Root) -> u32 {
    BODIES.with(|count| {
        let (leaves, parents) = count.get();
        count.set((leaves, parents + 1));
    });
    input.leaves(db).iter().map(|input| leaf(db, *input)).sum()
}

fn fixture(fanout: usize, durability: Durability) -> (DatabaseImpl, Root, Leaf) {
    let db = DatabaseImpl::default();
    let leaves: Vec<_> = (0..fanout)
        .map(|_| Leaf::builder(0).durability(durability).new(&db))
        .collect();
    let first = leaves[0];
    let root = Root::builder(leaves).durability(durability).new(&db);
    BODIES.set((0, 0));
    assert_eq!(parent(&db, root), 0);
    assert_eq!(BODIES.get(), (fanout, 1));
    BODIES.set((0, 0));
    (db, root, first)
}

fn hot() {
    let (db, root, _) = fixture(32, Durability::LOW);
    let ingredient = parent::fn_ingredient_(&db, db.zalsa());
    let revision = db.zalsa().current_revision();
    let repeats = 1_000_000;
    let start = Instant::now();
    let mut unchanged = 0;
    for _ in 0..repeats {
        unchanged += usize::from(
            black_box(ingredient.maybe_changed_after(
                black_box(&db),
                black_box(root.as_id()),
                black_box(revision),
            ))
            .is_unchanged(),
        );
    }
    let elapsed = start.elapsed().as_nanos();
    assert_eq!(unchanged, repeats);
    assert_eq!(BODIES.get(), (0, 0));
    println!("VALIDATION_PROBE\thot\t{repeats}\t{elapsed}\t0\t0");
}

fn edited(fanout: usize, shallow: bool) {
    let (mut db, root, first) = fixture(
        fanout,
        if shallow {
            Durability::HIGH
        } else {
            Durability::LOW
        },
    );
    let repeats: u32 = 4_000;
    let mut elapsed = 0;
    for index in 0..repeats {
        let revision = db.zalsa().current_revision();
        if shallow {
            db.synthetic_write(Durability::LOW);
        } else {
            first.set_value(&mut db).to((index + 1) * 2);
        }
        let ingredient = parent::fn_ingredient_(&db, db.zalsa());
        let memo = ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                root.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), root.as_id()),
            )
            .expect("the fixture warmed this exact memo");
        assert!(memo.header.verified_at.load() < db.zalsa().current_revision());
        let changed_at = memo.header.revisions.changed_at;
        let start = Instant::now();
        let result = black_box(ingredient.maybe_changed_after(
            black_box(&db),
            black_box(root.as_id()),
            black_box(revision),
        ));
        elapsed += start.elapsed().as_nanos();
        assert!(result.is_unchanged());
        assert_eq!(
            memo.header.verified_at.load(),
            db.zalsa().current_revision()
        );
        assert_eq!(memo.header.revisions.changed_at, changed_at);
        assert_eq!(
            BODIES.get(),
            (if shallow { 0 } else { index as usize + 1 }, 0)
        );
    }
    let name = if shallow { "shallow" } else { "deep" };
    let (leaves, parents) = BODIES.get();
    println!("VALIDATION_PROBE\t{name}_{fanout}\t{repeats}\t{elapsed}\t{leaves}\t{parents}");
}

#[test]
#[ignore = "paired measurement only; no timing assertions"]
fn validation_cost_probe() {
    hot();
    edited(32, true);
    for fanout in [1, 8, 32] {
        edited(fanout, false);
    }
}

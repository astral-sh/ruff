#![cfg(feature = "inventory")]

//! Test case for fixpoint iteration cycle resolution.
//!
//! This test case is intended to simulate a (very simplified) version of a real dataflow analysis
//! using fixpoint iteration.
use std::collections::BTreeSet;
use std::iter::IntoIterator;

use salsa::{Database as Db, Setter};

/// A Use of a symbol.
#[salsa::input]
struct Use {
    reaching_definitions: Vec<Definition>,
}

/// A Definition of a symbol, either of the form `base + increment` or `0 + increment`.
#[salsa::input]
struct Definition {
    #[returns(copy)]
    base: Option<Use>,
    #[returns(copy)]
    increment: usize,
}

#[derive(Eq, PartialEq, Clone, Debug)]
enum Type {
    Bottom,
    Values(Box<[usize]>),
    Top,
}

impl Type {
    fn join(tys: impl IntoIterator<Item = Type>) -> Type {
        let mut result = Type::Bottom;
        for ty in tys.into_iter() {
            result = match (result, ty) {
                (result, Type::Bottom) => result,
                (_, Type::Top) => Type::Top,
                (Type::Top, _) => Type::Top,
                (Type::Bottom, ty) => ty,
                (Type::Values(a_ints), Type::Values(b_ints)) => {
                    let mut set = BTreeSet::new();
                    set.extend(a_ints);
                    set.extend(b_ints);
                    Type::Values(set.into_iter().collect())
                }
            }
        }
        result
    }
}

#[salsa::tracked(returns(clone), cycle_fn=use_cycle_recover, cycle_initial=use_cycle_initial)]
fn infer_use(db: &dyn Db, u: Use) -> Type {
    let defs = u.reaching_definitions(db);
    match defs[..] {
        [] => Type::Bottom,
        [def] => infer_definition(db, def),
        _ => Type::join(defs.iter().map(|&def| infer_definition(db, def))),
    }
}

#[salsa::tracked(returns(clone), cycle_fn=def_cycle_recover, cycle_initial=def_cycle_initial)]
fn infer_definition(db: &dyn Db, def: Definition) -> Type {
    let increment_ty = Type::Values(Box::from([def.increment(db)]));
    if let Some(base) = def.base(db) {
        let base_ty = infer_use(db, base);
        add(&base_ty, &increment_ty)
    } else {
        increment_ty
    }
}

fn def_cycle_initial(_db: &dyn Db, _id: salsa::Id, _def: Definition) -> Type {
    Type::Bottom
}

fn def_cycle_recover(
    _db: &dyn Db,
    cycle: &salsa::Cycle,
    last_provisional_value: &Type,
    value: Type,
    _def: Definition,
) -> Type {
    if &value == last_provisional_value {
        value
    } else {
        cycle_recover(value, cycle.iteration())
    }
}

fn use_cycle_initial(_db: &dyn Db, _id: salsa::Id, _use: Use) -> Type {
    Type::Bottom
}

fn use_cycle_recover(
    _db: &dyn Db,
    cycle: &salsa::Cycle,
    last_provisional_value: &Type,
    value: Type,
    _use: Use,
) -> Type {
    if &value == last_provisional_value {
        value
    } else {
        cycle_recover(value, cycle.iteration())
    }
}

fn cycle_recover(value: Type, count: u32) -> Type {
    match &value {
        Type::Bottom => value,
        Type::Values(_) => {
            if count > 4 {
                Type::Top
            } else {
                value
            }
        }
        Type::Top => value,
    }
}

fn add(a: &Type, b: &Type) -> Type {
    match (a, b) {
        (Type::Bottom, _) | (_, Type::Bottom) => Type::Bottom,
        (Type::Top, _) | (_, Type::Top) => Type::Top,
        (Type::Values(a_ints), Type::Values(b_ints)) => {
            let mut set = BTreeSet::new();
            set.extend(
                a_ints
                    .into_iter()
                    .flat_map(|a| b_ints.into_iter().map(move |b| a + b)),
            );
            Type::Values(set.into_iter().collect())
        }
    }
}

/// x = 1
#[test]
fn simple() {
    let db = salsa::DatabaseImpl::new();

    let def = Definition::new(&db, None, 1);
    let u = Use::new(&db, vec![def]);

    let ty = infer_use(&db, u);

    assert_eq!(ty, Type::Values(Box::from([1])));
}

/// x = 1 if flag else 2
#[test]
fn union() {
    let db = salsa::DatabaseImpl::new();

    let def1 = Definition::new(&db, None, 1);
    let def2 = Definition::new(&db, None, 2);
    let u = Use::new(&db, vec![def1, def2]);

    let ty = infer_use(&db, u);

    assert_eq!(ty, Type::Values(Box::from([1, 2])));
}

/// x = 1 if flag else 2; y = x + 1
#[test]
fn union_add() {
    let db = salsa::DatabaseImpl::new();

    let x1 = Definition::new(&db, None, 1);
    let x2 = Definition::new(&db, None, 2);
    let x_use = Use::new(&db, vec![x1, x2]);
    let y_def = Definition::new(&db, Some(x_use), 1);
    let y_use = Use::new(&db, vec![y_def]);

    let ty = infer_use(&db, y_use);

    assert_eq!(ty, Type::Values(Box::from([2, 3])));
}

/// x = 1; loop { x = x + 0 }
#[test]
fn cycle_converges_then_diverges() {
    let mut db = salsa::DatabaseImpl::new();

    let def1 = Definition::new(&db, None, 1);
    let def2 = Definition::new(&db, None, 0);
    let u = Use::new(&db, vec![def1, def2]);
    def2.set_base(&mut db).to(Some(u));

    let ty = infer_use(&db, u);

    // Loop converges on 1
    assert_eq!(ty, Type::Values(Box::from([1])));

    // Set the increment on x from 0 to 1
    let new_increment = 1;
    def2.set_increment(&mut db).to(new_increment);

    // Now the loop diverges and we fall back to Top
    assert_eq!(infer_use(&db, u), Type::Top);
}

/// x = 1; loop { x = x + 1 }
#[test]
fn cycle_diverges_then_converges() {
    let mut db = salsa::DatabaseImpl::new();

    let def1 = Definition::new(&db, None, 1);
    let def2 = Definition::new(&db, None, 1);
    let u = Use::new(&db, vec![def1, def2]);
    def2.set_base(&mut db).to(Some(u));

    let ty = infer_use(&db, u);

    // Loop diverges. Cut it off and fallback to Type::Top
    assert_eq!(ty, Type::Top);

    // Set the increment from 1 to 0.
    def2.set_increment(&mut db).to(0);

    // Now the loop converges on 1.
    assert_eq!(infer_use(&db, u), Type::Values(Box::from([1])));
}

/// x = 0; y = 0; loop { x = y + 0; y = x + 0 }
#[test_log::test]
fn multi_symbol_cycle_converges_then_diverges() {
    let mut db = salsa::DatabaseImpl::new();

    let defx0 = Definition::new(&db, None, 0);
    let defy0 = Definition::new(&db, None, 0);
    let defx1 = Definition::new(&db, None, 0);
    let defy1 = Definition::new(&db, None, 0);
    let use_x = Use::new(&db, vec![defx0, defx1]);
    let use_y = Use::new(&db, vec![defy0, defy1]);
    defx1.set_base(&mut db).to(Some(use_y));
    defy1.set_base(&mut db).to(Some(use_x));

    // Both symbols converge on 0
    assert_eq!(infer_use(&db, use_x), Type::Values(Box::from([0])));
    assert_eq!(infer_use(&db, use_y), Type::Values(Box::from([0])));

    // Set the increment on x to 0.
    defx1.set_increment(&mut db).to(0);

    // Both symbols still converge on 0.
    assert_eq!(infer_use(&db, use_x), Type::Values(Box::from([0])));
    assert_eq!(infer_use(&db, use_y), Type::Values(Box::from([0])));

    // Set the increment on x from 0 to 1.
    defx1.set_increment(&mut db).to(1);

    // Now the loop diverges and we fall back to Top.
    assert_eq!(infer_use(&db, use_x), Type::Top);
    assert_eq!(infer_use(&db, use_y), Type::Top);
}

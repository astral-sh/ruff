#![cfg(feature = "inventory")]

//! Test cases for fixpoint iteration cycle resolution.
//!
//! These test cases use a generic query setup that allows constructing arbitrary dependency
//! graphs, and attempts to achieve good coverage of various cases.
mod common;
use common::{ExecuteValidateLoggerDatabase, LogDatabase};
use expect_test::expect;
use salsa::{Database as Db, DatabaseImpl as DbImpl, Durability, Setter};
#[cfg(not(miri))]
use test_log::test;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    N(u8),
    OutOfBounds,
    TooManyIterations,
}

impl Value {
    fn to_value(self) -> Option<u8> {
        if let Self::N(val) = self {
            Some(val)
        } else {
            None
        }
    }
}

/// A vector of inputs a query can evaluate to get an iterator of values to operate on.
///
/// This allows creating arbitrary query graphs between the four queries below (`min_iterate`,
/// `max_iterate`, `min_panic`, `max_panic`) for testing cycle behaviors.
#[salsa::input]
struct Inputs {
    #[returns(ref)]
    inputs: Vec<Input>,
}

impl Inputs {
    fn values(self, db: &dyn Db) -> impl Iterator<Item = Value> + use<'_> {
        self.inputs(db).iter().map(|input| input.eval(db))
    }
}

/// A single input, evaluating to a single [`Value`].
#[derive(Clone)]
enum Input {
    /// a simple value
    Value(Value),

    /// a simple value, reported as an untracked read
    UntrackedRead(Value),

    /// minimum of the given inputs, with fixpoint iteration on cycles
    MinIterate(Inputs),

    /// maximum of the given inputs, with fixpoint iteration on cycles
    MaxIterate(Inputs),

    /// minimum of the given inputs, panicking on cycles
    MinPanic(Inputs),

    /// maximum of the given inputs, panicking on cycles
    MaxPanic(Inputs),

    /// value of the given input, plus one; propagates error values
    Successor(Box<Input>),

    /// successor, converts error values to zero
    SuccessorOrZero(Box<Input>),
}

impl Input {
    fn eval(&self, db: &dyn Db) -> Value {
        match *self {
            Self::Value(value) => value,
            Self::UntrackedRead(value) => {
                db.report_untracked_read();
                value
            }
            Self::MinIterate(inputs) => min_iterate(db, inputs),
            Self::MaxIterate(inputs) => max_iterate(db, inputs),
            Self::MinPanic(inputs) => min_panic(db, inputs),
            Self::MaxPanic(inputs) => max_panic(db, inputs),
            Self::Successor(ref input) => match input.eval(db) {
                Value::N(num) => Value::N(num + 1),
                other => other,
            },
            Self::SuccessorOrZero(ref input) => match input.eval(db) {
                Value::N(num) => Value::N(num + 1),
                _ => Value::N(0),
            },
        }
    }

    #[track_caller]
    fn assert(&self, db: &dyn Db, expected: Value) {
        assert_eq!(self.eval(db), expected)
    }

    #[track_caller]
    fn assert_value(&self, db: &dyn Db, expected: u8) {
        self.assert(db, Value::N(expected))
    }

    #[track_caller]
    fn assert_bounds(&self, db: &dyn Db) {
        self.assert(db, Value::OutOfBounds)
    }

    #[track_caller]
    fn assert_count(&self, db: &dyn Db) {
        self.assert(db, Value::TooManyIterations)
    }
}

const MIN_VALUE: u8 = 10;
const MAX_VALUE: u8 = 245;
const MAX_ITERATIONS: u32 = 3;

/// Recover from a cycle by falling back to `Value::OutOfBounds` if the value is out of bounds,
/// `Value::TooManyIterations` if we've iterated more than `MAX_ITERATIONS` times, or else
/// returning the computed value to continue iterating.
fn cycle_recover(
    _db: &dyn Db,
    cycle: &salsa::Cycle,
    last_provisional_value: &Value,
    value: Value,
    _inputs: Inputs,
) -> Value {
    if &value == last_provisional_value {
        value
    } else if value
        .to_value()
        .is_some_and(|val| val <= MIN_VALUE || val >= MAX_VALUE)
    {
        Value::OutOfBounds
    } else if cycle.iteration() > MAX_ITERATIONS {
        Value::TooManyIterations
    } else {
        value
    }
}

/// Fold an iterator of `Value` into a `Value`, given some binary operator to apply to two `u8`.
/// `Value::TooManyIterations` and `Value::OutOfBounds` will always propagate, with
/// `Value::TooManyIterations` taking precedence.
fn fold_values<F>(values: impl IntoIterator<Item = Value>, op: F) -> Value
where
    F: Fn(u8, u8) -> u8,
{
    values
        .into_iter()
        .fold(None, |accum, elem| {
            let Some(accum) = accum else {
                return Some(elem);
            };
            match (accum, elem) {
                (Value::TooManyIterations, _) | (_, Value::TooManyIterations) => {
                    Some(Value::TooManyIterations)
                }
                (Value::OutOfBounds, _) | (_, Value::OutOfBounds) => Some(Value::OutOfBounds),
                (Value::N(val1), Value::N(val2)) => Some(Value::N(op(val1, val2))),
            }
        })
        .expect("inputs should not be empty")
}

/// Query minimum value of inputs, with cycle recovery.
#[salsa::tracked(returns(copy), cycle_fn=cycle_recover, cycle_initial=min_initial)]
fn min_iterate(db: &dyn Db, inputs: Inputs) -> Value {
    fold_values(inputs.values(db), u8::min)
}

fn min_initial(_db: &dyn Db, _id: salsa::Id, _inputs: Inputs) -> Value {
    Value::N(255)
}

/// Query maximum value of inputs, with cycle recovery.
#[salsa::tracked(returns(copy), cycle_fn=cycle_recover, cycle_initial=max_initial)]
fn max_iterate(db: &dyn Db, inputs: Inputs) -> Value {
    fold_values(inputs.values(db), u8::max)
}

fn max_initial(_db: &dyn Db, _id: salsa::Id, _inputs: Inputs) -> Value {
    Value::N(0)
}

/// Query minimum value of inputs, without cycle recovery.
#[salsa::tracked(returns(copy))]
fn min_panic(db: &dyn Db, inputs: Inputs) -> Value {
    fold_values(inputs.values(db), u8::min)
}

/// Query maximum value of inputs, without cycle recovery.
#[salsa::tracked(returns(copy))]
fn max_panic(db: &dyn Db, inputs: Inputs) -> Value {
    fold_values(inputs.values(db), u8::max)
}

fn untracked(num: u8) -> Input {
    Input::UntrackedRead(Value::N(num))
}

fn value(num: u8) -> Input {
    Input::Value(Value::N(num))
}

// Diagram nomenclature for nodes: Each node is represented as a:xx(ii), where `a` is a sequential
// identifier from `a`, `b`, `c`..., xx is one of the four query kinds:
// - `Ni` for `min_iterate`
// - `Xi` for `max_iterate`
// - `Np` for `min_panic`
// - `Xp` for `max_panic`
//\
// and `ii` is the inputs for that query, represented as a comma-separated list, with each
// component representing an input:
// - `a`, `b`, `c`... where the input is another node,
// - `uXX` for `UntrackedRead(XX)`
// - `vXX` for `Value(XX)`
// - `sY` for `Successor(Y)`
// - `zY` for `SuccessorOrZero(Y)`
//
// We always enter from the top left node in the diagram.

/// a:Np(a) -+
/// ^        |
/// +--------+
///
/// Simple self-cycle, no iteration, should panic.
#[test]
#[should_panic(expected = "dependency graph cycle")]
fn self_panic() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    a_in.set_inputs(&mut db).to(vec![a.clone()]);
    a.eval(&db);
}

/// a:Np(u10, a) -+
/// ^             |
/// +-------------+
///
/// Simple self-cycle with untracked read, no iteration, should panic.
#[test]
#[should_panic(expected = "dependency graph cycle")]
fn self_untracked_panic() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    a_in.set_inputs(&mut db).to(vec![untracked(10), a.clone()]);

    a.eval(&db);
}

/// a:Ni(a) -+
/// ^        |
/// +--------+
///
/// Simple self-cycle, iteration converges on initial value.
#[test]
fn self_converge_initial_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    a_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 255);
}

/// a:Ni(b) --> b:Np(a)
/// ^                 |
/// +-----------------+
///
/// Two-query cycle, one with iteration and one without.
/// If we enter from the one with iteration, we converge on its initial value.
#[test]
fn two_mixed_converge_initial_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinPanic(b_in);
    a_in.set_inputs(&mut db).to(vec![b]);
    b_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 255);
}

/// a:Np(b) --> b:Ni(a)
/// ^                 |
/// +-----------------+
///
/// Two-query cycle, one with iteration and one without.
/// If we enter from the one with no iteration, we panic.
#[test]
#[should_panic(expected = "dependency graph cycle")]
fn two_mixed_panic() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(b_in);
    let b = Input::MinIterate(a_in);
    a_in.set_inputs(&mut db).to(vec![b]);
    b_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.eval(&db);
}

/// a:Ni(b) --> b:Xi(a)
/// ^                 |
/// +-----------------+
///
/// Two-query cycle, both with iteration.
/// We converge on the initial value of whichever we first enter from.
#[test]
fn two_iterate_converge_initial_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MaxIterate(b_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 255);
    b.assert_value(&db, 255);
}

/// a:Xi(b) --> b:Ni(a)
/// ^                 |
/// +-----------------+
///
/// Two-query cycle, both with iteration.
/// We converge on the initial value of whichever we enter from.
/// (Same setup as above test, different query order.)
#[test]
fn two_iterate_converge_initial_value_2() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MinIterate(b_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 0);
    b.assert_value(&db, 0);
}

/// a:Np(b) --> b:Ni(c) --> c:Xp(b)
///             ^                 |
///             +-----------------+
///
/// Two-query cycle, enter indirectly at node with iteration, converge on its initial value.
#[test]
fn two_indirect_iterate_converge_initial_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db).to(vec![b]);

    a.assert_value(&db, 255);
}

/// a:Xp(b) --> b:Np(c) --> c:Xi(b)
///             ^                 |
///             +-----------------+
///
/// Two-query cycle, enter indirectly at node without iteration, panic.
#[test]
#[should_panic(expected = "dependency graph cycle")]
fn two_indirect_panic() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    let b = Input::MinPanic(b_in);
    let c = Input::MaxIterate(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db).to(vec![b]);

    a.eval(&db);
}

/// a:Np(b) -> b:Ni(v200,c) -> c:Xp(b)
///            ^                     |
///            +---------------------+
///
/// Two-query cycle, converges to non-initial value.
#[test]
fn two_converge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![value(200), c]);
    c_in.set_inputs(&mut db).to(vec![b]);

    a.assert_value(&db, 200);
}

/// a:Xp(b) -> b:Xi(v20,c) -> c:Xp(sb)
///            ^                     |
///            +---------------------+
///
/// Two-query cycle, falls back due to >3 iterations.
#[test]
fn two_fallback_count() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxPanic(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![value(20), c]);
    c_in.set_inputs(&mut db)
        .to(vec![Input::Successor(Box::new(b))]);

    a.assert_count(&db);
}

/// a:Xp(b) -> b:Xi(v20,c) -> c:Xp(zb)
///            ^                     |
///            +---------------------+
///
/// Two-query cycle, falls back but fallback does not converge.
#[test]
fn two_fallback_diverge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxPanic(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![value(20), c.clone()]);
    c_in.set_inputs(&mut db)
        .to(vec![Input::SuccessorOrZero(Box::new(b))]);

    a.assert_count(&db);
}

/// a:Xp(b) -> b:Xi(v244,c) -> c:Xp(sb)
///            ^                     |
///            +---------------------+
///
/// Two-query cycle, falls back due to value reaching >MAX_VALUE (we start at 244 and each
/// iteration increments until we reach >245).
#[test]
fn two_fallback_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxPanic(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![value(244), c]);
    c_in.set_inputs(&mut db)
        .to(vec![Input::Successor(Box::new(b))]);

    a.assert_bounds(&db);
}

/// a:Ni(b) -> b:Np(a, c) -> c:Np(v25, a)
/// ^          |                        |
/// +----------+------------------------+
///
/// Three-query cycle, (b) and (c) both depend on (a). We converge on 25.
#[test]
fn three_fork_converge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinPanic(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b]);
    b_in.set_inputs(&mut db).to(vec![a.clone(), c]);
    c_in.set_inputs(&mut db).to(vec![value(25), a.clone()]);

    a.assert_value(&db, 25);
}

/// a:Ni(b) -> b:Ni(a, c) -> c:Np(v25, b)
/// ^          |        ^          |
/// +----------+        +----------+
///
/// Layered cycles. We converge on 25.
#[test]
fn layered_converge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![a.clone(), c]);
    c_in.set_inputs(&mut db).to(vec![value(25), b]);

    a.assert_value(&db, 25);
}

/// a:Xi(b) -> b:Xi(a, c) -> c:Xp(v25, sb)
/// ^          |        ^          |
/// +----------+        +----------+
///
/// Layered cycles. We hit max iterations and fall back.
#[test]
fn layered_fallback_count() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![a.clone(), c]);
    c_in.set_inputs(&mut db)
        .to(vec![value(25), Input::Successor(Box::new(b))]);
    a.assert_count(&db);
}

/// a:Xi(b) -> b:Xi(a, c) -> c:Xp(v243, sb)
/// ^          |        ^          |
/// +----------+        +----------+
///
/// Layered cycles. We hit max value and fall back.
#[test]
fn layered_fallback_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![a.clone(), c]);
    c_in.set_inputs(&mut db)
        .to(vec![value(243), Input::Successor(Box::new(b))]);

    a.assert_bounds(&db);
}

/// a:Ni(b) -> b:Ni(c) -> c:Np(v25, a, b)
/// ^          ^                        |
/// +----------+------------------------+
///
/// Nested cycles. We converge on 25.
#[test]
fn nested_converge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db).to(vec![value(25), a.clone(), b]);

    a.assert_value(&db, 25);
}

/// a:Ni(b) -> b:Ni(c) -> c:Np(v25, b, a)
/// ^          ^                        |
/// +----------+------------------------+
///
/// Nested cycles, inner first. We converge on 25.
#[test]
fn nested_inner_first_converge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db).to(vec![value(25), b, a.clone()]);

    a.assert_value(&db, 25);
}

/// a:Xi(b) -> b:Xi(c) -> c:Xp(v25, a, sb)
/// ^          ^                         |
/// +----------+-------------------------+
///
/// Nested cycles. We hit max iterations and fall back.
#[test]
fn nested_fallback_count() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db)
        .to(vec![value(25), a.clone(), Input::Successor(Box::new(b))]);

    a.assert_count(&db);
}

/// a:Xi(b) -> b:Xi(c) -> c:Xp(v25, b, sa)
/// ^          ^                         |
/// +----------+-------------------------+
///
/// Nested cycles, inner first. We hit max iterations and fall back.
#[test]
fn nested_inner_first_fallback_count() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db)
        .to(vec![value(25), b, Input::Successor(Box::new(a.clone()))]);

    a.assert_count(&db);
}

/// a:Xi(b) -> b:Xi(c) -> c:Xp(v243, a, sb)
/// ^          ^                          |
/// +----------+--------------------------+
///
/// Nested cycles. We hit max value and fall back.
#[test]
fn nested_fallback_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c.clone()]);
    c_in.set_inputs(&mut db).to(vec![
        value(243),
        a.clone(),
        Input::Successor(Box::new(b.clone())),
    ]);
    a.assert_bounds(&db);
    b.assert_bounds(&db);
    c.assert_bounds(&db);
}

/// a:Xi(b) -> b:Xi(c) -> c:Xp(v243, b, sa)
/// ^          ^                          |
/// +----------+--------------------------+
///
/// Nested cycles, inner first. We hit max value and fall back.
#[test]
fn nested_inner_first_fallback_value() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c]);
    c_in.set_inputs(&mut db)
        .to(vec![value(243), b, Input::Successor(Box::new(a.clone()))]);

    a.assert_bounds(&db);
}

/// a:Ni(b) -> b:Ni(c, a) -> c:Np(v25, a, b)
/// ^          ^        |                  |
/// +----------+--------|------------------+
/// |                   |
/// +-------------------+
///
/// Nested cycles, double head. We converge on 25.
#[test]
fn nested_double_converge() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c, a.clone()]);
    c_in.set_inputs(&mut db).to(vec![value(25), a.clone(), b]);

    a.assert_value(&db, 25);
}

// Multiple-revision cycles

/// a:Ni(b) --> b:Np(a)
/// ^                 |
/// +-----------------+
///
/// a:Ni(b) --> b:Np(v30)
///
/// Cycle becomes not-a-cycle in next revision.
#[test]
fn cycle_becomes_non_cycle() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinPanic(b_in);
    a_in.set_inputs(&mut db).to(vec![b]);
    b_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 255);

    b_in.set_inputs(&mut db).to(vec![value(30)]);

    a.assert_value(&db, 30);
}

/// a:Ni(b) --> b:Np(v30)
///
/// a:Ni(b) --> b:Np(a)
/// ^                 |
/// +-----------------+
///
/// Non-cycle becomes a cycle in next revision.
#[test]
fn non_cycle_becomes_cycle() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinPanic(b_in);
    a_in.set_inputs(&mut db).to(vec![b]);
    b_in.set_inputs(&mut db).to(vec![value(30)]);

    a.assert_value(&db, 30);

    b_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 255);
}

/// a:Xi(b) -> b:Xi(c, a) -> c:Xp(v25, a, sb)
/// ^          ^        |                   |
/// +----------+--------|-------------------+
/// |                   |
/// +-------------------+
///
/// Nested cycles, double head. We hit max iterations and fall back, then max value on the next
/// revision, then converge on the next.
#[test]
fn nested_double_multiple_revisions() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MaxIterate(a_in);
    let b = Input::MaxIterate(b_in);
    let c = Input::MaxPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![b.clone()]);
    b_in.set_inputs(&mut db).to(vec![c, a.clone()]);
    c_in.set_inputs(&mut db).to(vec![
        value(25),
        a.clone(),
        Input::Successor(Box::new(b.clone())),
    ]);

    a.assert_count(&db);

    tracing::info!("New revision, expect max value");
    // next revision, we hit max value instead
    c_in.set_inputs(&mut db).to(vec![
        value(243),
        a.clone(),
        Input::Successor(Box::new(b.clone())),
    ]);

    a.assert_bounds(&db);

    // and next revision, we converge
    tracing::info!("New revision, expect converge");
    c_in.set_inputs(&mut db)
        .to(vec![value(240), a.clone(), b.clone()]);

    a.assert_value(&db, 240);

    tracing::info!("New revision, expect converge again");
    // one more revision, without relevant changes
    a_in.set_inputs(&mut db).to(vec![b]);

    a.assert_value(&db, 240);
}

/// a:Ni(b) -> b:Ni(c) -> c:Ni(a)
/// ^                           |
/// +---------------------------+
///
/// In a cycle with some LOW durability and some HIGH durability inputs, changing a LOW durability
/// input still re-executes the full cycle in the next revision.
#[test]
fn cycle_durability() {
    let mut db = DbImpl::new();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinIterate(c_in);
    a_in.set_inputs(&mut db)
        .with_durability(Durability::LOW)
        .to(vec![b.clone()]);
    b_in.set_inputs(&mut db)
        .with_durability(Durability::HIGH)
        .to(vec![c]);
    c_in.set_inputs(&mut db)
        .with_durability(Durability::HIGH)
        .to(vec![a.clone()]);

    a.assert_value(&db, 255);

    // next revision, we converge instead
    a_in.set_inputs(&mut db)
        .with_durability(Durability::LOW)
        .to(vec![value(45), b]);

    a.assert_value(&db, 45);
}

/// a:Np(v59, b) -> b:Ni(v60, c) -> c:Np(b)
///                 ^                     |
///                 +---------------------+
///
/// If nothing in a cycle changed in the new revision, no part of the cycle should re-execute.
#[test]
fn cycle_unchanged() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![value(59), b.clone()]);
    b_in.set_inputs(&mut db).to(vec![value(60), c]);
    c_in.set_inputs(&mut db).to(vec![b.clone()]);

    a.assert_value(&db, 59);
    b.assert_value(&db, 60);

    db.assert_logs_len(6);

    // next revision, we change only A, which is not part of the cycle and the cycle does not
    // depend on.
    a_in.set_inputs(&mut db).to(vec![value(45), b.clone()]);
    b.assert_value(&db, 60);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateMemoizedValue { database_key: min_iterate(Id(1)) })",
        ]"#]]);

    a.assert_value(&db, 45);
}

/// a:Np(v59, b) -> b:Ni(v60, c) -> c:Np(d) -> d:Ni(v61, b, e) -> e:Np(d)
///                 ^                          |   ^              |
///                 +--------------------------+   +--------------+
///
/// If nothing in a nested cycle changed in the new revision, no part of the cycle should
/// re-execute.
#[test_log::test]
fn cycle_unchanged_nested() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let d_in = Inputs::new(&db, vec![]);
    let e_in = Inputs::new(&db, vec![]);
    let a = Input::MinPanic(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    let d = Input::MinIterate(d_in);
    let e = Input::MinPanic(e_in);
    a_in.set_inputs(&mut db).to(vec![value(59), b.clone()]);
    b_in.set_inputs(&mut db).to(vec![value(60), c.clone()]);
    c_in.set_inputs(&mut db).to(vec![d.clone()]);
    d_in.set_inputs(&mut db)
        .to(vec![value(61), b.clone(), e.clone()]);
    e_in.set_inputs(&mut db).to(vec![d.clone()]);

    a.assert_value(&db, 59);
    b.assert_value(&db, 60);

    db.assert_logs_len(14);

    // next revision, we change only A, which is not part of the cycle and the cycle does not
    // depend on.
    a_in.set_inputs(&mut db).to(vec![value(45), b.clone()]);
    b.assert_value(&db, 60);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateMemoizedValue { database_key: min_iterate(Id(1)) })",
        ]"#]]);

    a.assert_value(&db, 45);
}

///                                 +--------------------------------+
///                                 |                                v
/// a:Np(v59, b) -> b:Ni(v60, c) -> c:Np(d, e) -> d:Ni(v61, b, e) -> e:Ni(d)
///                 ^                             |   ^              |
///                 +-----------------------------+   +--------------+
///
/// If nothing in a nested cycle changed in the new revision, no part of the cycle should
/// re-execute.
#[test_log::test]
fn cycle_unchanged_nested_intertwined() {
    // We run this test twice in order to catch some subtly different cases; see below.
    for i in 0..1 {
        let mut db = ExecuteValidateLoggerDatabase::default();
        let a_in = Inputs::new(&db, vec![]);
        let b_in = Inputs::new(&db, vec![]);
        let c_in = Inputs::new(&db, vec![]);
        let d_in = Inputs::new(&db, vec![]);
        let e_in = Inputs::new(&db, vec![]);
        let a = Input::MinPanic(a_in);
        let b = Input::MinIterate(b_in);
        let c = Input::MinPanic(c_in);
        let d = Input::MinIterate(d_in);
        let e = Input::MinIterate(e_in);
        a_in.set_inputs(&mut db).to(vec![value(59), b.clone()]);
        b_in.set_inputs(&mut db).to(vec![value(60), c.clone()]);
        c_in.set_inputs(&mut db).to(vec![d.clone(), e.clone()]);
        d_in.set_inputs(&mut db)
            .to(vec![value(61), b.clone(), e.clone()]);
        e_in.set_inputs(&mut db).to(vec![d.clone()]);

        a.assert_value(&db, 59);
        b.assert_value(&db, 60);

        // First time we run this test, don't fetch c/d/e here; this means they won't get marked
        // `verified_final` in R6 (this revision), which will leave us in the next revision (R7)
        // with a chain of could-be-provisional memos from the previous revision which should be
        // final but were never confirmed as such; this triggers the case in `deep_verify_memo`
        // where we need to double-check `validate_provisional` after traversing dependencies.
        //
        // Second time we run this test, fetch everything in R6, to check the behavior of
        // `maybe_changed_after` with all validated-final memos.
        if i == 1 {
            c.assert_value(&db, 60);
            d.assert_value(&db, 60);
            e.assert_value(&db, 60);
        }

        db.assert_logs_len(14 + i);

        // next revision, we change only A, which is not part of the cycle and the cycle does not
        // depend on.
        a_in.set_inputs(&mut db).to(vec![value(45), b.clone()]);
        b.assert_value(&db, 60);

        db.assert_logs(expect![[r#"
            [
                "salsa_event(DidValidateMemoizedValue { database_key: min_iterate(Id(1)) })",
            ]"#]]);

        a.assert_value(&db, 45);
    }
}

/// Test that cycle heads from one dependency don't interfere with sibling verification.
///
/// a:Ni(b, c, d) -> b:Ni(a)        [cycle with a, unchanged]
///               \-> c:Np(v100)    [no cycle, unchanged]
///                \-> d:Np(v200->v201) [no cycle, changes]
///
/// When verifying a in a new revision:
/// 1. b goes through deep verification (detects b->a cycle, adds cycle heads, returns unchanged)
/// 2. c gets verified (should not be affected by b's cycle heads with the fix)
/// 3. d returns changed, causing a to re-execute
///
/// Without the fix: cycle heads from b's verification remain in shared context and interfere with c
/// With the fix: c gets fresh cycle head context and verifies cleanly
#[test]
fn cycle_sibling_interference() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let a_in = Inputs::new(&db, vec![]); // a = min_iterate(Id(0))
    let b_in = Inputs::new(&db, vec![]); // b = min_iterate(Id(1))
    let c_in = Inputs::new(&db, vec![]); // c = min_panic(Id(2))
    let d_in = Inputs::new(&db, vec![]); // d = min_panic(Id(3))
    let a = Input::MinIterate(a_in);
    let b = Input::MinIterate(b_in);
    let c = Input::MinPanic(c_in);
    let d = Input::MinPanic(d_in);

    a_in.set_inputs(&mut db)
        .to(vec![b.clone(), c.clone(), d.clone()]); // a depends on b, c, d (in that order)
    b_in.set_inputs(&mut db).to(vec![a.clone()]); // b depends on a (forming a->b->a cycle)
    c_in.set_inputs(&mut db).to(vec![value(100)]); // c is independent, no cycles
    d_in.set_inputs(&mut db).to(vec![value(200)]); // d is independent, no cycles

    // First execution - this will establish the cycle and memos
    // The cycle: a depends on b, b depends on a
    // During fixpoint iteration, initial values are 255
    // a computes min(255, 100, 200) = 100
    // b computes min(100) = 100
    // Next iteration: a computes min(100, 100, 200) = 100 (converged)
    a.assert_value(&db, 100);
    b.assert_value(&db, 100);
    c.assert_value(&db, 100);
    d.assert_value(&db, 200);

    // Clear logs to prepare for the next revision
    db.clear_logs();

    // Change d's input to trigger a new revision
    // This forces verification of all dependencies in the new revision
    d_in.set_inputs(&mut db).to(vec![value(201)]);

    // Verify a - this should trigger:
    // 1. b: deep verification (cycle detected, cycle heads added to context, but b unchanged)
    // 2. c: verification (should be clean without cycle head interference)
    // 3. d: changed, causing a to re-execute
    a.assert_value(&db, 100); // min(255, 100, 201) = 100

    // Query mapping: a=min_iterate(Id(0)), b=min_iterate(Id(1)), c=min_panic(Id(2)), d=min_panic(Id(3))
    // - c gets validated cleanly during verification of `a`. The fact that `a` and `b` form a cycle shouldn't prevent that
    // - a re-executes (due to d changing)
    // - b re-executes (as part of a-b cycle)
    // - d re-executes (input changed)
    // - cycle iteration continues
    // - b re-executes again during cycle iteration
    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateMemoizedValue { database_key: min_panic(Id(2)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(3)) })",
            "salsa_event(WillExecute { database_key: min_iterate(Id(0)) })",
            "salsa_event(WillExecute { database_key: min_iterate(Id(1)) })",
            "salsa_event(WillIterateCycle { database_key: min_iterate(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: min_iterate(Id(1)) })",
            "salsa_event(DidFinalizeCycle { database_key: min_iterate(Id(0)), iteration: 1 })",
        ]"#]]);
}

/// Provisional query results in a cycle should still be cached within a single iteration.
///
/// a:Ni(v59, b) -> b:Np(v60, c, c, c) -> c:Np(a)
/// ^                                          |
/// +------------------------------------------+
#[test]
fn repeat_provisional_query() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinPanic(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![value(59), b.clone()]);
    b_in.set_inputs(&mut db)
        .to(vec![value(60), c.clone(), c.clone(), c]);
    c_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 59);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: min_iterate(Id(0)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(1)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(2)) })",
            "salsa_event(WillIterateCycle { database_key: min_iterate(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: min_panic(Id(1)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(2)) })",
            "salsa_event(DidFinalizeCycle { database_key: min_iterate(Id(0)), iteration: 1 })",
        ]"#]]);
}

#[test]
fn repeat_provisional_query_incremental() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let a_in = Inputs::new(&db, vec![]);
    let b_in = Inputs::new(&db, vec![]);
    let c_in = Inputs::new(&db, vec![]);
    let a = Input::MinIterate(a_in);
    let b = Input::MinPanic(b_in);
    let c = Input::MinPanic(c_in);
    a_in.set_inputs(&mut db).to(vec![value(59), b.clone()]);
    b_in.set_inputs(&mut db)
        .to(vec![value(60), c.clone(), c.clone(), c]);
    c_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 59);

    db.clear_logs();

    c_in.set_inputs(&mut db).to(vec![a.clone()]);

    a.assert_value(&db, 59);

    // `min_panic(Id(2)) should only run twice:
    // * Once before iterating
    // * Once as part of iterating
    //
    // If it runs more than once before iterating, than this suggests that
    // `validate_same_iteration` incorrectly returns `false`.
    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: min_iterate(Id(0)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(1)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(2)) })",
            "salsa_event(WillIterateCycle { database_key: min_iterate(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: min_panic(Id(1)) })",
            "salsa_event(WillExecute { database_key: min_panic(Id(2)) })",
            "salsa_event(DidFinalizeCycle { database_key: min_iterate(Id(0)), iteration: 1 })",
        ]"#]]);
}

/// Tests a situation where a query participating in a cycle gets called many times (think thousands of times).
///
/// We want to avoid calling `deep_verify_memo` for that query over and over again.
/// This isn't an issue for regular queries because a non-cyclic query is guaranteed to be verified
/// after `maybe_changed_after` because:
/// * It can be shallow verified
/// * `deep_verify_memo` returns `unchanged` and it updates the `verified_at` revision.
/// * `deep_verify_memo` returns `changed` and Salsa re-executes the query. The query is verified once `execute` completes.
///
/// The same guarantee doesn't exist for queries participating in cycles because:
///
/// * Salsa update `verified_at` because it depends on the cycle head if the query didn't change.
/// * Salsa doesn't run `execute` because some inputs may not have been verified yet (which can lead to all sort of pancis).
#[test]
fn repeat_query_participating_in_cycle() {
    #[salsa::input]
    struct Input {
        #[returns(copy)]
        value: u32,
        #[returns(copy)]
        stable: u32,
    }

    #[salsa::interned]
    struct Interned {
        #[returns(copy)]
        value: u32,
    }

    #[salsa::tracked(returns(copy), cycle_initial=initial)]
    fn head(db: &dyn Db, input: Input) -> u32 {
        let a = query_a(db, input);

        a.min(2)
    }

    fn initial(_db: &dyn Db, _id: salsa::Id, _input: Input) -> u32 {
        0
    }

    #[salsa::tracked(returns(copy))]
    fn query_a(db: &dyn Db, input: Input) -> u32 {
        let _ = query_b(db, input);

        query_hot(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_b(db: &dyn Db, input: Input) -> u32 {
        let _ = query_c(db, input);

        query_hot(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_c(db: &dyn Db, input: Input) -> u32 {
        let _ = query_d(db, input);

        query_hot(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_d(db: &dyn Db, input: Input) -> u32 {
        query_hot(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_hot(db: &dyn Db, input: Input) -> u32 {
        let value = head(db, input);

        let _ = input.stable(db);

        let _ = Interned::new(db, 2);

        let _ = input.value(db);

        value + 1
    }

    let mut db = ExecuteValidateLoggerDatabase::default();

    let input = Input::new(&db, 1, 0);

    assert_eq!(head(&db, input), 2);

    db.clear_logs();

    input.set_value(&mut db).to(10);

    assert_eq!(head(&db, input), 2);

    // The interned value should only be validate once. We otherwise have a
    // run-away situation where `deep_verify_memo` of `query_hot` is called over and over again.
    // * First: when checking if `head` has changed
    // * Second: when checking if `query_a` has changed
    // * Third: when checking if `query_b` has changed
    // * ...
    // Ultimately, this can easily be more expensive than running the cycle head again.
    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateInternedValue { key: Interned(Id(80)), revision: R2 })",
            "salsa_event(WillExecute { database_key: head(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_hot(Id(0)) })",
            "salsa_event(WillIterateCycle { database_key: head(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_hot(Id(0)) })",
            "salsa_event(WillIterateCycle { database_key: head(Id(0)), iteration: 2 })",
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_hot(Id(0)) })",
            "salsa_event(DidFinalizeCycle { database_key: head(Id(0)), iteration: 2 })",
        ]"#]]);
}

/// Tests a similar scenario as `repeat_query_participating_in_cycle` with the main difference
/// that `query_hot` is called before calling the next `query_xxx`.
#[test]
fn repeat_query_participating_in_cycle2() {
    #[salsa::input]
    struct Input {
        #[returns(copy)]
        value: u32,
        #[returns(copy)]
        stable: u32,
    }

    #[salsa::interned]
    struct Interned {
        #[returns(copy)]
        value: u32,
    }

    #[salsa::tracked(returns(copy), cycle_initial=initial)]
    fn head(db: &dyn Db, input: Input) -> u32 {
        let a = query_a(db, input);

        a.min(2)
    }

    fn initial(_db: &dyn Db, _id: salsa::Id, _input: Input) -> u32 {
        0
    }

    #[salsa::tracked(returns(copy), cycle_initial=initial)]
    fn query_a(db: &dyn Db, input: Input) -> u32 {
        let _ = query_hot(db, input);
        query_b(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_b(db: &dyn Db, input: Input) -> u32 {
        let _ = query_hot(db, input);
        query_c(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_c(db: &dyn Db, input: Input) -> u32 {
        let _ = query_hot(db, input);
        query_d(db, input)
    }

    #[salsa::tracked(returns(copy))]
    fn query_d(db: &dyn Db, input: Input) -> u32 {
        let _ = query_hot(db, input);

        let value = head(db, input);
        let _ = input.value(db);

        value + 1
    }

    #[salsa::tracked(returns(copy))]
    fn query_hot(db: &dyn Db, input: Input) -> u32 {
        let _ = input.stable(db);

        let _ = Interned::new(db, 2);

        let _ = head(db, input);

        1
    }

    let mut db = ExecuteValidateLoggerDatabase::default();

    let input = Input::new(&db, 1, 0);

    assert_eq!(head(&db, input), 2);

    db.clear_logs();

    input.set_value(&mut db).to(10);

    assert_eq!(head(&db, input), 2);

    // `DidValidateInternedValue { key: Interned(Id(100)), revision: R2 }` should only be logged
    // once per `maybe_changed_after` root-call (e.g. validating `head` shouldn't validate `query_hot` multiple times).
    //
    // This is important to avoid a run-away situation where a query is called many times within a cycle and
    // Salsa would end up recusively validating the hot query over and over again.
    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateInternedValue { key: Interned(Id(80)), revision: R2 })",
            "salsa_event(WillExecute { database_key: head(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_hot(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(0)) })",
            "salsa_event(WillIterateCycle { database_key: head(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_hot(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(0)) })",
            "salsa_event(WillIterateCycle { database_key: head(Id(0)), iteration: 2 })",
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_hot(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(0)) })",
            "salsa_event(DidFinalizeCycle { database_key: head(Id(0)), iteration: 2 })",
        ]"#]]);
}

#![cfg(feature = "inventory")]

//! Test for stale cycle heads when nested cycles are discovered incrementally.
//!
//! Scenario from ty:
/// ```txt
/// E -> C -> D -> B -> A -> B (cycle)
///                     -- A completes, heads = [B]
/// E -> C -> D -> B -> C (cycle)
///                  -> D (cycle)
///                -- B completes, heads = [B, C, D]
/// E -> C -> D -> E (cycle)
///           -- D completes, heads = [E, B, C, D]
/// E -> C
///      -- C completes, heads = [E, B, C, D]
/// E -> X -> A
///      -- X completes, heads = [B]
/// ```
///
/// Note how `X` only depends on `B`, but not on `E`, unless we collect the cycle heads transitively,
/// which is what this test is asserting.

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

// Outer cycle head - should iterate
#[salsa::tracked(returns(copy), cycle_initial = initial_zero)]
fn query_e(db: &dyn salsa::Database, input: Input) -> u32 {
    // First call C to establish the nested cycles
    let c_val = query_c(db, input);

    // Then later call X which will read A with stale cycle heads
    // By this point, A has already completed and memoized with cycle_heads=[B]
    // But E is still on the stack
    let x_val = query_x(db, input);

    c_val.min(x_val)
}

#[salsa::tracked(returns(copy), cycle_initial = initial_zero)]
fn query_c(db: &dyn salsa::Database, input: Input) -> u32 {
    query_d(db, input)
}

#[salsa::tracked(returns(copy), cycle_initial = initial_zero)]
fn query_d(db: &dyn salsa::Database, input: Input) -> u32 {
    let b_val = query_b(db, input);

    // Create cycle back to E
    let e_val = query_e(db, input);

    b_val.min(e_val)
}

#[salsa::tracked(returns(copy), cycle_initial = initial_zero)]
fn query_b(db: &dyn salsa::Database, input: Input) -> u32 {
    // First call A - this will detect A<->B cycle and A will complete
    let a_val = query_a(db, input);

    let c_val = query_c(db, input);
    let d_val = query_d(db, input);

    // Then read C - this reveals B is part of C's cycle
    (a_val + d_val + c_val).min(50)
}

#[salsa::tracked(returns(copy), cycle_initial = initial_zero)]
fn query_a(db: &dyn salsa::Database, input: Input) -> u32 {
    // Read B to create A<->B cycle
    let b_val = query_b(db, input);

    // Also read input
    let val = input.value(db);

    b_val.max(val)
}

#[salsa::tracked(returns(copy), cycle_initial = initial_zero)]
fn query_x(db: &dyn salsa::Database, input: Input) -> u32 {
    // This reads A's memoized result which has stale cycle_heads
    query_a(db, input)
}

fn initial_zero(_db: &dyn salsa::Database, _id: salsa::Id, _input: Input) -> u32 {
    0
}

#[test]
fn run() {
    let db = salsa::DatabaseImpl::new();
    let input = Input::new(&db, 50);

    let result = query_e(&db, input);

    assert_eq!(result, 0);
}

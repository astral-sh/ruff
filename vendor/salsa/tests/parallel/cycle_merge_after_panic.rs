//! A poisoned cycle head must not be merged with an earlier provisional iteration.
//!
//! This models the overlapping attribute/range cycles from astral-sh/ty#4077. Each scope contains
//! sixteen expressions in chunks of four, giving the following dependency shape:
//!
//! ```text
//! attribute(scope, index, false)
//! ├─ attribute(scope, small_index, true)
//! └─ once the provisional value reaches 3:
//!    attribute(other_scope, late_index, true)
//!    └─ range(other_scope, preceding_chunk)
//!       └─ attribute(other_scope, chunk_expressions, true)
//! ```
//!
//! Four threads enter different attribute queries together. An overlapping range cycle eventually
//! becomes a head and its recovery panics. Unwinding poisons provisional memos with `None` at
//! iteration 0 while another thread can still hold an attribute head from iteration 2. That thread
//! must propagate the panic, not merge the two iterations. All dependencies are determined by
//! tracked arguments and provisional results; there is no thread-local query state.

#![cfg(not(feature = "shuttle"))]

use salsa::Database;
use std::marker::PhantomData;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Barrier};
use std::thread;

const NUM_SCOPES: u32 = 3;
const NUM_EXPRESSIONS: u32 = 16;
const CHUNK_SIZE: u32 = 4;
const FIXPOINT_LIMIT: u32 = 3;
const NUM_WORKERS: usize = 4;

struct StaticClassLiteral<'db>(PhantomData<&'db ()>);

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    #[salsa::tracked(
        returns(copy),
        cycle_fn = |_, _, previous: &u32, _, _, _, _| (previous + 1).min(FIXPOINT_LIMIT),
        cycle_initial = |_, _, _, _, _| 0
    )]
    fn implicit_attribute_inner(db: &'db dyn Database, scope: u32, index: u32, infer: bool) -> u32 {
        if infer {
            for chunk in 0..index / CHUNK_SIZE {
                analyze_non_terminal_call_range(db, scope, chunk);
            }
            let target_scope = (scope + 1 + (index + 1) % (NUM_SCOPES - 1)) % NUM_SCOPES;
            Self::implicit_attribute_inner(db, target_scope, index % 7, false)
        } else {
            let first = Self::implicit_attribute_inner(db, scope, (index + 2) % 3, true);
            if first >= FIXPOINT_LIMIT {
                let expression = NUM_EXPRESSIONS - 1 - (3 * index) % (NUM_EXPRESSIONS / 2);
                let other_scope = (scope + 1 + index % NUM_SCOPES) % NUM_SCOPES;
                Self::implicit_attribute_inner(db, other_scope, expression, true);
            }

            (first + 1).min(FIXPOINT_LIMIT)
        }
    }
}

#[salsa::tracked(
    returns(copy),
    cycle_fn = |_, _, _, _, _, _| panic!("range cycle"),
    cycle_initial = |_, _, _, _| ()
)]
fn analyze_non_terminal_call_range(db: &dyn Database, scope: u32, chunk: u32) {
    for expression in chunk * CHUNK_SIZE..(chunk + 1) * CHUNK_SIZE {
        StaticClassLiteral::implicit_attribute_inner(db, scope, expression, true);
    }
}

fn attempt() {
    let db = salsa::DatabaseImpl::default();
    let barrier = Arc::new(Barrier::new(NUM_WORKERS));
    let mut threads = Vec::with_capacity(NUM_WORKERS);

    for worker in 0..NUM_WORKERS {
        let db = db.clone();
        let barrier = barrier.clone();
        threads.push(thread::spawn(move || {
            barrier.wait();

            let scope = worker as u32 % NUM_SCOPES;
            panic::catch_unwind(AssertUnwindSafe(|| {
                StaticClassLiteral::implicit_attribute_inner(&db, scope, worker as u32, false)
            }))
        }));
    }

    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    let mut range_panics = 0;
    for result in results {
        match result {
            Ok(_) => {}
            Err(error) if error.is::<salsa::Cancelled>() => {}
            Err(error)
                if error
                    .downcast_ref::<&str>()
                    .is_some_and(|message| *message == "range cycle") =>
            {
                range_panics += 1;
            }
            Err(error) => panic::resume_unwind(error),
        }
    }

    assert!(range_panics > 0, "the range recovery was never invoked");
}

#[test]
fn cycle_heads_from_different_iterations() {
    let count = if cfg!(miri) { 1 } else { 100 };
    for _ in 0..count {
        attempt();
    }
}

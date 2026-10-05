//! Observes retained children from their first poll until their futures are dropped.
//!
//! Child IDs start at one after each reset. Entry cancellation and cancellation after the first
//! actual `Pending` are independently configurable. The bounded journal stores no child owner.

use std::cell::Cell;
use std::future::{Future, poll_fn};

use crate::Db;

/// Identifies a retained child from entry through its actual future's retirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    Entered(usize),
    /// The first `Pending` returned by this child's underlying future.
    Pending(usize),
    Retired(usize),
}

/// A bounded child log; `overflowed` must be false before treating it as complete evidence.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct Snapshot {
    pub(in crate::types) events: [Option<Event>; 256],
    pub(in crate::types) count: usize,
    pub(in crate::types) overflowed: bool,
}

impl Snapshot {
    const fn new() -> Self {
        Self { events: [None; 256], count: 0, overflowed: false }
    }
}

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static ENTERED: Cell<usize> = const { Cell::new(0) };
    static POLLING: Cell<usize> = const { Cell::new(0) };
    static MAX_POLLING: Cell<usize> = const { Cell::new(0) };
    static CANCEL_AT: Cell<Option<usize>> = const { Cell::new(None) };
    static CANCEL_AT_PENDING: Cell<Option<usize>> = const { Cell::new(None) };
    static JOURNAL: Cell<Snapshot> = const { Cell::new(Snapshot::new()) };
}

/// Resets child IDs and both cancellation settings after every observed child has retired.
pub(in crate::types) fn reset(cancel_at: Option<usize>) {
    assert_eq!(LIVE.get(), 0);
    assert_eq!(POLLING.get(), 0);
    ENTERED.set(0);
    MAX_POLLING.set(0);
    CANCEL_AT.set(cancel_at);
    CANCEL_AT_PENDING.set(None);
    JOURNAL.set(Snapshot::new());
}

/// Sets cancellation after the selected child's first actual `Pending`, without changing entry cancellation.
pub(in crate::types) fn set_cancel_at_pending(child: Option<usize>) {
    CANCEL_AT_PENDING.set(child);
}

pub(in crate::types) fn snapshot() -> Snapshot {
    JOURNAL.get()
}

fn record(event: Event) {
    let mut snapshot = JOURNAL.get();
    if let Some(slot) = snapshot.events.get_mut(snapshot.count) {
        *slot = Some(event);
        snapshot.count += 1;
    } else {
        snapshot.overflowed = true;
    }
    JOURNAL.set(snapshot);
    // Signature scopes use the same event order even if this independent journal is full.
    super::super::signature_observations::child(event);
}

/// Returns live children, total entered children, and maximum overlapping poll calls on this thread.
pub(in crate::types) fn progress() -> (usize, usize, usize) {
    (LIVE.get(), ENTERED.get(), MAX_POLLING.get())
}

/// Records retirement after the subsequently declared child future has been dropped.
struct Lifetime(usize);

impl Drop for Lifetime {
    fn drop(&mut self) {
        LIVE.set(LIVE.get() - 1);
        record(Event::Retired(self.0));
    }
}

/// Balances the count of active poll calls without declaring the child retired.
struct Poll;

impl Drop for Poll {
    fn drop(&mut self) {
        POLLING.set(POLLING.get() - 1);
    }
}

/// Forwards the child future while observing its lifetime and first pending result.
/// Configured cancellation is triggered at entry or after that first pending result.
pub(super) async fn observe<F: Future>(db: &dyn Db, operation: F) -> F::Output {
    LIVE.set(LIVE.get() + 1);
    ENTERED.set(ENTERED.get() + 1);
    let child = ENTERED.get();
    let _lifetime = Lifetime(child);
    record(Event::Entered(child));
    if CANCEL_AT.get() == Some(ENTERED.get()) {
        CANCEL_AT.set(None);
        db.cancellation_token().cancel();
    }
    let mut operation = std::pin::pin!(operation);
    let mut pending_observed = false;
    poll_fn(|cx| {
        POLLING.set(POLLING.get() + 1);
        MAX_POLLING.set(MAX_POLLING.get().max(POLLING.get()));
        let _poll = Poll;
        let result = operation.as_mut().poll(cx);
        if result.is_pending() && !pending_observed {
            pending_observed = true;
            record(Event::Pending(child));
            if CANCEL_AT_PENDING.get() == Some(child) {
                CANCEL_AT_PENDING.set(None);
                db.cancellation_token().cancel();
            }
        }
        result
    })
    .await
}

//! Records signature transfer boundaries and cleanup without retaining semantic owners.
//!
//! Original parameter arrays are observed through weak references in the comparison frame. This journal
//! keeps only their strong counts and actual retained-child events. Check `overflowed` before
//! treating the bounded log as complete evidence.

use std::cell::Cell;

use super::super::retained::observations::Event as ChildEvent;
use crate::Db;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Stage {
    BeforeClone,
    AfterClone,
    BeforeNormalize,
    AfterNormalize,
    BeforeTransfer,
    BeforeTransferCompletion,
    AfterTransfer,
    BeforePrefix,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    Boundary { stage: Stage, remaining_work: Option<usize> },
    StorageStarted { source: usize, target: usize },
    StorageNormalized { source: usize, target: usize },
    /// The comparison's storage-observation scope has ended; the original arrays may remain alive.
    StorageRetired { source: usize, target: usize },
    Child(ChildEvent),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct Snapshot {
    pub(in crate::types) events: [Option<Event>; 128],
    pub(in crate::types) count: usize,
    pub(in crate::types) overflowed: bool,
    pub(in crate::types) active_scopes: usize,
}

impl Snapshot {
    const fn new() -> Self {
        Self { events: [None; 128], count: 0, overflowed: false, active_scopes: 0 }
    }
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static JOURNAL: Cell<Snapshot> = const { Cell::new(Snapshot::new()) };
    static CANCEL_AT: Cell<Option<Stage>> = const { Cell::new(None) };
    static INCOMPLETE_AT: Cell<Option<Stage>> = const { Cell::new(None) };
    static TRANSFER_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Starts recording and optionally cancels at the first matching transfer boundary.
pub(in crate::types) fn reset(cancel_at: Option<Stage>) {
    assert_eq!(JOURNAL.get().active_scopes, 0);
    JOURNAL.set(Snapshot::new());
    CANCEL_AT.set(cancel_at);
    INCOMPLETE_AT.set(None);
    TRANSFER_PENDING.set(false);
    ENABLED.set(true);
}

/// Latches incompleteness at a boundary so the following production completion check can refuse.
pub(in crate::types) fn set_incomplete_at(stage: Option<Stage>) {
    INCOMPLETE_AT.set(stage);
}

pub(in crate::types) fn is_recording() -> bool {
    ENABLED.get()
}

/// Stops recording after all observed parameter scopes have retired.
pub(in crate::types) fn stop() {
    assert_eq!(JOURNAL.get().active_scopes, 0);
    ENABLED.set(false);
}

pub(in crate::types) fn snapshot() -> Snapshot {
    JOURNAL.get()
}

fn record(event: Event) {
    if !ENABLED.get() {
        return;
    }
    let mut snapshot = JOURNAL.get();
    if let Some(slot) = snapshot.events.get_mut(snapshot.count) {
        *slot = Some(event);
        snapshot.count += 1;
    } else {
        snapshot.overflowed = true;
    }
    JOURNAL.set(snapshot);
}

/// Captures remaining work and applies a configured interruption before the next admitted action.
pub(super) fn boundary(db: &dyn Db, stage: Stage) {
    if !ENABLED.get() {
        return;
    }
    record(Event::Boundary { stage, remaining_work: salsa::attempt_probe::remaining_allowance_for_diagnostics(db) });
    if stage == Stage::BeforeTransfer {
        TRANSFER_PENDING.set(true);
    }
    if CANCEL_AT.get() == Some(stage) {
        CANCEL_AT.set(None);
        db.cancellation_token().cancel();
    }
    if INCOMPLETE_AT.get() == Some(stage) {
        INCOMPLETE_AT.set(None);
        salsa::attempt_probe::report_incomplete(db, salsa::attempt_probe::Incomplete::Allowance);
    }
}

/// Observes the transfer's completion barrier after its work and byte admissions succeed.
pub(super) fn admitted(db: &dyn Db) {
    if TRANSFER_PENDING.replace(false) {
        boundary(db, Stage::BeforeTransferCompletion);
    }
}

pub(in crate::types) fn storage_started(source: usize, target: usize) {
    let mut snapshot = JOURNAL.get();
    snapshot.active_scopes += 1;
    JOURNAL.set(snapshot);
    record(Event::StorageStarted { source, target });
}

pub(in crate::types) fn storage_normalized(source: usize, target: usize) {
    record(Event::StorageNormalized { source, target });
}

pub(in crate::types) fn storage_retired(source: usize, target: usize) {
    let mut snapshot = JOURNAL.get();
    snapshot.active_scopes -= 1;
    JOURNAL.set(snapshot);
    record(Event::StorageRetired { source, target });
}

pub(in crate::types) fn child(event: ChildEvent) {
    record(Event::Child(event));
}

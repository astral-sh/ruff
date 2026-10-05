//! Observes native callable and relation cleanup without retaining either owner.
//!
//! Source inference reports child-builder destruction through `source_child_dropped`. The shared
//! event sequence lets tests check that a conversion's suspended child drains before its enclosing
//! callable and relation scopes clean up. Only admitted callable storage and relation visitors
//! registered by `relation_conversion` contribute scope events. Tests must check
//! `Snapshot::overflowed` before using the bounded event log as complete evidence.

use std::cell::{Cell, RefCell};

use super::{CallableGuardStorage, CallableGuardTable, CallableRecursionGuard};

const EVENT_CAPACITY: usize = 128;
const VISITOR_CAPACITY: usize = 16;

/// Identifies an admitted storage allocation while it is alive, including when its guard moves.
/// Allocators can reuse the address after retirement; the identifier is not globally unique.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct GuardId(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct GuardState {
    pub(in crate::types) id: GuardId,
    pub(in crate::types) exact: usize,
    pub(in crate::types) identities: usize,
    pub(in crate::types) definitions: usize,
    pub(in crate::types) anchors: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    ScopeOpened(GuardState),
    ScopeDropBefore(GuardState),
    ScopeDropAfter(GuardState),
    StorageDropped {
        id: GuardId,
        /// Outstanding key weights for Exact, Identity, and DefinitionDispatch, in that order.
        outstanding_removal_weights: [usize; 3],
    },
    RelationConversion {
        visitor: usize,
        counts: (usize, usize),
    },
    RelationDropBefore {
        visitor: usize,
        counts: (usize, usize),
        had_item: bool,
    },
    RelationDropAfter {
        visitor: usize,
        counts: (usize, usize),
        had_item: bool,
    },
    SourceChildDropped {
        definition: Option<salsa::Id>,
    },
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct Snapshot {
    pub(in crate::types) events: [Option<Event>; EVENT_CAPACITY],
    pub(in crate::types) count: usize,
    pub(in crate::types) overflowed: bool,
    pub(in crate::types) active_scopes: usize,
}

struct Journal {
    snapshot: Snapshot,
    visitors: [Option<usize>; VISITOR_CAPACITY],
}

impl Journal {
    const fn new() -> Self {
        Self {
            snapshot: Snapshot {
                events: [None; EVENT_CAPACITY],
                count: 0,
                overflowed: false,
                active_scopes: 0,
            },
            visitors: [None; VISITOR_CAPACITY],
        }
    }

    fn record(&mut self, event: Event) {
        if let Some(slot) = self.snapshot.events.get_mut(self.snapshot.count) {
            *slot = Some(event);
            self.snapshot.count += 1;
        } else {
            self.snapshot.overflowed = true;
        }
    }
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static JOURNAL: RefCell<Journal> = const { RefCell::new(Journal::new()) };
}

/// Starts a fresh recording after all previously observed callable scopes have closed.
pub(in crate::types) fn reset() {
    JOURNAL.with_borrow_mut(|journal| {
        assert_eq!(journal.snapshot.active_scopes, 0);
        *journal = Journal::new();
    });
    ENABLED.set(true);
}

/// Stops recording and retains the collected events, checking that observed callable scopes closed.
/// Callers must finish any storage or relation destruction they intend to observe before stopping.
pub(in crate::types) fn stop() {
    assert_eq!(snapshot().active_scopes, 0);
    ENABLED.set(false);
}

pub(in crate::types) fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(|journal| journal.snapshot)
}

/// Reads active collection lengths without changing admission or retaining the guard.
/// Ordinary guards have no admitted storage allocation and therefore return `None`.
pub(in crate::types) fn guard_state(guard: &CallableRecursionGuard<'_>) -> Option<GuardState> {
    let storage = guard.storage_admission.as_deref()?;
    Some(GuardState {
        id: GuardId(storage.as_ptr().addr()),
        exact: guard.active.seen.borrow().len(),
        identities: guard.identities.seen.borrow().len(),
        definitions: guard.growth.active.seen.borrow().len(),
        anchors: guard.growth.anchors.borrow().len(),
    })
}

pub(in crate::types::cyclic) fn scope_opened(guard: &CallableRecursionGuard<'_>) {
    if ENABLED.get()
        && let Some(state) = guard_state(guard)
    {
        JOURNAL.with_borrow_mut(|journal| {
            journal.snapshot.active_scopes += 1;
            journal.record(Event::ScopeOpened(state));
        });
    }
}

pub(in crate::types::cyclic) fn scope_drop_before(guard: &CallableRecursionGuard<'_>) {
    if ENABLED.get()
        && let Some(state) = guard_state(guard)
    {
        JOURNAL.with_borrow_mut(|journal| journal.record(Event::ScopeDropBefore(state)));
    }
}

pub(in crate::types::cyclic) fn scope_drop_after(guard: &CallableRecursionGuard<'_>) {
    if ENABLED.get()
        && let Some(state) = guard_state(guard)
    {
        JOURNAL.with_borrow_mut(|journal| {
            journal.snapshot.active_scopes -= 1;
            journal.record(Event::ScopeDropAfter(state));
        });
    }
}

pub(super) fn storage_dropped(storage: &CallableGuardStorage) {
    if ENABLED.get() {
        let outstanding_removal_weights = [
            CallableGuardTable::Exact,
            CallableGuardTable::Identity,
            CallableGuardTable::DefinitionDispatch,
        ]
        .map(|table| storage.tables[table.index()].key_weight_sum);
        JOURNAL.with_borrow_mut(|journal| {
            journal.record(Event::StorageDropped {
                id: GuardId(std::ptr::from_ref(storage).addr()),
                outstanding_removal_weights,
            });
        });
    }
}

/// Registers the borrowed relation detector's address and records its `(active, cached)` counts.
/// Native scope destruction uses this address until `reset`. Callers must keep that detector alive
/// through observation or reset before another detector can reuse its address.
pub(in crate::types) fn relation_conversion(visitor: usize, counts: (usize, usize)) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| {
            if !journal.visitors.contains(&Some(visitor)) {
                if let Some(slot) = journal.visitors.iter_mut().find(|slot| slot.is_none()) {
                    *slot = Some(visitor);
                } else {
                    journal.snapshot.overflowed = true;
                }
            }
            journal.record(Event::RelationConversion { visitor, counts });
        });
    }
}

pub(in crate::types::cyclic) fn relation_drop_before(
    visitor: usize,
    had_item: bool,
    counts: impl FnOnce() -> (usize, usize),
) -> bool {
    if !ENABLED.get() || !JOURNAL.with_borrow(|journal| journal.visitors.contains(&Some(visitor))) {
        return false;
    }
    let counts = counts();
    JOURNAL.with_borrow_mut(|journal| {
        journal.record(Event::RelationDropBefore {
            visitor,
            counts,
            had_item,
        });
    });
    true
}

pub(in crate::types::cyclic) fn relation_drop_after(
    visitor: usize,
    counts: (usize, usize),
    had_item: bool,
) {
    JOURNAL.with_borrow_mut(|journal| {
        journal.record(Event::RelationDropAfter {
            visitor,
            counts,
            had_item,
        });
    });
}

pub(in crate::types) fn source_child_dropped(definition: Option<salsa::Id>) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| {
            journal.record(Event::SourceChildDropped { definition });
        });
    }
}

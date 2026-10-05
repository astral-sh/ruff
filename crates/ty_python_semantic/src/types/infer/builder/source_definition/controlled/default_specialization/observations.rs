//! Scalar observations of stored callable/protocol admission and destruction order in runtime tests.

use std::cell::RefCell;

use crate::Db;
use crate::types::constraints::ReceiverCursorState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Boundary {
    CursorCreate,
    OwningEnqueue,
    StackSpill,
    ReusedTransfer,
    Pop,
    ReceiverStep,
    ScopeStorageCreate,
    VisitorCreate,
    ScopeEnter,
    ScopeRestore,
    ProtocolMember,
    ProtocolTypes,
    ProtocolSlot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct StackState {
    pub len: usize,
    pub capacity: usize,
    pub spilled: bool,
}

/// Length and capacity of the collector's nested-visitor vector, excluding its borrowed root visitor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ScopeState {
    pub len: usize,
    pub capacity: usize,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct Admission {
    pub boundary: Boundary,
    pub work: usize,
    pub bytes: usize,
    pub remaining_work: usize,
    pub stack: Option<StackState>,
    pub cursor: Option<ReceiverCursorState>,
    pub scopes: Option<ScopeState>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum Event {
    Before(Admission),
    Accepted {
        boundary: Boundary,
        stack: Option<StackState>,
        cursor: Option<ReceiverCursorState>,
        scopes: Option<ScopeState>,
    },
    CursorCreated(usize),
    CursorDrop {
        id: usize,
        state: ReceiverCursorState,
    },
    CollectorDrop {
        live_cursors: usize,
    },
    OwnerRetired {
        live_cursors: usize,
    },
    CancellationRequested {
        live_cursors: usize,
        pending: usize,
    },
    ScopeStorageRetired {
        state: ScopeState,
    },
    ProtocolCancellationRequested {
        scopes: ScopeState,
        cursor: ReceiverCursorState,
        pending: usize,
    },
}

#[derive(Clone, Debug, Default)]
pub(in crate::types) struct Snapshot {
    pub events: Vec<Event>,
    pub live_cursors: usize,
    pub scopes: Option<ScopeState>,
    next_cursor: usize,
    pending: usize,
    enabled: bool,
    cancel_populated_step: bool,
    cancel_protocol_step: bool,
}

thread_local! {
    static JOURNAL: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

/// Enables observations until dropped; cancellation targets an admitted populated cursor step.
#[derive(Debug)]
pub(in crate::types) struct Recording;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Cancellation {
    Never,
    PopulatedReceiver,
    NestedProtocolReceiver,
}

impl Recording {
    pub fn start(cancellation: Cancellation) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(!journal.enabled);
            assert_eq!(journal.live_cursors, 0);
            assert_eq!(journal.scopes, None);
            *journal = Snapshot {
                enabled: true,
                cancel_populated_step: cancellation == Cancellation::PopulatedReceiver,
                cancel_protocol_step: cancellation == Cancellation::NestedProtocolReceiver,
                ..Snapshot::default()
            };
        });
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| {
            journal.enabled = false;
            journal.cancel_populated_step = false;
            journal.cancel_protocol_step = false;
        });
    }
}

pub(in crate::types) fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(Clone::clone)
}

/// Records the carrier and its next fixed or stack-storage quote before admission.
///
/// Receiver-step quotes cover the returned type pair; `OwnedConstraintTypeCursor::next_with`
/// separately admits retained-node work and seen-table growth through `TddControl`. Cursor creation
/// and enqueue also bracket a `StackSpill` or `ReusedTransfer` event. Those overlapping observations
/// describe the same stack admission and must not be summed as separate charges.
pub(in crate::types) fn before(
    db: &dyn Db,
    boundary: Boundary,
    work: usize,
    bytes: usize,
    stack: Option<StackState>,
    cursor: Option<ReceiverCursorState>,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.enabled {
            journal.events.push(Event::Before(Admission {
                boundary,
                work,
                bytes,
                remaining_work: salsa::attempt_probe::remaining_allowance_for_diagnostics(db)
                    .expect("a recorded admission belongs to an active attempt"),
                stack,
                cursor,
                scopes: journal.scopes,
            }));
        }
    });
}

/// Records a completed mutation, optionally requesting native cancellation at this boundary.
pub(in crate::types) fn accepted(
    db: &dyn Db,
    boundary: Boundary,
    stack: Option<StackState>,
    cursor: Option<ReceiverCursorState>,
) {
    let cancel = JOURNAL.with_borrow_mut(|journal| {
        if !journal.enabled {
            return false;
        }
        if let Some(stack) = stack {
            journal.pending = stack.len;
        }
        journal.events.push(Event::Accepted {
            boundary,
            stack,
            cursor,
            scopes: journal.scopes,
        });
        if journal.cancel_populated_step
            && boundary == Boundary::ReceiverStep
            && cursor.is_some_and(|cursor| cursor.seen_len > 0)
            && journal.live_cursors > 1
            && journal.pending > 0
        {
            journal.cancel_populated_step = false;
            journal.events.push(Event::CancellationRequested {
                live_cursors: journal.live_cursors,
                pending: journal.pending,
            });
            true
        } else if journal.cancel_protocol_step
            && boundary == Boundary::ReceiverStep
            && let Some(scopes) = journal.scopes
            && scopes.len >= 2
            && let Some(cursor) = cursor
            && cursor.seen_len > 0
            && journal.pending > 0
        {
            journal.cancel_protocol_step = false;
            journal.events.push(Event::ProtocolCancellationRequested {
                scopes,
                cursor,
                pending: journal.pending,
            });
            true
        } else {
            false
        }
    });
    if cancel {
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
    }
}

/// Records a protocol or visitor-scope quote before its operation can mutate stored state.
/// `Some(scopes)` supplies its current vector state; `None` reuses the last observed state.
pub(in crate::types) fn before_protocol(
    db: &dyn Db,
    boundary: Boundary,
    work: usize,
    bytes: usize,
    scopes: Option<ScopeState>,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.enabled {
            journal.events.push(Event::Before(Admission {
                boundary,
                work,
                bytes,
                remaining_work: salsa::attempt_probe::remaining_allowance_for_diagnostics(db)
                    .expect("a recorded admission belongs to an active attempt"),
                stack: None,
                cursor: None,
                scopes: scopes.or(journal.scopes),
            }));
        }
    });
}

/// Records a completed protocol or visitor operation and its scope-vector state.
/// `Some(scopes)` supplies the resulting state; `None` preserves the last observed state.
pub(in crate::types) fn accepted_protocol(
    _db: &dyn Db,
    boundary: Boundary,
    scopes: Option<ScopeState>,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.enabled {
            journal.scopes = scopes.or(journal.scopes);
            journal.events.push(Event::Accepted {
                boundary,
                stack: None,
                cursor: None,
                scopes: journal.scopes,
            });
        }
    });
}

/// Observes scope storage after destruction; retain this guard across the collector invocation
/// that owns the scope vector, then drop it after that invocation returns or is drained.
#[derive(Debug)]
pub(in crate::types) struct ScopeLifetime(bool);

/// Starts observation of the scope vector owned by the next collector invocation.
pub(in crate::types) fn scopes_started() -> ScopeLifetime {
    ScopeLifetime(JOURNAL.with_borrow(|journal| journal.enabled))
}

impl Drop for ScopeLifetime {
    fn drop(&mut self) {
        if self.0 {
            JOURNAL.with_borrow_mut(|journal| {
                if let Some(state) = journal.scopes.take() {
                    journal.events.push(Event::ScopeStorageRetired { state });
                }
            });
        }
    }
}

/// Assigns a scalar identity so moving a cursor cannot hide repeated destructor observations.
pub(in crate::types) fn receiver_cursor_created() -> Option<usize> {
    JOURNAL.with_borrow_mut(|journal| {
        if !journal.enabled {
            return None;
        }
        let id = journal.next_cursor;
        journal.next_cursor += 1;
        journal.live_cursors += 1;
        journal.events.push(Event::CursorCreated(id));
        Some(id)
    })
}

/// Records destructor entry only; the cursor's hash storage is dropped after this hook returns.
pub(in crate::types) fn receiver_cursor_drop(id: Option<usize>, state: ReceiverCursorState) {
    let Some(id) = id else { return };
    JOURNAL.with_borrow_mut(|journal| {
        journal.live_cursors -= 1;
        journal.events.push(Event::CursorDrop { id, state });
    });
}

/// Observes the enclosing collector scope after its later-declared pending stack is dropped.
#[derive(Debug)]
pub(in crate::types) struct CollectorLifetime(bool);

pub(in crate::types) fn collector_started() -> CollectorLifetime {
    CollectorLifetime(JOURNAL.with_borrow(|journal| journal.enabled))
}

impl Drop for CollectorLifetime {
    fn drop(&mut self) {
        if self.0 {
            JOURNAL.with_borrow_mut(|journal| {
                journal.events.push(Event::CollectorDrop {
                    live_cursors: journal.live_cursors,
                });
            });
        }
    }
}

/// Marks the test entry's enclosing scope; declare it before the source builder owner.
#[derive(Debug)]
pub(in crate::types) struct OwnerLifetime;

pub(in crate::types) const fn owner_started() -> OwnerLifetime {
    OwnerLifetime
}

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        owner_retired();
    }
}

/// Observes the source test entry after its collector future and builder owner have retired.
fn owner_retired() {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.enabled {
            journal.events.push(Event::OwnerRetired {
                live_cursors: journal.live_cursors,
            });
        }
    });
}

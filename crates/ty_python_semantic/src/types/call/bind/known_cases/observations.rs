//! Observes the dataclass-transform helper and its enclosing call without retaining either owner.
//!
//! A helper lifetime ends when its future releases its locals. This does not imply that the
//! field-specifier payload is freed: successful interning transfers that payload to Salsa.
//! A recording may also request cancellation at one selected stage, for the runtime to observe
//! at its next cancellation boundary.

use std::cell::RefCell;

use salsa::Database;
use salsa::attempt_probe::remaining_allowance_for_diagnostics;
use salsa::plumbing::AsId;

use crate::Db;
use crate::types::Type;
use crate::types::call::bind::ownership::observations::{
    self as owners, RetirementEvent, RetirementObserver, RetirementPhase,
};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::cyclic::guard_storage::observations as guards;

/// Identifies a boundary before a subsequent admitted operation or after return assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Stage {
    BeforeBuffer,
    BufferReady,
    Populated,
    BeforeInterner,
    BeforeReturn,
    Returned,
}

/// Records progress and destruction without retaining semantic values.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum Event {
    Stage {
        stage: Stage,
        len: usize,
        remaining_work: usize,
    },
    HelperStarted,
    HelperEnded {
        guard_retired: bool,
    },
    Suspended {
        helper_live: bool,
        guard_retired: bool,
    },
    BindingsRetired {
        phase: RetirementPhase,
        helper_live: bool,
        guard_retired: bool,
    },
}

#[derive(Clone, Debug, Default)]
pub(in crate::types) struct Snapshot {
    pub(in crate::types) events: Vec<Event>,
    pub(in crate::types) guard: Option<guards::GuardId>,
    pub(in crate::types) live_helpers: usize,
}

impl Snapshot {
    pub(in crate::types) fn reached(&self, target: Stage) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event, Event::Stage { stage, .. } if *stage == target))
    }

    /// Returns the remaining semantic-work allowance at the first occurrence of this stage.
    pub(in crate::types) fn remaining_at(&self, target: Stage) -> Option<usize> {
        self.events.iter().find_map(|event| match event {
            Event::Stage {
                stage,
                remaining_work,
                ..
            } if *stage == target => Some(*remaining_work),
            _ => None,
        })
    }
}

#[derive(Default)]
struct Journal {
    database: Option<usize>,
    callable: Option<salsa::Id>,
    cancel_at: Option<Stage>,
    snapshot: Snapshot,
}

thread_local! {
    static JOURNAL: RefCell<Journal> = RefCell::new(Journal::default());
}

/// Restricts recording to one database and optionally one enclosing callable's destruction.
#[derive(Debug)]
pub(in crate::types) struct Recording {
    previous: Option<RetirementObserver>,
}

impl Recording {
    /// Starts a fresh recording, optionally arming one cancellation request at `cancel_at`.
    /// The request selects this database; interruption occurs at its next runtime boundary.
    pub(in crate::types) fn start(
        db: &dyn Db,
        callable: Option<salsa::Id>,
        cancel_at: Option<Stage>,
    ) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.database.is_none());
            *journal = Journal {
                database: Some(std::ptr::from_ref(db.zalsa()).addr()),
                callable,
                cancel_at,
                snapshot: Snapshot::default(),
            };
        });
        guards::reset();
        Self {
            previous: owners::set_retirement_observer(Some(retired)),
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        owners::set_retirement_observer(self.previous);
        guards::stop();
        JOURNAL.with_borrow_mut(|journal| journal.database = None);
    }
}

pub(in crate::types) fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(|journal| journal.snapshot.clone())
}

/// Identifies the admitted supplied guard when no scope event exposes its ID: direct function
/// signatures need not open the guard scopes that `stage` otherwise uses to discover the guard.
pub(in crate::types) fn supplied_guard(guard: &CallableRecursionGuard<'_>) {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.database.is_some() {
            journal.snapshot.guard = guards::guard_state(guard).map(|state| state.id);
        }
    });
}

fn guard_retired(id: Option<guards::GuardId>) -> bool {
    guards::snapshot().events.iter().flatten().any(|event| {
        matches!(event, guards::Event::StorageDropped { id: retired, .. } if Some(*retired) == id)
    })
}

/// Records matching bindings' destruction alongside the helper and guard lifetime state.
fn retired(event: RetirementEvent<'_>) {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.database.is_none()
            || !journal.snapshot.reached(Stage::BeforeBuffer)
            || !matches!(event.callable_type, Type::FunctionLiteral(function) if Some(function.as_id()) == journal.callable)
        {
            return;
        }
        journal.snapshot.events.push(Event::BindingsRetired {
            phase: event.phase,
            helper_live: journal.snapshot.live_helpers != 0,
            guard_retired: guard_retired(journal.snapshot.guard),
        });
    });
}

/// Records controlled calls in the selected database and requests any cancellation armed here.
/// Calls outside the controlled runtime are excluded; the next runtime boundary observes cancellation.
pub(in crate::types) fn stage(db: &dyn Db, stage: Stage, len: usize) {
    let Some(remaining_work) = remaining_allowance_for_diagnostics(db) else {
        return;
    };
    let cancel =
        JOURNAL.with_borrow_mut(|journal| {
            if journal.database != Some(std::ptr::from_ref(db.zalsa()).addr()) {
                return false;
            }
            if journal.snapshot.guard.is_none() {
                journal.snapshot.guard = guards::snapshot().events.iter().flatten().rev().find_map(
                    |event| match event {
                        guards::Event::ScopeOpened(state) if !guard_retired(Some(state.id)) => {
                            Some(state.id)
                        }
                        _ => None,
                    },
                );
            }
            journal.snapshot.events.push(Event::Stage {
                stage,
                len,
                remaining_work,
            });
            if journal.cancel_at == Some(stage) {
                journal.cancel_at = None;
                true
            } else {
                false
            }
        });
    if cancel {
        db.cancellation_token().cancel();
    }
}

/// Marks an actual `Pending` from the invocation while its future still owns all its captures.
pub(in crate::types) fn suspended() {
    JOURNAL.with_borrow_mut(|journal| {
        if journal.database.is_some() && journal.snapshot.reached(Stage::BeforeBuffer) {
            journal.snapshot.events.push(Event::Suspended {
                helper_live: journal.snapshot.live_helpers != 0,
                guard_retired: guard_retired(journal.snapshot.guard),
            });
        }
    });
}

/// Observes the helper's local-owner lifetime, including after payload transfer to the interner.
#[derive(Debug)]
pub(in crate::types) struct BufferLifetime(bool);

impl BufferLifetime {
    pub(in crate::types) fn new(db: &dyn Db) -> Self {
        let recording = JOURNAL.with_borrow_mut(|journal| {
            if journal.database != Some(std::ptr::from_ref(db.zalsa()).addr())
                || remaining_allowance_for_diagnostics(db).is_none()
            {
                return false;
            }
            journal.snapshot.live_helpers += 1;
            journal.snapshot.events.push(Event::HelperStarted);
            true
        });
        Self(recording)
    }
}

impl Drop for BufferLifetime {
    fn drop(&mut self) {
        if self.0 {
            JOURNAL.with_borrow_mut(|journal| {
                journal.snapshot.live_helpers -= 1;
                journal.snapshot.events.push(Event::HelperEnded {
                    guard_retired: guard_retired(journal.snapshot.guard),
                });
            });
        }
    }
}

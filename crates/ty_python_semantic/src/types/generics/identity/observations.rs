//! Passive events for admitted identity collection and retirement of its local owner.

use std::cell::RefCell;

use super::IdentityStorage;
use crate::Db;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    BeforeAppend(IdentityStorage),
    AfterAppend(IdentityStorage),
    BeforeBox(IdentityStorage),
    /// The local boxing callback has produced the slice for the later specialization child.
    BoxTransferred,
    /// The collection owner retired; true means its Vec moved into the boxing callback.
    ArgumentsRetired { transferred: bool },
    OperationRetired,
}

#[derive(Debug)]
struct Trace {
    db: usize,
    events: Vec<Event>,
}

thread_local! {
    static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

/// Starts a journal filtered to one database; dropping it removes all passive observation state.
#[derive(Debug)]
pub(in crate::types) struct Recording;

impl Recording {
    pub(in crate::types) fn start(db: &dyn Db) -> Self {
        TRACE.with_borrow_mut(|trace| {
            assert!(trace.is_none(), "identity recording already active");
            *trace = Some(Trace {
                db: database_id(db),
                events: Vec::new(),
            });
        });
        Self
    }

    /// Copies the events recorded so far without changing the running attempt.
    pub(in crate::types) fn events(&self) -> Vec<Event> {
        TRACE.with_borrow(|trace| {
            trace
                .as_ref()
                .map(|trace| trace.events.clone())
                .unwrap_or_default()
        })
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        TRACE.with_borrow_mut(|trace| *trace = None);
    }
}

fn database_id(db: &dyn Db) -> usize {
    std::ptr::from_ref(db).cast::<()>() as usize
}

/// Records an observation only for the selected database, without requesting semantic work.
pub(in crate::types) fn record(db: &dyn Db, event: Event) {
    record_id(database_id(db), event);
}

fn record_id(db: usize, event: Event) {
    TRACE.with_borrow_mut(|trace| {
        if let Some(trace) = trace
            && trace.db == db
        {
            trace.events.push(event);
        }
    });
}

/// Observes collection retirement after its Vec drops or moves into the local boxing callback.
/// This does not observe physical allocator deallocation or retirement of canonical interned data.
#[derive(Debug)]
pub(in crate::types) struct ArgumentsLifetime {
    db: usize,
    transferred: bool,
}

impl ArgumentsLifetime {
    pub(in crate::types) fn new(db: &dyn Db) -> Self {
        Self {
            db: database_id(db),
            transferred: false,
        }
    }

    pub(in crate::types) fn transferred(&mut self) {
        self.transferred = true;
    }
}

impl Drop for ArgumentsLifetime {
    fn drop(&mut self) {
        record_id(
            self.db,
            Event::ArgumentsRetired {
                transferred: self.transferred,
            },
        );
    }
}

/// Marks when the enclosing test operation retires after its production child future.
#[derive(Debug)]
pub(in crate::types) struct OperationLifetime(usize);

impl OperationLifetime {
    pub(in crate::types) fn new(db: &dyn Db) -> Self {
        Self(database_id(db))
    }
}

impl Drop for OperationLifetime {
    fn drop(&mut self) {
        record_id(self.0, Event::OperationRetired);
    }
}

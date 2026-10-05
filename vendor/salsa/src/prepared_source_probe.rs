//! Structural preparation and diagnostic observations of synchronous query reads.
//!
//! Capturing a trace does not certify an arbitrary value returned by the closure. A consumer
//! must separately establish which declaration operation produced the value it retains.

use std::cell::RefCell;

use crate::attempt_probe::{self, QueryPolicy};
use crate::zalsa::Zalsa;
use crate::zalsa_local::ZalsaLocal;
use crate::{Database, DatabaseKeyIndex, Revision};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stamp {
    database: usize,
    revision: Revision,
    cancellation: u8,
}

impl Stamp {
    pub fn current(db: &dyn Database) -> Self {
        Self::of(db.zalsa())
    }

    pub fn belongs_to(self, db: &dyn Database) -> bool {
        self == Self::current(db)
    }

    fn of(zalsa: &Zalsa) -> Self {
        Self {
            database: std::ptr::from_ref(zalsa).addr(),
            revision: zalsa.current_revision(),
            cancellation: zalsa.runtime().cancellation_count(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationError {
    ActiveAttempt,
    ActiveQuery,
    ActiveOperation,
    ChangedDatabaseStamp,
    UnsupportedDependency,
    InvalidDependency,
}

/// Prepares structural inputs before an analysis attempt starts.
///
/// Only complete-only queries may run inside `body`. An installed attempt, including one that has
/// already refused work or belongs to another database, prevents preparation from starting. Query
/// and operation scopes also prevent entry. These entry errors leave `body` uncalled.
///
/// Preparation does not consume an analysis allowance. Its result is returned only if the database
/// stamp remains unchanged and native cancellation has been checked after queries finish. Its
/// operation scope is removed even if `body` panics.
pub fn try_with_preparation<T>(
    db: &dyn Database,
    body: impl FnOnce() -> T,
) -> Result<T, PreparationError> {
    attempt_probe::check_structural_access(db).map_err(|_| PreparationError::ActiveAttempt)?;
    if attempt_probe::current().is_some() {
        return Err(PreparationError::ActiveAttempt);
    }
    if db
        .zalsa_local()
        .try_with_query_stack(|stack| stack.is_empty())
        != Some(true)
    {
        return Err(PreparationError::ActiveQuery);
    }
    if attempt_probe::stack_depths() != (0, 0) {
        return Err(PreparationError::ActiveOperation);
    }

    // A query can finish while local cancellation is masked. Retain the attachment so that the
    // last query cannot clear that request before preparation checks it outside the query scope.
    crate::attach(db, || {
        let stamp = Stamp::current(db);
        let operation = attempt_probe::enter(db.zalsa(), QueryPolicy::CompleteOnly, "preparation");
        let value = body();
        drop(operation);
        db.unwind_if_revision_cancelled();
        if !stamp.belongs_to(db) {
            return Err(PreparationError::ChangedDatabaseStamp);
        }
        Ok(value)
    })
}

/// Guards native lazy work that must finish before any analysis attempt starts.
///
/// This checks the installed attempt independently of its database and completion status. Ordinary
/// query and operation scopes remain allowed, so callers can retain their usual lazy behavior.
///
/// # Panics
///
/// Panics if an attempt is installed on this worker, including after it has refused work.
#[track_caller]
pub fn assert_no_active_attempt() {
    assert!(
        attempt_probe::current().is_none(),
        "lazy preparation started inside an analysis attempt"
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Final,
    Provisional,
    Incomplete,
}

#[derive(Clone, Copy, Debug)]
pub struct Read {
    pub stamp: Stamp,
    pub key: DatabaseKeyIndex,
    pub memo_address: usize,
    pub status: Status,
    pub parent: Option<DatabaseKeyIndex>,
}

thread_local! {
    static READS: RefCell<Option<Vec<Read>>> = const { RefCell::new(None) };
}

pub(crate) fn observe(
    zalsa: &Zalsa,
    local: &ZalsaLocal,
    key: DatabaseKeyIndex,
    memo_address: usize,
    status: impl FnOnce() -> Status,
) {
    READS.with_borrow_mut(|slot| {
        if let Some(reads) = slot {
            reads.push(Read {
                stamp: Stamp::of(zalsa),
                key,
                memo_address,
                status: status(),
                parent: local.active_query().map(|(key, _)| key),
            });
        }
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureError {
    NestedCapture,
    ActiveQuery,
    ChangedDatabaseStamp,
    ForeignDatabaseRead,
    NoRootReads,
    ProvisionalRoot,
    IncompleteRoot,
}

/// Holds the database borrow that protects the captured revision and memo addresses.
pub struct Captured<'db, T> {
    db: &'db dyn Database,
    pub value: T,
    pub reads: Vec<Read>,
    pub stamp: Stamp,
}

impl<T> Captured<'_, T> {
    /// Checks the trace, not the provenance of `value` or the work done by preparation.
    pub fn check_root_reads(&self) -> Result<(), CaptureError> {
        if Stamp::of(self.db.zalsa()) != self.stamp {
            return Err(CaptureError::ChangedDatabaseStamp);
        }
        let mut root_read = false;
        for read in &self.reads {
            if read.stamp.database != self.stamp.database {
                return Err(CaptureError::ForeignDatabaseRead);
            }
            if read.stamp != self.stamp {
                return Err(CaptureError::ChangedDatabaseStamp);
            }
            if read.parent.is_none() {
                root_read = true;
                match read.status {
                    Status::Final => {}
                    Status::Provisional => return Err(CaptureError::ProvisionalRoot),
                    Status::Incomplete => return Err(CaptureError::IncompleteRoot),
                }
            }
        }
        if root_read {
            Ok(())
        } else {
            Err(CaptureError::NoRootReads)
        }
    }

    pub fn belongs_to(&self, db: &dyn Database) -> bool {
        self.stamp.belongs_to(db)
    }
}

struct Installed;

impl Drop for Installed {
    fn drop(&mut self) {
        READS.with_borrow_mut(|slot| *slot = None);
    }
}

pub fn capture<'db, T>(
    db: &'db dyn Database,
    body: impl FnOnce() -> T,
) -> Result<Captured<'db, T>, CaptureError> {
    if READS.with_borrow(Option::is_some) {
        return Err(CaptureError::NestedCapture);
    }
    if db.zalsa_local().active_query().is_some() {
        return Err(CaptureError::ActiveQuery);
    }
    let stamp = Stamp::of(db.zalsa());
    READS.with_borrow_mut(|slot| *slot = Some(Vec::new()));
    let installed = Installed;
    let value = body();
    let reads = READS.with_borrow_mut(|slot| slot.take().unwrap_or_default());
    drop(installed);
    if Stamp::of(db.zalsa()) != stamp {
        return Err(CaptureError::ChangedDatabaseStamp);
    }
    Ok(Captured {
        db,
        value,
        reads,
        stamp,
    })
}

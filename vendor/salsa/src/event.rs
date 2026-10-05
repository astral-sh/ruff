use crate::Revision;
use crate::key::DatabaseKeyIndex;
use crate::sync::thread::{self, ThreadId};

/// The `Event` struct identifies various notable things that can
/// occur during salsa execution. Instances of this struct are given
/// to `salsa_event`.
#[derive(Debug)]
pub struct Event {
    /// The id of the thread that triggered the event.
    pub thread_id: ThreadId,

    /// What sort of event was it.
    pub kind: EventKind,
}

impl Event {
    pub fn new(kind: EventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

/// An enum identifying the various kinds of events that can occur.
#[derive(Debug)]
pub enum EventKind {
    /// Occurs when we found that all inputs to a memoized value are
    /// up-to-date and hence the value can be re-used without
    /// executing the closure.
    ///
    /// Executes before the "re-used" value is returned.
    DidValidateMemoizedValue {
        /// The database-key for the affected value. Implements `Debug`.
        database_key: DatabaseKeyIndex,
    },

    /// Indicates that another thread (with id `other_thread_id`) is processing the
    /// given query (`database_key`), so we will block until they
    /// finish.
    ///
    /// Executes after we have registered with the other thread but
    /// before they have answered us.
    WillBlockOn {
        /// The id of the thread we will block on.
        other_thread_id: ThreadId,

        /// The database-key for the affected value. Implements `Debug`.
        database_key: DatabaseKeyIndex,
    },

    /// Indicates that the function for this query will be executed.
    /// This is either because it has never executed before or because
    /// its inputs may be out of date.
    WillExecute {
        /// The database-key for the affected value. Implements `Debug`.
        database_key: DatabaseKeyIndex,
    },

    /// Salsa starts a new fixpoint iteration for the cycle with `database_key` as its
    /// outermost cycle.
    WillIterateCycle {
        /// The database-key for the cycle head. Implements `Debug`.
        database_key: DatabaseKeyIndex,
        iteration: u8,
    },

    /// Salsa completed a fixpoint iteration for the cycle with `database_key` as its
    /// outermost cycle.
    DidFinalizeCycle {
        database_key: DatabaseKeyIndex,
        iteration: u8,
    },

    /// Indicates that `unwind_if_cancelled` was called and salsa will check if
    /// the current revision has been cancelled.
    WillCheckCancellation,

    /// Indicates that one [`Handle`](`crate::Handle`) has set the cancellation flag.
    /// When other active handles execute salsa methods, they will observe this flag
    /// and panic with a sentinel value of type [`Cancelled`](`crate::Cancelled`).
    DidSetCancellationFlag,

    /// Discovered that a query used to output a given output but no longer does.
    WillDiscardStaleOutput {
        /// Key for the query that is executing and which no longer outputs the given value.
        execute_key: DatabaseKeyIndex,

        /// Key for the query that is no longer output
        output_key: DatabaseKeyIndex,
    },

    /// Tracked structs or memoized data were discarded (freed).
    DidDiscard {
        /// Value being discarded.
        key: DatabaseKeyIndex,
    },

    /// Discarded accumulated data from a given fn
    DidDiscardAccumulated {
        /// The key of the fn that accumulated results
        executor_key: DatabaseKeyIndex,

        /// Accumulator that was accumulated into
        accumulator: DatabaseKeyIndex,
    },

    /// Indicates that a value was newly interned.
    DidInternValue {
        // The key of the interned value.
        key: DatabaseKeyIndex,

        // The revision the value was interned in.
        revision: Revision,
    },

    /// Indicates that a value was interned by reusing an existing slot.
    DidReuseInternedValue {
        // The key of the interned value.
        key: DatabaseKeyIndex,

        // The revision the value was interned in.
        revision: Revision,
    },

    /// Indicates that a previously interned value was read in a new revision.
    DidValidateInternedValue {
        // The key of the interned value.
        key: DatabaseKeyIndex,

        // The revision the value was interned in.
        revision: Revision,
    },
}

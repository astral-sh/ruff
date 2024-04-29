use crate::cancellation::CancellationTokenSource;
use crate::db::{QueryError, QueryResult};

/// Holds the jar agnostic state of the database.
#[derive(Debug, Default)]
pub struct DbRuntime {
    /// The cancellation token source used to signal other works that the queries should be aborted and
    /// exit at the next possible point.
    cancellation_token: CancellationTokenSource,
}

impl DbRuntime {
    pub(super) fn snapshot(&self) -> Self {
        Self {
            cancellation_token: self.cancellation_token.clone(),
        }
    }

    /// Cancels the pending queries of other workers. The current worker cannot have any pending
    /// queries because we're holding a mutable reference to the runtime.
    pub(super) fn cancel_other_workers(&mut self) {
        self.cancellation_token.cancel();
        // Set a new cancellation token so that we're in a non-cancelled state again when running the next
        // query.
        self.cancellation_token = CancellationTokenSource::default();
    }

    /// Returns `Ok` if the queries have not been cancelled and `Err(QueryError::Cancelled)` otherwise.
    pub(super) fn cancelled(&self) -> QueryResult<()> {
        if self.cancellation_token.is_cancelled() {
            Err(QueryError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Returns `true` if the queries have been cancelled.
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }
}

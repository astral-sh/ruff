//! Test hooks for explicit semantic execution.

use std::cell::Cell;

use crate::Db;

#[derive(Clone, Copy)]
enum ScopeMergeCancellation {
    Inactive,
    Armed,
    Fired,
}

thread_local! {
    static SCOPE_MERGE_CANCELLATION: Cell<ScopeMergeCancellation> = const {
        Cell::new(ScopeMergeCancellation::Inactive)
    };
}

struct RestoreScopeMergeCancellation(ScopeMergeCancellation);

impl Drop for RestoreScopeMergeCancellation {
    fn drop(&mut self) {
        SCOPE_MERGE_CANCELLATION.set(self.0);
    }
}

/// Requests native Salsa cancellation after the first controlled scope merge in `body` on this thread.
///
/// Returns the body's value and whether cancellation was requested. Catch cancellation inside `body`
/// to receive that flag. The previous hook state is restored even if the body unwinds.
pub fn with_scope_merge_cancellation<T>(body: impl FnOnce() -> T) -> (T, bool) {
    let _restore = RestoreScopeMergeCancellation(
        SCOPE_MERGE_CANCELLATION.replace(ScopeMergeCancellation::Armed),
    );
    let result = body();
    let fired = matches!(
        SCOPE_MERGE_CANCELLATION.get(),
        ScopeMergeCancellation::Fired
    );
    (result, fired)
}

pub(crate) fn scope_merged(db: &dyn Db) {
    if matches!(
        SCOPE_MERGE_CANCELLATION.get(),
        ScopeMergeCancellation::Armed
    ) {
        SCOPE_MERGE_CANCELLATION.set(ScopeMergeCancellation::Fired);
        db.cancellation_token().cancel();
    }
}

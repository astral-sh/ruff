use crate::Backtrace;
use crate::DatabaseKeyIndex;
use crate::function::memo::{Memo, MemoHeader};
use crate::function::{Configuration, IngredientImpl};
use crate::zalsa_local::QueryRevisions;
use std::fmt;

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    /// If the value/durability of this memo is equal to what is found in `revisions`/`value`,
    /// then update `revisions.changed_at` to match `self.revisions.changed_at`. This is invoked
    /// on an old memo when a new memo has been produced to check whether there have been changed.
    pub(super) fn backdate_if_appropriate<'db>(
        &self,
        old_memo: &'db Memo<C>,
        index: DatabaseKeyIndex,
        revisions: &mut QueryRevisions,
        value: &C::Output<'db>,
    ) {
        if old_memo
            .backdate_comparison(revisions, value)
            .is_some_and(|(old_value, value)| C::values_equal(old_value, value))
        {
            old_memo.header.backdate(index, revisions);
        }
    }
}

impl<C: Configuration> Memo<C> {
    pub(super) fn backdate_comparison<'db, 'value>(
        &'db self,
        revisions: &QueryRevisions,
        value: &'value C::Output<'db>,
    ) -> Option<(&'db C::Output<'db>, &'value C::Output<'db>)> {
        if !self.header.can_backdate(revisions) {
            return None;
        }
        Some((self.value()?, value))
    }
}

impl MemoHeader {
    fn can_backdate(&self, revisions: &QueryRevisions) -> bool {
        if self.has_incomplete_attempt() || revisions.has_incomplete_attempt() {
            return false;
        }

        // We've seen issues where queries weren't re-validated when backdating provisional values
        // in ty. This is more of a bandaid because we're close to a release and don't have the time to prove
        // right now whether backdating could be made safe for queries participating in queries.
        // TODO: Write a test that demonstrates that backdating queries participating in a cycle isn't safe
        // OR write many tests showing that it is (and fixing the case where it didn't correctly account for today).
        revisions.cycle_heads().is_empty()
            && !self.may_be_provisional()
            // Careful: if the value became less durable than it
            // used to be, that is a "breaking change" that our
            // consumers must be aware of. Becoming *more* durable
            // is not. See the test `durable_to_less_durable`.
            && revisions.durability >= self.revisions.durability
    }

    fn backdate(&self, index: DatabaseKeyIndex, revisions: &mut QueryRevisions) {
        crate::tracing::debug!(
            "{index:?} value is equal, back-dating to {:?}",
            self.revisions.changed_at,
        );

        if self.revisions.changed_at > revisions.changed_at {
            report_backdate_violation(index, self.revisions.changed_at, revisions.changed_at);
        }

        revisions.changed_at = self.revisions.changed_at;
    }
}

#[cold]
#[inline(never)]
fn report_backdate_violation(
    index: DatabaseKeyIndex,
    old_changed_at: crate::Revision,
    new_changed_at: crate::Revision,
) {
    if cfg!(debug_assertions) {
        let message = BackdateViolation {
            index,
            old_changed_at,
            new_changed_at,
            backtrace: None,
        };
        panic!("{message}");
    } else {
        let message = BackdateViolation {
            index,
            old_changed_at,
            new_changed_at,
            backtrace: Backtrace::capture(),
        };
        crate::tracing::warn!("{message}");
    }
}

struct BackdateViolation {
    index: DatabaseKeyIndex,
    old_changed_at: crate::Revision,
    new_changed_at: crate::Revision,
    backtrace: Option<Backtrace>,
}

impl fmt::Display for BackdateViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "query {:?} returned the same value, but the previous execution changed at {:?} and \
             the new execution changed at {:?}. This usually means the query re-executed because \
             an input changed, but then branched on untracked state (for example, a global \
             variable, a non-salsa field on the database, or filesystem state read outside salsa) \
             and no longer read that input. This is usually a bug in the query implementation. \
             Queries that branch on untracked state can also produce stale results. If the query \
             has no untracked reads, please open a Salsa issue.",
            self.index, self.old_changed_at, self.new_changed_at,
        )?;

        if let Some(backtrace) = &self.backtrace {
            write!(f, "\n{backtrace}")?;
        }

        Ok(())
    }
}

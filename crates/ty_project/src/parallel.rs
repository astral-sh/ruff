//! Helpers for parallel operations that need an independent Salsa database on each Rayon job.

use rayon::iter::ParallelIterator;

use crate::{Db, ProjectDatabase};

/// Chooses a minimum Rayon job length without starving the worker pool on smaller inputs.
///
/// The maximum should be an empirically chosen upper bound for the operation's minimum job length.
/// Larger inputs use that minimum, while smaller inputs lower it to retain enough jobs for work
/// stealing.
pub fn minimum_parallel_job_len(item_count: usize, maximum: usize) -> usize {
    const TARGET_JOBS_PER_THREAD: usize = 4;

    let target_jobs = rayon::current_num_threads().saturating_mul(TARGET_JOBS_PER_THREAD);
    item_count.div_ceil(target_jobs).clamp(1, maximum.max(1))
}

/// Extension methods for Rayon parallel iterators that need a Salsa database.
pub trait ParallelIteratorExt: ParallelIterator + Sized {
    /// Maps items in parallel with a separate Salsa database for each Rayon job.
    ///
    /// Rayon's standard scheduling adapters can be applied before this method. For example, an
    /// indexed iterator can use [`rayon::iter::IndexedParallelIterator::with_min_len`] to reduce
    /// task and database-cloning overhead.
    ///
    /// # Warning
    ///
    /// This method uses [`salsa::attach_allow_change`] and must never be called from within a Salsa
    /// query. It is intended for top-level request and command handlers that are outside the Salsa
    /// query stack.
    fn map_with_db<Output>(
        self,
        db: &dyn Db,
        map: impl Fn(&dyn Db, Self::Item) -> Output + Send + Sync,
    ) -> impl ParallelIterator<Item = Output>
    where
        Output: Send,
    {
        self.map_with(ParallelDb(Db::dyn_clone(db)), move |db, item| {
            salsa::attach_allow_change(&*db.0, || map(&*db.0, item))
        })
    }

    /// Runs an operation in parallel with a cloned [`ProjectDatabase`] for each Rayon job.
    ///
    /// Rayon's standard scheduling adapters can be applied before this method. For example, an
    /// indexed iterator can use [`rayon::iter::IndexedParallelIterator::with_min_len`] to reduce
    /// task and database-cloning overhead.
    ///
    /// # Warning
    ///
    /// This method uses [`salsa::attach_allow_change`] and must never be called from within a Salsa
    /// query. It is intended for top-level request and command handlers that are outside the Salsa
    /// query stack.
    fn for_each_with_project_db(
        self,
        db: &ProjectDatabase,
        op: impl Fn(&ProjectDatabase, Self::Item) + Send + Sync,
    ) {
        self.for_each_with(db.clone(), move |db, item| {
            salsa::attach_allow_change(db, || op(db, item));
        });
    }
}

impl<Iter> ParallelIteratorExt for Iter where Iter: ParallelIterator {}

/// An owned Salsa snapshot that Rayon can clone whenever it splits a parallel iterator.
struct ParallelDb(Box<dyn Db>);

impl Clone for ParallelDb {
    fn clone(&self) -> Self {
        Self(Db::dyn_clone(&*self.0))
    }
}

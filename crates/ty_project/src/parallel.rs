//! Helpers for parallel operations that need an independent Salsa database on each Rayon job.

use std::cmp::Reverse;

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

/// Give the largest files a head start across Rayon's work-stealing ranges.
/// Leave the rest in their original relative order.
pub(super) fn distribute_large_files<T: Copy>(
    files: &mut [T],
    workers: usize,
    size: impl Fn(T) -> Option<usize>,
) {
    // Source length is only a rough proxy for cost, so give several files
    // a head start in each range. If these are at least half the files,
    // they also account for at least half the source bytes.
    let ranges = workers.next_power_of_two();
    let count = 4 * ranges;
    if workers <= 1 || count >= files.len().div_ceil(2) {
        return;
    }

    // An unknown size could hide an expensive file. Keep the current order
    // instead of pushing it behind all the files of known size.
    let Some(mut ranked) = files
        .iter()
        .enumerate()
        .map(|(i, &file)| size(file).map(|bytes| (i, file, bytes)))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };

    // Estimate how much time the other workers could spend idle while one
    // checks the largest file. Skip reordering if that adds up to less than
    // one worker's share of the check, or if all files are the same size.
    let total: usize = ranked.iter().map(|&(_, _, bytes)| bytes).sum();
    let threshold = total / workers / (workers - 1);
    if ranked.iter().all(|&(_, _, bytes)| bytes <= threshold)
        || ranked.iter().all(|&(_, _, bytes)| bytes == ranked[0].2)
    {
        return;
    }

    let priority = |&(i, _, bytes): &(usize, T, usize)| (Reverse(bytes), i);
    ranked.select_nth_unstable_by_key(count, priority);
    let (early, rest) = ranked.split_at_mut(count);

    // Leave the bulk of the work in its original order. Starting most of
    // the expensive checks together can increase contention even if it
    // shortens the wait for the last file.
    let early_bytes: usize = early.iter().map(|&(_, _, bytes)| bytes).sum();
    if early_bytes >= total - early_bytes {
        return;
    }

    early.sort_unstable_by_key(priority);
    rest.sort_unstable_by_key(|&(i, _, _)| i);
    let mut rest = rest.iter().map(|&(_, file, _)| file);
    distribute(early, files, ranges, &mut rest);
}

/// Split the priorities at the same midpoints that Rayon uses for the output.
/// Each leaf starts with its largest files, then takes a contiguous run of the rest.
fn distribute<T: Copy>(
    early: &[(usize, T, usize)],
    output: &mut [T],
    ranges: usize,
    rest: &mut impl Iterator<Item = T>,
) {
    if ranges == 1 {
        let order = early.iter().map(|&(_, file, _)| file).chain(rest);
        for (slot, file) in output.iter_mut().zip(order) {
            *slot = file;
        }
        return;
    }

    // Send one file from each pair to each half. Always give the larger file
    // to the lighter half. Both lists stay sorted by descending size.
    let (pairs, remainder) = early.as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    let mut left = Vec::with_capacity(pairs.len());
    let mut right = Vec::with_capacity(pairs.len());
    let (mut left_bytes, mut right_bytes) = (0, 0);
    for &[large, small] in pairs {
        let (a, b) = if left_bytes <= right_bytes {
            (large, small)
        } else {
            (small, large)
        };
        left.push(a);
        right.push(b);
        left_bytes += a.2;
        right_bytes += b.2;
    }

    let (first, second) = output.split_at_mut(output.len() / 2);
    distribute(&left, first, ranges / 2, rest);
    distribute(&right, second, ranges / 2, rest);
}
#[cfg(test)]
mod tests {
    use super::distribute_large_files;

    #[test]
    fn large_files_lead_each_range_and_ordinary_files_keep_their_order() {
        let mut files: Vec<_> = (0..67).collect();
        distribute_large_files(&mut files, 3, |i| {
            Some(if i == 66 { 2500 } else { 100 + i })
        });

        // Bisection splits 67 into 33 + 34, then 16 + 17 and 17 + 17.
        let mut heads = [files[0], files[16], files[33], files[50]];
        heads.sort_unstable();
        assert_eq!(heads, [63, 64, 65, 66]);
        for start in [0, 16, 33, 50] {
            let early = &files[start..start + 4];
            assert!(early.iter().all(|&i| i >= 51));
            assert!(early.is_sorted_by(|x, y| x > y));
        }
        assert!(files.iter().copied().filter(|&i| i < 51).eq(0..51));
        files.sort_unstable();
        assert!(files.into_iter().eq(0..67));
    }

    #[test]
    fn single_worker_and_small_checks_skip_size_lookups() {
        for (len, workers) in [(0, 4), (64, 1), (32, 3)] {
            let mut files: Vec<_> = (0..len).collect();
            distribute_large_files(&mut files, workers, |_| unreachable!());
            assert!(files.into_iter().eq(0..len));
        }
    }

    #[test]
    fn short_estimated_wait_preserves_order() {
        let mut files: Vec<_> = (0..64).collect();
        distribute_large_files(&mut files, 4, Some);
        assert!(files.into_iter().eq(0..64));
    }

    #[test]
    fn preserves_order_if_priority_files_dominate_the_check() {
        let mut files: Vec<_> = (0..64).collect();
        distribute_large_files(&mut files, 4, |i| Some(if i == 63 { 1000 } else { 1 }));
        assert!(files.into_iter().eq(0..64));
    }

    #[test]
    fn missing_size_preserves_order() {
        let mut files: Vec<_> = (0..64).collect();
        distribute_large_files(&mut files, 4, |i| match i {
            0 => Some(1000),
            63 => None,
            _ => Some(i),
        });
        assert!(files.into_iter().eq(0..64));
    }
}

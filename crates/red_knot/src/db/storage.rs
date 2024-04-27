use std::sync::Arc;

use crossbeam::sync::WaitGroup;

use crate::cancellation::CancellationTokenSource;
use crate::db::jars::HasJars;
use crate::db::query::{QueryError, QueryResult};

pub struct JarStorage<T>
where
    T: HasJars,
{
    db: T,
}

#[derive(Clone, Debug)]
pub struct SharedStorage<T>
where
    T: HasJars,
{
    // It's important that the wait group is declared after `jars` to ensure that `jars` is dropped first.
    // See https://doc.rust-lang.org/reference/destructors.html
    jars: Arc<T::Jars>,

    /// Used to count the references to `jars`. Allows implementing [`jars_mut`] without requiring to clone `jars`.
    jars_references: WaitGroup,

    cancellation_token_source: CancellationTokenSource,
}

impl<T> SharedStorage<T>
where
    T: HasJars,
{
    pub(super) fn jars(&self) -> QueryResult<&T::Jars> {
        self.err_if_cancelled()?;
        Ok(&self.jars)
    }

    pub(super) fn jars_mut(&mut self) -> &mut T::Jars {
        // Cancel all pending queries.
        self.cancellation_token_source.cancel();

        let existing_wait = std::mem::take(&mut self.jars_references);
        existing_wait.wait();
        self.cancellation_token_source = CancellationTokenSource::new();

        // Now all other references to `self.jars` should have been released. We can now safely return a mutable reference
        // to the Arc's content.
        let jars =
            Arc::get_mut(&mut self.jars).expect("All references to jars should have been released");

        jars
    }

    pub(super) fn err_if_cancelled(&self) -> QueryResult<()> {
        if self.cancellation_token_source.is_cancelled() {
            Err(QueryError::Cancelled)
        } else {
            Ok(())
        }
    }
}

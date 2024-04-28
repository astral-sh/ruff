use std::fmt::Formatter;
use std::sync::Arc;

use crossbeam::sync::WaitGroup;

use crate::db::query::QueryResult;
use crate::db::runtime::DbRuntime;
use crate::db::{HasJars, ParallelDatabase};

pub struct JarsStorage<T>
where
    T: HasJars + Sized,
{
    // It's important that `jars_wait_group` is declared after `jars` to ensure that `jars` is dropped first.
    // See https://doc.rust-lang.org/reference/destructors.html
    jars: Arc<T::Jars>,

    /// Used to count the references to `jars`. Allows implementing `jars_mut` without requiring to clone `jars`.
    jars_wait_group: WaitGroup,

    runtime: DbRuntime,
}

impl<Db> JarsStorage<Db>
where
    Db: HasJars,
{
    pub(super) fn new() -> Self {
        Self {
            jars: Arc::new(Db::Jars::default()),
            jars_wait_group: WaitGroup::default(),
            runtime: DbRuntime::default(),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> JarsStorage<Db>
    where
        Db: ParallelDatabase,
    {
        Self {
            jars: self.jars.clone(),
            jars_wait_group: self.jars_wait_group.clone(),
            runtime: self.runtime.snapshot(),
        }
    }

    pub(crate) fn jars(&self) -> QueryResult<&Db::Jars> {
        self.runtime.cancelled()?;
        Ok(&self.jars)
    }

    pub(crate) fn jars_unwrap(&self) -> &Db::Jars {
        &self.jars
    }

    pub(crate) fn jars_mut(&mut self) -> &mut Db::Jars {
        // We have a mutable ref here, so no more workers can be spawned between calling this function and taking the mut ref below.
        self.cancel_other_workers();

        // Now all other references to `self.jars` should have been released. We can now safely return a mutable reference
        // to the Arc's content.
        let jars =
            Arc::get_mut(&mut self.jars).expect("All references to jars should have been released");

        jars
    }

    pub(crate) fn runtime(&self) -> &DbRuntime {
        &self.runtime
    }

    pub(crate) fn runtime_mut(&mut self) -> &mut DbRuntime {
        // Note: This method may need to use a similar trick to `jars_mut` if `DbRuntime` is ever to store data that is shared between workers.
        &mut self.runtime
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn cancel_other_workers(&mut self) {
        self.runtime.cancel_other_workers();

        // Wait for all other works to complete.
        let existing_wait = std::mem::take(&mut self.jars_wait_group);
        existing_wait.wait();
    }
}

impl<Db> Default for JarsStorage<Db>
where
    Db: HasJars,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::fmt::Debug for JarsStorage<T>
where
    T: HasJars,
    <T as HasJars>::Jars: std::fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedStorage")
            .field("jars", &self.jars)
            .field("jars_wait_group", &self.jars_wait_group)
            .field("runtime", &self.runtime)
            .finish()
    }
}

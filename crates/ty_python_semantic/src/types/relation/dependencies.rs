//! Completion checks for finite dependencies of resumable type relations.

use std::convert::Infallible;

use crate::Db;

/// Checks completion before a callback's value participates in a relation.
///
/// A refusing implementation checks before and after the callback: legacy queries can return
/// recovery values after nested work has stopped. This contract does not bound recursive work
/// inside the callback; recursive dependencies need their own supervised child operations.
pub(in crate::types) trait RelationDependencies {
    type Error;

    fn run<T>(&self, db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Self::Error>;
}

pub(in crate::types) struct OrdinaryDependencies;

impl RelationDependencies for OrdinaryDependencies {
    type Error = Infallible;

    fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(operation())
    }
}

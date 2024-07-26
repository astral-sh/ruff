use std::iter::FusedIterator;

pub use db::{Db, Jar};
pub use module::{Module, ModuleKind};
pub use module_name::ModuleName;
pub use resolver::{file_to_module, resolve_module};
use ruff_db::system::SystemPath;
pub use typeshed::{
    vendored_typeshed_stubs, TypeshedVersionsParseError, TypeshedVersionsParseErrorKind,
};

use crate::resolver::{module_resolution_settings, SearchPathIterator};

mod db;
mod module;
mod module_name;
mod path;
mod resolver;
mod state;
mod typeshed;

#[cfg(test)]
mod testing;

/// Returns an iterator over all search paths pointing to a system path
pub fn system_module_search_paths(db: &dyn Db) -> SystemModuleSearchPathsIter {
    SystemModuleSearchPathsIter {
        inner: module_resolution_settings(db).search_paths(db),
    }
}

pub struct SystemModuleSearchPathsIter<'db> {
    inner: SearchPathIterator<'db>,
}

impl<'db> Iterator for SystemModuleSearchPathsIter<'db> {
    type Item = &'db SystemPath;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.inner.next()?;

            if let Some(system_path) = next.as_system_path() {
                return Some(system_path);
            }
        }
    }
}

impl FusedIterator for SystemModuleSearchPathsIter<'_> {}

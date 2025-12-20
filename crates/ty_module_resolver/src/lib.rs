use std::iter::FusedIterator;

use ruff_db::system::SystemPath;

pub use db::Db;
pub use module::KnownModule;
pub use module::Module;
pub use module_name::{ModuleName, ModuleNameResolutionError};
pub use path::{SearchPath, SearchPathError};
pub use resolve::{
    SearchPaths, file_to_module, resolve_module, resolve_module_confident, resolve_real_module,
    resolve_real_module_confident, resolve_real_shadowable_module,
};
pub use settings::{MisconfigurationMode, SearchPathSettings, SearchPathSettingsError};
pub use typeshed::{
    PyVersionRange, TypeshedVersions, TypeshedVersionsParseError, vendored_typeshed_versions,
};

pub use list::{all_modules, list_modules};
pub use resolve::{ModuleResolveMode, SearchPathIterator, search_paths};

mod db;
mod list;
mod module;
mod module_name;
mod path;
mod resolve;
mod settings;
mod typeshed;

#[cfg(test)]
mod testing;

/// Returns an iterator over all search paths pointing to a system path
pub fn system_module_search_paths(db: &dyn Db) -> SystemModuleSearchPathsIter<'_> {
    SystemModuleSearchPathsIter {
        // Always run in `StubsAllowed` mode because we want to include as much as possible
        // and we don't care about the "real" stdlib
        inner: search_paths(db, ModuleResolveMode::StubsAllowed),
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

use std::iter::FusedIterator;

use ruff_db::system::{SystemPath, SystemPathBuf};

pub use db::Db;
pub use module::KnownModule;
pub use module::Module;
pub use module_name::{ModuleName, ModuleNameResolutionError};
pub use path::{SearchPath, SearchPathError};
pub use resolve::{
    SearchPaths, file_to_module, resolve_module, resolve_module_confident, resolve_real_module,
    resolve_real_module_confident, resolve_real_shadowable_module,
};
pub use settings::{SearchPathSettings, SearchPathSettingsError};
pub use strategy::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};
pub use typeshed::{
    PyVersionRange, TypeshedVersions, TypeshedVersionsParseError, vendored_typeshed_versions,
};

pub use list::{all_modules, list_modules};
pub use module_glob::{ModuleGlobError, ModuleGlobSet, ModuleGlobSetBuilder, ModuleNameMatch};
pub use resolve::{ModuleResolveMode, SearchPathIterator, search_paths};

mod db;
mod list;
mod module;
mod module_glob;
mod module_name;
mod path;
mod resolve;
mod settings;
mod strategy;
mod typeshed;

#[cfg(test)]
mod testing;

/// Returns an iterator over system directories whose contents can affect module resolution.
///
/// This includes ordinary module search paths plus the external roots referenced by supported
/// setuptools editable finders, which are not themselves `sys.path` entries but still need file
/// watcher coverage.
pub fn system_module_search_paths(db: &dyn Db) -> SystemModuleSearchPathsIter<'_> {
    SystemModuleSearchPathsIter {
        // Always run in `StubsAllowed` mode because we want to include as much as possible
        // and we don't care about the "real" stdlib
        search_paths: search_paths(db, ModuleResolveMode::StubsAllowed),
        finder_paths: resolve::setuptools_editable_finder_search_paths(db).iter(),
    }
}

pub struct SystemModuleSearchPathsIter<'db> {
    search_paths: SearchPathIterator<'db>,
    finder_paths: std::slice::Iter<'db, SystemPathBuf>,
}

impl<'db> Iterator for SystemModuleSearchPathsIter<'db> {
    type Item = &'db SystemPath;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(next) = self.search_paths.next() {
            if let Some(system_path) = next.as_system_path() {
                return Some(system_path);
            }
        }

        self.finder_paths.next().map(SystemPathBuf::as_path)
    }
}

impl FusedIterator for SystemModuleSearchPathsIter<'_> {}

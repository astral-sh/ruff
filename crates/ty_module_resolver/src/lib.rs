use std::hash::BuildHasherDefault;
use std::iter::FusedIterator;

use ruff_db::system::SystemPath;
use rustc_hash::FxHasher;

pub use db::Db;
pub use environment::{ResolverEnvironment, ResolverFile};
pub use module::KnownModule;
pub use module::Module;
pub use module_name::{ImportingFile, ModuleName, ModuleNameResolutionError};
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
mod environment;
mod list;
mod module;
mod module_glob;
mod module_name;
mod path;
mod resolve;
mod settings;
mod strategy;
mod typeshed;

type FxOrderMap<K, V> = ordermap::map::OrderMap<K, V, BuildHasherDefault<FxHasher>>;

#[cfg(test)]
mod testing;

/// Returns an iterator over all search paths pointing to a system path
pub fn system_module_search_paths<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
) -> SystemModuleSearchPathsIter<'db> {
    SystemModuleSearchPathsIter {
        // Always run in `Typing` mode because we want to include as much as possible
        // and we don't care about the "real" stdlib
        inner: search_paths(db, resolver_environment, ModuleResolveMode::Typing),
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

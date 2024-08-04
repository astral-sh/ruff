use std::iter::FusedIterator;

pub use db::Db;
pub use module::{Module, ModuleKind};
pub use module_name::ModuleName;
pub use path::SearchPathValidationError;
pub use resolver::resolve_module;
pub use settings_resolution::{program_from_raw_settings, try_resolve_module_resolution_settings};
pub use typeshed::{
    vendored_typeshed_stubs, TypeshedVersionsParseError, TypeshedVersionsParseErrorKind,
};

mod db;
mod module;
mod module_name;
mod path;
mod resolver;
mod settings_resolution;
mod state;
mod typeshed;

#[cfg(test)]
mod testing;

/// Returns an iterator over all search paths
pub fn module_search_paths(db: &dyn Db) -> ModuleSearchPathsIter {
    ModuleSearchPathsIter {
        inner: resolver::module_search_paths(db),
    }
}

// Unlike the internal `SearchPathIterator` struct,
// which yields instances of the private
// `red_knot_module_resolver::path::SearchPath` type,
// this public iterator yields instances of the public
// `ruff_db::program::SearchPath` type
pub struct ModuleSearchPathsIter<'db> {
    inner: resolver::SearchPathIterator<'db>,
}

impl<'db> Iterator for ModuleSearchPathsIter<'db> {
    type Item = ruff_db::program::SearchPath;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|path| ruff_db::program::SearchPath::from(&*path))
    }
}

impl FusedIterator for ModuleSearchPathsIter<'_> {}

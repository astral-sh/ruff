use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Arc;

use crate::cache::MapCache;
use crate::files::FileId;
use crate::module::{Module, ModuleData, ModuleName, ModuleResolver, ModuleSearchPath};
use crate::parse::{Parsed, ParsedStorage};
use crate::source::Source;

pub trait SourceDb {
    fn file_id(&self, path: &std::path::Path) -> FileId;

    fn file_path(&self, file_id: FileId) -> Arc<std::path::Path>;

    fn source(&self, file_id: FileId) -> Source;

    fn parse(&self, file_id: FileId) -> Parsed;
}

pub trait ModuleDb {
    fn resolve_module(&self, name: ModuleName) -> Option<Module>;

    fn path_to_module(&mut self, path: &Path) -> Option<Module>;

    fn add_module(&mut self, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)>;

    fn set_module_search_paths(&mut self, paths: Vec<ModuleSearchPath>);
}

#[derive(Debug, Default)]
pub struct SourceJar {
    pub module_resolver: ModuleResolver,
    pub sources: SourceStorage,
    pub parsed: ParsedStorage,
}

#[derive(Debug, Default)]
pub struct SourceStorage(pub(crate) MapCache<FileId, Source>);

impl Deref for SourceStorage {
    type Target = MapCache<FileId, Source>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SourceStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl HasIngredient<ModuleResolver> for SourceJar {
    fn ingredient(&self) -> &ModuleResolver {
        &self.module_resolver
    }

    fn ingredient_mut(&mut self) -> &mut ModuleResolver {
        &mut self.module_resolver
    }
}

impl HasIngredient<SourceStorage> for SourceJar {
    fn ingredient(&self) -> &SourceStorage {
        &self.sources
    }

    fn ingredient_mut(&mut self) -> &mut SourceStorage {
        &mut self.sources
    }
}

impl HasIngredient<ParsedStorage> for SourceJar {
    fn ingredient(&self) -> &ParsedStorage {
        &self.parsed
    }

    fn ingredient_mut(&mut self) -> &mut ParsedStorage {
        &mut self.parsed
    }
}

pub trait Db: ModuleDb + SourceDb {}

pub trait HasJar<T> {
    fn jar(&self) -> &T;

    fn jar_mut(&mut self) -> &mut T;
}

pub trait HasIngredient<T> {
    fn ingredient(&self) -> &T;

    fn ingredient_mut(&mut self) -> &mut T;
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::db::{HasJar, ModuleDb, SourceDb, SourceJar};
    use crate::files::{FileId, Files};
    use crate::module::{
        add_module, path_to_module, resolve_module, set_module_search_paths, Module, ModuleData,
        ModuleName, ModuleSearchPath,
    };
    use crate::parse::{parse, Parsed};
    use crate::source::{source_text, Source};
    use std::path::Path;
    use std::sync::Arc;

    // This can be a partial database used in a single crate for testing.
    // It would hold fewer data than the full database.
    #[derive(Debug, Default)]
    pub(crate) struct TestDb {
        files: Files,
        source: SourceJar,
    }

    impl HasJar<SourceJar> for TestDb {
        fn jar(&self) -> &SourceJar {
            &self.source
        }

        fn jar_mut(&mut self) -> &mut SourceJar {
            &mut self.source
        }
    }

    impl SourceDb for TestDb {
        fn file_id(&self, path: &Path) -> FileId {
            self.files.intern(path)
        }

        fn file_path(&self, file_id: FileId) -> Arc<Path> {
            self.files.path(file_id)
        }

        fn source(&self, file_id: FileId) -> Source {
            source_text(self, file_id)
        }

        fn parse(&self, file_id: FileId) -> Parsed {
            parse(self, file_id)
        }
    }

    impl ModuleDb for TestDb {
        fn resolve_module(&self, name: ModuleName) -> Option<Module> {
            resolve_module(self, name)
        }

        fn path_to_module(&mut self, path: &Path) -> Option<Module> {
            path_to_module(self, path)
        }

        fn add_module(&mut self, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)> {
            add_module(self, path)
        }

        fn set_module_search_paths(&mut self, paths: Vec<ModuleSearchPath>) {
            set_module_search_paths(self, paths);
        }
    }
}

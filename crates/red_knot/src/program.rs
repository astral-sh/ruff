#![allow(dead_code)]

use crate::cache::{Cache, MapCache};
use crate::{Db, ModuleDb, SourceDb};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::files::{FileId, Files};
use crate::module::{Module, ModuleId, ModuleName, ModuleResolver, ModuleSearchPath};
use crate::parse::{parse, Parsed};
use crate::source::Source;

#[derive(Debug)]
pub struct Program {
    module_resolver: ModuleResolver,
    files: Files,
    sources: MapCache<FileId, Source>,
    parsed: MapCache<FileId, Parsed>,
}

impl Program {
    pub fn new(module_search_paths: Vec<ModuleSearchPath>, files: Files) -> Self {
        Self {
            module_resolver: ModuleResolver::new(module_search_paths, files.clone()),
            files,
            sources: Default::default(),
            parsed: Default::default(),
        }
    }

    // TODO figure out the ownership to make this work
    //    fn symbols(&self, module_id: ModuleId) -> Result<Symbols> {
    //        Ok(Symbols::from_ast(&self.parse(module_id)?.ast))
    //    }

    fn analyze_imports(&self, _name: ModuleName) -> Result<Vec<String>> {
        // TODO
        Ok(Vec::new())
    }

    pub fn file_changed(&mut self, path: &Path) {
        let Some(file_id) = self.files.try_get(path) else {
            return;
        };

        self.module_resolver.remove_module(path);
        self.sources.remove(&file_id);
        self.parsed.remove(&file_id);
    }
}

impl SourceDb for Program {
    fn file_id(&self, path: &Path) -> FileId {
        self.files.intern(path)
    }

    fn file_path(&self, file_id: FileId) -> Arc<Path> {
        self.files.path(file_id)
    }

    fn source(&self, file_id: FileId) -> Source {
        self.sources.get(&file_id, |file_id| {
            let path = self.file_path(*file_id);

            let source_text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
                tracing::error!(
                    "Failed to read file '{path:?}: {err}'. Falling back to empty text"
                );
                String::new()
            });

            Source::new(*file_id, source_text)
        })
    }

    fn parse(&self, source: &Source) -> Parsed {
        // TODO still performs two lookups instead of just one.
        // Can be avoided by lifting the requirement to pass the source text to the caller.
        // But could make for more awkward code.
        self.parsed.get(&source.file(), |_| parse(&source))
    }
}

impl ModuleDb for Program {
    fn resolve_module(&self, name: ModuleName) -> Option<ModuleId> {
        // FIXME: The fact that `resolve_module` returns the id only is ab it annoying.
        // What salsa does is that Module is only an id and the
        // `path` or `name` methods call into the database
        self.module_resolver.resolve(name)
    }

    fn module(&self, module_id: ModuleId) -> Module {
        self.module_resolver.module(module_id)
    }
}

impl Db for Program {}

#[cfg(test)]
mod tests {
    use crate::files::Files;
    use crate::module::{ModuleSearchPath, ModuleSearchPathKind};

    use super::*;

    #[test]
    fn resolve_imports() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path().canonicalize()?;
        let mod1 = root.join("mod1.py");
        let mod2 = root.join("mod2.py");
        std::fs::write(&mod1, "class C: pass")?;
        std::fs::write(&mod2, "from mod1 import C")?;

        let program = Program::new(
            vec![ModuleSearchPath::new(
                root,
                ModuleSearchPathKind::FirstParty,
            )],
            Files::default(),
        );
        let imported_symbol_names = program.analyze_imports(ModuleName::new("mod2")).unwrap();

        assert_eq!(imported_symbol_names, vec!["C"]);

        Ok(())
    }
}

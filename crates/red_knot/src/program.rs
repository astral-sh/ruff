use std::path::Path;
use std::sync::Arc;

use crate::cache::Cache;
use crate::db::{Db, HasJar, ModuleDb, SourceDb, SourceJar, SourceStorage};
use crate::files::{FileId, Files};
use crate::module::{
    add_module, path_to_module, resolve_module, set_module_search_paths, Module, ModuleData,
    ModuleName, ModuleResolver, ModuleSearchPath,
};
use crate::parse::{parse, Parsed, ParsedStorage};
use crate::source::{source_text, Source};
use crate::symbols::Symbols;

#[derive(Debug)]
pub struct Program {
    files: Files,
    source: SourceJar,
}

impl Program {
    pub fn new(module_search_paths: Vec<ModuleSearchPath>, files: Files) -> Self {
        Self {
            source: SourceJar {
                module_resolver: ModuleResolver::new(module_search_paths),
                sources: SourceStorage::default(),
                parsed: ParsedStorage::default(),
            },
            files,
        }
    }

    fn analyze_imports(&self, name: ModuleName) -> Vec<String> {
        if let Some(module) = self.resolve_module(name) {
            let parsed = self.parse(module.path(self).file());
            let symbols = Symbols::from_ast(parsed.ast());
            symbols
                .table
                .root_symbol_ids()
                .map(|symbol_id| {
                    if let Some(defs) = &symbols.defs.get(&symbol_id) {
                        format!("{} defs", defs.len())
                    } else {
                        "undef".to_owned()
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn file_changed(&mut self, path: &Path) {
        let Some(file_id) = self.files.try_get(path) else {
            return;
        };

        self.source.module_resolver.remove_module(path);
        self.source.sources.remove(&file_id);
        self.source.parsed.remove(&file_id);
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
        source_text(self, file_id)
    }

    fn parse(&self, file_id: FileId) -> Parsed {
        parse(self, file_id)
    }
}

impl ModuleDb for Program {
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

impl Db for Program {}

impl HasJar<SourceJar> for Program {
    fn jar(&self) -> &SourceJar {
        &self.source
    }

    fn jar_mut(&mut self) -> &mut SourceJar {
        &mut self.source
    }
}

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
        std::fs::write(mod1, "class C: pass")?;
        std::fs::write(mod2, "from mod1 import C")?;

        let program = Program::new(
            vec![ModuleSearchPath::new(
                root,
                ModuleSearchPathKind::FirstParty,
            )],
            Files::default(),
        );
        let imported_symbol_names = program.analyze_imports(ModuleName::new("mod2"));

        // TODO should be "C"
        assert_eq!(imported_symbol_names, vec!["1 defs"]);

        Ok(())
    }
}

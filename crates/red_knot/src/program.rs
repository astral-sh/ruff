#![allow(dead_code)]
use crate::module::{ModuleId, ModuleName, ModuleResolver};
use crate::parse::{parse, Parsed, SourceText};
use crate::symbols::Symbols;
use crate::FxDashMap;
use anyhow::Result;
use std::fs;
use std::sync::Arc;

struct Program {
    module_resolver: ModuleResolver,
    parsed: FxDashMap<ModuleId, Arc<Parsed>>,
}

impl Program {
    pub fn new(module_resolver: ModuleResolver) -> Self {
        Self {
            module_resolver,
            parsed: FxDashMap::default(),
        }
    }

    fn load_source(&self, module_id: ModuleId) -> Result<SourceText> {
        // TODO filesystem access doesn't belong here, this is temporary until we have VFS
        let path = self.module_resolver.path(module_id);
        let contents = fs::read_to_string(path)?;
        Ok(SourceText { text: contents })
    }

    fn parse(&self, module_id: ModuleId) -> Result<Arc<Parsed>> {
        let parsed = parse(&self.load_source(module_id)?);
        Ok(self
            .parsed
            .entry(module_id)
            .insert(Arc::new(parsed))
            .clone())
    }

    // TODO figure out the ownership to make this work
    //    fn symbols(&self, module_id: ModuleId) -> Result<Symbols> {
    //        Ok(Symbols::from_ast(&self.parse(module_id)?.ast))
    //    }

    fn analyze_imports(&self, name: ModuleName) -> Result<Vec<String>> {
        // TODO
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Files;
    use crate::module::{ModuleSearchPath, ModuleSearchPathKind};

    #[test]
    fn resolve_imports() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path().canonicalize()?;
        let mod1 = root.join("mod1.py");
        let mod2 = root.join("mod2.py");
        std::fs::write(&mod1, "class C: pass")?;
        std::fs::write(&mod2, "from mod1 import C")?;
        let resolver = ModuleResolver::new(
            vec![ModuleSearchPath::new(
                root,
                ModuleSearchPathKind::FirstParty,
            )],
            Files::default(),
        );
        let program = Program::new(resolver);
        let imported_symbol_names = program.analyze_imports(ModuleName::new("mod2")).unwrap();

        assert_eq!(imported_symbol_names, vec!["C"]);

        Ok(())
    }
}

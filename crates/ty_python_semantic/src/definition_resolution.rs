use smallvec::SmallVec;
use ty_module_resolver::Module;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::{
    BindingWithConstraintsIterator, Program, ProgramFile, global_scope, place_table, use_def_map,
};

use crate::Db;
use crate::reachability::ReachabilityConstraintsExtension;

/// Returns the definitions that may supply the value for a module global at the end of its scope.
pub(crate) fn definitions_for_module_global<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    module: Module<'db>,
    name: &str,
) -> Option<DefinitionResolution<'db>> {
    let file = ProgramFile::new(db, module.file(db)?, program);
    let scope = global_scope(db, file);
    let symbol = place_table(db, scope).symbol_id(name)?;

    Some(DefinitionResolution::from_bindings(
        db,
        use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
    ))
}

/// A set of definitions found by name resolution along with facts about their availability.
pub(crate) struct DefinitionResolution<'db> {
    definitions: SmallVec<[Definition<'db>; 2]>,
}

impl<'db> DefinitionResolution<'db> {
    /// Returns the definitions found by name resolution.
    pub(crate) fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    fn from_bindings(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut resolution = Self {
            definitions: SmallVec::new(),
        };

        while let Some(binding) = bindings.next() {
            let reachability = bindings.reachability_constraints().evaluate(
                db,
                bindings.predicates(),
                binding.reachability_constraint,
            );
            if reachability.is_always_false() {
                continue;
            }

            match binding.binding {
                DefinitionState::Defined(definition) => {
                    resolution.push_definition(definition);
                }
                DefinitionState::Deleted | DefinitionState::Undefined => {}
            }
        }

        resolution
    }

    fn push_definition(&mut self, definition: Definition<'db>) {
        if !self.definitions.contains(&definition) {
            self.definitions.push(definition);
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ty_python_core::ProgramFile;

    use super::definitions_for_module_global;
    use crate::SemanticModel;
    use crate::db::tests::TestDbBuilder;

    #[test]
    fn definitions_for_module_global_retains_conditional_definitions() {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/pkg/__init__.py",
                r#"
if flag:
    from . import first as value
else:
    from . import second as value
"#,
            )
            .with_file("/src/pkg/first.py", "")
            .with_file("/src/pkg/second.py", "")
            .with_file("/src/use.py", "import pkg")
            .build()
            .expect("valid TestDb setup");
        let file = system_path_to_file(&db, "/src/use.py").expect("test file should exist");
        let program = db.program_environment().program(&db);
        let model = SemanticModel::new(&db, ProgramFile::new(&db, file, program));
        let module = model
            .resolve_module(Some("pkg"), 0)
            .expect("test package should resolve");

        let resolution = definitions_for_module_global(&db, program, module, "value")
            .expect("module global should exist");

        assert_eq!(resolution.definitions().len(), 2);
    }
}

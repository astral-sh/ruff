use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState};
use ty_python_core::scope::ScopeId;
use ty_python_core::{BindingWithConstraintsIterator, global_scope, place_table, use_def_map};

use crate::Db;
use crate::place::{
    Place, builtins_module_scope, class_body_implicit_symbol, implicit_builtins_symbol,
    loop_header_reachability, module_type_implicit_global_symbol,
};
use crate::place_load::{ImplicitPlaceLoad, PlaceLoadSource, PlaceLoadSourceKind};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::ProgramEnvironment;

/// Records the definitions that can supply a name's value and the limits of that resolution.
///
/// A consumer needs more than the definitions to decide whether it can rewrite a name safely.
/// Resolution also tracks whether values lack explicit definitions, whether the name can be
/// deleted, and whether lookup crosses a `global` or `nonlocal` declaration.
#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DefinitionResolution<'db> {
    definitions: SmallVec<[Definition<'db>; 2]>,
    is_complete: bool,
    may_be_deleted: bool,
    crosses_scope_declaration: bool,
}

#[allow(
    dead_code,
    reason = "definition-resolution metadata is retained for IDE consumers"
)]
impl<'db> DefinitionResolution<'db> {
    /// Returns the definitions found by name resolution.
    pub(crate) fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Returns whether every possible result is represented by a definition.
    ///
    /// Implicit builtin values are incomplete because no explicit import connects the name
    /// to their definitions. A complete resolution can still leave a name possibly unbound.
    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Returns whether a reachable deletion may leave the value unbound.
    pub(crate) fn may_be_deleted(&self) -> bool {
        self.may_be_deleted
    }

    /// Returns whether resolution crosses a `global` or `nonlocal` declaration.
    pub(crate) fn crosses_scope_declaration(&self) -> bool {
        self.crosses_scope_declaration
    }

    fn from_place_load_source(
        db: &'db dyn Db,
        environment: &ProgramEnvironment<'db>,
        scope: ScopeId<'db>,
        source: &PlaceLoadSource<'db>,
    ) -> Self {
        match &source.kind {
            PlaceLoadSourceKind::Bindings(bindings) => Self::from_bindings(db, bindings.clone()),
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                Self::from_bindings(db, use_def_map(db, *scope).reachable_bindings(*id))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file,
                name,
            }) => {
                let scope = global_scope(db, *file);
                let Some(symbol) = place_table(db, scope).symbol_id(name) else {
                    return Self {
                        definitions: SmallVec::new(),
                        is_complete: true,
                        may_be_deleted: false,
                        crosses_scope_declaration: false,
                    };
                };
                Self::from_bindings(db, use_def_map(db, scope).reachable_symbol_bindings(symbol))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::DunderClass(class_def)) => {
                let mut resolution = Self {
                    definitions: SmallVec::new(),
                    is_complete: true,
                    may_be_deleted: false,
                    crosses_scope_declaration: false,
                };
                resolution.push_definition(*class_def);
                resolution
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ClassBodySymbol(name)) => {
                Self::from_place_without_definition(
                    class_body_implicit_symbol(db, environment, name).place,
                )
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ModuleImplicitGlobal {
                file,
                name,
            }) => Self::from_place_without_definition(
                module_type_implicit_global_symbol(db, *file, name).place,
            ),
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::Builtin(name)) => {
                Self::from_builtin(db, environment, scope, name)
            }
        }
    }

    fn from_builtin(
        db: &'db dyn Db,
        environment: &ProgramEnvironment<'db>,
        scope: ScopeId<'db>,
        name: &str,
    ) -> Self {
        if Some(scope) == builtins_module_scope(db, environment) {
            // A missing name in `builtins` cannot fall back to the module that is currently being
            // resolved. Treating it as undefined also avoids a recursive semantic query.
            return Self::from_place_without_definition(Place::Undefined);
        }

        // Builtin values have no explicit import for a consumer to follow. Record whether a
        // value exists without collecting definitions that consumers cannot use.
        Self::from_place_without_definition(implicit_builtins_symbol(db, environment, name).place)
    }

    /// Resolves the reachable definitions supplied by the given bindings.
    pub(crate) fn from_bindings(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut resolution = Self {
            definitions: SmallVec::new(),
            is_complete: true,
            may_be_deleted: false,
            crosses_scope_declaration: false,
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
                    if matches!(definition.kind(db), DefinitionKind::LoopHeader(_)) {
                        let deleted_reachability =
                            loop_header_reachability(db, definition).deleted_reachability;
                        let may_be_deleted = !deleted_reachability.is_always_false();
                        resolution.may_be_deleted |= may_be_deleted;
                    }
                    resolution.push_definition(definition);
                }
                DefinitionState::Deleted => {
                    let may_be_deleted = reachability.may_be_true();
                    resolution.may_be_deleted |= may_be_deleted;
                }
                DefinitionState::Undefined => {}
            }
        }

        resolution
    }

    fn push_definition(&mut self, definition: Definition<'db>) {
        if !self.definitions.contains(&definition) {
            self.definitions.push(definition);
        }
    }

    fn from_place_without_definition(place: Place<'db>) -> Self {
        Self {
            definitions: SmallVec::new(),
            is_complete: place.is_undefined(),
            may_be_deleted: false,
            crosses_scope_declaration: false,
        }
    }

    fn extend(&mut self, other: Self) {
        for definition in other.definitions {
            if !self.definitions.contains(&definition) {
                self.definitions.push(definition);
            }
        }
        self.is_complete &= other.is_complete;
        self.may_be_deleted |= other.may_be_deleted;
        self.crosses_scope_declaration |= other.crosses_scope_declaration;
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_trivia::textwrap::dedent;
    use ty_python_core::ProgramFile;
    use ty_python_core::semantic_index;

    use super::*;
    use crate::db::tests::TestDbBuilder;

    #[test]
    fn loop_header_resolution_reports_reachable_deletions() {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/test.py",
                &dedent(
                    "
                    def f(flag):
                        deleted = 1
                        retained = 1
                        while flag:
                            first = deleted
                            second = retained
                            del deleted
                            retained = 2
                    ",
                ),
            )
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let function = module.suite()[0].as_function_def_stmt().unwrap();
        let loop_statement = function.body[2].as_while_stmt().unwrap();
        let index = semantic_index(&db, file);

        for (statement, may_be_deleted) in [
            (&loop_statement.body[0], true),
            (&loop_statement.body[1], false),
        ] {
            let name = statement.as_assign_stmt().unwrap().value.as_ref();
            let scope = index.try_expression_scope_id(name).unwrap();
            let use_id = index.try_expression_use_id(name.into()).unwrap();
            let resolution = DefinitionResolution::from_bindings(
                &db,
                index.use_def_map(scope).bindings_at_use(use_id),
            );

            assert!(
                resolution.definitions().iter().any(|definition| {
                    matches!(definition.kind(&db), DefinitionKind::LoopHeader(_))
                }),
                "no loop header for {name:?}"
            );
            assert_eq!(resolution.may_be_deleted(), may_be_deleted, "{name:?}");
        }
    }

    #[test]
    fn implicit_builtin_resolution_is_incomplete() {
        let db = TestDbBuilder::new()
            .with_file("/src/test.py", "print")
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let environment = ProgramEnvironment::from_file(file);

        let resolution =
            DefinitionResolution::from_builtin(&db, &environment, global_scope(&db, file), "print");

        assert!(!resolution.is_complete());
        assert_eq!(resolution.definitions(), []);
    }
}

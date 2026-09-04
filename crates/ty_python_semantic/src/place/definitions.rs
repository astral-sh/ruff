use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    BindingWithConstraintsIterator, ProgramFile, global_scope, place_table, use_def_map,
};

use crate::Db;
use crate::place::{
    Place, builtins_module_scope, class_body_implicit_symbol, implicit_builtins_symbol,
    module_type_implicit_global_symbol,
};
use crate::place_load::{ImplicitPlaceLoad, PlaceLoadSource, PlaceLoadSourceKind};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::{ProgramEnvironment, user_visible_definitions};

/// Returns the definitions that may supply a module global at the end of its scope.
pub(crate) fn definitions_for_module_global<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &str,
) -> Option<DefinitionResolution<'db>> {
    let scope = global_scope(db, file);
    let symbol = place_table(db, scope).symbol_id(name)?;
    Some(DefinitionResolution::from_bindings(
        db,
        use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
    ))
}

/// The definitions that may bind a name and information about the resolution.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DefinitionResolution<'db> {
    /// Definitions that may bind the name.
    definitions: SmallVec<[Definition<'db>; 2]>,
    /// Whether the definitions include a source definition for every possible binding.
    is_complete: bool,
}

#[allow(dead_code, reason = "awaiting IDE integration")]
impl<'db> DefinitionResolution<'db> {
    pub(crate) fn new() -> Self {
        Self {
            definitions: SmallVec::new(),
            is_complete: true,
        }
    }

    /// Returns the definitions that may bind the name.
    pub fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Returns whether the definitions include a source definition for every possible binding.
    ///
    /// For example, implicit builtin values are incomplete because no explicit import connects the
    /// name to their definitions.
    ///
    /// Note that a complete resolution can still leave a name possibly unbound.
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn into_source_definitions(mut self, db: &'db dyn Db) -> Self {
        let definitions = std::mem::take(&mut self.definitions);
        for definition in definitions {
            let sources = user_visible_definitions(db, [definition]);
            self.is_complete &= !sources.is_empty();
            for source in sources {
                self.push_definition(source);
            }
        }
        self
    }

    pub(crate) fn add_source(
        &mut self,
        db: &'db dyn Db,
        environment: &ProgramEnvironment<'db>,
        scope: ScopeId<'db>,
        source: &PlaceLoadSource<'db>,
    ) {
        self.extend(Self::from_place_load_source(db, environment, scope, source));
    }

    pub(crate) fn mark_incomplete(&mut self) {
        self.is_complete = false;
    }

    pub(crate) fn finish(mut self) -> Self {
        self.definitions.shrink_to_fit();
        self
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
                    return Self::new();
                };
                Self::from_bindings(db, use_def_map(db, scope).reachable_symbol_bindings(symbol))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::DunderClass(class_def)) => {
                let mut resolution = Self::new();
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

    /// Collects reachable definitions.
    pub(crate) fn from_bindings(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut resolution = Self::new();

        while let Some(binding) = bindings.next() {
            if let DefinitionState::Defined(definition) = binding.binding
                && !bindings
                    .reachability_constraints()
                    .evaluate(db, bindings.predicates(), binding.reachability_constraint)
                    .is_always_false()
            {
                resolution.push_definition(definition);
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
            is_complete: place.is_undefined(),
            ..Self::new()
        }
    }

    fn extend(&mut self, other: Self) {
        for definition in other.definitions {
            self.push_definition(definition);
        }
        self.is_complete &= other.is_complete;
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::Ranged;
    use ty_python_core::ProgramFile;
    use ty_python_core::place::PlaceExpr;
    use ty_python_core::semantic_index;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::place_load::{PlaceLoadMode, PlaceLoadResolutionStep, resolve_place_load};

    #[test]
    fn resolution_collects_and_merges_reachable_definitions() {
        let source = dedent(
            "
            value = 1
            if condition:
                value = 2
            if condition:
                del value
            other = 4
            read = value
            ",
        );
        let db = TestDbBuilder::new()
            .with_file("/src/test.py", &source)
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let scope = global_scope(&db, file);
        let environment = ProgramEnvironment::from_file(file);
        let name = module.suite()[4]
            .as_assign_stmt()
            .unwrap()
            .value
            .as_name_expr()
            .unwrap();
        let mut sources = resolve_place_load(
            &db,
            semantic_index(&db, file),
            scope,
            PlaceExpr::from_expr_name(name),
            PlaceLoadMode::AtExpression(name.into()),
        );
        let Some(PlaceLoadResolutionStep::Source(binding_source)) = sources.next() else {
            panic!("expected a binding source");
        };
        let mut resolution =
            DefinitionResolution::from_place_load_source(&db, &environment, scope, &binding_source)
                .finish();

        let mut definitions: Vec<_> = resolution
            .definitions()
            .iter()
            .map(|definition| &source[definition.full_range(&db, &module).range()])
            .collect();
        definitions.sort_unstable();
        assert_eq!(definitions, ["value = 1", "value = 2"]);
        assert!(resolution.is_complete());

        let other = definitions_for_module_global(&db, file, "other")
            .unwrap()
            .into_source_definitions(&db);
        resolution.extend(other);
        resolution.extend(resolution.clone());
        assert_eq!(resolution.definitions().len(), 3);

        assert!(
            DefinitionResolution::from_builtin(&db, &environment, scope, "value").is_complete()
        );
        resolution.extend(DefinitionResolution::from_builtin(
            &db,
            &environment,
            scope,
            "print",
        ));
        assert!(!resolution.is_complete());
    }
}

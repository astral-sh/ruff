use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState};
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    BindingWithConstraintsIterator, BoundnessAnalysis, global_scope, place_table, use_def_map,
};

use crate::Db;
use crate::place::{
    Place, RequiresExplicitReExport, builtins_module_scope, class_body_implicit_symbol,
    implicit_builtins_symbol, implicit_builtins_symbol_scope, is_reexported,
    loop_header_reachability, module_type_implicit_global_symbol,
};
use crate::place_load::{ImplicitPlaceLoad, PlaceLoadSource, PlaceLoadSourceKind};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::ProgramEnvironment;

/// A set of definitions found by name resolution along with facts about their availability.
#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "these flags describe independent facts about the resolved definitions"
)]
pub(crate) struct DefinitionResolution<'db> {
    definitions: SmallVec<[Definition<'db>; 2]>,
    is_complete: bool,
    may_be_unbound: bool,
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
    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Returns whether the value is bound on every reachable control-flow path.
    pub(crate) fn is_definitely_bound(&self) -> bool {
        !self.may_be_unbound
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
                        may_be_unbound: true,
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
                    may_be_unbound: false,
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

        let Some(builtins_scope) = implicit_builtins_symbol_scope(db, environment, name) else {
            // No runtime-visible builtin supplies this name.
            return Self::from_place_without_definition(Place::Undefined);
        };

        if builtins_scope == scope {
            // End-of-scope lookup in a project-level `__builtins__` module can select a binding
            // that occurs after this load. Keep its inferred value unrepresented instead of
            // exposing that later binding as the load's source definition.
            return Self::from_place_without_definition(
                implicit_builtins_symbol(db, environment, name).place,
            );
        }

        let Some(symbol) = place_table(db, builtins_scope).symbol_id(name) else {
            // The module supplies this name through a synthetic module attribute rather than an
            // explicit symbol, so there is no source definition to expose.
            return Self::from_place_without_definition(
                implicit_builtins_symbol(db, environment, name).place,
            );
        };

        let mut resolution = Self::from_bindings_with_reexport_requirement(
            db,
            use_def_map(db, builtins_scope).end_of_scope_symbol_bindings(symbol),
            RequiresExplicitReExport::Yes,
        );

        // The target definitions are useful for navigation, but the implicit fallback that
        // connects this load to them has no source representation that a refactor can safely
        // rewrite.
        resolution.is_complete = false;

        resolution
    }

    /// Resolves the reachable definitions supplied by the given bindings.
    pub(crate) fn from_bindings(
        db: &'db dyn Db,
        bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        Self::from_bindings_with_reexport_requirement(db, bindings, RequiresExplicitReExport::No)
    }

    fn from_bindings_with_reexport_requirement(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
        requires_explicit_reexport: RequiresExplicitReExport,
    ) -> Self {
        let mut resolution = Self {
            definitions: SmallVec::new(),
            is_complete: true,
            may_be_unbound: false,
            may_be_deleted: false,
            crosses_scope_declaration: false,
        };
        let boundness = bindings.boundness_analysis();
        let mut has_defined_binding = false;

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
                DefinitionState::Defined(definition)
                    if matches!(requires_explicit_reexport, RequiresExplicitReExport::Yes)
                        && !is_reexported(db, definition) =>
                {
                    resolution.may_be_unbound |= reachability.may_be_true();
                }
                DefinitionState::Defined(definition) => {
                    if matches!(definition.kind(db), DefinitionKind::LoopHeader(_)) {
                        let deleted_reachability =
                            loop_header_reachability(db, definition).deleted_reachability;
                        let may_be_deleted = !deleted_reachability.is_always_false();
                        resolution.may_be_unbound |= may_be_deleted;
                        resolution.may_be_deleted |= may_be_deleted;
                    }
                    has_defined_binding = true;
                    resolution.push_definition(definition);
                }
                DefinitionState::Deleted => {
                    let may_be_deleted = reachability.may_be_true();
                    resolution.may_be_unbound |= may_be_deleted;
                    resolution.may_be_deleted |= may_be_deleted;
                }
                DefinitionState::Undefined
                    if boundness == BoundnessAnalysis::BasedOnUnboundVisibility =>
                {
                    resolution.may_be_unbound |= reachability.may_be_true();
                }
                DefinitionState::Undefined => {}
            }
        }

        if !has_defined_binding {
            resolution.may_be_unbound = true;
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
            may_be_unbound: !place.is_definitely_bound(),
            may_be_deleted: false,
            crosses_scope_declaration: false,
        }
    }

    /// Returns whether resolution found a value, including one without a source definition.
    fn has_value(&self) -> bool {
        !self.definitions.is_empty() || !self.is_complete
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

/// Accumulates definition information while type inference resolves a place load.
pub(crate) struct DefinitionResolutionBuilder<'db> {
    resolution: DefinitionResolution<'db>,
}

impl<'db> DefinitionResolutionBuilder<'db> {
    pub(crate) fn new() -> Self {
        Self {
            resolution: DefinitionResolution {
                definitions: SmallVec::new(),
                is_complete: true,
                may_be_unbound: true,
                may_be_deleted: false,
                crosses_scope_declaration: false,
            },
        }
    }

    pub(crate) fn add_source(
        &mut self,
        db: &'db dyn Db,
        environment: &ProgramEnvironment<'db>,
        scope: ScopeId<'db>,
        source: &PlaceLoadSource<'db>,
    ) {
        let mut source_resolution =
            DefinitionResolution::from_place_load_source(db, environment, scope, source);
        if source.is_class_body_global_fallback() && source_resolution.has_value() {
            source_resolution.may_be_unbound = false;
        }
        self.resolution.extend(source_resolution);
    }

    pub(crate) fn mark_incomplete(&mut self) {
        self.resolution.is_complete = false;
    }

    pub(crate) fn finish(
        mut self,
        is_definitely_bound: bool,
        crosses_scope_declaration: bool,
    ) -> DefinitionResolution<'db> {
        self.resolution.may_be_unbound = !is_definitely_bound;
        self.resolution.crosses_scope_declaration |= crosses_scope_declaration;
        self.resolution.definitions.shrink_to_fit();
        self.resolution
    }
}

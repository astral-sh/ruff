use ruff_python_ast::{self as ast, ExprRef};
use smallvec::SmallVec;
use ty_python_core::SemanticIndex;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::ScopeId;
use ty_python_core::{BindingWithConstraintsIterator, global_scope, place_table, use_def_map};

use crate::Db;
use crate::place::{
    Place, PlaceAndQualifiers, builtins_module_scope, class_body_implicit_symbol,
    implicit_builtins_symbol, module_type_implicit_global_symbol,
};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadFailure, PlaceLoadMode, PlaceLoadResolutionStep, PlaceLoadSource,
    PlaceLoadSourceKind, evaluate_place_load_source, resolve_place_load,
    unimported_reveal_type_fallback,
};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::ProgramEnvironment;

/// Resolves a name using the same ordered sources as type inference.
pub(crate) fn reaching_definitions<'db>(
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    scope: ScopeId<'db>,
    name: &ast::ExprName,
    mode: PlaceLoadMode<'_>,
) -> DefinitionResolution<'db> {
    let env = ProgramEnvironment::from_scope(scope);
    let mut load = resolve_place_load(db, index, scope, PlaceExpr::from_expr_name(name), mode);
    let mut resolution = DefinitionResolution::new();
    let mut place = PlaceAndQualifiers::from(Place::Undefined);
    let mut failure = None;

    for step in load.by_ref() {
        match step {
            PlaceLoadResolutionStep::Source(source) => {
                resolution.add_source(db, &env, scope, &source);
                place = place.or_fall_back_to(db, &env, || {
                    evaluate_place_load_source(db, &env, scope, source, None)
                });
                if place.place.is_definitely_bound() {
                    break;
                }
            }
            PlaceLoadResolutionStep::MemberResolutionCondition(_) => break,
            PlaceLoadResolutionStep::Exhausted(exhaustion_failure) => {
                failure = Some(exhaustion_failure);
                break;
            }
        }
    }

    resolution.crosses_scope_declaration = load.crosses_scope_declaration();

    // A possibly unbound `reveal_type` can fall back to ty's implicit helper. There is no
    // explicit source binding for that fallback, so it makes the resolution incomplete.
    if failure == Some(PlaceLoadFailure::NotFound)
        && !place.place.is_definitely_bound()
        && !unimported_reveal_type_fallback(db, &env, ExprRef::Name(name)).is_undefined()
    {
        resolution.is_complete = false;
    }

    resolution
}

/// The definitions that may bind a name and information about the resolution.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DefinitionResolution<'db> {
    /// Definitions that may bind the name.
    definitions: SmallVec<[Definition<'db>; 2]>,
    /// Whether lookup crosses a `global` or `nonlocal` declaration.
    crosses_scope_declaration: bool,
    /// Whether the definitions include a source definition for every possible binding.
    is_complete: bool,
}

#[allow(dead_code, reason = "awaiting IDE integration")]
impl<'db> DefinitionResolution<'db> {
    fn new() -> Self {
        Self {
            definitions: SmallVec::new(),
            crosses_scope_declaration: false,
            is_complete: true,
        }
    }

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
                resolution.push_definition(db, definition);
            }
        }

        resolution
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
                resolution.push_definition(db, *class_def);

                // The class definition identifies the class referenced by `__class__`, but
                // Python creates the `__class__` binding implicitly, so it has no source definition.
                resolution.is_complete = false;

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
            // Skip builtin fallback in the `builtins` global scope to avoid a recursive lookup.
            return Self::from_place_without_definition(Place::Undefined);
        }

        // Builtin values have no explicit import for a consumer to follow. Record whether a
        // value exists without collecting definitions that consumers cannot use.
        Self::from_place_without_definition(implicit_builtins_symbol(db, environment, name).place)
    }

    fn from_place_without_definition(place: Place<'db>) -> Self {
        Self {
            is_complete: place.is_undefined(),
            ..Self::new()
        }
    }

    fn add_source(
        &mut self,
        db: &'db dyn Db,
        environment: &ProgramEnvironment<'db>,
        scope: ScopeId<'db>,
        source: &PlaceLoadSource<'db>,
    ) {
        self.extend(
            db,
            Self::from_place_load_source(db, environment, scope, source),
        );
    }

    fn push_definition(&mut self, db: &'db dyn Db, definition: Definition<'db>) {
        self.is_complete &= definition.kind(db).is_user_visible();
        if !self.definitions.contains(&definition) {
            self.definitions.push(definition);
        }
    }

    fn extend(&mut self, db: &'db dyn Db, other: Self) {
        for definition in other.definitions {
            self.push_definition(db, definition);
        }
        self.is_complete &= other.is_complete;
    }

    /// Returns the definitions that may bind the name.
    ///
    /// This can include synthetic definitions, such as loop headers.
    pub fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Returns whether lookup crosses a `global` or `nonlocal` declaration.
    pub fn crosses_scope_declaration(&self) -> bool {
        self.crosses_scope_declaration
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
}

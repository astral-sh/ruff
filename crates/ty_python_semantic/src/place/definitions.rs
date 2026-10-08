use std::collections::VecDeque;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use ty_python_core::definition::{
    Definition, DefinitionKind, DefinitionState, NestedBindingExecution,
};
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    BindingWithConstraintsIterator, global_scope, place_table, semantic_index, use_def_map,
};

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
pub struct DefinitionResolution<'db> {
    definitions: SmallVec<[Definition<'db>; 2]>,
    is_complete: bool,
    may_be_deleted: bool,
    crosses_scope_declaration: bool,
}

impl<'db> DefinitionResolution<'db> {
    /// Returns the definitions found by name resolution.
    pub fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Returns whether every possible result is represented by a definition.
    ///
    /// Implicit builtin values are incomplete because no explicit import connects the name
    /// to their definitions. A complete resolution can still leave a name possibly unbound.
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Returns whether a reachable deletion may leave the value unbound.
    pub fn may_be_deleted(&self) -> bool {
        self.may_be_deleted
    }

    /// Returns whether resolution crosses a `global` or `nonlocal` declaration.
    pub fn crosses_scope_declaration(&self) -> bool {
        self.crosses_scope_declaration
    }

    /// Replaces synthetic bindings with the source definitions they represent.
    pub(crate) fn source_backed(mut self, db: &'db dyn Db) -> Self {
        for root in std::mem::take(&mut self.definitions) {
            if root.kind(db).is_user_visible() {
                self.push_definition(root);
                continue;
            }

            let mut pending = VecDeque::from([root]);
            let mut seen = FxHashSet::default();
            let mut has_source = false;

            while let Some(definition) = pending.pop_front() {
                if !seen.insert(definition) {
                    continue;
                }
                match definition.kind(db) {
                    DefinitionKind::LoopHeader(_) => {
                        let header = loop_header_reachability(db, definition);
                        self.may_be_deleted |= !header.deleted_reachability.is_always_false();
                        pending.extend(
                            header
                                .reachable_bindings
                                .iter()
                                .map(|binding| binding.definition),
                        );
                    }
                    DefinitionKind::NestedBindings(nested) => {
                        let index = semantic_index(db, definition.program_file(db));
                        for bindings in
                            nested.visible_binding_sources(index, definition.file_scope(db))
                        {
                            if nested.execution == NestedBindingExecution::Eager {
                                // Like inference, include bindings from later comprehension
                                // iterations even when the first iteration cannot reach them.
                                pending.extend(
                                    bindings.filter_map(|binding| binding.binding.definition()),
                                );
                            } else {
                                let source = Self::from_bindings(db, bindings);
                                self.may_be_deleted |= source.may_be_deleted;
                                pending.extend(source.definitions);
                            }
                        }
                    }
                    kind if kind.is_user_visible() => {
                        has_source = true;
                        self.push_definition(definition);
                    }
                    _ => self.is_complete = false,
                }
            }
            self.is_complete &= has_source;
        }
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

/// Accumulates the definitions and flags from sources visited during name inference.
pub(crate) struct DefinitionResolutionBuilder<'db> {
    resolution: DefinitionResolution<'db>,
}

impl<'db> DefinitionResolutionBuilder<'db> {
    pub(crate) fn new() -> Self {
        Self {
            resolution: DefinitionResolution {
                definitions: SmallVec::new(),
                is_complete: true,
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
        self.resolution
            .extend(DefinitionResolution::from_place_load_source(
                db,
                environment,
                scope,
                source,
            ));
    }

    pub(crate) fn mark_incomplete(&mut self) {
        self.resolution.is_complete = false;
    }

    pub(crate) fn finish(mut self, crosses_scope_declaration: bool) -> DefinitionResolution<'db> {
        self.resolution.crosses_scope_declaration |= crosses_scope_declaration;
        self.resolution.definitions.shrink_to_fit();
        self.resolution
    }
}

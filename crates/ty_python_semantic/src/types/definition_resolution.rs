//! Source-definition resolution shared by type inference and IDE features.
//!
//! Name lookup reads binding and declaration information from the semantic index. Member
//! lookup takes an already-inferred receiver type. Neither lookup obtains use-site types
//! through `SemanticModel` or requests completed inference of the caller's scope.

use std::collections::VecDeque;

use indexmap::IndexSet;
use itertools::Either;
use ruff_db::files::FileRange;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use ty_module_resolver::{
    ImportingFile, ModuleName, resolve_module, resolve_module_for_import_from,
};
use ty_python_core::definition::{
    Definition, DefinitionCategory, DefinitionKind, NestedBindingExecution,
};
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    BindingWithConstraints, DeclarationWithConstraint, ProgramFile, attribute_scopes, global_scope,
    place_table, semantic_index, use_def_map,
};

use crate::place::implicit_builtins_symbol_scope;
use crate::types::{ClassBase, ClassLiteral, ClassType, SubclassOfInner, Type, binding_type};
use crate::{Db, FxIndexSet, ProgramEnvironment, module_docstring};

/// Controls whether local import aliases should be resolved to their targets or returned as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAliasResolution {
    /// Resolve import aliases to their original definitions
    ResolveAliases,
    /// Keep import aliases as-is, don't resolve to original definitions
    PreserveAliases,
}

/// A binding or declaration encountered while resolving a name to its source definitions.
///
/// This includes intermediate imports, whose guards can determine the value of a name even when
/// the final definition is unconditional. The reachability constraint belongs to `scope`'s use-def
/// map; constraints from different scopes must be inspected separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DefinitionOrigin<'db> {
    pub(crate) definition: Definition<'db>,
    pub(crate) scope: ScopeId<'db>,
    pub(crate) reachability: ScopedReachabilityConstraintId,
}

impl<'db> DefinitionOrigin<'db> {
    fn from_binding(
        scope: ScopeId<'db>,
        binding: &BindingWithConstraints<'_, 'db>,
    ) -> Option<Self> {
        Some(Self {
            definition: binding.binding.definition()?,
            scope,
            reachability: binding.reachability_constraint,
        })
    }

    fn from_declaration(
        scope: ScopeId<'db>,
        declaration: &DeclarationWithConstraint<'db>,
    ) -> Option<Self> {
        Some(Self {
            definition: declaration.declaration.definition()?,
            scope,
            reachability: declaration.reachability_constraint,
        })
    }
}

/// Source definitions together with the bindings and declarations encountered while resolving them.
///
/// Origins include intermediate imports and retain their reachability constraints. They describe
/// the lookup as a whole rather than individual paths from an import to its targets.
#[derive(Debug)]
pub(crate) struct DefinitionProvenance<'db> {
    pub(crate) definitions: Vec<ResolvedDefinition<'db>>,
    pub(crate) origins: Vec<DefinitionOrigin<'db>>,
}

/// Records origins for diagnostics without allocating provenance for navigation-only callers.
#[derive(Default)]
struct OriginCollector<'db> {
    origins: Option<FxIndexSet<DefinitionOrigin<'db>>>,
}

impl<'db> OriginCollector<'db> {
    fn enabled() -> Self {
        Self {
            origins: Some(FxIndexSet::default()),
        }
    }

    fn record(&mut self, origin: DefinitionOrigin<'db>) {
        if let Some(origins) = &mut self.origins {
            origins.insert(origin);
        }
    }

    fn into_origins(self) -> Vec<DefinitionOrigin<'db>> {
        self.origins.into_iter().flatten().collect()
    }
}

struct ImportResolution<'a, 'db> {
    visited: &'a mut FxHashSet<Definition<'db>>,
    alias_resolution: ImportAliasResolution,
    origins: &'a mut OriginCollector<'db>,
}

/// Represents the result of resolving an import to either a specific definition or
/// a specific range within a file.
/// This enum helps distinguish between cases where an import resolves to:
/// - A specific definition within a module (e.g., `from os import path` -> definition of `path`)
/// - A specific range within a file, sometimes an empty range at the top of the file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedDefinition<'db> {
    /// The import resolved to a specific definition within a module
    Definition(Definition<'db>),
    /// The import resolved to an entire module
    Module(ProgramFile<'db>),
    /// The import resolved to a file with a specific range
    FileWithRange(FileRange),
}

impl<'db> ResolvedDefinition<'db> {
    pub fn focus_range(&self, db: &dyn Db) -> FileRange {
        match self {
            ResolvedDefinition::Definition(definition) => {
                let parsed = parsed_module(db, definition.python_file(db)).load(db);
                definition.focus_range(db, &parsed)
            }
            // For modules, navigate to the start of the file
            ResolvedDefinition::Module(module) => {
                FileRange::new(module.file(db), TextRange::default())
            }
            ResolvedDefinition::FileWithRange(file_range) => *file_range,
        }
    }

    pub(crate) fn category(&self, db: &dyn Db) -> DefinitionCategory {
        match self {
            ResolvedDefinition::Definition(definition) => {
                let file = definition.file(db);
                let parsed = parsed_module(db, definition.python_file(db)).load(db);
                definition.kind(db).category(file.is_stub(db), &parsed)
            }
            ResolvedDefinition::Module(_) | ResolvedDefinition::FileWithRange(_) => {
                DefinitionCategory::DeclarationAndBinding
            }
        }
    }

    pub fn definition(&self) -> Option<Definition<'db>> {
        match self {
            ResolvedDefinition::Definition(definition) => Some(*definition),
            ResolvedDefinition::Module(_) => None,
            ResolvedDefinition::FileWithRange(_) => None,
        }
    }

    pub(crate) fn program_file(&self, db: &'db dyn Db) -> Option<ProgramFile<'db>> {
        match *self {
            ResolvedDefinition::Definition(definition) => Some(definition.program_file(db)),
            ResolvedDefinition::Module(file) => Some(file),
            ResolvedDefinition::FileWithRange(_) => None,
        }
    }

    pub fn docstring(&self, db: &'db dyn Db) -> Option<String> {
        match self {
            ResolvedDefinition::Definition(definition) => definition.docstring(db),
            ResolvedDefinition::Module(file) => module_docstring(db, file.python_file(db)),
            ResolvedDefinition::FileWithRange(_) => None,
        }
    }

    pub fn implementation_docstring(&self, db: &'db dyn Db) -> Option<String> {
        match self {
            ResolvedDefinition::Definition(definition) => implementation_docstring(db, *definition),
            ResolvedDefinition::Module(_) | ResolvedDefinition::FileWithRange(_) => None,
        }
    }
}

// Overload declarations often omit docstrings, while the runtime
// implementation appears as the last sibling binding for the same symbol.
// Fall back to that binding's docstring when the resolved overload has none.
//
// Uses type-aware matching: resolves each end-of-scope binding's type to a
// function literal, then checks whether that function's overloads contain the
// current definition. This correctly handles version-conditional branches and
// avoids picking up unrelated reassignments of the same name.
fn implementation_docstring<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Option<String> {
    let DefinitionKind::Function(_) = definition.kind(db) else {
        return None;
    };

    let name = definition.name(db)?;
    let scope = definition.scope(db);
    let symbol_id = place_table(db, scope).symbol_id(&name)?;
    let use_def = use_def_map(db, scope);

    let current_overload = binding_type(db, definition)
        .as_function_literal()?
        .literal(db)
        .last_definition;

    // Find the last end-of-scope binding whose function type contains this overload.
    let implementation = use_def
        .end_of_scope_symbol_bindings(symbol_id)
        .filter_map(|binding| {
            let ty = binding_type(db, binding.binding.definition()?).as_function_literal()?;
            ty.iter_overloads_and_implementation(db)
                .any(|overload| overload == current_overload)
                .then_some(ty)
        })
        .last()?;

    implementation.definition(db).docstring(db)
}

/// Resolves a name's source definitions in `scope` and its visible ancestors, falling back to
/// implicit builtins if none are found.
///
/// This function reads bindings and declarations from the semantic index without asking
/// `SemanticModel` to infer the name expression's type. It can therefore be used during inference
/// of the enclosing scope. Otherwise, requesting the expression's type through `SemanticModel`
/// could require that same scope's inference to finish, creating an inference cycle.
///
/// Python's numeric compatibility rules mean that a `float` annotation accepts `int` values,
/// and a `complex` annotation accepts both `int` and `float` values. For these builtin names,
/// this function returns only the named class's definition. For editor navigation,
/// [`ide_support::definitions_for_name`](super::ide_support::definitions_for_name) uses the
/// inferred expression type to recognize these annotations and include the additional numeric classes
/// as navigation targets.
pub(crate) fn definitions_for_name<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    definitions_for_name_impl(
        db,
        scope,
        name,
        alias_resolution,
        &mut OriginCollector::default(),
    )
}

/// Returns a name's source definitions and the binding origins along its import chains.
///
/// Unlike navigation targets, these origins retain the guards on intermediate imports and the
/// constraints on the final definitions. This uses the same scope and import lookup as
/// [`definitions_for_name`], so callers can inspect provenance during scope inference.
pub(crate) fn definition_provenance_for_name<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> DefinitionProvenance<'db> {
    let mut origins = OriginCollector::enabled();
    let definitions = definitions_for_name_impl(
        db,
        scope,
        name,
        ImportAliasResolution::ResolveAliases,
        &mut origins,
    );
    DefinitionProvenance {
        definitions,
        origins: origins.into_origins(),
    }
}

fn definitions_for_name_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    alias_resolution: ImportAliasResolution,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    let definitions = scoped_definitions_for_name_impl(db, scope, name, alias_resolution, origins);
    if !definitions.is_empty() {
        return definitions;
    }
    let env = ProgramEnvironment::from_scope(scope);
    implicit_builtins_symbol_scope(db, &env, name)
        .map(|scope| definitions_for_builtin_impl(db, scope, name, origins))
        .unwrap_or_default()
}

/// Resolves definitions in visible scopes, without falling back to implicit builtins.
pub(crate) fn scoped_definitions_for_name<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name_str: &str,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    scoped_definitions_for_name_impl(
        db,
        scope,
        name_str,
        alias_resolution,
        &mut OriginCollector::default(),
    )
}

fn scoped_definitions_for_name_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name_str: &str,
    alias_resolution: ImportAliasResolution,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    let env = ProgramEnvironment::from_scope(scope);
    let file = scope.program_file(db);
    let index = semantic_index(db, file);
    let file_scope = scope.file_scope_id(db);

    let mut all_definitions = FxIndexSet::default();

    // Search through the scope hierarchy: start from the current scope and
    // traverse up through parent scopes to find definitions
    for (scope_id, _scope) in index.visible_ancestor_scopes(file_scope) {
        let place_table = index.place_table(scope_id);

        let Some(symbol_id) = place_table.symbol_id(name_str) else {
            continue; // Name not found in this scope, try parent scope
        };

        let use_def_map = index.use_def_map(scope_id);
        let binding_scope = scope_id.to_scope_id(db, file);

        // Check if this place is marked as global or nonlocal
        let place_expr = place_table.symbol(symbol_id);
        let is_global = place_expr.is_global();
        let is_nonlocal = place_expr.is_nonlocal();

        if is_global || is_nonlocal {
            // Assignments in a forwarding scope remain valid navigation targets, including eager
            // walrus bindings exported from comprehensions.
            all_definitions.extend(user_visible_origins(
                db,
                use_def_map
                    .reachable_symbol_bindings(symbol_id)
                    .filter_map(|binding| DefinitionOrigin::from_binding(binding_scope, &binding))
                    .filter(|origin| match origin.definition.kind(db) {
                        DefinitionKind::NamedExpression(_) => true,
                        DefinitionKind::NestedBindings(nested) => {
                            nested.execution == NestedBindingExecution::Eager
                        }
                        _ => false,
                    }),
                origins,
            ));
        }

        // TODO: The current algorithm doesn't return definitions or bindings
        // for other scopes that are outside of this scope hierarchy that target
        // this name using a nonlocal or global binding. The semantic analyzer
        // doesn't appear to track these in a way that we can easily access
        // them from here without walking all scopes in the module.

        // If marked as global, skip to global scope
        if is_global {
            let global_scope_id = global_scope(db, file);
            let global_place_table = ty_python_core::place_table(db, global_scope_id);

            if let Some(global_symbol_id) = global_place_table.symbol_id(name_str) {
                let global_use_def_map = ty_python_core::use_def_map(db, global_scope_id);
                all_definitions.extend(user_visible_origins(
                    db,
                    global_use_def_map
                        .reachable_symbol_bindings(global_symbol_id)
                        .filter_map(|binding| {
                            DefinitionOrigin::from_binding(global_scope_id, &binding)
                        })
                        .chain(
                            global_use_def_map
                                .reachable_symbol_declarations(global_symbol_id)
                                .filter_map(|declaration| {
                                    DefinitionOrigin::from_declaration(
                                        global_scope_id,
                                        &declaration,
                                    )
                                }),
                        ),
                    origins,
                ));
            }
            break;
        }

        // If marked as nonlocal, skip current scope and search in ancestor scopes
        if is_nonlocal {
            // Continue searching in parent scopes, but skip the current scope
            continue;
        }

        // Get all definitions (both bindings and declarations) for this place
        all_definitions.extend(user_visible_origins(
            db,
            use_def_map
                .reachable_symbol_bindings(symbol_id)
                .filter_map(|binding| DefinitionOrigin::from_binding(binding_scope, &binding))
                .chain(
                    use_def_map
                        .reachable_symbol_declarations(symbol_id)
                        .filter_map(|declaration| {
                            DefinitionOrigin::from_declaration(binding_scope, &declaration)
                        }),
                ),
            origins,
        ));

        // If we found definitions in this scope, we can stop searching
        if !all_definitions.is_empty() {
            break;
        }
    }

    // Resolve import definitions to their targets
    let mut resolved_definitions = Vec::new();

    for definition in &all_definitions {
        let resolved = resolve_definition_impl(
            db,
            &env,
            *definition,
            Some(name_str),
            alias_resolution,
            origins,
        );
        resolved_definitions.extend(resolved);
    }

    resolved_definitions
}

/// Resolves a symbol in an implicit builtins scope.
pub(crate) fn definitions_for_builtin<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> Vec<ResolvedDefinition<'db>> {
    definitions_for_builtin_impl(db, scope, name, &mut OriginCollector::default())
}

fn definitions_for_builtin_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    let env = ProgramEnvironment::from_scope(scope);
    find_symbol_in_scope_impl(db, scope, name, origins)
        .into_iter()
        .filter(|def| def.is_reexported(db))
        .flat_map(|def| {
            resolve_definition_impl(
                db,
                &env,
                def,
                Some(name),
                ImportAliasResolution::ResolveAliases,
                origins,
            )
        })
        .collect()
}

/// Returns source definitions for a member of an already-inferred receiver type.
///
/// During type inference, a caller may already know the type of `obj` in `obj.attr` while
/// inference of the enclosing scope is still in progress. Asking `SemanticModel` for `obj`'s
/// type here could request inference of that same scope again, creating an inference cycle.
/// Accepting the receiver type directly lets the caller reuse its existing result without
/// introducing that dependency.
///
/// This function duplicates much of the functionality in the semantic
/// analyzer, but it has somewhat different behavior so we've decided
/// to keep it separate for now. One key difference is that this function
/// doesn't model the descriptor protocol when accessing attributes.
/// For "go to definition", we want to get the type of the descriptor object
/// rather than "invoking" its `__get__` or `__set__` method.
/// If this becomes a maintenance burden in the future, it may be worth
/// changing the corresponding logic in the semantic analyzer to conditionally
/// handle this case through the use of mode flags.
pub(crate) fn definitions_for_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    lhs_ty: Type<'db>,
    name_str: &str,
) -> Vec<ResolvedDefinition<'db>> {
    definitions_for_attribute_impl(db, env, lhs_ty, name_str, &mut OriginCollector::default())
}

/// Returns a member's source definitions and origins, including imports and inherited definitions.
///
/// The caller supplies the receiver type for the same inference-safety reasons as
/// [`definitions_for_attribute`].
pub(crate) fn definition_provenance_for_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    lhs_ty: Type<'db>,
    name_str: &str,
) -> DefinitionProvenance<'db> {
    let mut origins = OriginCollector::enabled();
    let definitions = definitions_for_attribute_impl(db, env, lhs_ty, name_str, &mut origins);
    DefinitionProvenance {
        definitions,
        origins: origins.into_origins(),
    }
}

fn definitions_for_attribute_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    lhs_ty: Type<'db>,
    name_str: &str,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    let mut resolved = Vec::new();

    // A structural protocol meta-type still uses its nominal protocol declaration as the source
    // location for go-to-definition, even though the origin is not a nominal upper bound.
    let subclass_origin = |subclass_of: SubclassOfInner<'db>| {
        let class = match subclass_of {
            SubclassOfInner::Protocol(protocol) => protocol.class_origin(db).map(|origin| *origin),
            subclass_of => subclass_of.into_class(db, env),
        }?;
        class
            .static_class_literal(db)
            .map(|(literal, _)| ClassLiteral::Static(literal))
    };

    let tys = match lhs_ty {
        Type::Union(union) => union.elements(db),
        _ => std::slice::from_ref(&lhs_ty),
    };

    // Expand intersections for each subtype into their components
    let expanded_tys = tys
        .iter()
        .flat_map(|ty| match ty {
            Type::Intersection(intersection) => Either::Left(intersection.positive(db).iter()),
            _ => Either::Right(std::iter::once(ty)),
        })
        .copied();

    for ty in expanded_tys {
        // Handle modules
        if let Type::ModuleLiteral(module_literal) = ty {
            if let Some(module_file) = module_literal
                .module(db)
                .file(db)
                .map(|file| ProgramFile::new(db, file, env.program(db)))
            {
                let module_scope = global_scope(db, module_file);
                for def in find_symbol_in_scope_impl(db, module_scope, name_str, origins) {
                    resolved.extend(resolve_definition_impl(
                        db,
                        env,
                        def,
                        Some(name_str),
                        ImportAliasResolution::ResolveAliases,
                        origins,
                    ));
                }
            }
            continue;
        }

        // Prevent lookup on BoundSuper proxy object
        if matches!(ty, Type::BoundSuper(_)) {
            continue;
        }

        let meta_type = ty.to_meta_type(db, env);

        // Look up the attribute first on the meta-type, unless it's already a class-like type.
        let lookup_type = match ty {
            Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_) => ty,
            _ => meta_type,
        };

        let class_literal = match lookup_type {
            Type::ClassLiteral(class_literal) => class_literal,
            Type::SubclassOf(subclass) => {
                let Some(class_literal) = subclass_origin(subclass.subclass_of()) else {
                    continue;
                };
                class_literal
            }
            _ => continue,
        };

        resolved.extend(definitions_for_attribute_in_class_hierarchy(
            db,
            env,
            &class_literal,
            name_str,
            origins,
        ));

        // The metaclass of a derived class must be a subclass of the metaclasses of all of
        // its base classes. This is why we only have to look at the metaclass of the
        // class_literal.
        // Only look up definitions on the metaclass if the type is a class object to begin with in
        // order to prevent looking up instance members on the class metaclass
        if resolved.is_empty() && meta_type != lookup_type {
            let class_literal = match meta_type {
                Type::ClassLiteral(class_literal) => class_literal,
                Type::SubclassOf(subclass) => {
                    let Some(class_literal) = subclass_origin(subclass.subclass_of()) else {
                        continue;
                    };
                    class_literal
                }
                _ => continue,
            };

            resolved.extend(definitions_for_attribute_in_class_hierarchy(
                db,
                env,
                &class_literal,
                name_str,
                origins,
            ));
        }
    }

    resolved
}

fn definitions_for_attribute_in_class_hierarchy<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class_literal: &ClassLiteral<'db>,
    attribute_name: &str,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    let mut resolved = Vec::new();
    'scopes: for ancestor in class_literal
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|cls: ClassType<'db>| cls.static_class_literal(db).map(|(lit, _)| lit))
    {
        let class_scope = ancestor.body_scope(db);
        let class_place_table = ty_python_core::place_table(db, class_scope);

        // Look for class-level declarations and bindings
        if let Some(place_id) = class_place_table.symbol_id(attribute_name) {
            let use_def = use_def_map(db, class_scope);
            let resolved_in_scope = resolve_reachable_definitions(
                db,
                env,
                attribute_name,
                use_def
                    .reachable_symbol_declarations(place_id)
                    .filter_map(|declaration| {
                        DefinitionOrigin::from_declaration(class_scope, &declaration)
                    })
                    .chain(
                        use_def
                            .reachable_symbol_bindings(place_id)
                            .filter_map(|binding| {
                                DefinitionOrigin::from_binding(class_scope, &binding)
                            }),
                    ),
                origins,
            );
            if !resolved_in_scope.is_empty() {
                resolved.extend(resolved_in_scope);
                break 'scopes;
            }
        }

        // Look for instance attributes in method scopes (e.g., self.x = 1)
        let index = semantic_index(db, class_scope.program_file(db));

        for function_scope_id in attribute_scopes(db, class_scope) {
            if let Some(place_id) = index
                .place_table(function_scope_id)
                .member_id_by_instance_attribute_name(attribute_name)
            {
                let use_def = index.use_def_map(function_scope_id);
                let binding_scope = function_scope_id.to_scope_id(db, class_scope.program_file(db));
                let resolved_in_scope =
                    resolve_reachable_definitions(
                        db,
                        env,
                        attribute_name,
                        use_def
                            .reachable_member_declarations(place_id)
                            .filter_map(|declaration| {
                                DefinitionOrigin::from_declaration(binding_scope, &declaration)
                            })
                            .chain(use_def.reachable_member_bindings(place_id).filter_map(
                                |binding| DefinitionOrigin::from_binding(binding_scope, &binding),
                            )),
                        origins,
                    );
                if !resolved_in_scope.is_empty() {
                    resolved.extend(resolved_in_scope);
                    break 'scopes;
                }
            }
        }
    }

    resolved
}

/// Returns the user-visible definitions represented by a use-def binding.
///
/// Comprehension walruses are represented in the containing scope by synthetic eager bindings:
///
/// ```python
/// [(last := item) for item in items]
/// print(last)  # Go to definition should select `last := item` above.
/// ```
///
/// The binding for the use in `print` is synthetic, so follow it into the comprehension's
/// end-of-scope bindings. Nested comprehensions can produce a chain of these proxies. Only
/// follow sources that resolve to the same variable, so `global` and `nonlocal` writes do not
/// become definitions of each other.
pub(super) fn user_visible_definitions<'db>(
    db: &'db dyn Db,
    definitions: impl IntoIterator<Item = Definition<'db>>,
) -> FxIndexSet<Definition<'db>> {
    user_visible_definitions_impl(
        db,
        definitions.into_iter().collect(),
        &mut OriginCollector::default(),
    )
}

fn user_visible_origins<'db>(
    db: &'db dyn Db,
    definitions: impl IntoIterator<Item = DefinitionOrigin<'db>>,
    origins: &mut OriginCollector<'db>,
) -> FxIndexSet<Definition<'db>> {
    let pending = definitions
        .into_iter()
        .map(|origin| {
            origins.record(origin);
            origin.definition
        })
        .collect();
    user_visible_definitions_impl(db, pending, origins)
}

fn user_visible_definitions_impl<'db>(
    db: &'db dyn Db,
    mut pending: VecDeque<Definition<'db>>,
    origins: &mut OriginCollector<'db>,
) -> FxIndexSet<Definition<'db>> {
    let mut seen = FxHashSet::default();
    let mut result = FxIndexSet::default();

    while let Some(definition) = pending.pop_front() {
        if !seen.insert(definition) {
            continue;
        }

        match definition.kind(db) {
            DefinitionKind::NestedBindings(nested) => {
                let index = semantic_index(db, definition.program_file(db));
                let sources = nested
                    .visible_binding_sources(index, definition.file_scope(db))
                    .flatten()
                    .filter_map(|binding| {
                        let source = binding.binding.definition()?;
                        if nested.execution != NestedBindingExecution::Eager
                            && !matches!(source.kind(db), DefinitionKind::NestedBindings(_))
                        {
                            return None;
                        }
                        origins.record(DefinitionOrigin {
                            definition: source,
                            scope: source.scope(db),
                            reachability: binding.reachability_constraint,
                        });
                        Some(source)
                    });
                // A lazy function proxy can lead to an eager comprehension proxy. Follow that
                // proxy-only chain without exposing ordinary lazy nested assignments.
                pending.extend(sources);
            }
            kind if kind.is_user_visible() => {
                result.insert(definition);
            }
            _ => {}
        }
    }

    result
}

fn resolve_reachable_definitions<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol_name: &str,
    definitions: impl IntoIterator<Item = DefinitionOrigin<'db>>,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    user_visible_origins(db, definitions, origins)
        .into_iter()
        .flat_map(|definition| {
            resolve_definition_impl(
                db,
                env,
                definition,
                Some(symbol_name),
                ImportAliasResolution::ResolveAliases,
                origins,
            )
        })
        .collect()
}

/// Resolve import definitions to their targets.
/// Returns resolved definitions which can be either specific definitions or module files.
/// For non-import definitions, returns the definition wrapped in `ResolvedDefinition::Definition`.
/// Always returns at least the original definition as a fallback if resolution fails.
pub(crate) fn resolve_definition<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    definition: Definition<'db>,
    symbol_name: Option<&str>,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    resolve_definition_impl(
        db,
        env,
        definition,
        symbol_name,
        alias_resolution,
        &mut OriginCollector::default(),
    )
}

fn resolve_definition_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    definition: Definition<'db>,
    symbol_name: Option<&str>,
    alias_resolution: ImportAliasResolution,
    origins: &mut OriginCollector<'db>,
) -> Vec<ResolvedDefinition<'db>> {
    let mut visited = FxHashSet::default();
    let mut resolution = ImportResolution {
        visited: &mut visited,
        alias_resolution,
        origins,
    };
    let resolved = resolve_definition_recursive(db, env, definition, symbol_name, &mut resolution);

    // If resolution failed, return the original definition as fallback
    if resolved.is_empty() {
        vec![ResolvedDefinition::Definition(definition)]
    } else {
        resolved
    }
}

/// Helper function to resolve import definitions recursively.
fn resolve_definition_recursive<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    definition: Definition<'db>,
    symbol_name: Option<&str>,
    resolution: &mut ImportResolution<'_, 'db>,
) -> Vec<ResolvedDefinition<'db>> {
    // Prevent infinite recursion if there are circular imports
    if resolution.visited.contains(&definition) {
        return Vec::new(); // Return empty list for circular imports
    }
    resolution.visited.insert(definition);

    let kind = definition.kind(db);

    match kind {
        DefinitionKind::Import(import_def) => {
            let file = definition.program_file(db);
            let module = parsed_module(db, file.python_file(db)).load(db);
            let alias = import_def.alias(&module);

            if alias.asname.is_some()
                && resolution.alias_resolution == ImportAliasResolution::PreserveAliases
            {
                return vec![ResolvedDefinition::Definition(definition)];
            }

            // Get the full module name being imported
            let Some(module_name) = ModuleName::new(&alias.name) else {
                return Vec::new(); // Invalid module name, return empty list
            };

            // Resolve the module to its file
            let importing_file = ImportingFile::File(file.file(db), env.resolver_environment(db));
            let Some(resolved_module) = resolve_module(db, importing_file, &module_name) else {
                return Vec::new(); // Module not found, return empty list
            };

            let Some(module_file) = resolved_module.file(db) else {
                return Vec::new(); // No file for module, return empty list
            };
            let module_file = ProgramFile::new(db, module_file, env.program(db));

            // For simple imports like "import os", we want to navigate to the module itself.
            // Return the module file directly instead of trying to find definitions within it.
            vec![ResolvedDefinition::Module(module_file)]
        }

        DefinitionKind::ImportFrom(import_from_def) => {
            let file = definition.program_file(db);
            let module = parsed_module(db, file.python_file(db)).load(db);
            let import_node = import_from_def.import(&module);
            let alias = import_from_def.alias(&module);

            if alias.asname.is_some()
                && resolution.alias_resolution == ImportAliasResolution::PreserveAliases
            {
                return vec![ResolvedDefinition::Definition(definition)];
            }

            // For `ImportFrom`, we need to resolve the original imported symbol name
            // (alias.name), not the local alias (symbol_name)
            resolve_from_import_definitions_impl(
                db,
                env,
                ImportingFile::File(file.file(db), env.resolver_environment(db)),
                import_node,
                &alias.name,
                resolution,
            )
        }

        // For star imports, try to resolve to the specific symbol being accessed
        DefinitionKind::StarImport(star_import_def) => {
            let file = definition.program_file(db);
            let module = parsed_module(db, file.python_file(db)).load(db);
            let import_node = star_import_def.import(&module);

            // If we have a symbol name, use the helper to resolve it in the target module
            if let Some(symbol_name) = symbol_name {
                resolve_from_import_definitions_impl(
                    db,
                    env,
                    ImportingFile::File(file.file(db), env.resolver_environment(db)),
                    import_node,
                    symbol_name,
                    resolution,
                )
            } else {
                // No symbol context provided, can't resolve star import
                Vec::new()
            }
        }

        // For non-import definitions, return the definition as is
        _ => vec![ResolvedDefinition::Definition(definition)],
    }
}

/// Helper function to resolve import definitions for `ImportFrom` and `StarImport` cases.
pub(crate) fn resolve_from_import_definitions<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    importing_file: ImportingFile<'db>,
    import_node: &ast::StmtImportFrom,
    symbol_name: &str,
    visited: &mut FxHashSet<Definition<'db>>,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    let mut origins = OriginCollector::default();
    let mut resolution = ImportResolution {
        visited,
        alias_resolution,
        origins: &mut origins,
    };
    resolve_from_import_definitions_impl(
        db,
        env,
        importing_file,
        import_node,
        symbol_name,
        &mut resolution,
    )
}

fn resolve_from_import_definitions_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    importing_file: ImportingFile<'db>,
    import_node: &ast::StmtImportFrom,
    symbol_name: &str,
    resolution: &mut ImportResolution<'_, 'db>,
) -> Vec<ResolvedDefinition<'db>> {
    if resolution.alias_resolution == ImportAliasResolution::PreserveAliases {
        for alias in &import_node.names {
            if let Some(asname) = &alias.asname {
                if asname.as_str() == symbol_name {
                    return vec![ResolvedDefinition::FileWithRange(FileRange::new(
                        importing_file.file(db),
                        asname.range,
                    ))];
                }
            }
        }
    }

    let Some(resolved_module) = resolve_module_for_import_from(db, importing_file, import_node)
    else {
        return Vec::new();
    };

    // Resolve the target module file
    let module_file = resolved_module
        .file(db)
        .map(|file| ProgramFile::new(db, file, env.program(db)));

    let Some(module_file) = module_file else {
        // No file means this is a namespace package, try to import the submodule
        return Vec::from_iter(resolve_from_import_submodule_definitions(
            db,
            env,
            importing_file,
            symbol_name,
            resolved_module.name(db),
        ));
    };

    // Find the definition of this symbol in the imported module's global scope
    let global_scope = global_scope(db, module_file);
    let definitions_in_module =
        find_symbol_in_scope_impl(db, global_scope, symbol_name, resolution.origins);

    // Recursively resolve any import definitions found in the target module
    let mut resolved_definitions = Vec::new();
    for def in definitions_in_module {
        let resolved = resolve_definition_recursive(db, env, def, Some(symbol_name), resolution);
        resolved_definitions.extend(resolved);
    }

    if resolved_definitions.is_empty() {
        // In `pkg/__init__.py`, `from . import child` resolves `.` to
        // `pkg/__init__.py`. Looking up `child` there can find an import definition
        // that recursively resolves back here (possibly through `from . import *`),
        // so recursive resolution bottoms out before reaching the `pkg.child`
        // submodule target. Fall back to the same submodule candidate we use when
        // `child` has no binding in `pkg/__init__.py`.
        Vec::from_iter(resolve_from_import_submodule_definitions(
            db,
            env,
            importing_file,
            symbol_name,
            resolved_module.name(db),
        ))
    } else {
        resolved_definitions
    }
}

// Helper to resolve `from x.y import z` assuming `x.y.z` is a module.
fn resolve_from_import_submodule_definitions<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    importing_file: ImportingFile<'db>,
    symbol_name: &str,
    module_name: &ModuleName,
) -> Option<ResolvedDefinition<'db>> {
    let submodule_name = ModuleName::new(symbol_name)?;
    let mut full_submodule_name = module_name.clone();
    full_submodule_name.extend(&submodule_name);
    let module = resolve_module(db, importing_file, &full_submodule_name)?;
    let file = ProgramFile::new(db, module.file(db)?, env.program(db));

    Some(ResolvedDefinition::Module(file))
}

/// Find definitions for a symbol name in a specific scope.
pub(crate) fn find_symbol_in_scope<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol_name: &str,
) -> IndexSet<Definition<'db>> {
    find_symbol_in_scope_impl(db, scope, symbol_name, &mut OriginCollector::default())
}

fn find_symbol_in_scope_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol_name: &str,
    origins: &mut OriginCollector<'db>,
) -> IndexSet<Definition<'db>> {
    let place_table = place_table(db, scope);
    let Some(symbol_id) = place_table.symbol_id(symbol_name) else {
        return IndexSet::new();
    };

    let use_def_map = use_def_map(db, scope);
    let mut definitions = IndexSet::new();

    // Get all definitions (both bindings and declarations) for this place
    let bindings = use_def_map.reachable_symbol_bindings(symbol_id);
    let declarations = use_def_map.reachable_symbol_declarations(symbol_id);

    for binding in bindings {
        if let Some(origin) = DefinitionOrigin::from_binding(scope, &binding) {
            origins.record(origin);
            definitions.insert(origin.definition);
        }
    }

    for declaration in declarations {
        if let Some(origin) = DefinitionOrigin::from_declaration(scope, &declaration) {
            origins.record(origin);
            definitions.insert(origin.definition);
        }
    }

    user_visible_definitions_impl(db, definitions.into_iter().collect(), origins)
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use ruff_db::files::system_path_to_file;
    use ruff_db::testing::assert_function_query_was_not_run_by_name;

    use super::*;
    use crate::db::tests::TestDbBuilder;

    #[test]
    fn builtin_names_do_not_infer_scope() -> anyhow::Result<()> {
        for name in ["isinstance", "float", "complex"] {
            let mut db = TestDbBuilder::new()
                .with_file("/src/foo.py", name)
                .build()?;
            let file = system_path_to_file(&db, "/src/foo.py")?;
            let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
            let definitions = definitions_for_name(
                &db,
                global_scope(&db, file),
                name,
                ImportAliasResolution::ResolveAliases,
            );
            let [ResolvedDefinition::Definition(definition)] = definitions.as_slice() else {
                anyhow::bail!("expected one definition for {name}");
            };
            assert_eq!(definition.name(&db).as_deref(), Some(name));

            let events = db.take_salsa_events();
            assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
        }
        Ok(())
    }

    #[test]
    fn attribute_lookup_does_not_infer_scope() -> anyhow::Result<()> {
        let mut db = TestDbBuilder::new()
            .with_file("/src/foo.py", "class C:\n    flag = (1, 2)\n")
            .build()?;
        let file = system_path_to_file(&db, "/src/foo.py")?;
        let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let parsed = parsed_module(&db, file.python_file(&db)).load(&db);
        let class = parsed
            .suite()
            .first()
            .and_then(ast::Stmt::as_class_def_stmt)
            .context("expected a class definition")?;
        let definition = semantic_index(&db, file).expect_single_definition(class);
        let receiver = binding_type(&db, definition);
        let definitions =
            definitions_for_attribute(&db, &db.program_environment(), receiver, "flag");
        let [ResolvedDefinition::Definition(definition)] = definitions.as_slice() else {
            anyhow::bail!("expected one definition for C.flag");
        };
        assert_eq!(definition.name(&db).as_deref(), Some("flag"));

        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
        Ok(())
    }

    #[test]
    fn import_provenance_preserves_guards_through_cycles() -> anyhow::Result<()> {
        let mut db = TestDbBuilder::new()
            .with_file(
                "/src/first.py",
                "import sys\nif sys.platform == 'win32':\n    from second import value\n",
            )
            .with_file("/src/second.py", "from first import value\n")
            .build()?;
        let first = system_path_to_file(&db, "/src/first.py")?;
        let second = system_path_to_file(&db, "/src/second.py")?;
        let program = db.program_environment().program(&db);
        let first_scope = global_scope(&db, ProgramFile::new(&db, first, program));
        let second_scope = global_scope(&db, ProgramFile::new(&db, second, program));
        let origins = definition_provenance_for_name(&db, second_scope, "value").origins;
        assert_eq!(origins.len(), 2);
        let guarded = origins
            .iter()
            .find(|origin| origin.scope == first_scope)
            .context("expected the guarded import from first.py")?;
        assert_ne!(
            guarded.reachability,
            ScopedReachabilityConstraintId::ALWAYS_TRUE
        );
        assert!(
            use_def_map(&db, guarded.scope)
                .reachability_constraints()
                .predicate_ids(guarded.reachability)
                .next()
                .is_some()
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
        Ok(())
    }
}

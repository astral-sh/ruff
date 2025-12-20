use std::iter::{FusedIterator, once};
use std::sync::Arc;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_index::{IndexSlice, IndexVec};
use ruff_python_ast::NodeIndex;
use ruff_python_parser::semantic_errors::SemanticSyntaxError;
use rustc_hash::{FxHashMap, FxHashSet};
use salsa::Update;
use salsa::plumbing::AsId;
use ty_module_resolver::ModuleName;

use crate::Db;
use crate::node_key::NodeKey;
use crate::semantic_index::ast_ids::AstIds;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::builder::SemanticIndexBuilder;
use crate::semantic_index::definition::{Definition, DefinitionNodeKey, Definitions};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::narrowing_constraints::ScopedNarrowingConstraint;
use crate::semantic_index::place::{PlaceExprRef, PlaceTable};
pub use crate::semantic_index::scope::FileScopeId;
use crate::semantic_index::scope::{
    NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopeKind, ScopeLaziness,
};
use crate::semantic_index::symbol::ScopedSymbolId;
use crate::semantic_index::use_def::{EnclosingSnapshotKey, ScopedEnclosingSnapshotId, UseDefMap};
use crate::semantic_model::HasTrackedScope;

pub mod ast_ids;
mod builder;
pub mod definition;
pub mod expression;
pub(crate) mod member;
pub(crate) mod narrowing_constraints;
pub mod place;
pub(crate) mod predicate;
mod re_exports;
mod reachability_constraints;
pub(crate) mod scope;
pub(crate) mod symbol;
mod use_def;

pub(crate) use self::use_def::{
    ApplicableConstraints, BindingWithConstraints, BindingWithConstraintsIterator,
    DeclarationWithConstraint, DeclarationsIterator,
};

/// Returns the semantic index for `file`.
///
/// Prefer using [`symbol_table`] when working with symbols from a single scope.
#[salsa::tracked(returns(ref), no_eq, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn semantic_index(db: &dyn Db, file: File) -> SemanticIndex<'_> {
    let _span = tracing::trace_span!("semantic_index", ?file).entered();

    let module = parsed_module(db, file).load(db);

    SemanticIndexBuilder::new(db, file, &module).build()
}

/// Returns the place table for a specific `scope`.
///
/// Using [`place_table`] over [`semantic_index`] has the advantage that
/// Salsa can avoid invalidating dependent queries if this scope's place table
/// is unchanged.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn place_table<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Arc<PlaceTable> {
    let file = scope.file(db);
    let _span = tracing::trace_span!("place_table", scope=?scope.as_id(), ?file).entered();
    let index = semantic_index(db, file);
    Arc::clone(&index.place_tables[scope.file_scope_id(db)])
}

/// Returns the set of modules that are imported anywhere in `file`.
///
/// This set only considers `import` statements, not `from...import` statements.
/// See [`ModuleLiteralType::available_submodule_attributes`] for discussion
/// of why this analysis is intentionally limited.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn imported_modules<'db>(db: &'db dyn Db, file: File) -> Arc<FxHashSet<ModuleName>> {
    semantic_index(db, file).imported_modules.clone()
}

/// Returns the use-def map for a specific `scope`.
///
/// Using [`use_def_map`] over [`semantic_index`] has the advantage that
/// Salsa can avoid invalidating dependent queries if this scope's use-def map
/// is unchanged.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn use_def_map<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Arc<UseDefMap<'db>> {
    let file = scope.file(db);
    let _span = tracing::trace_span!("use_def_map", scope=?scope.as_id(), ?file).entered();
    let index = semantic_index(db, file);
    Arc::clone(&index.use_def_maps[scope.file_scope_id(db)])
}

/// Returns all attribute assignments (and their method scope IDs) with a symbol name matching
/// the one given for a specific class body scope.
///
/// Only call this when doing type inference on the same file as `class_body_scope`, otherwise it
/// introduces a direct dependency on that file's AST.
pub(crate) fn attribute_assignments<'db, 's>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
    name: &'s str,
) -> impl Iterator<Item = (BindingWithConstraintsIterator<'db, 'db>, FileScopeId)> + use<'s, 'db> {
    let file = class_body_scope.file(db);
    let index = semantic_index(db, file);

    attribute_scopes(db, class_body_scope).filter_map(|function_scope_id| {
        let place_table = index.place_table(function_scope_id);
        let member = place_table.member_id_by_instance_attribute_name(name)?;
        let use_def = &index.use_def_maps[function_scope_id];
        Some((use_def.reachable_member_bindings(member), function_scope_id))
    })
}

/// Returns all attribute declarations (and their method scope IDs) with a symbol name matching
/// the one given for a specific class body scope.
///
/// Only call this when doing type inference on the same file as `class_body_scope`, otherwise it
/// introduces a direct dependency on that file's AST.
pub(crate) fn attribute_declarations<'db, 's>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
    name: &'s str,
) -> impl Iterator<Item = (DeclarationsIterator<'db, 'db>, FileScopeId)> + use<'s, 'db> {
    let file = class_body_scope.file(db);
    let index = semantic_index(db, file);

    attribute_scopes(db, class_body_scope).filter_map(|function_scope_id| {
        let place_table = index.place_table(function_scope_id);
        let member = place_table.member_id_by_instance_attribute_name(name)?;
        let use_def = &index.use_def_maps[function_scope_id];
        Some((
            use_def.reachable_member_declarations(member),
            function_scope_id,
        ))
    })
}

/// Returns all attribute assignments as scope IDs for a specific class body scope.
///
/// Only call this when doing type inference on the same file as `class_body_scope`, otherwise it
/// introduces a direct dependency on that file's AST.
pub(crate) fn attribute_scopes<'db>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
) -> impl Iterator<Item = FileScopeId> + 'db {
    let file = class_body_scope.file(db);
    let index = semantic_index(db, file);
    let class_scope_id = class_body_scope.file_scope_id(db);
    ChildrenIter::new(&index.scopes, class_scope_id)
        .filter_map(move |(child_scope_id, scope)| {
            let (function_scope_id, function_scope) =
                if scope.node().scope_kind() == ScopeKind::TypeParams {
                    // This could be a generic method with a type-params scope.
                    // Go one level deeper to find the function scope. The first
                    // descendant is the (potential) function scope.
                    let function_scope_id = scope.descendants().start;
                    (function_scope_id, index.scope(function_scope_id))
                } else {
                    (child_scope_id, scope)
                };
            function_scope.node().as_function()?;
            Some(function_scope_id)
        })
        .flat_map(move |func_id| {
            // Add any descendent scope that is eager and have eager scopes between the scope
            // and the method scope. Since attributes can be defined in this scope.
            let nested = index.descendent_scopes(func_id).filter_map(move |(id, s)| {
                let is_eager = s.kind().is_eager();
                let parents_are_eager = {
                    let mut all_parents_eager = true;
                    let mut current = Some(id);

                    while let Some(scope_id) = current {
                        if scope_id == func_id {
                            break;
                        }
                        let scope = index.scope(scope_id);
                        if !scope.is_eager() {
                            all_parents_eager = false;
                            break;
                        }
                        current = scope.parent();
                    }

                    all_parents_eager
                };

                (parents_are_eager && is_eager).then_some(id)
            });
            once(func_id).chain(nested)
        })
}

/// Returns the module global scope of `file`.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn global_scope(db: &dyn Db, file: File) -> ScopeId<'_> {
    let _span = tracing::trace_span!("global_scope", ?file).entered();

    FileScopeId::global().to_scope_id(db, file)
}

pub(crate) enum EnclosingSnapshotResult<'map, 'db> {
    FoundConstraint(ScopedNarrowingConstraint),
    FoundBindings(BindingWithConstraintsIterator<'map, 'db>),
    NotFound,
    NoLongerInEagerContext,
}

/// The place tables and use-def maps for all scopes in a file.
#[derive(Debug, Update, get_size2::GetSize)]
pub(crate) struct SemanticIndex<'db> {
    /// List of all place tables in this file, indexed by scope.
    place_tables: IndexVec<FileScopeId, Arc<PlaceTable>>,

    /// List of all scopes in this file.
    scopes: IndexVec<FileScopeId, Scope>,

    /// Map expressions to their corresponding scope.
    scopes_by_expression: ExpressionsScopeMap,

    /// Map from a node creating a definition to its definition.
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definitions<'db>>,

    /// Map from a standalone expression to its [`Expression`] ingredient.
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,

    /// Map from nodes that create a scope to the scope they create.
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,

    /// Map from the file-local [`FileScopeId`] to the salsa-ingredient [`ScopeId`].
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,

    /// Use-def map for each scope in this file.
    use_def_maps: IndexVec<FileScopeId, Arc<UseDefMap<'db>>>,

    /// Lookup table to map between node ids and ast nodes.
    ///
    /// Note: We should not depend on this map when analysing other files or
    /// changing a file invalidates all dependents.
    ast_ids: IndexVec<FileScopeId, AstIds>,

    /// The set of modules that are imported anywhere within this file.
    imported_modules: Arc<FxHashSet<ModuleName>>,

    /// Flags about the global scope (code usage impacting inference)
    has_future_annotations: bool,

    /// Map of all of the enclosing snapshots that appear in this file.
    enclosing_snapshots: FxHashMap<EnclosingSnapshotKey, ScopedEnclosingSnapshotId>,

    /// List of all semantic syntax errors in this file.
    semantic_syntax_errors: Vec<SemanticSyntaxError>,

    /// Set of all generator functions in this file.
    generator_functions: FxHashSet<FileScopeId>,
}

impl<'db> SemanticIndex<'db> {
    /// Returns the place table for a specific scope.
    ///
    /// Use the Salsa cached [`place_table()`] query if you only need the
    /// place table for a single scope.
    #[track_caller]
    pub(super) fn place_table(&self, scope_id: FileScopeId) -> &PlaceTable {
        &self.place_tables[scope_id]
    }

    /// Returns the use-def map for a specific scope.
    ///
    /// Use the Salsa cached [`use_def_map()`] query if you only need the
    /// use-def map for a single scope.
    #[track_caller]
    pub(super) fn use_def_map(&self, scope_id: FileScopeId) -> &UseDefMap<'db> {
        &self.use_def_maps[scope_id]
    }

    #[track_caller]
    pub(crate) fn ast_ids(&self, scope_id: FileScopeId) -> &AstIds {
        &self.ast_ids[scope_id]
    }

    /// Returns the ID of the `expression`'s enclosing scope.
    #[track_caller]
    pub(crate) fn expression_scope_id<E>(&self, expression: &E) -> FileScopeId
    where
        E: HasTrackedScope,
    {
        self.try_expression_scope_id(expression)
            .expect("Expression to be part of a scope if it is from the same module")
    }

    /// Returns the ID of the `expression`'s enclosing scope.
    pub(crate) fn try_expression_scope_id<E>(&self, expression: &E) -> Option<FileScopeId>
    where
        E: HasTrackedScope,
    {
        self.scopes_by_expression.try_get(expression)
    }

    /// Returns the [`Scope`] of the `expression`'s enclosing scope.
    #[allow(unused)]
    #[track_caller]
    pub(crate) fn expression_scope(&self, expression: &impl HasTrackedScope) -> &Scope {
        &self.scopes[self.expression_scope_id(expression)]
    }

    /// Returns the [`Scope`] with the given id.
    #[track_caller]
    pub(crate) fn scope(&self, id: FileScopeId) -> &Scope {
        &self.scopes[id]
    }

    pub(crate) fn scope_ids(&self) -> impl Iterator<Item = ScopeId<'db>> + '_ {
        self.scope_ids_by_scope.iter().copied()
    }

    pub(crate) fn symbol_is_global_in_scope(
        &self,
        symbol: ScopedSymbolId,
        scope: FileScopeId,
    ) -> bool {
        self.place_table(scope).symbol(symbol).is_global()
    }

    pub(crate) fn symbol_is_nonlocal_in_scope(
        &self,
        symbol: ScopedSymbolId,
        scope: FileScopeId,
    ) -> bool {
        self.place_table(scope).symbol(symbol).is_nonlocal()
    }

    /// Returns the id of the parent scope.
    pub(crate) fn parent_scope_id(&self, scope_id: FileScopeId) -> Option<FileScopeId> {
        let scope = self.scope(scope_id);
        scope.parent()
    }

    /// Returns the parent scope of `scope_id`.
    #[expect(unused)]
    #[track_caller]
    pub(crate) fn parent_scope(&self, scope_id: FileScopeId) -> Option<&Scope> {
        Some(&self.scopes[self.parent_scope_id(scope_id)?])
    }

    /// Return the [`Definition`] of the class enclosing this method, given the
    /// method's body scope, or `None` if it is not a method.
    pub(crate) fn class_definition_of_method(
        &self,
        function_body_scope: FileScopeId,
    ) -> Option<Definition<'db>> {
        let current_scope = self.scope(function_body_scope);
        if current_scope.kind() != ScopeKind::Function {
            return None;
        }
        let parent_scope_id = current_scope.parent()?;
        let parent_scope = self.scope(parent_scope_id);

        let class_scope = match parent_scope.kind() {
            ScopeKind::Class => parent_scope,
            ScopeKind::TypeParams => {
                let class_scope_id = parent_scope.parent()?;
                let potentially_class_scope = self.scope(class_scope_id);

                match potentially_class_scope.kind() {
                    ScopeKind::Class => potentially_class_scope,
                    _ => return None,
                }
            }
            _ => return None,
        };

        class_scope
            .node()
            .as_class()
            .map(|node_ref| self.expect_single_definition(node_ref))
    }

    fn is_scope_reachable(&self, db: &'db dyn Db, scope_id: FileScopeId) -> bool {
        self.parent_scope_id(scope_id)
            .is_none_or(|parent_scope_id| {
                if !self.is_scope_reachable(db, parent_scope_id) {
                    return false;
                }

                let parent_use_def = self.use_def_map(parent_scope_id);
                let reachability = self.scope(scope_id).reachability();

                parent_use_def.is_reachable(db, reachability)
            })
    }

    /// Returns true if a given AST node is reachable from the start of the scope. For example,
    /// in the following code, expression `2` is reachable, but expressions `1` and `3` are not:
    /// ```py
    /// def f():
    ///     x = 1
    ///     if False:
    ///         x  # 1
    ///     x  # 2
    ///     return
    ///     x  # 3
    /// ```
    pub(crate) fn is_node_reachable(
        &self,
        db: &'db dyn crate::Db,
        scope_id: FileScopeId,
        node_key: NodeKey,
    ) -> bool {
        self.is_scope_reachable(db, scope_id)
            && self.use_def_map(scope_id).is_node_reachable(db, node_key)
    }

    /// Returns an iterator over the descendent scopes of `scope`.
    #[allow(unused)]
    pub(crate) fn descendent_scopes(&self, scope: FileScopeId) -> DescendantsIter<'_> {
        DescendantsIter::new(&self.scopes, scope)
    }

    /// Returns an iterator over the direct child scopes of `scope`.
    #[allow(unused)]
    pub(crate) fn child_scopes(&self, scope: FileScopeId) -> ChildrenIter<'_> {
        ChildrenIter::new(&self.scopes, scope)
    }

    /// Returns an iterator over all ancestors of `scope`, starting with `scope` itself.
    pub(crate) fn ancestor_scopes(&self, scope: FileScopeId) -> AncestorsIter<'_> {
        AncestorsIter::new(&self.scopes, scope)
    }

    /// Returns an iterator over ancestors of `scope` that are visible for name resolution,
    /// starting with `scope` itself. This follows Python's lexical scoping rules where
    /// class scopes are skipped during name resolution (except for the starting scope
    /// if it happens to be a class scope).
    ///
    /// For example, in this code:
    /// ```python
    /// x = 1
    /// class A:
    ///     x = 2
    ///     def method(self):
    ///         print(x)  # Refers to global x=1, not class x=2
    /// ```
    /// The `method` function can see the global scope but not the class scope.
    pub(crate) fn visible_ancestor_scopes(&self, scope: FileScopeId) -> VisibleAncestorsIter<'_> {
        VisibleAncestorsIter::new(&self.scopes, scope)
    }

    /// Returns the [`definition::Definition`] salsa ingredient(s) for `definition_key`.
    ///
    /// There will only ever be >1 `Definition` associated with a `definition_key`
    /// if the definition is created by a wildcard (`*`) import.
    #[track_caller]
    pub(crate) fn definitions(
        &self,
        definition_key: impl Into<DefinitionNodeKey>,
    ) -> &Definitions<'db> {
        &self.definitions_by_node[&definition_key.into()]
    }

    /// Returns the [`definition::Definition`] salsa ingredient for `definition_key`.
    ///
    /// ## Panics
    ///
    /// If the number of definitions associated with the key is not exactly 1 and
    /// the `debug_assertions` feature is enabled, this method will panic.
    #[track_caller]
    pub(crate) fn expect_single_definition(
        &self,
        definition_key: impl Into<DefinitionNodeKey> + std::fmt::Debug + Copy,
    ) -> Definition<'db> {
        let definitions = self.definitions(definition_key);
        debug_assert_eq!(
            definitions.len(),
            1,
            "Expected exactly one definition to be associated with AST node {definition_key:?} but found {}",
            definitions.len()
        );
        definitions[0]
    }

    /// Returns the [`Expression`] ingredient for an expression node.
    /// Panics if we have no expression ingredient for that node. We can only call this method for
    /// standalone-inferable expressions, which we call `add_standalone_expression` for in
    /// [`SemanticIndexBuilder`].
    #[track_caller]
    pub(crate) fn expression(
        &self,
        expression_key: impl Into<ExpressionNodeKey>,
    ) -> Expression<'db> {
        self.expressions_by_node[&expression_key.into()]
    }

    pub(crate) fn try_expression(
        &self,
        expression_key: impl Into<ExpressionNodeKey>,
    ) -> Option<Expression<'db>> {
        self.expressions_by_node
            .get(&expression_key.into())
            .copied()
    }

    pub(crate) fn is_standalone_expression(
        &self,
        expression_key: impl Into<ExpressionNodeKey>,
    ) -> bool {
        self.expressions_by_node
            .contains_key(&expression_key.into())
    }

    /// Returns the id of the scope that `node` creates.
    /// This is different from [`definition::Definition::scope`] which
    /// returns the scope in which that definition is defined in.
    #[track_caller]
    pub(crate) fn node_scope(&self, node: NodeWithScopeRef) -> FileScopeId {
        self.scopes_by_node[&node.node_key()]
    }

    /// Returns the id of the scope that `node` creates, if it exists.
    pub(crate) fn try_node_scope(&self, node: NodeWithScopeRef) -> Option<FileScopeId> {
        self.scopes_by_node.get(&node.node_key()).copied()
    }

    /// Checks if there is an import of `__future__.annotations` in the global scope, which affects
    /// the logic for type inference.
    pub(super) fn has_future_annotations(&self) -> bool {
        self.has_future_annotations
    }

    /// Returns
    /// * `NoLongerInEagerContext` if the nested scope is no longer in an eager context
    ///   (that is, not every scope that will be traversed is eager) and no lazy snapshots were found.
    /// *  an iterator of bindings for a particular nested scope reference if the bindings exist.
    /// *  a narrowing constraint if there are no bindings, but there is a narrowing constraint for an enclosing scope place.
    /// * `NotFound` if the narrowing constraint / bindings do not exist in the nested scope.
    pub(crate) fn enclosing_snapshot(
        &self,
        enclosing_scope: FileScopeId,
        expr: PlaceExprRef,
        nested_scope: FileScopeId,
    ) -> EnclosingSnapshotResult<'_, 'db> {
        for (ancestor_scope_id, ancestor_scope) in self.ancestor_scopes(nested_scope) {
            if ancestor_scope_id == enclosing_scope {
                break;
            }
            if !ancestor_scope.is_eager() {
                if let PlaceExprRef::Symbol(symbol) = expr
                    && let Some(place_id) =
                        self.place_tables[enclosing_scope].symbol_id(symbol.name())
                {
                    let key = EnclosingSnapshotKey {
                        enclosing_scope,
                        enclosing_place: place_id.into(),
                        nested_scope,
                        nested_laziness: ScopeLaziness::Lazy,
                    };
                    if let Some(id) = self.enclosing_snapshots.get(&key) {
                        return self.use_def_maps[enclosing_scope]
                            .enclosing_snapshot(*id, key.nested_laziness);
                    }
                }
                return EnclosingSnapshotResult::NoLongerInEagerContext;
            }
        }
        let Some(place_id) = self.place_tables[enclosing_scope].place_id(expr) else {
            return EnclosingSnapshotResult::NotFound;
        };
        let key = EnclosingSnapshotKey {
            enclosing_scope,
            enclosing_place: place_id,
            nested_scope,
            nested_laziness: ScopeLaziness::Eager,
        };
        let Some(id) = self.enclosing_snapshots.get(&key) else {
            return EnclosingSnapshotResult::NotFound;
        };
        self.use_def_maps[enclosing_scope].enclosing_snapshot(*id, key.nested_laziness)
    }

    pub(crate) fn semantic_syntax_errors(&self) -> &[SemanticSyntaxError] {
        &self.semantic_syntax_errors
    }
}

pub(crate) struct AncestorsIter<'a> {
    scopes: &'a IndexSlice<FileScopeId, Scope>,
    next_id: Option<FileScopeId>,
}

impl<'a> AncestorsIter<'a> {
    fn new(scopes: &'a IndexSlice<FileScopeId, Scope>, start: FileScopeId) -> Self {
        Self {
            scopes,
            next_id: Some(start),
        }
    }
}

impl<'a> Iterator for AncestorsIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let current_id = self.next_id?;
        let current = &self.scopes[current_id];
        self.next_id = current.parent();

        Some((current_id, current))
    }
}

impl FusedIterator for AncestorsIter<'_> {}

pub(crate) struct VisibleAncestorsIter<'a> {
    inner: AncestorsIter<'a>,
    starting_scope_kind: ScopeKind,
    yielded_count: usize,
}

impl<'a> VisibleAncestorsIter<'a> {
    fn new(scopes: &'a IndexSlice<FileScopeId, Scope>, start: FileScopeId) -> Self {
        let starting_scope = &scopes[start];
        Self {
            inner: AncestorsIter::new(scopes, start),
            starting_scope_kind: starting_scope.kind(),
            yielded_count: 0,
        }
    }
}

impl<'a> Iterator for VisibleAncestorsIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (scope_id, scope) = self.inner.next()?;
            self.yielded_count += 1;

            // Always return the first scope (the starting scope)
            if self.yielded_count == 1 {
                return Some((scope_id, scope));
            }

            // Skip class scopes for subsequent scopes (following Python's lexical scoping rules)
            // Exception: type parameter scopes can see names defined in an immediately-enclosing class scope
            if scope.kind() == ScopeKind::Class {
                // Allow annotation scopes to see their immediately-enclosing class scope exactly once
                if self.starting_scope_kind.is_annotation() && self.yielded_count == 2 {
                    return Some((scope_id, scope));
                }
                continue;
            }

            return Some((scope_id, scope));
        }
    }
}

impl FusedIterator for VisibleAncestorsIter<'_> {}

pub(crate) struct DescendantsIter<'a> {
    next_id: FileScopeId,
    descendants: std::slice::Iter<'a, Scope>,
}

impl<'a> DescendantsIter<'a> {
    fn new(scopes: &'a IndexSlice<FileScopeId, Scope>, scope_id: FileScopeId) -> Self {
        let scope = &scopes[scope_id];
        let scopes = &scopes[scope.descendants()];

        Self {
            next_id: scope_id + 1,
            descendants: scopes.iter(),
        }
    }
}

impl<'a> Iterator for DescendantsIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let descendant = self.descendants.next()?;
        let id = self.next_id;
        self.next_id = self.next_id + 1;

        Some((id, descendant))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.descendants.size_hint()
    }
}

impl FusedIterator for DescendantsIter<'_> {}

impl ExactSizeIterator for DescendantsIter<'_> {}

pub(crate) struct ChildrenIter<'a> {
    parent: FileScopeId,
    descendants: DescendantsIter<'a>,
}

impl<'a> ChildrenIter<'a> {
    pub(crate) fn new(scopes: &'a IndexSlice<FileScopeId, Scope>, parent: FileScopeId) -> Self {
        let descendants = DescendantsIter::new(scopes, parent);

        Self {
            parent,
            descendants,
        }
    }
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        self.descendants
            .find(|(_, scope)| scope.parent() == Some(self.parent))
    }
}

impl FusedIterator for ChildrenIter<'_> {}

/// Interval map that maps a range of expression node ids to their corresponding scopes.
///
/// Lookups require `O(log n)` time, where `n` is roughly the number of scopes (roughly
/// because sub-scopes can be interleaved with expressions in the outer scope, e.g. function, some statements, a function).
#[derive(Eq, PartialEq, Debug, get_size2::GetSize, Default)]
struct ExpressionsScopeMap(Box<[(std::ops::RangeInclusive<NodeIndex>, FileScopeId)]>);

impl ExpressionsScopeMap {
    fn try_get<E>(&self, node: &E) -> Option<FileScopeId>
    where
        E: HasTrackedScope,
    {
        let node_index = node.node_index().load();

        let entry = self
            .0
            .binary_search_by_key(&node_index, |(range, _)| *range.start());

        let index = match entry {
            Ok(index) => index,
            Err(index) => index.checked_sub(1)?,
        };

        let (range, scope) = &self.0[index];
        if range.contains(&node_index) {
            Some(*scope)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use ruff_python_ast::{self as ast};
    use ruff_text_size::{Ranged, TextRange};

    use crate::Db;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::semantic_index::ast_ids::{HasScopedUseId, ScopedUseId};
    use crate::semantic_index::definition::{Definition, DefinitionKind};
    use crate::semantic_index::place::PlaceTable;
    use crate::semantic_index::scope::{FileScopeId, Scope, ScopeKind};
    use crate::semantic_index::symbol::ScopedSymbolId;
    use crate::semantic_index::use_def::UseDefMap;
    use crate::semantic_index::{global_scope, place_table, semantic_index, use_def_map};

    impl UseDefMap<'_> {
        fn first_public_binding(&self, symbol: ScopedSymbolId) -> Option<Definition<'_>> {
            self.end_of_scope_symbol_bindings(symbol)
                .find_map(|constrained_binding| constrained_binding.binding.definition())
        }

        fn first_binding_at_use(&self, use_id: ScopedUseId) -> Option<Definition<'_>> {
            self.bindings_at_use(use_id)
                .find_map(|constrained_binding| constrained_binding.binding.definition())
        }
    }

    struct TestCase {
        db: TestDb,
        file: File,
    }

    fn test_case(content: &str) -> TestCase {
        const FILENAME: &str = "test.py";

        let db = TestDbBuilder::new()
            .with_file(FILENAME, content)
            .build()
            .unwrap();

        let file = system_path_to_file(&db, FILENAME).unwrap();

        TestCase { db, file }
    }

    fn names(table: &PlaceTable) -> Vec<String> {
        table
            .symbols()
            .map(|expr| expr.name().to_string())
            .collect()
    }

    #[test]
    fn empty() {
        let TestCase { db, file } = test_case("");
        let global_table = place_table(&db, global_scope(&db, file));

        let global_names = names(global_table);

        assert_eq!(global_names, Vec::<&str>::new());
    }

    #[test]
    fn simple() {
        let TestCase { db, file } = test_case("x");
        let global_table = place_table(&db, global_scope(&db, file));

        assert_eq!(names(global_table), vec!["x"]);
    }

    #[test]
    fn annotation_only() {
        let TestCase { db, file } = test_case("x: int");
        let global_table = place_table(&db, global_scope(&db, file));

        assert_eq!(names(global_table), vec!["int", "x"]);
        // TODO record definition
    }

    #[test]
    fn import() {
        let TestCase { db, file } = test_case("import foo");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(names(global_table), vec!["foo"]);
        let foo = global_table.symbol_id("foo").unwrap();

        let use_def = use_def_map(&db, scope);
        let binding = use_def.first_public_binding(foo).unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Import(_)));
    }

    #[test]
    fn import_sub() {
        let TestCase { db, file } = test_case("import foo.bar");
        let global_table = place_table(&db, global_scope(&db, file));

        assert_eq!(names(global_table), vec!["foo"]);
    }

    #[test]
    fn import_as() {
        let TestCase { db, file } = test_case("import foo.bar as baz");
        let global_table = place_table(&db, global_scope(&db, file));

        assert_eq!(names(global_table), vec!["baz"]);
    }

    #[test]
    fn import_from() {
        let TestCase { db, file } = test_case("from bar import foo");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(names(global_table), vec!["foo"]);
        assert!(
            global_table
                .symbol_by_name("foo")
                .is_some_and(|symbol| { symbol.is_bound() && !symbol.is_used() }),
            "symbols that are defined get the defined flag"
        );

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id("foo").expect("symbol to exist"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::ImportFrom(_)));
    }

    #[test]
    fn assign() {
        let TestCase { db, file } = test_case("x = foo");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(names(global_table), vec!["foo", "x"]);
        assert!(
            global_table
                .symbol_by_name("foo")
                .is_some_and(|symbol| { !symbol.is_bound() && symbol.is_used() }),
            "a symbol used but not bound in a scope should have only the used flag"
        );
        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id("x").expect("symbol exists"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Assignment(_)));
    }

    #[test]
    fn augmented_assignment() {
        let TestCase { db, file } = test_case("x += 1");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(names(global_table), vec!["x"]);

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id("x").unwrap())
            .unwrap();

        assert!(matches!(
            binding.kind(&db),
            DefinitionKind::AugmentedAssignment(_)
        ));
    }

    #[test]
    fn class_scope() {
        let TestCase { db, file } = test_case(
            "
class C:
    x = 1
y = 2
",
        );
        let global_table = place_table(&db, global_scope(&db, file));

        assert_eq!(names(global_table), vec!["C", "y"]);

        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);

        let [(class_scope_id, class_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };
        assert_eq!(class_scope.kind(), ScopeKind::Class);
        assert_eq!(
            class_scope_id.to_scope_id(&db, file).name(&db, &module),
            "C"
        );

        let class_table = index.place_table(class_scope_id);
        assert_eq!(names(class_table), vec!["x"]);

        let use_def = index.use_def_map(class_scope_id);
        let binding = use_def
            .first_public_binding(class_table.symbol_id("x").expect("symbol exists"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Assignment(_)));
    }

    #[test]
    fn function_scope() {
        let TestCase { db, file } = test_case(
            "
def func():
    x = 1
y = 2
",
        );
        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["func", "y"]);

        let [(function_scope_id, function_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };
        assert_eq!(function_scope.kind(), ScopeKind::Function);
        assert_eq!(
            function_scope_id.to_scope_id(&db, file).name(&db, &module),
            "func"
        );

        let function_table = index.place_table(function_scope_id);
        assert_eq!(names(function_table), vec!["x"]);

        let use_def = index.use_def_map(function_scope_id);
        let binding = use_def
            .first_public_binding(function_table.symbol_id("x").expect("symbol exists"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Assignment(_)));
    }

    #[test]
    fn function_parameter_symbols() {
        let TestCase { db, file } = test_case(
            "
def f(a: str, /, b: str, c: int = 1, *args, d: int = 2, **kwargs):
    pass
",
        );

        let index = semantic_index(&db, file);
        let global_table = place_table(&db, global_scope(&db, file));

        assert_eq!(names(global_table), vec!["str", "int", "f"]);

        let [(function_scope_id, _function_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("Expected a function scope")
        };

        let function_table = index.place_table(function_scope_id);
        assert_eq!(
            names(function_table),
            vec!["a", "b", "c", "d", "args", "kwargs"],
        );

        let use_def = index.use_def_map(function_scope_id);
        for name in ["a", "b", "c", "d"] {
            let binding = use_def
                .first_public_binding(function_table.symbol_id(name).expect("symbol exists"))
                .unwrap();
            assert!(matches!(binding.kind(&db), DefinitionKind::Parameter(_)));
        }
        let args_binding = use_def
            .first_public_binding(function_table.symbol_id("args").expect("symbol exists"))
            .unwrap();
        assert!(matches!(
            args_binding.kind(&db),
            DefinitionKind::VariadicPositionalParameter(_)
        ));
        let kwargs_binding = use_def
            .first_public_binding(function_table.symbol_id("kwargs").expect("symbol exists"))
            .unwrap();
        assert!(matches!(
            kwargs_binding.kind(&db),
            DefinitionKind::VariadicKeywordParameter(_)
        ));
    }

    #[test]
    fn lambda_parameter_symbols() {
        let TestCase { db, file } = test_case("lambda a, b, c=1, *args, d=2, **kwargs: None");

        let index = semantic_index(&db, file);
        let global_table = place_table(&db, global_scope(&db, file));

        assert!(names(global_table).is_empty());

        let [(lambda_scope_id, _lambda_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("Expected a lambda scope")
        };

        let lambda_table = index.place_table(lambda_scope_id);
        assert_eq!(
            names(lambda_table),
            vec!["a", "b", "c", "d", "args", "kwargs"],
        );

        let use_def = index.use_def_map(lambda_scope_id);
        for name in ["a", "b", "c", "d"] {
            let binding = use_def
                .first_public_binding(lambda_table.symbol_id(name).expect("symbol exists"))
                .unwrap();
            assert!(matches!(binding.kind(&db), DefinitionKind::Parameter(_)));
        }
        let args_binding = use_def
            .first_public_binding(lambda_table.symbol_id("args").expect("symbol exists"))
            .unwrap();
        assert!(matches!(
            args_binding.kind(&db),
            DefinitionKind::VariadicPositionalParameter(_)
        ));
        let kwargs_binding = use_def
            .first_public_binding(lambda_table.symbol_id("kwargs").expect("symbol exists"))
            .unwrap();
        assert!(matches!(
            kwargs_binding.kind(&db),
            DefinitionKind::VariadicKeywordParameter(_)
        ));
    }

    /// Test case to validate that the comprehension scope is correctly identified and that the target
    /// variable is defined only in the comprehension scope and not in the global scope.
    #[test]
    fn comprehension_scope() {
        let TestCase { db, file } = test_case(
            "
[x for x, y in iter1]
",
        );

        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["iter1"]);

        let [(comprehension_scope_id, comprehension_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };

        assert_eq!(comprehension_scope.kind(), ScopeKind::Comprehension);
        assert_eq!(
            comprehension_scope_id
                .to_scope_id(&db, file)
                .name(&db, &module),
            "<listcomp>"
        );

        let comprehension_symbol_table = index.place_table(comprehension_scope_id);

        assert_eq!(names(comprehension_symbol_table), vec!["x", "y"]);

        let use_def = index.use_def_map(comprehension_scope_id);
        for name in ["x", "y"] {
            let binding = use_def
                .first_public_binding(
                    comprehension_symbol_table
                        .symbol_id(name)
                        .expect("symbol exists"),
                )
                .unwrap();
            assert!(matches!(
                binding.kind(&db),
                DefinitionKind::Comprehension(_)
            ));
        }
    }

    /// Test case to validate that the `x` variable used in the comprehension is referencing the
    /// `x` variable defined by the inner generator (`for x in iter2`) and not the outer one.
    #[test]
    fn multiple_generators() {
        let TestCase { db, file } = test_case(
            "
[x for x in iter1 for x in iter2]
",
        );

        let index = semantic_index(&db, file);
        let [(comprehension_scope_id, _)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };

        let use_def = index.use_def_map(comprehension_scope_id);

        let module = parsed_module(&db, file).load(&db);
        let syntax = module.syntax();
        let element = syntax.body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_list_comp_expr()
            .unwrap()
            .elt
            .as_name_expr()
            .unwrap();
        let element_use_id =
            element.scoped_use_id(&db, comprehension_scope_id.to_scope_id(&db, file));

        let binding = use_def.first_binding_at_use(element_use_id).unwrap();
        let DefinitionKind::Comprehension(comprehension) = binding.kind(&db) else {
            panic!("expected generator definition")
        };
        let target = comprehension.target(&module);
        let name = target.as_name_expr().unwrap().id().as_str();

        assert_eq!(name, "x");
        assert_eq!(target.range(), TextRange::new(23.into(), 24.into()));
    }

    /// Test case to validate that the nested comprehension creates a new scope which is a child of
    /// the outer comprehension scope and the variables are correctly defined in the respective
    /// scopes.
    #[test]
    fn nested_generators() {
        let TestCase { db, file } = test_case(
            "
[{x for x in iter2} for y in iter1]
",
        );

        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["iter1"]);

        let [(comprehension_scope_id, comprehension_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };

        assert_eq!(comprehension_scope.kind(), ScopeKind::Comprehension);
        assert_eq!(
            comprehension_scope_id
                .to_scope_id(&db, file)
                .name(&db, &module),
            "<listcomp>"
        );

        let comprehension_symbol_table = index.place_table(comprehension_scope_id);

        assert_eq!(names(comprehension_symbol_table), vec!["y", "iter2"]);

        let [(inner_comprehension_scope_id, inner_comprehension_scope)] = index
            .child_scopes(comprehension_scope_id)
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one inner generator scope")
        };

        assert_eq!(inner_comprehension_scope.kind(), ScopeKind::Comprehension);
        assert_eq!(
            inner_comprehension_scope_id
                .to_scope_id(&db, file)
                .name(&db, &module),
            "<setcomp>"
        );

        let inner_comprehension_symbol_table = index.place_table(inner_comprehension_scope_id);

        assert_eq!(names(inner_comprehension_symbol_table), vec!["x"]);
    }

    #[test]
    fn with_item_definition() {
        let TestCase { db, file } = test_case(
            "
with item1 as x, item2 as y:
    pass
",
        );

        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["item1", "x", "item2", "y"]);

        let use_def = index.use_def_map(FileScopeId::global());
        for name in ["x", "y"] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            assert!(matches!(binding.kind(&db), DefinitionKind::WithItem(_)));
        }
    }

    #[test]
    fn with_item_unpacked_definition() {
        let TestCase { db, file } = test_case(
            "
with context() as (x, y):
    pass
",
        );

        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["context", "x", "y"]);

        let use_def = index.use_def_map(FileScopeId::global());
        for name in ["x", "y"] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            assert!(matches!(binding.kind(&db), DefinitionKind::WithItem(_)));
        }
    }

    #[test]
    fn dupes() {
        let TestCase { db, file } = test_case(
            "
def func():
    x = 1
def func():
    y = 2
",
        );
        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["func"]);
        let [
            (func_scope1_id, func_scope_1),
            (func_scope2_id, func_scope_2),
        ] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected two child scopes");
        };

        assert_eq!(func_scope_1.kind(), ScopeKind::Function);

        assert_eq!(
            func_scope1_id.to_scope_id(&db, file).name(&db, &module),
            "func"
        );
        assert_eq!(func_scope_2.kind(), ScopeKind::Function);
        assert_eq!(
            func_scope2_id.to_scope_id(&db, file).name(&db, &module),
            "func"
        );

        let func1_table = index.place_table(func_scope1_id);
        let func2_table = index.place_table(func_scope2_id);
        assert_eq!(names(func1_table), vec!["x"]);
        assert_eq!(names(func2_table), vec!["y"]);

        let use_def = index.use_def_map(FileScopeId::global());
        let binding = use_def
            .first_public_binding(global_table.symbol_id("func").expect("symbol exists"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Function(_)));
    }

    #[test]
    fn generic_function() {
        let TestCase { db, file } = test_case(
            "
def func[T]():
    x = 1
",
        );

        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["func"]);

        let [(ann_scope_id, ann_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };

        assert_eq!(ann_scope.kind(), ScopeKind::TypeParams);
        assert_eq!(
            ann_scope_id.to_scope_id(&db, file).name(&db, &module),
            "func"
        );
        let ann_table = index.place_table(ann_scope_id);
        assert_eq!(names(ann_table), vec!["T"]);

        let [(func_scope_id, func_scope)] =
            index.child_scopes(ann_scope_id).collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };
        assert_eq!(func_scope.kind(), ScopeKind::Function);
        assert_eq!(
            func_scope_id.to_scope_id(&db, file).name(&db, &module),
            "func"
        );
        let func_table = index.place_table(func_scope_id);
        assert_eq!(names(func_table), vec!["x"]);
    }

    #[test]
    fn generic_class() {
        let TestCase { db, file } = test_case(
            "
class C[T]:
    x = 1
",
        );

        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);
        let global_table = index.place_table(FileScopeId::global());

        assert_eq!(names(global_table), vec!["C"]);

        let [(ann_scope_id, ann_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };

        assert_eq!(ann_scope.kind(), ScopeKind::TypeParams);
        assert_eq!(ann_scope_id.to_scope_id(&db, file).name(&db, &module), "C");
        let ann_table = index.place_table(ann_scope_id);
        assert_eq!(names(ann_table), vec!["T"]);
        assert!(
            ann_table
                .symbol_by_name("T")
                .is_some_and(|s| s.is_bound() && !s.is_used()),
            "type parameters are defined by the scope that introduces them"
        );

        let [(class_scope_id, class_scope)] =
            index.child_scopes(ann_scope_id).collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };

        assert_eq!(class_scope.kind(), ScopeKind::Class);
        assert_eq!(
            class_scope_id.to_scope_id(&db, file).name(&db, &module),
            "C"
        );
        assert_eq!(names(index.place_table(class_scope_id)), vec!["x"]);
    }

    #[test]
    fn reachability_trivial() {
        let TestCase { db, file } = test_case("x = 1; x");
        let module = parsed_module(&db, file).load(&db);
        let scope = global_scope(&db, file);
        let ast = module.syntax();
        let ast::Stmt::Expr(ast::StmtExpr {
            value: x_use_expr, ..
        }) = &ast.body[1]
        else {
            panic!("should be an expr")
        };
        let ast::Expr::Name(x_use_expr_name) = x_use_expr.as_ref() else {
            panic!("expected a Name");
        };
        let x_use_id = x_use_expr_name.scoped_use_id(&db, scope);
        let use_def = use_def_map(&db, scope);
        let binding = use_def.first_binding_at_use(x_use_id).unwrap();
        let DefinitionKind::Assignment(assignment) = binding.kind(&db) else {
            panic!("should be an assignment definition")
        };
        let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(num),
            ..
        }) = assignment.value(&module)
        else {
            panic!("should be a number literal")
        };
        assert_eq!(*num, 1);
    }

    #[test]
    fn expression_scope() {
        let TestCase { db, file } = test_case("x = 1;\ndef test():\n  y = 4");

        let index = semantic_index(&db, file);
        let module = parsed_module(&db, file).load(&db);
        let ast = module.syntax();

        let x_stmt = ast.body[0].as_assign_stmt().unwrap();
        let x = &x_stmt.targets[0];

        assert_eq!(index.expression_scope(x).kind(), ScopeKind::Module);
        assert_eq!(index.expression_scope_id(x), FileScopeId::global());

        let def = ast.body[1].as_function_def_stmt().unwrap();
        let y_stmt = def.body[0].as_assign_stmt().unwrap();
        let y = &y_stmt.targets[0];

        assert_eq!(index.expression_scope(y).kind(), ScopeKind::Function);
    }

    #[test]
    fn scope_iterators() {
        fn scope_names<'a, 'db>(
            scopes: impl Iterator<Item = (FileScopeId, &'db Scope)>,
            db: &'db dyn Db,
            file: File,
            module: &'a ParsedModuleRef,
        ) -> Vec<&'a str> {
            scopes
                .into_iter()
                .map(|(scope_id, _)| scope_id.to_scope_id(db, file).name(db, module))
                .collect()
        }

        let TestCase { db, file } = test_case(
            r"
class Test:
    def foo():
        def bar():
            ...
    def baz():
        pass

def x():
    pass",
        );

        let module = parsed_module(&db, file).load(&db);
        let index = semantic_index(&db, file);

        let descendants = index.descendent_scopes(FileScopeId::global());
        assert_eq!(
            scope_names(descendants, &db, file, &module),
            vec!["Test", "foo", "bar", "baz", "x"]
        );

        let children = index.child_scopes(FileScopeId::global());
        assert_eq!(scope_names(children, &db, file, &module), vec!["Test", "x"]);

        let test_class = index.child_scopes(FileScopeId::global()).next().unwrap().0;
        let test_child_scopes = index.child_scopes(test_class);
        assert_eq!(
            scope_names(test_child_scopes, &db, file, &module),
            vec!["foo", "baz"]
        );

        let bar_scope = index
            .descendent_scopes(FileScopeId::global())
            .nth(2)
            .unwrap()
            .0;
        let ancestors = index.ancestor_scopes(bar_scope);

        assert_eq!(
            scope_names(ancestors, &db, file, &module),
            vec!["bar", "foo", "Test", "<module>"]
        );
    }

    #[test]
    fn match_stmt() {
        let TestCase { db, file } = test_case(
            "
match subject:
    case a: ...
    case [b, c, *d]: ...
    case e as f: ...
    case {'x': g, **h}: ...
    case Foo(i, z=j): ...
    case k | l: ...
    case _: ...
",
        );

        let global_scope_id = global_scope(&db, file);
        let global_table = place_table(&db, global_scope_id);

        assert!(global_table.symbol_by_name("Foo").unwrap().is_used());
        assert_eq!(
            names(global_table),
            vec![
                "subject", "a", "b", "c", "d", "e", "f", "g", "h", "Foo", "i", "j", "k", "l"
            ]
        );

        let use_def = use_def_map(&db, global_scope_id);
        for (name, expected_index) in [
            ("a", 0),
            ("b", 0),
            ("c", 1),
            ("d", 2),
            ("e", 0),
            ("f", 1),
            ("g", 0),
            ("h", 1),
            ("i", 0),
            ("j", 1),
            ("k", 0),
            ("l", 1),
        ] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            if let DefinitionKind::MatchPattern(pattern) = binding.kind(&db) {
                assert_eq!(pattern.index(), expected_index);
            } else {
                panic!("Expected match pattern definition for {name}");
            }
        }
    }

    #[test]
    fn nested_match_case() {
        let TestCase { db, file } = test_case(
            "
match 1:
    case first:
        match 2:
            case second:
                pass
",
        );

        let global_scope_id = global_scope(&db, file);
        let global_table = place_table(&db, global_scope_id);

        assert_eq!(names(global_table), vec!["first", "second"]);

        let use_def = use_def_map(&db, global_scope_id);
        for (name, expected_index) in [("first", 0), ("second", 0)] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            if let DefinitionKind::MatchPattern(pattern) = binding.kind(&db) {
                assert_eq!(pattern.index(), expected_index);
            } else {
                panic!("Expected match pattern definition for {name}");
            }
        }
    }

    #[test]
    fn for_loops_single_assignment() {
        let TestCase { db, file } = test_case("for x in a: pass");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(&names(global_table), &["a", "x"]);

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id("x").unwrap())
            .unwrap();

        assert!(matches!(binding.kind(&db), DefinitionKind::For(_)));
    }

    #[test]
    fn for_loops_simple_unpacking() {
        let TestCase { db, file } = test_case("for (x, y) in a: pass");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(&names(global_table), &["a", "x", "y"]);

        let use_def = use_def_map(&db, scope);
        let x_binding = use_def
            .first_public_binding(global_table.symbol_id("x").unwrap())
            .unwrap();
        let y_binding = use_def
            .first_public_binding(global_table.symbol_id("y").unwrap())
            .unwrap();

        assert!(matches!(x_binding.kind(&db), DefinitionKind::For(_)));
        assert!(matches!(y_binding.kind(&db), DefinitionKind::For(_)));
    }

    #[test]
    fn for_loops_complex_unpacking() {
        let TestCase { db, file } = test_case("for [((a,) b), (c, d)] in e: pass");
        let scope = global_scope(&db, file);
        let global_table = place_table(&db, scope);

        assert_eq!(&names(global_table), &["e", "a", "b", "c", "d"]);

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id("a").unwrap())
            .unwrap();

        assert!(matches!(binding.kind(&db), DefinitionKind::For(_)));
    }
}

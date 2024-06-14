use std::hash::Hash;
use std::iter::FusedIterator;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_db::parsed::parsed_module;
use ruff_db::vfs::VfsFile;
use ruff_index::{IndexSlice, IndexVec};
use ruff_python_ast as ast;

use crate::red_knot::node_key::NodeKey;
use crate::red_knot::semantic_index::ast_ids::AstIds;
use crate::red_knot::semantic_index::builder::SemanticIndexBuilder;
use crate::red_knot::semantic_index::symbol::{
    GlobalScope, GlobalSymbolId, LocalSymbolId, Scope, ScopeId, ScopesMap, SymbolId, SymbolTable,
};
use crate::Db;

pub mod ast_ids;
mod builder;
pub mod definition;
pub mod symbol;

type SymbolMap = hashbrown::HashMap<LocalSymbolId, (), ()>;

/// Returns the semantic index for `file`.
///
/// Prefer using [`GlobalScope::symbol_table`] when working with symbols from a single scope.
#[salsa::tracked(return_ref, no_eq)]
pub(crate) fn semantic_index(db: &dyn Db, file: VfsFile) -> SemanticIndex {
    let parsed = parsed_module(db.upcast(), file);

    SemanticIndexBuilder::new(parsed).build()
}

/// Returns the symbol table for `scope`.
#[salsa::tracked]
pub fn symbol_table(db: &dyn Db, scope: GlobalScope) -> Arc<SymbolTable> {
    let index = semantic_index(db, scope.file(db));

    index.symbol_table(scope.scope_id(db))
}

/// Returns a mapping from [`ScopeId`] to globally unique [`GlobalScope`].
#[salsa::tracked(return_ref)]
pub(crate) fn scopes_map(db: &dyn Db, file: VfsFile) -> ScopesMap {
    let index = semantic_index(db, file);

    let scopes: IndexVec<_, _> = index
        .scopes
        .indices()
        .map(|id| GlobalScope::new(db, file, id))
        .collect();

    ScopesMap::new(scopes)
}

/// Returns the root scope of `file`.
pub fn root_scope(db: &dyn Db, file: VfsFile) -> GlobalScope {
    let scopes = scopes_map(db, file);
    scopes[ScopeId::root()]
}

/// Returns the symbol with the given name in `file`'s public scope or `None` if
/// no symbol with the given name exists.
pub fn global_symbol(db: &dyn Db, file: VfsFile, name: &str) -> Option<GlobalSymbolId> {
    let root_scope = root_scope(db, file);
    let symbol_table = symbol_table(db, root_scope);
    let local_id = symbol_table.symbol_id_by_name(name)?;
    let symbol_id = SymbolId::new(root_scope.scope_id(db), local_id);

    Some(GlobalSymbolId::new(file, symbol_id))
}

/// The symbol table for an entire file.
#[derive(Debug)]
pub struct SemanticIndex {
    /// List of all symbol tables in this file, indexed by scope.
    symbol_tables: IndexVec<ScopeId, Arc<SymbolTable>>,

    /// List of all scopes in this file.
    scopes: IndexVec<ScopeId, Scope>,

    /// Maps expressions to their corresponding scope.
    /// We can't use [`ExpressionId`] here, because the challenge is how to get from
    /// an [`ast::Expr`] to an [`ExpressionId`] (which requires knowing the scope).
    expression_scopes: FxHashMap<NodeKey, ScopeId>,

    /// Lookup table to map between node ids and ast nodes.
    ast_ids: IndexVec<ScopeId, Arc<AstIds>>, // expressions_map: Arc<FxHashMap<AstNodeRef, ExpressionId>> where `ExpressionId` is a combination of `ModuleScopeId` and `LocalExpressionId`

    scopes_by_node: FxHashMap<NodeWithScope, ScopeId>,
}

impl SemanticIndex {
    /// Returns the symbol table for a specific scope.
    fn symbol_table(&self, scope_id: ScopeId) -> Arc<SymbolTable> {
        self.symbol_tables[scope_id].clone()
    }

    fn ast_ids(&self, scope_id: ScopeId) -> Arc<AstIds> {
        self.ast_ids[scope_id].clone()
    }

    pub(crate) fn expression_scope_id(&self, expression: &ast::Expr) -> ScopeId {
        self.expression_scopes[&NodeKey::from_node(expression)]
    }

    pub(crate) fn expression_scope(&self, expression: &ast::Expr) -> &Scope {
        &self.scopes[self.expression_scope_id(expression)]
    }

    /// Returns the [`Scope`] with the given id.
    pub(crate) fn scope(&self, id: ScopeId) -> &Scope {
        &self.scopes[id]
    }

    /// Returns the id of the parent scope.
    pub(crate) fn parent_scope_id(&self, scope_id: ScopeId) -> Option<ScopeId> {
        let scope = &self.scopes[scope_id];
        scope.parent
    }

    /// Returns the parent scope of `scope_id`.
    pub(crate) fn parent_scope(&self, scope_id: ScopeId) -> Option<&Scope> {
        Some(&self.scopes[self.parent_scope_id(scope_id)?])
    }

    /// Returns an iterator over the descendent scopes of `scope`.
    pub(crate) fn descendent_scopes(&self, scope: ScopeId) -> DescendentsIter {
        DescendentsIter::new(self, scope)
    }

    /// Returns an iterator over the direct child scopes of `scope`.
    pub(crate) fn child_scopes(&self, scope: ScopeId) -> ChildrenIter {
        ChildrenIter::new(self, scope)
    }

    /// Returns an iterator over all ancestors of `scope`, starting with `scope` itself.
    pub(crate) fn ancestor_scopes(&self, scope: ScopeId) -> AncestorsIter {
        AncestorsIter::new(self, scope)
    }

    pub(crate) fn node_scope_id(&self, node: impl Into<NodeWithScope>) -> ScopeId {
        self.scopes_by_node[&node.into()]
    }

    pub(crate) fn node_scope(&self, node: impl Into<NodeWithScope>) -> &Scope {
        &self.scopes[self.node_scope_id(node)]
    }
}

/// ID that uniquely identifies an expression inside a [`Scope`].

pub struct AncestorsIter<'a> {
    scopes: &'a IndexSlice<ScopeId, Scope>,
    next_id: Option<ScopeId>,
}

impl<'a> AncestorsIter<'a> {
    fn new(module_symbol_table: &'a SemanticIndex, start: ScopeId) -> Self {
        Self {
            scopes: &module_symbol_table.scopes,
            next_id: Some(start),
        }
    }
}

impl<'a> Iterator for AncestorsIter<'a> {
    type Item = (ScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let current_id = self.next_id?;
        let current = &self.scopes[current_id];
        self.next_id = current.parent;

        Some((current_id, current))
    }
}

impl FusedIterator for AncestorsIter<'_> {}

pub struct DescendentsIter<'a> {
    next_id: ScopeId,
    descendents: std::slice::Iter<'a, Scope>,
}

impl<'a> DescendentsIter<'a> {
    fn new(symbol_table: &'a SemanticIndex, scope_id: ScopeId) -> Self {
        let scope = &symbol_table.scopes[scope_id];
        let scopes = &symbol_table.scopes[scope.descendents.clone()];

        Self {
            next_id: scope_id + 1,
            descendents: scopes.iter(),
        }
    }
}

impl<'a> Iterator for DescendentsIter<'a> {
    type Item = (ScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let descendent = self.descendents.next()?;
        let id = self.next_id;
        self.next_id = self.next_id + 1;

        Some((id, descendent))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.descendents.size_hint()
    }
}

impl FusedIterator for DescendentsIter<'_> {}

impl ExactSizeIterator for DescendentsIter<'_> {}

pub struct ChildrenIter<'a> {
    parent: ScopeId,
    descendents: DescendentsIter<'a>,
}

impl<'a> ChildrenIter<'a> {
    fn new(module_symbol_table: &'a SemanticIndex, parent: ScopeId) -> Self {
        let descendents = DescendentsIter::new(module_symbol_table, parent);

        Self {
            descendents,
            parent,
        }
    }
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = (ScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        self.descendents
            .find(|(_, scope)| scope.parent == Some(self.parent))
    }
}

impl FusedIterator for ChildrenIter<'_> {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum NodeWithScope {
    Module(NodeKey),
    Function(NodeKey),
    Class(NodeKey),
    Lambda(NodeKey),
}

impl From<&ast::StmtFunctionDef> for NodeWithScope {
    fn from(value: &ast::StmtFunctionDef) -> Self {
        Self::Function(NodeKey::from_node(value))
    }
}

impl From<&ast::StmtClassDef> for NodeWithScope {
    fn from(value: &ast::StmtClassDef) -> Self {
        Self::Class(NodeKey::from_node(value))
    }
}

impl From<&ast::ModModule> for NodeWithScope {
    fn from(value: &ast::ModModule) -> Self {
        Self::Module(NodeKey::from_node(value))
    }
}

impl From<&ast::ExprLambda> for NodeWithScope {
    fn from(value: &ast::ExprLambda) -> Self {
        Self::Lambda(NodeKey::from_node(value))
    }
}

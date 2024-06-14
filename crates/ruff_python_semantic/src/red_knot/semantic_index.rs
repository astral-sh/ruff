use std::hash::{Hash, Hasher};
use std::iter::FusedIterator;
use std::ops::{Index, Range};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHasher};

use ruff_db::parsed::parsed_module;
use ruff_db::vfs::VfsFile;
use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_python_ast as ast;

use crate::name::Name;
use crate::red_knot::node_key::NodeKey;
use crate::red_knot::semantic_index::ast_ids::AstIds;
use crate::red_knot::semantic_index::builder::SemanticIndexBuilder;
use crate::red_knot::semantic_index::definition::Definition;
use crate::red_knot::semantic_index::symbol::{GlobalSymbolId, LocalSymbolId, Symbol, SymbolId};
use crate::Db;

pub mod ast_ids;
mod builder;
mod definition;
pub mod symbol;

type SymbolMap = hashbrown::HashMap<LocalSymbolId, (), ()>;

/// Maps from the file specific [`ScopeId`] to the global [`GlobalScope`] that can be used as a Salsa query parameter.
///
/// The [`SemanticIndex`] uses [`ScopeId`] on a per-file level to identify scopes
/// because they allow for more efficient storage of associated data
/// (use of an [`IndexVec`] keyed by [`ScopeId`] over an [`FxHashMap`] keyed by [`GlobalScope`]).
#[derive(Eq, PartialEq, Debug)]
pub struct ScopesMap {
    scopes: IndexVec<ScopeId, GlobalScope>,
}

impl Index<ScopeId> for ScopesMap {
    type Output = GlobalScope;

    fn index(&self, index: ScopeId) -> &Self::Output {
        &self.scopes[index]
    }
}

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked]
pub struct GlobalScope {
    pub file: VfsFile,
    pub scope_id: ScopeId,
}

#[salsa::tracked]
impl GlobalScope {
    /// Returns the symbol table for this scope.
    #[salsa::tracked]
    pub fn symbol_table(self, db: &dyn Db) -> Arc<SymbolTable> {
        let index = semantic_index(db, self.file(db));

        index.symbol_table(self.scope_id(db))
    }
}

/// ID that uniquely identifies a scope inside of a module.
#[newtype_index]
pub struct ScopeId;

impl ScopeId {
    /// Returns the scope id of the Root scope.
    pub fn root() -> Self {
        ScopeId::from_u32(0)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Scope {
    pub name: Name,

    pub parent: Option<ScopeId>,

    pub definition: Option<Definition>,

    // Always a symbol from the parent scope?
    pub defining_symbol: Option<SymbolId>,

    // Module, function, Class, Annotation
    pub kind: ScopeKind,

    pub descendents: Range<ScopeId>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
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
    pub(super) fn symbol_table(&self, scope_id: ScopeId) -> Arc<SymbolTable> {
        self.symbol_tables[scope_id].clone()
    }

    pub(super) fn ast_ids(&self, scope_id: ScopeId) -> Arc<AstIds> {
        self.ast_ids[scope_id].clone()
    }

    pub(super) fn expression_scope_id(&self, expression: &ast::Expr) -> ScopeId {
        self.expression_scopes[&NodeKey::from_node(expression)]
    }

    pub(super) fn expression_scope(&self, expression: &ast::Expr) -> &Scope {
        &self.scopes[self.expression_scope_id(expression)]
    }

    /// Returns the [`Scope`] with the given id.
    pub(super) fn scope(&self, id: ScopeId) -> &Scope {
        &self.scopes[id]
    }

    /// Returns the id of the parent scope.
    pub(super) fn parent_scope_id(&self, scope_id: ScopeId) -> Option<ScopeId> {
        let scope = &self.scopes[scope_id];
        scope.parent
    }

    /// Returns the parent scope of `scope_id`.
    pub(super) fn parent_scope(&self, scope_id: ScopeId) -> Option<&Scope> {
        Some(&self.scopes[self.parent_scope_id(scope_id)?])
    }

    /// Returns an iterator over the descendent scopes of `scope`.
    pub(super) fn descendent_scopes(&self, scope: ScopeId) -> DescendentsIter {
        DescendentsIter::new(self, scope)
    }

    /// Returns an iterator over the direct child scopes of `scope`.
    pub(super) fn child_scopes(&self, scope: ScopeId) -> ChildrenIter {
        ChildrenIter::new(self, scope)
    }

    /// Returns an iterator over all ancestors of `scope`, starting with `scope` itself.
    pub(super) fn ancestor_scopes(&self, scope: ScopeId) -> AncestorsIter {
        AncestorsIter::new(self, scope)
    }

    pub(super) fn node_scope_id(&self, node: impl Into<NodeWithScope>) -> ScopeId {
        self.scopes_by_node[&node.into()]
    }

    pub(super) fn node_scope(&self, node: impl Into<NodeWithScope>) -> &Scope {
        &self.scopes[self.node_scope_id(node)]
    }
}

/// Returns the semantic index for `file`.
///
/// Prefer using [`GlobalScope::symbol_table`] when working with symbols from a single scope.
#[salsa::tracked(return_ref, no_eq)]
pub(crate) fn semantic_index(db: &dyn Db, file: VfsFile) -> SemanticIndex {
    let parsed = parsed_module(db.upcast(), file);

    SemanticIndexBuilder::new(parsed).build()
}

/// Returns a mapping from [`ScopeId`] to globally unique [`GlobalScope`].
#[salsa::tracked(return_ref)]
pub fn scopes_map(db: &dyn Db, file: VfsFile) -> ScopesMap {
    let index = semantic_index(db, file);

    let scopes: IndexVec<_, _> = index
        .scopes
        .indices()
        .map(|id| GlobalScope::new(db, file, id))
        .collect();

    ScopesMap { scopes }
}

/// Returns the root scope of `file`.
pub fn root_scope(db: &dyn Db, file: VfsFile) -> GlobalScope {
    let scopes = scopes_map(db, file);
    scopes.scopes[ScopeId::root()]
}

/// Returns the symbol with the given name in `file`'s public scope or `None` if
/// no symbol with the given name exists.
pub fn global_symbol(db: &dyn Db, file: VfsFile, name: &str) -> Option<GlobalSymbolId> {
    let root_scope = root_scope(db, file);
    let symbol_table = root_scope.symbol_table(db);
    let symbol_id = symbol_table.symbol_id_by_name(name)?;

    Some(GlobalSymbolId::new(file, symbol_id))
}

/// Symbol table for a specific [`Scope`].
#[derive(Debug)]
pub struct SymbolTable {
    scope: ScopeId,

    /// The symbols in this scope.
    symbols: IndexVec<LocalSymbolId, Symbol>,

    /// The symbols indexed by name.
    symbols_by_name: SymbolMap,
}

impl SymbolTable {
    /// Returns the scope id of the Root scope.
    pub(super) fn root_scope_id() -> ScopeId {
        ScopeId::from_u32(0)
    }

    pub(super) fn new(scope_id: ScopeId) -> Self {
        Self {
            scope: scope_id,
            symbols: IndexVec::new(),
            symbols_by_name: SymbolMap::default(),
        }
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
    }
    /// TODO: Should these methods take [`LocalSymbolId`] or [`SymbolId`]?
    pub fn symbol(&self, symbol_id: SymbolId) -> &Symbol {
        debug_assert_eq!(self.scope, symbol_id.scope());

        &self.symbols[symbol_id.symbol()]
    }

    /// Returns the symbol named `name`.
    pub fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        let id = self.symbol_id_by_name(name)?;
        Some(&self.symbols[id.symbol()])
    }

    /// Returns the [`LocalSymbolId`] of the symbol named `name`.
    pub fn symbol_id_by_name(&self, name: &str) -> Option<SymbolId> {
        let (id, _) = self
            .symbols_by_name
            .raw_entry()
            .from_hash(Self::hash_name(name), |id| {
                self.symbols[*id].name().as_str() == name
            })?;

        Some(SymbolId::new(self.scope, *id))
    }

    pub(super) fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

impl PartialEq for SymbolTable {
    fn eq(&self, other: &Self) -> bool {
        // We don't need to compare the symbols_by_name because the name is already captured in `Symbol`.
        self.scope == other.scope && self.symbols == other.symbols
    }
}

impl Eq for SymbolTable {}

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

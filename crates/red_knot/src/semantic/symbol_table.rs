#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::iter::{Copied, DoubleEndedIterator, FusedIterator};
use std::num::NonZeroU32;

use bitflags::bitflags;
use hashbrown::hash_map::{Keys, RawEntryMut};
use rustc_hash::{FxHashMap, FxHasher};

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;

use crate::ast_ids::{NodeKey, TypedNodeKey};
use crate::module::ModuleName;
use crate::semantic::ExpressionId;
use crate::Name;

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

#[newtype_index]
pub struct ScopeId;

impl ScopeId {
    pub fn scope(self, table: &SymbolTable) -> &Scope {
        &table.scopes_by_id[self]
    }
}

#[newtype_index]
pub struct SymbolId;

impl SymbolId {
    pub fn symbol(self, table: &SymbolTable) -> &Symbol {
        &table.symbols_by_id[self]
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

#[derive(Debug)]
pub struct Scope {
    name: Name,
    kind: ScopeKind,
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    /// the definition (e.g. class or function) that created this scope
    definition: Option<Definition>,
    /// the symbol (e.g. class or function) that owns this scope
    defining_symbol: Option<SymbolId>,
    /// symbol IDs, hashed by symbol name
    symbols_by_name: Map<SymbolId, ()>,
}

impl Scope {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn kind(&self) -> ScopeKind {
        self.kind
    }

    pub fn definition(&self) -> Option<Definition> {
        self.definition.clone()
    }

    pub fn defining_symbol(&self) -> Option<SymbolId> {
        self.defining_symbol
    }
}

#[derive(Debug)]
pub(crate) enum Kind {
    FreeVar,
    CellVar,
    CellVarAssigned,
    ExplicitGlobal,
    ImplicitGlobal,
}

bitflags! {
    #[derive(Copy,Clone,Debug)]
    pub struct SymbolFlags: u8 {
        const IS_USED         = 1 << 0;
        const IS_DEFINED      = 1 << 1;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL   = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL = 1 << 3;
    }
}

#[derive(Debug)]
pub struct Symbol {
    name: Name,
    flags: SymbolFlags,
    scope_id: ScopeId,
    // kind: Kind,
}

impl Symbol {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    /// Is the symbol used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol defined in its containing scope?
    pub fn is_defined(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_DEFINED)
    }

    // TODO: implement Symbol.kind 2-pass analysis to categorize as: free-var, cell-var,
    // explicit-global, implicit-global and implement Symbol.kind by modifying the preorder
    // traversal code
}

// TODO storing TypedNodeKey for definitions means we have to search to find them again in the AST;
// this is at best O(log n). If looking up definitions is a bottleneck we should look for
// alternatives here.
// TODO intern Definitions in SymbolTable and reference using IDs?
#[derive(Clone, Debug)]
pub enum Definition {
    // For the import cases, we don't need reference to any arbitrary AST subtrees (annotations,
    // RHS), and referencing just the import statement node is imprecise (a single import statement
    // can assign many symbols, we'd have to re-search for the one we care about), so we just copy
    // the small amount of information we need from the AST.
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(TypedNodeKey<ast::StmtClassDef>),
    FunctionDef(TypedNodeKey<ast::StmtFunctionDef>),
    Assignment(TypedNodeKey<ast::StmtAssign>),
    AnnotatedAssignment(TypedNodeKey<ast::StmtAnnAssign>),
    /// represents the implicit initial definition of every name as "unbound"
    Unbound,
    // TODO with statements, except handlers, function args...
}

#[derive(Clone, Debug)]
pub struct ImportDefinition {
    pub module: ModuleName,
}

#[derive(Clone, Debug)]
pub struct ImportFromDefinition {
    pub module: Option<ModuleName>,
    pub name: Name,
    pub level: u32,
}

impl ImportFromDefinition {
    pub fn module(&self) -> Option<&ModuleName> {
        self.module.as_ref()
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn level(&self) -> u32 {
        self.level
    }
}

#[derive(Debug, Clone)]
pub enum Dependency {
    Module(ModuleName),
    Relative {
        level: NonZeroU32,
        module: Option<ModuleName>,
    },
}

/// Table of all symbols in all scopes for a module.
#[derive(Debug)]
pub struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
    /// the definitions for each symbol
    defs: FxHashMap<SymbolId, Vec<Definition>>,
    /// map of AST node (e.g. class/function def) to sub-scope it creates
    scopes_by_node: FxHashMap<NodeKey, ScopeId>,
    /// Maps expressions to their enclosing scope.
    expression_scopes: IndexVec<ExpressionId, ScopeId>,
    /// dependencies of this module
    dependencies: Vec<Dependency>,
}

impl SymbolTable {
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    pub const fn root_scope_id() -> ScopeId {
        ScopeId::from_usize(0)
    }

    pub fn root_scope(&self) -> &Scope {
        &self.scopes_by_id[SymbolTable::root_scope_id()]
    }

    pub fn symbol_ids_for_scope(&self, scope_id: ScopeId) -> Copied<Keys<SymbolId, ()>> {
        self.scopes_by_id[scope_id].symbols_by_name.keys().copied()
    }

    pub fn symbols_for_scope(
        &self,
        scope_id: ScopeId,
    ) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        SymbolIterator {
            table: self,
            ids: self.symbol_ids_for_scope(scope_id),
        }
    }

    pub fn root_symbol_ids(&self) -> Copied<Keys<SymbolId, ()>> {
        self.symbol_ids_for_scope(SymbolTable::root_scope_id())
    }

    pub fn root_symbols(&self) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        self.symbols_for_scope(SymbolTable::root_scope_id())
    }

    pub fn child_scope_ids_of(&self, scope_id: ScopeId) -> &[ScopeId] {
        &self.scopes_by_id[scope_id].children
    }

    pub fn child_scopes_of(&self, scope_id: ScopeId) -> ScopeIterator<&[ScopeId]> {
        ScopeIterator {
            table: self,
            ids: self.child_scope_ids_of(scope_id),
        }
    }

    pub fn root_child_scope_ids(&self) -> &[ScopeId] {
        self.child_scope_ids_of(SymbolTable::root_scope_id())
    }

    pub fn root_child_scopes(&self) -> ScopeIterator<&[ScopeId]> {
        self.child_scopes_of(SymbolTable::root_scope_id())
    }

    pub fn symbol_id_by_name(&self, scope_id: ScopeId, name: &str) -> Option<SymbolId> {
        let scope = &self.scopes_by_id[scope_id];
        let hash = SymbolTable::hash_name(name);
        let name = Name::new(name);
        Some(
            *scope
                .symbols_by_name
                .raw_entry()
                .from_hash(hash, |symid| self.symbols_by_id[*symid].name == name)?
                .0,
        )
    }

    pub fn symbol_by_name(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        Some(&self.symbols_by_id[self.symbol_id_by_name(scope_id, name)?])
    }

    pub fn root_symbol_id_by_name(&self, name: &str) -> Option<SymbolId> {
        self.symbol_id_by_name(SymbolTable::root_scope_id(), name)
    }

    pub fn root_symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        self.symbol_by_name(SymbolTable::root_scope_id(), name)
    }

    pub fn scope_id_of_symbol(&self, symbol_id: SymbolId) -> ScopeId {
        self.symbols_by_id[symbol_id].scope_id
    }

    pub fn scope_of_symbol(&self, symbol_id: SymbolId) -> &Scope {
        &self.scopes_by_id[self.scope_id_of_symbol(symbol_id)]
    }

    pub fn scope_id_of_expression(&self, expression: ExpressionId) -> ScopeId {
        self.expression_scopes[expression]
    }

    pub fn scope_of_expression(&self, expr_id: ExpressionId) -> &Scope {
        &self.scopes_by_id[self.scope_id_of_expression(expr_id)]
    }

    pub fn parent_scopes(
        &self,
        scope_id: ScopeId,
    ) -> ScopeIterator<impl Iterator<Item = ScopeId> + '_> {
        ScopeIterator {
            table: self,
            ids: std::iter::successors(Some(scope_id), |scope| self.scopes_by_id[*scope].parent),
        }
    }

    pub fn parent_scope(&self, scope_id: ScopeId) -> Option<ScopeId> {
        self.scopes_by_id[scope_id].parent
    }

    pub fn scope_id_for_node(&self, node_key: &NodeKey) -> ScopeId {
        self.scopes_by_node[node_key]
    }

    pub fn definitions(&self, symbol_id: SymbolId) -> &[Definition] {
        self.defs
            .get(&symbol_id)
            .map(std::vec::Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn all_definitions(&self) -> impl Iterator<Item = (SymbolId, &Definition)> + '_ {
        self.defs
            .iter()
            .flat_map(|(sym_id, defs)| defs.iter().map(move |def| (*sym_id, def)))
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

pub struct SymbolIterator<'a, I> {
    table: &'a SymbolTable,
    ids: I,
}

impl<'a, I> Iterator for SymbolIterator<'a, I>
where
    I: Iterator<Item = SymbolId>,
{
    type Item = &'a Symbol;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.next()?;
        Some(&self.table.symbols_by_id[id])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ids.size_hint()
    }
}

impl<'a, I> FusedIterator for SymbolIterator<'a, I> where
    I: Iterator<Item = SymbolId> + FusedIterator
{
}

impl<'a, I> DoubleEndedIterator for SymbolIterator<'a, I>
where
    I: Iterator<Item = SymbolId> + DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let id = self.ids.next_back()?;
        Some(&self.table.symbols_by_id[id])
    }
}

// TODO maybe get rid of this and just do all data access via methods on ScopeId?
pub struct ScopeIterator<'a, I> {
    table: &'a SymbolTable,
    ids: I,
}

/// iterate (`ScopeId`, `Scope`) pairs for given `ScopeId` iterator
impl<'a, I> Iterator for ScopeIterator<'a, I>
where
    I: Iterator<Item = ScopeId>,
{
    type Item = (ScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.next()?;
        Some((id, &self.table.scopes_by_id[id]))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ids.size_hint()
    }
}

impl<'a, I> FusedIterator for ScopeIterator<'a, I> where I: Iterator<Item = ScopeId> + FusedIterator {}

impl<'a, I> DoubleEndedIterator for ScopeIterator<'a, I>
where
    I: Iterator<Item = ScopeId> + DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let id = self.ids.next_back()?;
        Some((id, &self.table.scopes_by_id[id]))
    }
}

#[derive(Debug)]
pub(super) struct SymbolTableBuilder {
    symbol_table: SymbolTable,
}

impl SymbolTableBuilder {
    pub(super) fn new() -> Self {
        let mut table = SymbolTable {
            scopes_by_id: IndexVec::new(),
            symbols_by_id: IndexVec::new(),
            defs: FxHashMap::default(),
            scopes_by_node: FxHashMap::default(),
            expression_scopes: IndexVec::new(),
            dependencies: Vec::new(),
        };
        table.scopes_by_id.push(Scope {
            name: Name::new("<module>"),
            kind: ScopeKind::Module,
            parent: None,
            children: Vec::new(),
            definition: None,
            defining_symbol: None,
            symbols_by_name: Map::default(),
        });
        Self {
            symbol_table: table,
        }
    }

    pub(super) fn finish(self) -> SymbolTable {
        let mut symbol_table = self.symbol_table;
        symbol_table.scopes_by_id.shrink_to_fit();
        symbol_table.symbols_by_id.shrink_to_fit();
        symbol_table.defs.shrink_to_fit();
        symbol_table.scopes_by_node.shrink_to_fit();
        symbol_table.expression_scopes.shrink_to_fit();
        symbol_table.dependencies.shrink_to_fit();
        symbol_table
    }

    pub(super) fn add_or_update_symbol(
        &mut self,
        scope_id: ScopeId,
        name: &str,
        flags: SymbolFlags,
    ) -> SymbolId {
        let hash = SymbolTable::hash_name(name);
        let scope = &mut self.symbol_table.scopes_by_id[scope_id];
        let name = Name::new(name);

        let entry = scope
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |existing| {
                self.symbol_table.symbols_by_id[*existing].name == name
            });

        match entry {
            RawEntryMut::Occupied(entry) => {
                if let Some(symbol) = self.symbol_table.symbols_by_id.get_mut(*entry.key()) {
                    symbol.flags.insert(flags);
                };
                *entry.key()
            }
            RawEntryMut::Vacant(entry) => {
                let id = self.symbol_table.symbols_by_id.push(Symbol {
                    name,
                    flags,
                    scope_id,
                });
                entry.insert_with_hasher(hash, id, (), |symid| {
                    SymbolTable::hash_name(&self.symbol_table.symbols_by_id[*symid].name)
                });
                id
            }
        }
    }

    pub(super) fn add_definition(&mut self, symbol_id: SymbolId, definition: Definition) {
        self.symbol_table
            .defs
            .entry(symbol_id)
            .or_default()
            .push(definition);
    }

    pub(super) fn add_child_scope(
        &mut self,
        parent_scope_id: ScopeId,
        name: &str,
        kind: ScopeKind,
        definition: Option<Definition>,
        defining_symbol: Option<SymbolId>,
    ) -> ScopeId {
        let new_scope_id = self.symbol_table.scopes_by_id.push(Scope {
            name: Name::new(name),
            kind,
            parent: Some(parent_scope_id),
            children: Vec::new(),
            definition,
            defining_symbol,
            symbols_by_name: Map::default(),
        });
        let parent_scope = &mut self.symbol_table.scopes_by_id[parent_scope_id];
        parent_scope.children.push(new_scope_id);
        new_scope_id
    }

    pub(super) fn record_scope_for_node(&mut self, node_key: NodeKey, scope_id: ScopeId) {
        self.symbol_table.scopes_by_node.insert(node_key, scope_id);
    }

    pub(super) fn add_dependency(&mut self, dependency: Dependency) {
        self.symbol_table.dependencies.push(dependency);
    }

    /// Records the scope for the current expression
    pub(super) fn record_expression(&mut self, scope: ScopeId) -> ExpressionId {
        self.symbol_table.expression_scopes.push(scope)
    }
}

#[cfg(test)]
mod tests {
    use super::{ScopeKind, SymbolFlags, SymbolTable, SymbolTableBuilder};

    #[test]
    fn insert_same_name_symbol_twice() {
        let mut builder = SymbolTableBuilder::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let symbol_id_1 =
            builder.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::IS_DEFINED);
        let symbol_id_2 = builder.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::IS_USED);
        let table = builder.finish();

        assert_eq!(symbol_id_1, symbol_id_2);
        assert!(symbol_id_1.symbol(&table).is_used(), "flags must merge");
        assert!(symbol_id_1.symbol(&table).is_defined(), "flags must merge");
    }

    #[test]
    fn insert_different_named_symbols() {
        let mut builder = SymbolTableBuilder::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let symbol_id_1 = builder.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        let symbol_id_2 = builder.add_or_update_symbol(root_scope_id, "bar", SymbolFlags::empty());

        assert_ne!(symbol_id_1, symbol_id_2);
    }

    #[test]
    fn add_child_scope_with_symbol() {
        let mut builder = SymbolTableBuilder::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_top =
            builder.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        let c_scope = builder.add_child_scope(root_scope_id, "C", ScopeKind::Class, None, None);
        let foo_symbol_inner = builder.add_or_update_symbol(c_scope, "foo", SymbolFlags::empty());

        assert_ne!(foo_symbol_top, foo_symbol_inner);
    }

    #[test]
    fn scope_from_id() {
        let table = SymbolTableBuilder::new().finish();
        let root_scope_id = SymbolTable::root_scope_id();
        let scope = root_scope_id.scope(&table);

        assert_eq!(scope.name.as_str(), "<module>");
        assert_eq!(scope.kind, ScopeKind::Module);
    }

    #[test]
    fn symbol_from_id() {
        let mut builder = SymbolTableBuilder::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_id =
            builder.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        let table = builder.finish();
        let symbol = foo_symbol_id.symbol(&table);

        assert_eq!(symbol.name(), "foo");
    }

    #[test]
    fn bigger_symbol_table() {
        let mut builder = SymbolTableBuilder::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_id =
            builder.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        builder.add_or_update_symbol(root_scope_id, "bar", SymbolFlags::empty());
        builder.add_or_update_symbol(root_scope_id, "baz", SymbolFlags::empty());
        builder.add_or_update_symbol(root_scope_id, "qux", SymbolFlags::empty());
        let table = builder.finish();

        let foo_symbol_id_2 = table
            .root_symbol_id_by_name("foo")
            .expect("foo symbol to be found");

        assert_eq!(foo_symbol_id_2, foo_symbol_id);
    }
}

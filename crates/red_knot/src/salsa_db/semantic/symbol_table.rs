#![allow(dead_code)]

use std::collections::hash_map::{Entry, Values};
use std::hash::{Hash, Hasher};
use std::iter::{Copied, DoubleEndedIterator, FusedIterator};
use std::num::NonZeroU32;
use std::sync::Arc;

use bitflags::bitflags;
use rustc_hash::{FxHashMap, FxHasher};

use ruff_index::{newtype_index, IndexVec};

use crate::module::ModuleName;
use crate::salsa_db::semantic::ast_ids::{ClassId, FunctionId};
use crate::salsa_db::semantic::definition::Definition;
use crate::salsa_db::semantic::{semantic_index, Db, Jar};
use crate::salsa_db::source::File;
use crate::Name;

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn symbol_table(db: &dyn Db, file: File) -> Arc<SymbolTable> {
    semantic_index(db, file).symbol_table.clone()
}

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Scope {
    name: Name,
    kind: ScopeKind,
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    /// the definition (e.g. class or function) that created this scope
    definition: Option<Definition>,
    /// the symbol (e.g. class or function) that owns this scope
    defining_symbol: Option<SymbolId>,
    // TODO: Avoid storing the name by using the existing hashbrown trick.
    //  It will require a custom Eq implementation on `SymbolTable` that ignores `symbols_by_name` and `children`.
    //  Both these fields don't need to be compared because they are redundant (`symbols_by_name`: `Symbol::scope` already tracks the hierarchy, `children`: `Scope::parent` already tracks the hierarchy).
    symbols_by_name: FxHashMap<Name, SymbolId>,
}

impl Scope {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn kind(&self) -> ScopeKind {
        self.kind
    }

    pub fn definition(&self) -> Option<&Definition> {
        self.definition.as_ref()
    }

    pub fn defining_symbol(&self) -> Option<SymbolId> {
        self.defining_symbol
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum Kind {
    FreeVar,
    CellVar,
    CellVarAssigned,
    ExplicitGlobal,
    ImplicitGlobal,
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct SymbolFlags: u8 {
        const IS_USED         = 1 << 0;
        const IS_DEFINED      = 1 << 1;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL   = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL = 1 << 3;
    }
}

#[derive(Debug, Eq, PartialEq)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Dependency {
    Module(ModuleName),
    Relative {
        level: NonZeroU32,
        module: Option<ModuleName>,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum NodeWithScopeId {
    Class(ClassId),
    Function(FunctionId),
}

/// Table of all symbols in all scopes for a module.
#[derive(Debug, PartialEq, Eq)]
pub struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
    /// the definitions for each symbol
    defs: FxHashMap<SymbolId, Vec<Definition>>,
    /// map of AST node (e.g. class/function def) to sub-scope it creates
    scopes_by_node: FxHashMap<NodeWithScopeId, ScopeId>,
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

    pub fn symbol_ids_for_scope(&self, scope_id: ScopeId) -> Copied<Values<Name, SymbolId>> {
        self.scopes_by_id[scope_id]
            .symbols_by_name
            .values()
            .copied()
    }

    pub fn symbols_for_scope(
        &self,
        scope_id: ScopeId,
    ) -> SymbolIterator<Copied<Values<Name, SymbolId>>> {
        SymbolIterator {
            table: self,
            ids: self.symbol_ids_for_scope(scope_id),
        }
    }

    pub fn root_symbol_ids(&self) -> Copied<Values<Name, SymbolId>> {
        self.symbol_ids_for_scope(SymbolTable::root_scope_id())
    }

    pub fn root_symbols(&self) -> SymbolIterator<Copied<Values<Name, SymbolId>>> {
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
        let name = Name::new(name);
        Some(*scope.symbols_by_name.get(&name)?)
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

    pub fn scope_id_for_node(&self, node: NodeWithScopeId) -> ScopeId {
        self.scopes_by_node[&node]
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
pub(crate) struct SymbolTableBuilder {
    symbol_table: SymbolTable,
}

impl SymbolTableBuilder {
    pub(crate) fn new() -> Self {
        let mut table = SymbolTable {
            scopes_by_id: IndexVec::new(),
            symbols_by_id: IndexVec::new(),
            defs: FxHashMap::default(),
            scopes_by_node: FxHashMap::default(),
            dependencies: Vec::new(),
        };
        table.scopes_by_id.push(Scope {
            name: Name::new("<module>"),
            kind: ScopeKind::Module,
            parent: None,
            children: Vec::new(),
            definition: None,
            defining_symbol: None,
            symbols_by_name: FxHashMap::default(),
        });
        Self {
            symbol_table: table,
        }
    }

    pub(crate) fn finish(self) -> SymbolTable {
        self.symbol_table
    }

    pub(crate) fn add_or_update_symbol(
        &mut self,
        scope_id: ScopeId,
        name: &str,
        flags: SymbolFlags,
    ) -> SymbolId {
        let scope = &mut self.symbol_table.scopes_by_id[scope_id];
        let name = Name::new(name);

        let entry = scope.symbols_by_name.entry(name.clone());

        match entry {
            Entry::Occupied(entry) => {
                if let Some(symbol) = self.symbol_table.symbols_by_id.get_mut(*entry.get()) {
                    symbol.flags.insert(flags);
                };
                *entry.get()
            }
            Entry::Vacant(entry) => {
                let id = self.symbol_table.symbols_by_id.push(Symbol {
                    name,
                    flags,
                    scope_id,
                });
                entry.insert(id);
                id
            }
        }
    }

    pub(crate) fn add_definition(&mut self, symbol_id: SymbolId, definition: Definition) {
        self.symbol_table
            .defs
            .entry(symbol_id)
            .or_default()
            .push(definition);
    }

    pub(crate) fn add_child_scope(
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
            symbols_by_name: FxHashMap::default(),
        });
        let parent_scope = &mut self.symbol_table.scopes_by_id[parent_scope_id];
        parent_scope.children.push(new_scope_id);
        new_scope_id
    }

    pub(crate) fn record_scope_for_node(&mut self, node: NodeWithScopeId, scope_id: ScopeId) {
        self.symbol_table.scopes_by_node.insert(node, scope_id);
    }

    pub(crate) fn add_dependency(&mut self, dependency: Dependency) {
        self.symbol_table.dependencies.push(dependency);
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

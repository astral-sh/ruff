#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::iter::{Copied, DoubleEndedIterator, FusedIterator};
use std::num::NonZeroU32;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use bitflags::bitflags;
use hashbrown::hash_map::{Keys, RawEntryMut};
use rustc_hash::{FxHashMap, FxHasher};

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::visitor::preorder::PreorderVisitor;

use crate::ast_ids::{NodeKey, TypedNodeKey};
use crate::cache::KeyValueCache;
use crate::db::{QueryResult, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::module::{resolve_module, ModuleName};
use crate::parse::parse;
use crate::Name;

#[tracing::instrument(level = "debug", skip(db))]
pub fn symbol_table(db: &dyn SemanticDb, file_id: FileId) -> QueryResult<Arc<SymbolTable>> {
    let jar: &SemanticJar = db.jar()?;

    jar.symbol_tables.get(&file_id, |_| {
        let parsed = parse(db.upcast(), file_id)?;
        Ok(Arc::from(SymbolTable::from_ast(parsed.ast())))
    })
}

#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_global_symbol(
    db: &dyn SemanticDb,
    module: ModuleName,
    name: &str,
) -> QueryResult<Option<GlobalSymbolId>> {
    let Some(typing_module) = resolve_module(db, module)? else {
        return Ok(None);
    };
    let typing_file = typing_module.path(db)?.file();
    let typing_table = symbol_table(db, typing_file)?;
    let Some(typing_override) = typing_table.root_symbol_id_by_name(name) else {
        return Ok(None);
    };
    Ok(Some(GlobalSymbolId {
        file_id: typing_file,
        symbol_id: typing_override,
    }))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GlobalSymbolId {
    pub(crate) file_id: FileId,
    pub(crate) symbol_id: SymbolId,
}

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

#[newtype_index]
pub(crate) struct ScopeId;

impl ScopeId {
    pub(crate) fn scope(self, table: &SymbolTable) -> &Scope {
        &table.scopes_by_id[self]
    }
}

#[newtype_index]
pub struct SymbolId;

impl SymbolId {
    pub(crate) fn symbol(self, table: &SymbolTable) -> &Symbol {
        &table.symbols_by_id[self]
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

#[derive(Debug)]
pub(crate) struct Scope {
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
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn kind(&self) -> ScopeKind {
        self.kind
    }

    pub(crate) fn definition(&self) -> Option<Definition> {
        self.definition.clone()
    }

    pub(crate) fn defining_symbol(&self) -> Option<SymbolId> {
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
    pub(crate) struct SymbolFlags: u8 {
        const IS_USED         = 1 << 0;
        const IS_DEFINED      = 1 << 1;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL   = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL = 1 << 3;
    }
}

#[derive(Debug)]
pub(crate) struct Symbol {
    name: Name,
    flags: SymbolFlags,
    scope_id: ScopeId,
    // kind: Kind,
}

impl Symbol {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    /// Is the symbol used in its containing scope?
    pub(crate) fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol defined in its containing scope?
    pub(crate) fn is_defined(&self) -> bool {
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
pub(crate) enum Definition {
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
    // TODO with statements, except handlers, function args...
}

#[derive(Clone, Debug)]
pub(crate) struct ImportDefinition {
    pub(crate) module: ModuleName,
}

#[derive(Clone, Debug)]
pub(crate) struct ImportFromDefinition {
    pub(crate) module: Option<ModuleName>,
    pub(crate) name: Name,
    pub(crate) level: u32,
}

impl ImportFromDefinition {
    pub(crate) fn module(&self) -> Option<&ModuleName> {
        self.module.as_ref()
    }

    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn level(&self) -> u32 {
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
    /// dependencies of this module
    dependencies: Vec<Dependency>,
}

impl SymbolTable {
    pub(crate) fn from_ast(module: &ast::ModModule) -> Self {
        let root_scope_id = SymbolTable::root_scope_id();
        let mut builder = SymbolTableBuilder {
            table: SymbolTable::new(),
            scopes: vec![root_scope_id],
            current_definition: None,
        };
        builder.visit_body(&module.body);
        builder.table
    }

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
            symbols_by_name: Map::default(),
        });
        table
    }

    pub(crate) fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    pub(crate) const fn root_scope_id() -> ScopeId {
        ScopeId::from_usize(0)
    }

    pub(crate) fn root_scope(&self) -> &Scope {
        &self.scopes_by_id[SymbolTable::root_scope_id()]
    }

    pub(crate) fn symbol_ids_for_scope(&self, scope_id: ScopeId) -> Copied<Keys<SymbolId, ()>> {
        self.scopes_by_id[scope_id].symbols_by_name.keys().copied()
    }

    pub(crate) fn symbols_for_scope(
        &self,
        scope_id: ScopeId,
    ) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        SymbolIterator {
            table: self,
            ids: self.symbol_ids_for_scope(scope_id),
        }
    }

    pub(crate) fn root_symbol_ids(&self) -> Copied<Keys<SymbolId, ()>> {
        self.symbol_ids_for_scope(SymbolTable::root_scope_id())
    }

    pub(crate) fn root_symbols(&self) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        self.symbols_for_scope(SymbolTable::root_scope_id())
    }

    pub(crate) fn child_scope_ids_of(&self, scope_id: ScopeId) -> &[ScopeId] {
        &self.scopes_by_id[scope_id].children
    }

    pub(crate) fn child_scopes_of(&self, scope_id: ScopeId) -> ScopeIterator<&[ScopeId]> {
        ScopeIterator {
            table: self,
            ids: self.child_scope_ids_of(scope_id),
        }
    }

    pub(crate) fn root_child_scope_ids(&self) -> &[ScopeId] {
        self.child_scope_ids_of(SymbolTable::root_scope_id())
    }

    pub(crate) fn root_child_scopes(&self) -> ScopeIterator<&[ScopeId]> {
        self.child_scopes_of(SymbolTable::root_scope_id())
    }

    pub(crate) fn symbol_id_by_name(&self, scope_id: ScopeId, name: &str) -> Option<SymbolId> {
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

    pub(crate) fn symbol_by_name(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        Some(&self.symbols_by_id[self.symbol_id_by_name(scope_id, name)?])
    }

    pub(crate) fn root_symbol_id_by_name(&self, name: &str) -> Option<SymbolId> {
        self.symbol_id_by_name(SymbolTable::root_scope_id(), name)
    }

    pub(crate) fn root_symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        self.symbol_by_name(SymbolTable::root_scope_id(), name)
    }

    pub(crate) fn scope_id_of_symbol(&self, symbol_id: SymbolId) -> ScopeId {
        self.symbols_by_id[symbol_id].scope_id
    }

    pub(crate) fn scope_of_symbol(&self, symbol_id: SymbolId) -> &Scope {
        &self.scopes_by_id[self.scope_id_of_symbol(symbol_id)]
    }

    pub(crate) fn parent_scopes(
        &self,
        scope_id: ScopeId,
    ) -> ScopeIterator<impl Iterator<Item = ScopeId> + '_> {
        ScopeIterator {
            table: self,
            ids: std::iter::successors(Some(scope_id), |scope| self.scopes_by_id[*scope].parent),
        }
    }

    pub(crate) fn parent_scope(&self, scope_id: ScopeId) -> Option<ScopeId> {
        self.scopes_by_id[scope_id].parent
    }

    pub(crate) fn scope_id_for_node(&self, node_key: &NodeKey) -> ScopeId {
        self.scopes_by_node[node_key]
    }

    pub(crate) fn definitions(&self, symbol_id: SymbolId) -> &[Definition] {
        self.defs
            .get(&symbol_id)
            .map(std::vec::Vec::as_slice)
            .unwrap_or_default()
    }

    pub(crate) fn all_definitions(&self) -> impl Iterator<Item = (SymbolId, &Definition)> + '_ {
        self.defs
            .iter()
            .flat_map(|(sym_id, defs)| defs.iter().map(move |def| (*sym_id, def)))
    }

    pub(crate) fn add_or_update_symbol(
        &mut self,
        scope_id: ScopeId,
        name: &str,
        flags: SymbolFlags,
    ) -> SymbolId {
        let hash = SymbolTable::hash_name(name);
        let scope = &mut self.scopes_by_id[scope_id];
        let name = Name::new(name);

        let entry = scope
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |existing| self.symbols_by_id[*existing].name == name);

        match entry {
            RawEntryMut::Occupied(entry) => {
                if let Some(symbol) = self.symbols_by_id.get_mut(*entry.key()) {
                    symbol.flags.insert(flags);
                };
                *entry.key()
            }
            RawEntryMut::Vacant(entry) => {
                let id = self.symbols_by_id.push(Symbol {
                    name,
                    flags,
                    scope_id,
                });
                entry.insert_with_hasher(hash, id, (), |symid| {
                    SymbolTable::hash_name(&self.symbols_by_id[*symid].name)
                });
                id
            }
        }
    }

    fn add_child_scope(
        &mut self,
        parent_scope_id: ScopeId,
        name: &str,
        kind: ScopeKind,
        definition: Option<Definition>,
        defining_symbol: Option<SymbolId>,
    ) -> ScopeId {
        let new_scope_id = self.scopes_by_id.push(Scope {
            name: Name::new(name),
            kind,
            parent: Some(parent_scope_id),
            children: Vec::new(),
            definition,
            defining_symbol,
            symbols_by_name: Map::default(),
        });
        let parent_scope = &mut self.scopes_by_id[parent_scope_id];
        parent_scope.children.push(new_scope_id);
        new_scope_id
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

pub(crate) struct SymbolIterator<'a, I> {
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
pub(crate) struct ScopeIterator<'a, I> {
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

struct SymbolTableBuilder {
    table: SymbolTable,
    scopes: Vec<ScopeId>,
    /// the definition whose target(s) we are currently walking
    current_definition: Option<Definition>,
}

impl SymbolTableBuilder {
    fn add_or_update_symbol(&mut self, identifier: &str, flags: SymbolFlags) -> SymbolId {
        self.table
            .add_or_update_symbol(self.cur_scope(), identifier, flags)
    }

    fn add_or_update_symbol_with_def(
        &mut self,
        identifier: &str,
        definition: Definition,
    ) -> SymbolId {
        let symbol_id = self.add_or_update_symbol(identifier, SymbolFlags::IS_DEFINED);
        self.table
            .defs
            .entry(symbol_id)
            .or_default()
            .push(definition);
        symbol_id
    }

    fn push_scope(
        &mut self,
        name: &str,
        kind: ScopeKind,
        definition: Option<Definition>,
        defining_symbol: Option<SymbolId>,
    ) -> ScopeId {
        let scope_id =
            self.table
                .add_child_scope(self.cur_scope(), name, kind, definition, defining_symbol);
        self.scopes.push(scope_id);
        scope_id
    }

    fn pop_scope(&mut self) -> ScopeId {
        self.scopes
            .pop()
            .expect("Scope stack should never be empty")
    }

    fn cur_scope(&self) -> ScopeId {
        *self
            .scopes
            .last()
            .expect("Scope stack should never be empty")
    }

    fn record_scope_for_node(&mut self, node_key: NodeKey, scope_id: ScopeId) {
        self.table.scopes_by_node.insert(node_key, scope_id);
    }

    fn with_type_params(
        &mut self,
        name: &str,
        params: &Option<Box<ast::TypeParams>>,
        definition: Option<Definition>,
        defining_symbol: Option<SymbolId>,
        nested: impl FnOnce(&mut Self) -> ScopeId,
    ) -> ScopeId {
        if let Some(type_params) = params {
            self.push_scope(name, ScopeKind::Annotation, definition, defining_symbol);
            for type_param in &type_params.type_params {
                let name = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) => name,
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, .. }) => name,
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, .. }) => name,
                };
                self.add_or_update_symbol(name, SymbolFlags::IS_DEFINED);
            }
        }
        let scope_id = nested(self);
        if params.is_some() {
            self.pop_scope();
        }
        scope_id
    }
}

impl PreorderVisitor<'_> for SymbolTableBuilder {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        if let ast::Expr::Name(ast::ExprName { id, ctx, .. }) = expr {
            let flags = match ctx {
                ast::ExprContext::Load => SymbolFlags::IS_USED,
                ast::ExprContext::Store => SymbolFlags::IS_DEFINED,
                ast::ExprContext::Del => SymbolFlags::IS_DEFINED,
                ast::ExprContext::Invalid => SymbolFlags::empty(),
            };
            self.add_or_update_symbol(id, flags);
            if flags.contains(SymbolFlags::IS_DEFINED) {
                if let Some(curdef) = self.current_definition.clone() {
                    self.add_or_update_symbol_with_def(id, curdef);
                }
            }
        }
        ast::visitor::preorder::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        // TODO need to capture more definition statements here
        match stmt {
            ast::Stmt::ClassDef(node) => {
                let node_key = TypedNodeKey::from_node(node);
                let def = Definition::ClassDef(node_key.clone());
                let symbol_id = self.add_or_update_symbol_with_def(&node.name, def.clone());
                let scope_id = self.with_type_params(
                    &node.name,
                    &node.type_params,
                    Some(def.clone()),
                    Some(symbol_id),
                    |builder| {
                        let scope_id = builder.push_scope(
                            &node.name,
                            ScopeKind::Class,
                            Some(def.clone()),
                            Some(symbol_id),
                        );
                        ast::visitor::preorder::walk_stmt(builder, stmt);
                        builder.pop_scope();
                        scope_id
                    },
                );
                self.record_scope_for_node(*node_key.erased(), scope_id);
            }
            ast::Stmt::FunctionDef(node) => {
                let node_key = TypedNodeKey::from_node(node);
                let def = Definition::FunctionDef(node_key.clone());
                let symbol_id = self.add_or_update_symbol_with_def(&node.name, def.clone());
                let scope_id = self.with_type_params(
                    &node.name,
                    &node.type_params,
                    Some(def.clone()),
                    Some(symbol_id),
                    |builder| {
                        let scope_id = builder.push_scope(
                            &node.name,
                            ScopeKind::Function,
                            Some(def.clone()),
                            Some(symbol_id),
                        );
                        ast::visitor::preorder::walk_stmt(builder, stmt);
                        builder.pop_scope();
                        scope_id
                    },
                );
                self.record_scope_for_node(*node_key.erased(), scope_id);
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.split('.').next().unwrap()
                    };

                    let module = ModuleName::new(&alias.name.id);

                    let def = Definition::Import(ImportDefinition {
                        module: module.clone(),
                    });
                    self.add_or_update_symbol_with_def(symbol_name, def);
                    self.table.dependencies.push(Dependency::Module(module));
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                ..
            }) => {
                let module = module.as_ref().map(|m| ModuleName::new(&m.id));

                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.as_str()
                    };
                    let def = Definition::ImportFrom(ImportFromDefinition {
                        module: module.clone(),
                        name: Name::new(&alias.name.id),
                        level: *level,
                    });
                    self.add_or_update_symbol_with_def(symbol_name, def);
                }

                let dependency = if let Some(module) = module {
                    match NonZeroU32::new(*level) {
                        Some(level) => Dependency::Relative {
                            level,
                            module: Some(module),
                        },
                        None => Dependency::Module(module),
                    }
                } else {
                    Dependency::Relative {
                        level: NonZeroU32::new(*level)
                            .expect("Import without a module to have a level > 0"),
                        module,
                    }
                };

                self.table.dependencies.push(dependency);
            }
            ast::Stmt::Assign(node) => {
                debug_assert!(self.current_definition.is_none());
                self.current_definition =
                    Some(Definition::Assignment(TypedNodeKey::from_node(node)));
                ast::visitor::preorder::walk_stmt(self, stmt);
                self.current_definition = None;
            }
            _ => {
                ast::visitor::preorder::walk_stmt(self, stmt);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SymbolTablesStorage(KeyValueCache<FileId, Arc<SymbolTable>>);

impl Deref for SymbolTablesStorage {
    type Target = KeyValueCache<FileId, Arc<SymbolTable>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SymbolTablesStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use textwrap::dedent;

    use crate::parse::Parsed;
    use crate::symbols::ScopeKind;

    use super::{SymbolFlags, SymbolId, SymbolIterator, SymbolTable};

    mod from_ast {
        use super::*;

        fn parse(code: &str) -> Parsed {
            Parsed::from_text(&dedent(code))
        }

        fn names<I>(it: SymbolIterator<I>) -> Vec<&str>
        where
            I: Iterator<Item = SymbolId>,
        {
            let mut symbols: Vec<_> = it.map(|sym| sym.name.as_str()).collect();
            symbols.sort_unstable();
            symbols
        }

        #[test]
        fn empty() {
            let parsed = parse("");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()).len(), 0);
        }

        #[test]
        fn simple() {
            let parsed = parse("x");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["x"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("x").unwrap())
                    .len(),
                0
            );
        }

        #[test]
        fn annotation_only() {
            let parsed = parse("x: int");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["int", "x"]);
            // TODO record definition
        }

        #[test]
        fn import() {
            let parsed = parse("import foo");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("foo").unwrap())
                    .len(),
                1
            );
        }

        #[test]
        fn import_sub() {
            let parsed = parse("import foo.bar");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
        }

        #[test]
        fn import_as() {
            let parsed = parse("import foo.bar as baz");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["baz"]);
        }

        #[test]
        fn import_from() {
            let parsed = parse("from bar import foo");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("foo").unwrap())
                    .len(),
                1
            );
            assert!(
                table.root_symbol_id_by_name("foo").is_some_and(|sid| {
                    let s = sid.symbol(&table);
                    s.is_defined() || !s.is_used()
                }),
                "symbols that are defined get the defined flag"
            );
        }

        #[test]
        fn assign() {
            let parsed = parse("x = foo");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo", "x"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("x").unwrap())
                    .len(),
                1
            );
            assert!(
                table.root_symbol_id_by_name("foo").is_some_and(|sid| {
                    let s = sid.symbol(&table);
                    !s.is_defined() && s.is_used()
                }),
                "a symbol used but not defined in a scope should have only the used flag"
            );
        }

        #[test]
        fn class_scope() {
            let parsed = parse(
                "
                class C:
                    x = 1
                y = 2
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["C", "y"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let c_scope = scopes[0].scope(&table);
            assert_eq!(c_scope.kind(), ScopeKind::Class);
            assert_eq!(c_scope.name(), "C");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("C").unwrap())
                    .len(),
                1
            );
        }

        #[test]
        fn func_scope() {
            let parsed = parse(
                "
                def func():
                    x = 1
                y = 2
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["func", "y"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let func_scope = scopes[0].scope(&table);
            assert_eq!(func_scope.kind(), ScopeKind::Function);
            assert_eq!(func_scope.name(), "func");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("func").unwrap())
                    .len(),
                1
            );
        }

        #[test]
        fn dupes() {
            let parsed = parse(
                "
                def func():
                    x = 1
                def func():
                    y = 2
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["func"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 2);
            let func_scope_1 = scopes[0].scope(&table);
            let func_scope_2 = scopes[1].scope(&table);
            assert_eq!(func_scope_1.kind(), ScopeKind::Function);
            assert_eq!(func_scope_1.name(), "func");
            assert_eq!(func_scope_2.kind(), ScopeKind::Function);
            assert_eq!(func_scope_2.name(), "func");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
            assert_eq!(names(table.symbols_for_scope(scopes[1])), vec!["y"]);
            assert_eq!(
                table
                    .definitions(table.root_symbol_id_by_name("func").unwrap())
                    .len(),
                2
            );
        }

        #[test]
        fn generic_func() {
            let parsed = parse(
                "
                def func[T]():
                    x = 1
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["func"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let ann_scope_id = scopes[0];
            let ann_scope = ann_scope_id.scope(&table);
            assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
            assert_eq!(ann_scope.name(), "func");
            assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
            let scopes = table.child_scope_ids_of(ann_scope_id);
            assert_eq!(scopes.len(), 1);
            let func_scope_id = scopes[0];
            let func_scope = func_scope_id.scope(&table);
            assert_eq!(func_scope.kind(), ScopeKind::Function);
            assert_eq!(func_scope.name(), "func");
            assert_eq!(names(table.symbols_for_scope(func_scope_id)), vec!["x"]);
        }

        #[test]
        fn generic_class() {
            let parsed = parse(
                "
                class C[T]:
                    x = 1
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["C"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let ann_scope_id = scopes[0];
            let ann_scope = ann_scope_id.scope(&table);
            assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
            assert_eq!(ann_scope.name(), "C");
            assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
            assert!(
                table
                    .symbol_by_name(ann_scope_id, "T")
                    .is_some_and(|s| s.is_defined() && !s.is_used()),
                "type parameters are defined by the scope that introduces them"
            );
            let scopes = table.child_scope_ids_of(ann_scope_id);
            assert_eq!(scopes.len(), 1);
            let func_scope_id = scopes[0];
            let func_scope = func_scope_id.scope(&table);
            assert_eq!(func_scope.kind(), ScopeKind::Class);
            assert_eq!(func_scope.name(), "C");
            assert_eq!(names(table.symbols_for_scope(func_scope_id)), vec!["x"]);
        }
    }

    #[test]
    fn insert_same_name_symbol_twice() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let symbol_id_1 = table.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::IS_DEFINED);
        let symbol_id_2 = table.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::IS_USED);
        assert_eq!(symbol_id_1, symbol_id_2);
        assert!(symbol_id_1.symbol(&table).is_used(), "flags must merge");
        assert!(symbol_id_1.symbol(&table).is_defined(), "flags must merge");
    }

    #[test]
    fn insert_different_named_symbols() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let symbol_id_1 = table.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        let symbol_id_2 = table.add_or_update_symbol(root_scope_id, "bar", SymbolFlags::empty());
        assert_ne!(symbol_id_1, symbol_id_2);
    }

    #[test]
    fn add_child_scope_with_symbol() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_top = table.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        let c_scope = table.add_child_scope(root_scope_id, "C", ScopeKind::Class, None, None);
        let foo_symbol_inner = table.add_or_update_symbol(c_scope, "foo", SymbolFlags::empty());
        assert_ne!(foo_symbol_top, foo_symbol_inner);
    }

    #[test]
    fn scope_from_id() {
        let table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let scope = root_scope_id.scope(&table);
        assert_eq!(scope.name.as_str(), "<module>");
        assert_eq!(scope.kind, ScopeKind::Module);
    }

    #[test]
    fn symbol_from_id() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_id = table.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        let symbol = foo_symbol_id.symbol(&table);
        assert_eq!(symbol.name.as_str(), "foo");
    }

    #[test]
    fn bigger_symbol_table() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_id = table.add_or_update_symbol(root_scope_id, "foo", SymbolFlags::empty());
        table.add_or_update_symbol(root_scope_id, "bar", SymbolFlags::empty());
        table.add_or_update_symbol(root_scope_id, "baz", SymbolFlags::empty());
        table.add_or_update_symbol(root_scope_id, "qux", SymbolFlags::empty());

        let foo_symbol_id_2 = table
            .root_symbol_id_by_name("foo")
            .expect("foo symbol to be found");

        assert_eq!(foo_symbol_id_2, foo_symbol_id);
    }
}

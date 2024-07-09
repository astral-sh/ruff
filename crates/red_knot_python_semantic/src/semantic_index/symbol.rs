use std::hash::{Hash, Hasher};
use std::ops::Range;

use bitflags::bitflags;
use hashbrown::hash_map::RawEntryMut;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast};
use rustc_hash::FxHasher;

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::{root_scope, semantic_index, symbol_table, SymbolMap};
use crate::Db;

#[derive(Eq, PartialEq, Debug)]
pub struct Symbol<'db> {
    name: Name,
    flags: SymbolFlags,
    /// The nodes that define this symbol, in source order.
    ///
    /// TODO: Use smallvec here, but it creates the same lifetime issues as in [QualifiedName](https://github.com/astral-sh/ruff/blob/5109b50bb3847738eeb209352cf26bda392adf62/crates/ruff_python_ast/src/name.rs#L562-L569)
    definitions: Vec<Definition<'db>>,
}

impl<'db> Symbol<'db> {
    fn new(name: Name) -> Self {
        Self {
            name,
            flags: SymbolFlags::empty(),
            definitions: Vec::new(),
        }
    }

    fn push_definition(&mut self, definition: Definition<'db>) {
        self.definitions.push(definition);
    }

    fn insert_flags(&mut self, flags: SymbolFlags) {
        self.flags.insert(flags);
    }

    /// The symbol's name.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Is the symbol used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol defined in its containing scope?
    pub fn is_defined(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_DEFINED)
    }

    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub(super) struct SymbolFlags: u8 {
        const IS_USED         = 1 << 0;
        const IS_DEFINED      = 1 << 1;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL   = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL = 1 << 3;
    }
}

/// ID that uniquely identifies a public symbol defined in a module's root scope.
#[salsa::tracked]
pub struct PublicSymbolId<'db> {
    #[id]
    pub(crate) file: File,
    #[id]
    pub(crate) scoped_symbol_id: ScopedSymbolId,
}

/// ID that uniquely identifies a symbol in a file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FileSymbolId {
    scope: FileScopeId,
    scoped_symbol_id: ScopedSymbolId,
}

impl FileSymbolId {
    pub fn scope(self) -> FileScopeId {
        self.scope
    }

    pub(crate) fn scoped_symbol_id(self) -> ScopedSymbolId {
        self.scoped_symbol_id
    }
}

impl From<FileSymbolId> for ScopedSymbolId {
    fn from(val: FileSymbolId) -> Self {
        val.scoped_symbol_id()
    }
}

/// Symbol ID that uniquely identifies a symbol inside a [`Scope`].
#[newtype_index]
pub struct ScopedSymbolId;

impl ScopedSymbolId {
    /// Converts the symbol to a public symbol.
    ///
    /// # Panics
    /// May panic if the symbol does not belong to `file` or is not a symbol of `file`'s root scope.
    pub(crate) fn to_public_symbol(self, db: &dyn Db, file: File) -> PublicSymbolId {
        let symbols = public_symbols_map(db, file);
        symbols.public(self)
    }
}

#[salsa::tracked(return_ref)]
pub(crate) fn public_symbols_map(db: &dyn Db, file: File) -> PublicSymbolsMap<'_> {
    let _span = tracing::trace_span!("public_symbols_map", ?file).entered();

    let module_scope = root_scope(db, file);
    let symbols = symbol_table(db, module_scope);

    let public_symbols: IndexVec<_, _> = symbols
        .symbol_ids()
        .map(|id| PublicSymbolId::new(db, file, id))
        .collect();

    PublicSymbolsMap {
        symbols: public_symbols,
    }
}

/// Maps [`LocalSymbolId`] of a file's root scope to the corresponding [`PublicSymbolId`] (Salsa ingredients).
#[derive(Eq, PartialEq, Debug)]
pub(crate) struct PublicSymbolsMap<'db> {
    symbols: IndexVec<ScopedSymbolId, PublicSymbolId<'db>>,
}

impl<'db> PublicSymbolsMap<'db> {
    /// Resolve the [`PublicSymbolId`] for the module-level `symbol_id`.
    fn public(&self, symbol_id: ScopedSymbolId) -> PublicSymbolId<'db> {
        self.symbols[symbol_id]
    }
}

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked]
pub struct ScopeId<'db> {
    #[id]
    pub file: File,
    #[id]
    pub file_scope_id: FileScopeId,

    /// The node that introduces this scope.
    #[no_eq]
    #[return_ref]
    pub node: NodeWithScopeKind,
}

impl<'db> ScopeId<'db> {
    #[cfg(test)]
    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self.node(db) {
            NodeWithScopeKind::Module => "<module>",
            NodeWithScopeKind::Class(class) | NodeWithScopeKind::ClassTypeParameters(class) => {
                class.name.as_str()
            }
            NodeWithScopeKind::Function(function)
            | NodeWithScopeKind::FunctionTypeParameters(function) => function.name.as_str(),
        }
    }
}

/// ID that uniquely identifies a scope inside of a module.
#[newtype_index]
pub struct FileScopeId;

impl FileScopeId {
    /// Returns the scope id of the Root scope.
    pub fn root() -> Self {
        FileScopeId::from_u32(0)
    }

    pub fn to_scope_id(self, db: &dyn Db, file: File) -> ScopeId<'_> {
        let index = semantic_index(db, file);
        index.scope_ids_by_scope[self]
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Scope {
    pub(super) parent: Option<FileScopeId>,
    pub(super) kind: ScopeKind,
    pub(super) descendents: Range<FileScopeId>,
}

impl Scope {
    pub fn parent(self) -> Option<FileScopeId> {
        self.parent
    }

    pub fn kind(&self) -> ScopeKind {
        self.kind
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

/// Symbol table for a specific [`Scope`].
#[derive(Debug)]
pub struct SymbolTable<'db> {
    /// The symbols in this scope.
    symbols: IndexVec<ScopedSymbolId, Symbol<'db>>,

    /// The symbols indexed by name.
    symbols_by_name: SymbolMap,
}

impl<'db> SymbolTable<'db> {
    fn new() -> Self {
        Self {
            symbols: IndexVec::new(),
            symbols_by_name: SymbolMap::default(),
        }
    }

    fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
    }

    pub(crate) fn symbol(&self, symbol_id: impl Into<ScopedSymbolId>) -> &Symbol<'db> {
        &self.symbols[symbol_id.into()]
    }

    pub(crate) fn symbol_ids(&self) -> impl Iterator<Item = ScopedSymbolId> + 'db {
        self.symbols.indices()
    }

    pub fn symbols(&self) -> impl Iterator<Item = &Symbol<'db>> {
        self.symbols.iter()
    }

    /// Returns the symbol named `name`.
    #[allow(unused)]
    pub(crate) fn symbol_by_name(&self, name: &str) -> Option<&Symbol<'db>> {
        let id = self.symbol_id_by_name(name)?;
        Some(self.symbol(id))
    }

    /// Returns the [`ScopedSymbolId`] of the symbol named `name`.
    pub(crate) fn symbol_id_by_name(&self, name: &str) -> Option<ScopedSymbolId> {
        let (id, ()) = self
            .symbols_by_name
            .raw_entry()
            .from_hash(Self::hash_name(name), |id| {
                self.symbol(*id).name().as_str() == name
            })?;

        Some(*id)
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

impl PartialEq for SymbolTable<'_> {
    fn eq(&self, other: &Self) -> bool {
        // We don't need to compare the symbols_by_name because the name is already captured in `Symbol`.
        self.symbols == other.symbols
    }
}

impl Eq for SymbolTable<'_> {}

#[derive(Debug)]
pub(super) struct SymbolTableBuilder<'db> {
    table: SymbolTable<'db>,
}

impl<'db> SymbolTableBuilder<'db> {
    pub(super) fn new() -> Self {
        Self {
            table: SymbolTable::new(),
        }
    }

    pub(super) fn add_or_update_symbol(
        &mut self,
        name: Name,
        flags: SymbolFlags,
    ) -> ScopedSymbolId {
        let hash = SymbolTable::hash_name(&name);
        let entry = self
            .table
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |id| self.table.symbols[*id].name() == &name);

        match entry {
            RawEntryMut::Occupied(entry) => {
                let symbol = &mut self.table.symbols[*entry.key()];
                symbol.insert_flags(flags);

                *entry.key()
            }
            RawEntryMut::Vacant(entry) => {
                let mut symbol = Symbol::new(name);
                symbol.insert_flags(flags);

                let id = self.table.symbols.push(symbol);
                entry.insert_with_hasher(hash, id, (), |id| {
                    SymbolTable::hash_name(self.table.symbols[*id].name().as_str())
                });
                id
            }
        }
    }

    pub(super) fn add_definition(&mut self, symbol: ScopedSymbolId, definition: Definition<'db>) {
        self.table.symbols[symbol].push_definition(definition);
    }

    pub(super) fn finish(mut self) -> SymbolTable<'db> {
        self.table.shrink_to_fit();
        self.table
    }
}

/// Reference to a node that introduces a new scope.
#[derive(Copy, Clone, Debug)]
pub(crate) enum NodeWithScopeRef<'a> {
    Module,
    Class(&'a ast::StmtClassDef),
    Function(&'a ast::StmtFunctionDef),
    FunctionTypeParameters(&'a ast::StmtFunctionDef),
    ClassTypeParameters(&'a ast::StmtClassDef),
}

impl NodeWithScopeRef<'_> {
    /// Converts the unowned reference to an owned [`NodeWithScopeKind`].
    ///
    /// # Safety
    /// The node wrapped by `self` must be a child of `module`.
    #[allow(unsafe_code)]
    pub(super) unsafe fn to_kind(self, module: ParsedModule) -> NodeWithScopeKind {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKind::Module,
            NodeWithScopeRef::Class(class) => {
                NodeWithScopeKind::Class(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKind::Function(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKind::FunctionTypeParameters(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKind::Class(AstNodeRef::new(module, class))
            }
        }
    }

    pub(super) fn scope_kind(self) -> ScopeKind {
        match self {
            NodeWithScopeRef::Module => ScopeKind::Module,
            NodeWithScopeRef::Class(_) => ScopeKind::Class,
            NodeWithScopeRef::Function(_) => ScopeKind::Function,
            NodeWithScopeRef::FunctionTypeParameters(_)
            | NodeWithScopeRef::ClassTypeParameters(_) => ScopeKind::Annotation,
        }
    }

    pub(crate) fn node_key(self) -> NodeWithScopeKey {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKey::Module,
            NodeWithScopeRef::Class(class) => NodeWithScopeKey::Class(NodeKey::from_node(class)),
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKey::Function(NodeKey::from_node(function))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKey::FunctionTypeParameters(NodeKey::from_node(function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKey::ClassTypeParameters(NodeKey::from_node(class))
            }
        }
    }
}

/// Node that introduces a new scope.
#[derive(Clone, Debug)]
pub enum NodeWithScopeKind {
    Module,
    Class(AstNodeRef<ast::StmtClassDef>),
    ClassTypeParameters(AstNodeRef<ast::StmtClassDef>),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    FunctionTypeParameters(AstNodeRef<ast::StmtFunctionDef>),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum NodeWithScopeKey {
    Module,
    Class(NodeKey),
    ClassTypeParameters(NodeKey),
    Function(NodeKey),
    FunctionTypeParameters(NodeKey),
}

use std::hash::{Hash, Hasher};
use std::ops::Range;

use bitflags::bitflags;
use hashbrown::hash_map::RawEntryMut;
use rustc_hash::FxHasher;
use salsa::DebugWithDb;
use smallvec::SmallVec;

use ruff_db::vfs::VfsFile;
use ruff_index::{newtype_index, IndexVec};

use crate::name::Name;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::{root_scope, semantic_index, symbol_table, SymbolMap};
use crate::Db;

#[derive(Eq, PartialEq, Debug)]
pub struct Symbol {
    name: Name,
    flags: SymbolFlags,
    /// The nodes that define this symbol, in source order.
    definitions: SmallVec<[Definition; 4]>,
}

impl Symbol {
    fn new(name: Name, definition: Option<Definition>) -> Self {
        Self {
            name,
            flags: SymbolFlags::empty(),
            definitions: definition.into_iter().collect(),
        }
    }

    fn push_definition(&mut self, definition: Definition) {
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
    pub(crate) file: VfsFile,
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
    pub(super) fn new(scope: FileScopeId, symbol: ScopedSymbolId) -> Self {
        Self {
            scope,
            scoped_symbol_id: symbol,
        }
    }

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
    pub(crate) fn to_public_symbol(self, db: &dyn Db, file: VfsFile) -> PublicSymbolId {
        let symbols = public_symbols_map(db, file);
        symbols.public(self)
    }
}

/// Returns a mapping from [`FileScopeId`] to globally unique [`ScopeId`].
#[salsa::tracked(return_ref)]
pub(crate) fn scopes_map(db: &dyn Db, file: VfsFile) -> ScopesMap<'_> {
    let _ = tracing::trace_span!("scopes_map", file = ?file.debug(db.upcast())).enter();

    let index = semantic_index(db, file);

    let scopes: IndexVec<_, _> = index
        .scopes
        .indices()
        .map(|id| ScopeId::new(db, file, id))
        .collect();

    ScopesMap { scopes }
}

/// Maps from the file specific [`FileScopeId`] to the global [`ScopeId`] that can be used as a Salsa query parameter.
///
/// The [`SemanticIndex`] uses [`FileScopeId`] on a per-file level to identify scopes
/// because they allow for more efficient storage of associated data
/// (use of an [`IndexVec`] keyed by [`FileScopeId`] over an [`FxHashMap`] keyed by [`ScopeId`]).
#[derive(Eq, PartialEq, Debug)]
pub(crate) struct ScopesMap<'db> {
    scopes: IndexVec<FileScopeId, ScopeId<'db>>,
}

impl<'db> ScopesMap<'db> {
    /// Gets the program-wide unique scope id for the given file specific `scope_id`.
    fn get(&self, scope: FileScopeId) -> ScopeId<'db> {
        self.scopes[scope]
    }
}

#[salsa::tracked(return_ref)]
pub(crate) fn public_symbols_map(db: &dyn Db, file: VfsFile) -> PublicSymbolsMap<'_> {
    let _ = tracing::trace_span!("public_symbols_map", file = ?file.debug(db.upcast())).enter();

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
    pub file: VfsFile,
    #[id]
    pub file_scope_id: FileScopeId,
}

/// ID that uniquely identifies a scope inside of a module.
#[newtype_index]
pub struct FileScopeId;

impl FileScopeId {
    /// Returns the scope id of the Root scope.
    pub fn root() -> Self {
        FileScopeId::from_u32(0)
    }

    pub fn to_scope_id(self, db: &dyn Db, file: VfsFile) -> ScopeId<'_> {
        scopes_map(db, file).get(self)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Scope {
    pub(super) name: Name,
    pub(super) parent: Option<FileScopeId>,
    pub(super) definition: Option<Definition>,
    pub(super) defining_symbol: Option<FileSymbolId>,
    pub(super) kind: ScopeKind,
    pub(super) descendents: Range<FileScopeId>,
}

impl Scope {
    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn definition(&self) -> Option<Definition> {
        self.definition
    }

    pub fn defining_symbol(&self) -> Option<FileSymbolId> {
        self.defining_symbol
    }

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
pub struct SymbolTable {
    /// The symbols in this scope.
    symbols: IndexVec<ScopedSymbolId, Symbol>,

    /// The symbols indexed by name.
    symbols_by_name: SymbolMap,
}

impl SymbolTable {
    fn new() -> Self {
        Self {
            symbols: IndexVec::new(),
            symbols_by_name: SymbolMap::default(),
        }
    }

    fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
    }

    pub(crate) fn symbol(&self, symbol_id: impl Into<ScopedSymbolId>) -> &Symbol {
        &self.symbols[symbol_id.into()]
    }

    pub(crate) fn symbol_ids(&self) -> impl Iterator<Item = ScopedSymbolId> {
        self.symbols.indices()
    }

    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    /// Returns the symbol named `name`.
    #[allow(unused)]
    pub(crate) fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
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

impl PartialEq for SymbolTable {
    fn eq(&self, other: &Self) -> bool {
        // We don't need to compare the symbols_by_name because the name is already captured in `Symbol`.
        self.symbols == other.symbols
    }
}

impl Eq for SymbolTable {}

#[derive(Debug)]
pub(super) struct SymbolTableBuilder {
    table: SymbolTable,
}

impl SymbolTableBuilder {
    pub(super) fn new() -> Self {
        Self {
            table: SymbolTable::new(),
        }
    }

    pub(super) fn add_or_update_symbol(
        &mut self,
        name: Name,
        flags: SymbolFlags,
        definition: Option<Definition>,
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

                if let Some(definition) = definition {
                    symbol.push_definition(definition);
                }

                *entry.key()
            }
            RawEntryMut::Vacant(entry) => {
                let mut symbol = Symbol::new(name, definition);
                symbol.insert_flags(flags);

                let id = self.table.symbols.push(symbol);
                entry.insert_with_hasher(hash, id, (), |id| {
                    SymbolTable::hash_name(self.table.symbols[*id].name().as_str())
                });
                id
            }
        }
    }

    pub(super) fn finish(mut self) -> SymbolTable {
        self.table.shrink_to_fit();
        self.table
    }
}

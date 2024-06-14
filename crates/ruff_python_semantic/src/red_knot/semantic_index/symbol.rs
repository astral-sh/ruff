use std::hash::{Hash, Hasher};
use std::ops::{Index, Range};

use bitflags::bitflags;
use hashbrown::hash_map::RawEntryMut;
use rustc_hash::FxHasher;
use smallvec::{smallvec, SmallVec};

use ruff_db::vfs::VfsFile;
use ruff_index::{newtype_index, IndexVec};

use crate::name::Name;
use crate::red_knot::semantic_index::definition::Definition;
use crate::red_knot::semantic_index::SymbolMap;

#[derive(Eq, PartialEq, Debug)]
pub struct Symbol {
    name: Name,
    flags: SymbolFlags,
    scope: ScopeId,

    /// The nodes that define this symbol, in source order.
    definitions: SmallVec<[Definition; 4]>,
}

impl Symbol {
    pub(super) fn new(name: Name, scope: ScopeId, definition: Option<Definition>) -> Self {
        Self {
            name,
            scope,
            flags: SymbolFlags::empty(),
            definitions: smallvec![definition.unwrap_or(Definition::Unbound)],
        }
    }

    pub(super) fn push_definition(&mut self, definition: Definition) {
        self.definitions.push(definition);
    }

    pub(super) fn insert_flags(&mut self, flags: SymbolFlags) {
        self.flags.insert(flags);
    }
}

impl Symbol {
    /// The symbol's name.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// The scope in which this symbol is defined.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    /// Is the symbol used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol defined in its containing scope?
    pub fn is_defined(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_DEFINED)
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

/// ID that uniquely identifies a symbol, across modules.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct GlobalSymbolId {
    symbol: SymbolId,
    file: VfsFile,
}

impl GlobalSymbolId {
    pub fn new(file: VfsFile, symbol: SymbolId) -> Self {
        Self { symbol, file }
    }

    pub fn file(&self) -> VfsFile {
        self.file
    }

    pub fn symbol(&self) -> SymbolId {
        self.symbol
    }
}

/// ID that uniquely identifies a symbol in a module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId {
    scope: ScopeId,
    symbol: LocalSymbolId,
}

impl SymbolId {
    pub(super) fn new(scope: ScopeId, symbol: LocalSymbolId) -> Self {
        Self { scope, symbol }
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(crate) fn symbol(&self) -> LocalSymbolId {
        self.symbol
    }
}

/// Symbol ID that uniquely identifies a symbol inside a [`Scope`].
#[newtype_index]
pub(crate) struct LocalSymbolId;

/// Maps from the file specific [`ScopeId`] to the global [`GlobalScope`] that can be used as a Salsa query parameter.
///
/// The [`SemanticIndex`] uses [`ScopeId`] on a per-file level to identify scopes
/// because they allow for more efficient storage of associated data
/// (use of an [`IndexVec`] keyed by [`ScopeId`] over an [`FxHashMap`] keyed by [`GlobalScope`]).
#[derive(Eq, PartialEq, Debug)]
pub struct ScopesMap {
    scopes: IndexVec<ScopeId, GlobalScope>,
}

impl ScopesMap {
    pub(super) fn new(scopes: IndexVec<ScopeId, GlobalScope>) -> Self {
        Self { scopes }
    }
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

/// Symbol table for a specific [`Scope`].
#[derive(Debug)]
pub struct SymbolTable {
    /// The symbols in this scope.
    pub(super) symbols: IndexVec<LocalSymbolId, Symbol>,

    /// The symbols indexed by name.
    pub(super) symbols_by_name: SymbolMap,
}

impl SymbolTable {
    /// Returns the scope id of the Root scope.
    pub(crate) const fn root_scope_id() -> ScopeId {
        ScopeId::from_u32(0)
    }

    fn new() -> Self {
        Self {
            symbols: IndexVec::new(),
            symbols_by_name: SymbolMap::default(),
        }
    }

    fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
    }

    pub(crate) fn symbol(&self, symbol_id: LocalSymbolId) -> &Symbol {
        &self.symbols[symbol_id]
    }

    /// Returns the symbol named `name`.
    pub(crate) fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        let id = self.symbol_id_by_name(name)?;
        Some(&self.symbols[id])
    }

    /// Returns the [`LocalSymbolId`] of the symbol named `name`.
    pub(crate) fn symbol_id_by_name(&self, name: &str) -> Option<LocalSymbolId> {
        let (id, _) = self
            .symbols_by_name
            .raw_entry()
            .from_hash(Self::hash_name(name), |id| {
                self.symbols[*id].name().as_str() == name
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

    pub(super) fn add_or_update_symbol_with_flags(
        &mut self,
        name: Name,
        scope: ScopeId,
        flags: SymbolFlags,
        definition: Option<Definition>,
    ) -> LocalSymbolId {
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
                let mut symbol = Symbol::new(name, scope, definition);
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

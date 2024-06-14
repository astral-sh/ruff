use crate::name::Name;
use crate::red_knot::semantic_index::definition::Definition;
use crate::red_knot::semantic_index::ScopeId;
use bitflags::bitflags;
use ruff_db::vfs::VfsFile;
use ruff_index::newtype_index;
use smallvec::{smallvec, SmallVec};

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

    pub(super) fn symbol(&self) -> LocalSymbolId {
        self.symbol
    }
}

/// Symbol ID that uniquely identifies a symbol inside a [`Scope`].
#[newtype_index]
pub(super) struct LocalSymbolId;

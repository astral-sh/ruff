use crate::name::Name;
use crate::red_knot::symbol_table::definition::Definition;
use crate::red_knot::symbol_table::ScopeId;
use bitflags::bitflags;
use ruff_db::vfs::VfsFile;
use ruff_index::newtype_index;

#[derive(Eq, PartialEq, Debug)]
pub struct Symbol {
    pub(super) name: Name,
    pub(super) flags: SymbolFlags,
    pub(super) scope: ScopeId,
    pub(super) definitions: smallvec::SmallVec<[Definition; 1]>,
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
    pub(super) symbol_id: SymbolId,
    pub(super) file: VfsFile,
}

/// ID that uniquely identifies a symbol in a module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId {
    pub(super) scope: ScopeId,
    pub(super) local: LocalSymbolId,
}

/// Symbol ID that uniquely identifies a symbol inside a [`Scope`].
#[newtype_index]
pub(super) struct LocalSymbolId;

use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::name::Name;
use rustc_hash::FxHasher;
use std::hash::{Hash as _, Hasher as _};
use std::ops::{Deref, DerefMut};

/// Uniquely identifies a symbol in a given scope.
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct ScopedSymbolId;

/// A symbol in a given scope.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
pub(crate) struct Symbol {
    name: Name,
    flags: SymbolFlags,
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)
    }
}

bitflags! {
    /// Flags that can be queried to obtain information about a symbol in a given scope.
    ///
    /// See the doc-comment at the top of [`super::use_def`] for explanations of what it
    /// means for a symbol to be *bound* as opposed to *declared*.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct SymbolFlags: u8 {
        const IS_USED               = 1 << 0;
        const IS_BOUND              = 1 << 1;
        const IS_DECLARED           = 1 << 2;
        const MARKED_GLOBAL         = 1 << 3;
        const MARKED_NONLOCAL       = 1 << 4;
        /// true if the symbol is assigned more than once, or if it is assigned even though it is already in use
        const IS_REASSIGNED         = 1 << 5;
        const IS_PARAMETER          = 1 << 6;
    }
}

impl get_size2::GetSize for SymbolFlags {}

impl Symbol {
    pub(crate) const fn new(name: Name) -> Self {
        Self {
            name,
            flags: SymbolFlags::empty(),
        }
    }

    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    /// Is the symbol used in its containing scope?
    pub(crate) fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol given a value in its containing scope?
    pub(crate) const fn is_bound(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_BOUND)
    }

    /// Is the symbol declared in its containing scope?
    pub(crate) fn is_declared(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_DECLARED)
    }

    /// Is the symbol `global` its containing scope?
    pub(crate) fn is_global(&self) -> bool {
        self.flags.contains(SymbolFlags::MARKED_GLOBAL)
    }

    /// Is the symbol `nonlocal` its containing scope?
    pub(crate) fn is_nonlocal(&self) -> bool {
        self.flags.contains(SymbolFlags::MARKED_NONLOCAL)
    }

    /// Is the symbol defined in this scope, vs referring to some enclosing scope?
    ///
    /// There are three common cases where a name refers to an enclosing scope:
    ///
    /// 1. explicit `global` variables
    /// 2. explicit `nonlocal` variables
    /// 3. "free" variables, which are used in a scope where they're neither bound nor declared
    ///
    /// Note that even if `is_local` is false, that doesn't necessarily mean there's an enclosing
    /// scope that resolves the reference. The symbol could be a built-in like `print`, or a name
    /// error at runtime, or a global variable added dynamically with e.g. `globals()`.
    ///
    /// XXX: There's a fourth case that we don't (can't) handle here. A variable that's bound or
    /// declared (anywhere) in a class body, but used before it's bound (at runtime), resolves
    /// (unbelievably) to the global scope. For example:
    /// ```py
    /// x = 42
    /// def f():
    ///     x = 43
    ///     class Foo:
    ///         print(x)  # 42 (never 43)
    ///         if secrets.randbelow(2):
    ///             x = 44
    ///         print(x)  # 42 or 44
    /// ```
    /// In cases like this, the resolution isn't known until runtime, and in fact it varies from
    /// one use to the next. The semantic index alone can't resolve this, and instead it's a
    /// special case in type inference (see `infer_place_load`).
    pub(crate) fn is_local(&self) -> bool {
        !self.is_global() && !self.is_nonlocal() && (self.is_bound() || self.is_declared())
    }

    pub(crate) const fn is_reassigned(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_REASSIGNED)
    }

    pub(crate) fn is_parameter(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_PARAMETER)
    }

    pub(super) fn mark_global(&mut self) {
        self.insert_flags(SymbolFlags::MARKED_GLOBAL);
    }

    pub(super) fn mark_nonlocal(&mut self) {
        self.insert_flags(SymbolFlags::MARKED_NONLOCAL);
    }

    pub(super) fn mark_bound(&mut self) {
        if self.is_bound() || self.is_used() {
            self.insert_flags(SymbolFlags::IS_REASSIGNED);
        }

        self.insert_flags(SymbolFlags::IS_BOUND);
    }

    pub(super) fn mark_used(&mut self) {
        self.insert_flags(SymbolFlags::IS_USED);
    }

    pub(super) fn mark_declared(&mut self) {
        self.insert_flags(SymbolFlags::IS_DECLARED);
    }

    pub(super) fn mark_parameter(&mut self) {
        self.insert_flags(SymbolFlags::IS_PARAMETER);
    }

    fn insert_flags(&mut self, flags: SymbolFlags) {
        self.flags.insert(flags);
    }
}

/// The symbols of a given scope.
///
/// Allows lookup by name and a symbol's ID.
#[derive(Default, get_size2::GetSize)]
pub(super) struct SymbolTable {
    symbols: IndexVec<ScopedSymbolId, Symbol>,

    /// Map from symbol name to its ID.
    ///
    /// Uses a hash table to avoid storing the name twice.
    map: hashbrown::HashTable<ScopedSymbolId>,
}

impl SymbolTable {
    /// Look up a symbol by its ID.
    ///
    /// ## Panics
    /// If the ID is not valid for this symbol table.
    #[track_caller]
    pub(crate) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        &self.symbols[id]
    }

    /// Look up a symbol by its ID, mutably.
    ///
    /// ## Panics
    /// If the ID is not valid for this symbol table.
    #[track_caller]
    pub(crate) fn symbol_mut(&mut self, id: ScopedSymbolId) -> &mut Symbol {
        &mut self.symbols[id]
    }

    /// Look up the ID of a symbol by its name.
    pub(crate) fn symbol_id(&self, name: &str) -> Option<ScopedSymbolId> {
        self.map
            .find(Self::hash_name(name), |id| self.symbols[*id].name == name)
            .copied()
    }

    /// Iterate over the symbols in this symbol table.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Symbol> {
        self.symbols.iter()
    }

    fn hash_name(name: &str) -> u64 {
        let mut h = FxHasher::default();
        name.hash(&mut h);
        h.finish()
    }
}

impl PartialEq for SymbolTable {
    fn eq(&self, other: &Self) -> bool {
        // It's sufficient to compare the symbols as the map is only a reverse lookup.
        self.symbols == other.symbols
    }
}

impl Eq for SymbolTable {}

impl std::fmt::Debug for SymbolTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SymbolTable").field(&self.symbols).finish()
    }
}

#[derive(Debug, Default)]
pub(super) struct SymbolTableBuilder {
    table: SymbolTable,
}

impl SymbolTableBuilder {
    /// Add a new symbol to this scope or update the flags if a symbol with the same name already exists.
    pub(super) fn add(&mut self, mut symbol: Symbol) -> (ScopedSymbolId, bool) {
        let hash = SymbolTable::hash_name(symbol.name());
        let entry = self.table.map.entry(
            hash,
            |id| &self.table.symbols[*id].name == symbol.name(),
            |id| SymbolTable::hash_name(&self.table.symbols[*id].name),
        );

        match entry {
            Entry::Occupied(entry) => {
                let id = *entry.get();

                if !symbol.flags.is_empty() {
                    self.symbols[id].flags.insert(symbol.flags);
                }

                (id, false)
            }
            Entry::Vacant(entry) => {
                symbol.name.shrink_to_fit();
                let id = self.table.symbols.push(symbol);
                entry.insert(id);
                (id, true)
            }
        }
    }

    pub(super) fn build(self) -> SymbolTable {
        let mut table = self.table;
        table.symbols.shrink_to_fit();
        table
            .map
            .shrink_to_fit(|id| SymbolTable::hash_name(&table.symbols[*id].name));
        table
    }
}

impl Deref for SymbolTableBuilder {
    type Target = SymbolTable;

    fn deref(&self) -> &Self::Target {
        &self.table
    }
}

impl DerefMut for SymbolTableBuilder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.table
    }
}

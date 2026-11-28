use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::name::Name;
use rustc_hash::FxHasher;
use std::borrow::Cow;
use std::hash::{Hash as _, Hasher as _};
use std::ops::{Deref, DerefMut};

/// Uniquely identifies a symbol in a given scope.
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedSymbolId;

/// A symbol in a given scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct Symbol {
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
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    struct SymbolFlags: u8 {
        const IS_USED               = 1 << 0;
        const IS_BOUND              = 1 << 1;
        const IS_DECLARED           = 1 << 2;
        const MARKED_GLOBAL         = 1 << 3;
        const MARKED_NONLOCAL       = 1 << 4;
        /// true if the symbol is assigned more than once, or if it is assigned even though it is already in use
        const IS_REASSIGNED         = 1 << 5;
        const IS_PARAMETER          = 1 << 6;
        const HAS_SCOPE_DECLARATION = 1 << 7;
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

    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Is the symbol used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol given a value in its containing scope?
    pub const fn is_bound(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_BOUND)
    }

    /// Is the symbol declared in its containing scope?
    pub fn is_declared(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_DECLARED)
    }

    /// Is the symbol `global` its containing scope?
    pub fn is_global(&self) -> bool {
        self.flags.contains(SymbolFlags::MARKED_GLOBAL)
    }

    /// Is the symbol `nonlocal` its containing scope?
    pub fn is_nonlocal(&self) -> bool {
        self.flags.contains(SymbolFlags::MARKED_NONLOCAL)
    }

    /// Returns whether a `global` or `nonlocal` statement makes this symbol refer to another scope.
    pub fn has_scope_declaration(&self) -> bool {
        self.flags.contains(SymbolFlags::HAS_SCOPE_DECLARATION)
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
    pub fn is_local(&self) -> bool {
        !self.is_global() && !self.is_nonlocal() && (self.is_bound() || self.is_declared())
    }

    pub const fn is_reassigned(&self) -> bool {
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

    pub(super) fn mark_scope_declaration(&mut self) {
        self.insert_flags(SymbolFlags::HAS_SCOPE_DECLARATION);
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

/// Map from symbol name to its ID.
///
/// Uses a hash table to avoid storing the name twice.
#[derive(Debug, Default, get_size2::GetSize)]
struct SymbolReverseTable(hashbrown::HashTable<ScopedSymbolId>);

impl SymbolReverseTable {
    fn symbol_id(
        &self,
        symbols: &IndexVec<ScopedSymbolId, Symbol>,
        name: &str,
    ) -> Option<ScopedSymbolId> {
        self.0
            .find(Self::hash_name(name), |id| symbols[*id].name == name)
            .copied()
    }

    fn entry<'a>(
        &'a mut self,
        symbols: &IndexVec<ScopedSymbolId, Symbol>,
        name: &Name,
    ) -> Entry<'a, ScopedSymbolId> {
        self.0.entry(
            Self::hash_name(name),
            |id| &symbols[*id].name == name,
            |id| Self::hash_name(&symbols[*id].name),
        )
    }

    fn shrink_to_fit(&mut self, symbols: &IndexVec<ScopedSymbolId, Symbol>) {
        self.0
            .shrink_to_fit(|id| Self::hash_name(&symbols[*id].name));
    }

    fn hash_name(name: &str) -> u64 {
        let mut h = FxHasher::default();
        name.hash(&mut h);
        h.finish()
    }
}

/// Small scopes need no hash table. Each bucket holds a bitmap of the IDs
/// that might match, so a collision only costs an extra name comparison.
#[derive(Debug, Default, get_size2::GetSize)]
struct SmallSymbolIndex([u16; 8]);

impl SmallSymbolIndex {
    fn bucket(name: &str) -> usize {
        (SymbolReverseTable::hash_name(name) % 8) as usize
    }

    fn symbol_id(
        &self,
        symbols: &IndexVec<ScopedSymbolId, Symbol>,
        name: &str,
        bucket: usize,
    ) -> Option<ScopedSymbolId> {
        let mut candidates = self.0[bucket];
        while candidates != 0 {
            let id = ScopedSymbolId::from_u32(candidates.trailing_zeros());
            if symbols[id].name == name {
                return Some(id);
            }
            candidates &= candidates - 1;
        }
        None
    }
}

#[derive(Debug, get_size2::GetSize)]
enum SymbolIndex {
    Small(SmallSymbolIndex),
    Large(Box<SymbolReverseTable>),
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::Small(SmallSymbolIndex::default())
    }
}

/// The symbols of a given scope.
///
/// Allows lookup by name and a symbol's ID.
#[derive(Default, get_size2::GetSize)]
pub(super) struct SymbolTable {
    symbols: IndexVec<ScopedSymbolId, Symbol>,
    reverse: SymbolIndex,
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
        match &self.reverse {
            SymbolIndex::Small(index) => {
                index.symbol_id(&self.symbols, name, SmallSymbolIndex::bucket(name))
            }
            SymbolIndex::Large(index) => index.symbol_id(&self.symbols, name),
        }
    }

    /// Iterate over the symbols in this symbol table.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Symbol> {
        self.symbols.iter()
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
    /// Reuse an existing symbol without allocating or copying its name.
    pub(super) fn add_name(&mut self, name: &Name) -> (ScopedSymbolId, bool) {
        self.insert(Cow::Borrowed(name), SymbolFlags::empty())
    }

    /// Add a new symbol to this scope or update the flags of an existing symbol.
    pub(super) fn add(&mut self, symbol: Symbol) -> (ScopedSymbolId, bool) {
        self.insert(Cow::Owned(symbol.name), symbol.flags)
    }

    fn insert(&mut self, name: Cow<'_, Name>, flags: SymbolFlags) -> (ScopedSymbolId, bool) {
        let SymbolTable { symbols, reverse } = &mut self.table;
        let table = match reverse {
            SymbolIndex::Small(index) => {
                let bucket = SmallSymbolIndex::bucket(name.as_str());
                if let Some(id) = index.symbol_id(symbols, name.as_str(), bucket) {
                    symbols[id].flags.insert(flags);
                    return (id, false);
                }

                if symbols.len() < u16::BITS as usize {
                    let id = symbols.push(Symbol {
                        name: name.into_owned(),
                        flags,
                    });
                    index.0[bucket] |= 1 << id.as_u32();
                    return (id, true);
                }

                let mut table = hashbrown::HashTable::with_capacity(symbols.len() + 1);
                let hash = |id: &ScopedSymbolId| SymbolReverseTable::hash_name(&symbols[*id].name);
                for (id, _) in symbols.iter_enumerated() {
                    table.insert_unique(hash(&id), id, hash);
                }
                *reverse = SymbolIndex::Large(Box::new(SymbolReverseTable(table)));
                return self.insert(name, flags);
            }
            SymbolIndex::Large(table) => table,
        };

        match table.entry(symbols, &name) {
            Entry::Occupied(entry) => {
                let id = *entry.get();
                symbols[id].flags.insert(flags);
                (id, false)
            }
            Entry::Vacant(entry) => {
                let id = symbols.push(Symbol {
                    name: name.into_owned(),
                    flags,
                });
                entry.insert(id);
                (id, true)
            }
        }
    }

    pub(super) fn build(self) -> SymbolTable {
        let mut table = self.table;
        table.symbols.shrink_to_fit();
        if let SymbolIndex::Large(reverse) = &mut table.reverse {
            reverse.shrink_to_fit(&table.symbols);
        }
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

#[cfg(test)]
mod tests {
    use super::{Name, Symbol, SymbolFlags, SymbolTableBuilder};

    #[test]
    fn symbol_ids_survive_collisions_and_growth() {
        for count in [16, 17] {
            let mut table = SymbolTableBuilder::default();
            // Both sets exceed eight buckets, so at least two names collide.
            let names: Vec<_> = (0..count)
                .map(|n| Name::from(format!("α{n:02}").as_str()))
                .collect();
            let ids: Vec<_> = names
                .iter()
                .map(|name| {
                    let (id, added) = table.add_name(name);
                    assert!(added);
                    id
                })
                .collect();

            for (name, &id) in names.iter().zip(&ids) {
                let mut symbol = Symbol::new(name.clone());
                symbol.mark_parameter();
                assert_eq!(table.add(symbol), (id, false));
            }

            let table = table.build();
            assert_eq!(table.symbol_id("α99"), None);
            for (name, id) in names.iter().zip(ids) {
                assert_eq!(table.symbol_id(name), Some(id));
                assert!(table.symbol(id).flags.contains(SymbolFlags::IS_PARAMETER));
            }
        }
    }
}

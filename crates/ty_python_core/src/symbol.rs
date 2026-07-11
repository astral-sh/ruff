use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{FrozenIndexVec, IndexSlice, IndexVec, newtype_index};
use ruff_python_ast::name::Name;
use rustc_hash::FxHasher;
use std::hash::{Hash as _, Hasher as _};

// Selected using performance and memory profiling across the 162-project ecosystem corpus.
// Symbol-name equality is cheap enough that raising the cutoff from 8 to 16 reduced retained
// memory without a measurable performance regression.
const LINEAR_SEARCH_THRESHOLD: usize = 16;

/// Uniquely identifies a symbol in a given scope.
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedSymbolId;

/// A symbol in a given scope.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
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
    pub const fn new(name: Name) -> Self {
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

    pub fn is_parameter(&self) -> bool {
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

/// A borrowed symbol from a retained symbol table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SymbolRef<'a> {
    name: &'a Name,
    flags: SymbolFlags,
}

impl<'a> SymbolRef<'a> {
    pub fn name(self) -> &'a Name {
        self.name
    }

    pub fn is_used(self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    pub const fn is_bound(self) -> bool {
        self.flags.contains(SymbolFlags::IS_BOUND)
    }

    pub fn is_declared(self) -> bool {
        self.flags.contains(SymbolFlags::IS_DECLARED)
    }

    pub fn is_global(self) -> bool {
        self.flags.contains(SymbolFlags::MARKED_GLOBAL)
    }

    pub fn is_nonlocal(self) -> bool {
        self.flags.contains(SymbolFlags::MARKED_NONLOCAL)
    }

    pub fn is_local(self) -> bool {
        !self.is_global() && !self.is_nonlocal() && (self.is_bound() || self.is_declared())
    }

    pub const fn is_reassigned(self) -> bool {
        self.flags.contains(SymbolFlags::IS_REASSIGNED)
    }

    pub fn is_parameter(self) -> bool {
        self.flags.contains(SymbolFlags::IS_PARAMETER)
    }
}

impl std::fmt::Display for SymbolRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)
    }
}

impl std::fmt::Debug for SymbolRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Symbol")
            .field("name", self.name)
            .field("flags", &self.flags)
            .finish()
    }
}

impl<'a> From<&'a Symbol> for SymbolRef<'a> {
    fn from(symbol: &'a Symbol) -> Self {
        Self {
            name: &symbol.name,
            flags: symbol.flags,
        }
    }
}

trait SymbolName {
    fn symbol_name(&self) -> &str;
}

impl SymbolName for Name {
    fn symbol_name(&self) -> &str {
        self.as_str()
    }
}

impl SymbolName for Symbol {
    fn symbol_name(&self) -> &str {
        self.name.as_str()
    }
}

/// Map from symbol name to its ID.
///
/// Uses a hash table to avoid storing the name twice.
#[derive(Debug, Default, get_size2::GetSize)]
struct SymbolReverseTable(hashbrown::HashTable<ScopedSymbolId>);

impl SymbolReverseTable {
    fn symbol_id<T: SymbolName>(
        &self,
        symbols: &IndexSlice<ScopedSymbolId, T>,
        name: &str,
    ) -> Option<ScopedSymbolId> {
        self.0
            .find(Self::hash_name(name), |id| {
                symbols[*id].symbol_name() == name
            })
            .copied()
    }

    fn entry<'a>(
        &'a mut self,
        symbols: &IndexVec<ScopedSymbolId, Symbol>,
        symbol: &Symbol,
    ) -> Entry<'a, ScopedSymbolId> {
        self.0.entry(
            Self::hash_name(symbol.name()),
            |id| symbols[*id].name == symbol.name,
            |id| Self::hash_name(symbols[*id].name.as_str()),
        )
    }

    fn shrink_to_fit<T: SymbolName>(&mut self, symbols: &IndexSlice<ScopedSymbolId, T>) {
        self.0
            .shrink_to_fit(|id| Self::hash_name(symbols[*id].symbol_name()));
    }

    fn hash_name(name: &str) -> u64 {
        let mut h = FxHasher::default();
        name.hash(&mut h);
        h.finish()
    }
}

/// The symbols of a given scope.
///
/// Allows lookup by name and a symbol's ID.
#[derive(get_size2::GetSize)]
pub(super) struct SymbolTable {
    names: FrozenIndexVec<ScopedSymbolId, Name>,
    flags: FrozenIndexVec<ScopedSymbolId, SymbolFlags>,
    /// Reverse lookup retained only when linear search would be expensive.
    reverse: Option<Box<SymbolReverseTable>>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self {
            names: IndexVec::new().into(),
            flags: IndexVec::new().into(),
            reverse: None,
        }
    }
}

impl SymbolTable {
    /// Look up a symbol by its ID.
    ///
    /// ## Panics
    /// If the ID is not valid for this symbol table.
    #[track_caller]
    pub(crate) fn symbol(&self, id: ScopedSymbolId) -> SymbolRef<'_> {
        SymbolRef {
            name: &self.names[id],
            flags: self.flags[id],
        }
    }

    /// Look up the ID of a symbol by its name.
    pub(crate) fn symbol_id(&self, name: &str) -> Option<ScopedSymbolId> {
        if let Some(reverse) = self.reverse.as_deref() {
            return reverse.symbol_id(&self.names, name);
        }

        self.names
            .iter_enumerated()
            .find_map(|(id, symbol_name)| (symbol_name == name).then_some(id))
    }

    /// Iterate over the symbols in this symbol table.
    pub(crate) fn iter(&self) -> impl Iterator<Item = SymbolRef<'_>> {
        self.names
            .iter()
            .zip(self.flags.iter().copied())
            .map(|(name, flags)| SymbolRef { name, flags })
    }
}

impl PartialEq for SymbolTable {
    fn eq(&self, other: &Self) -> bool {
        // It's sufficient to compare the symbols as the map is only a reverse lookup.
        self.names == other.names && self.flags == other.flags
    }
}

impl Eq for SymbolTable {}

impl std::fmt::Debug for SymbolTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        struct SymbolsDebug<'a>(&'a SymbolTable);

        impl std::fmt::Debug for SymbolsDebug<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_list().entries(self.0.iter()).finish()
            }
        }

        f.debug_tuple("SymbolTable")
            .field(&SymbolsDebug(self))
            .finish()
    }
}

#[derive(Debug, Default)]
pub(super) struct SymbolTableBuilder {
    symbols: IndexVec<ScopedSymbolId, Symbol>,
    reverse: SymbolReverseTable,
}

impl SymbolTableBuilder {
    pub(super) fn symbol_id(&self, name: &str) -> Option<ScopedSymbolId> {
        self.reverse.symbol_id(&self.symbols, name)
    }

    #[track_caller]
    pub(super) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        &self.symbols[id]
    }

    #[track_caller]
    pub(super) fn symbol_mut(&mut self, id: ScopedSymbolId) -> &mut Symbol {
        &mut self.symbols[id]
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, Symbol> {
        self.symbols.iter()
    }

    /// Add a new symbol to this scope or update the flags if a symbol with the same name already exists.
    pub(super) fn add(&mut self, mut symbol: Symbol) -> (ScopedSymbolId, bool) {
        let entry = self.reverse.entry(&self.symbols, &symbol);

        match entry {
            Entry::Occupied(entry) => {
                let id = *entry.get();

                if !symbol.flags.is_empty() {
                    self.symbols[id].insert_flags(symbol.flags);
                }

                (id, false)
            }
            Entry::Vacant(entry) => {
                symbol.name.shrink_to_fit();
                let id = self.symbols.push(symbol);
                entry.insert(id);
                (id, true)
            }
        }
    }

    pub(super) fn build(self) -> SymbolTable {
        let Self {
            symbols,
            mut reverse,
        } = self;
        let symbol_count = symbols.len();
        let mut names = IndexVec::with_capacity(symbol_count);
        let mut flags = IndexVec::with_capacity(symbol_count);

        for symbol in symbols {
            names.push(symbol.name);
            flags.push(symbol.flags);
        }

        let reverse = if names.len() > LINEAR_SEARCH_THRESHOLD {
            reverse.shrink_to_fit(&names);
            Some(Box::new(reverse))
        } else {
            None
        };

        SymbolTable {
            names: names.into(),
            flags: flags.into(),
            reverse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_flags_round_trip() {
        for name in [
            "",
            "short",
            "éclair",
            "abcdefghijklmnopqrstuvwx",
            "abcdefghijklmnopqrstuvwxy",
        ] {
            let mut symbol = Symbol::new(Name::new(name));

            assert_eq!(symbol.name(), name);
            assert!(symbol.flags.is_empty());

            symbol.mark_used();
            symbol.mark_bound();
            symbol.mark_declared();
            symbol.mark_global();
            symbol.mark_nonlocal();
            symbol.mark_parameter();

            assert_eq!(symbol.flags, SymbolFlags::all());
            assert!(symbol.is_used());
            assert!(symbol.is_bound());
            assert!(symbol.is_declared());
            assert!(symbol.is_global());
            assert!(symbol.is_nonlocal());
            assert!(symbol.is_reassigned());
            assert!(symbol.is_parameter());
            assert_eq!(symbol.name(), name);
            assert_eq!(symbol.to_string(), name);
            assert_eq!(
                format!("{symbol:?}"),
                format!(
                    "Symbol {{ name: {:?}, flags: {:?} }}",
                    Name::new(name),
                    SymbolFlags::all()
                )
            );

            let mut builder = SymbolTableBuilder::default();
            let (id, _) = builder.add(symbol);
            let table = builder.build();
            let symbol = table.symbol(id);

            assert_eq!(symbol.name(), name);
            assert!(symbol.is_used());
            assert!(symbol.is_bound());
            assert!(symbol.is_declared());
            assert!(symbol.is_global());
            assert!(symbol.is_nonlocal());
            assert!(symbol.is_reassigned());
            assert!(symbol.is_parameter());
            assert_eq!(symbol.to_string(), name);
        }
    }

    #[test]
    fn symbol_lookup_ignores_flags() {
        let mut builder = SymbolTableBuilder::default();

        for index in 0..=LINEAR_SEARCH_THRESHOLD {
            let name = format!("symbol_{index}");
            let mut symbol = Symbol::new(Name::new(&name));
            symbol.mark_bound();
            builder.add(symbol);

            assert!(builder.symbol_id(&name).is_some());
        }

        let table = builder.build();
        for index in 0..=LINEAR_SEARCH_THRESHOLD {
            let name = format!("symbol_{index}");
            let id = table.symbol_id(&name).unwrap();
            assert!(table.symbol(id).is_bound());
        }
    }
}

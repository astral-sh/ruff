//! Implements logic used by the document symbol provider, workspace symbol
//! provider, and auto-import feature of the completion provider.

use std::borrow::Cow;
use std::ops::Range;

use regex::Regex;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::name::{Name, UnqualifiedName};
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{ModuleName, resolve_module};
use ty_project::Db;

use crate::completion::CompletionKind;

/// A compiled query pattern used for searching symbols.
///
/// This can be used with the `FlatSymbols::search` API.
#[derive(Clone, Debug)]
pub struct QueryPattern {
    re: Option<Regex>,
    original: String,
    original_is_exact: bool,
}

impl QueryPattern {
    /// Create a new query pattern from a literal search string given.
    pub fn fuzzy(literal_query_string: &str) -> QueryPattern {
        let mut pattern = "(?i)".to_string();
        for ch in literal_query_string.chars() {
            pattern.push_str(&regex::escape(ch.encode_utf8(&mut [0; 4])));
            pattern.push_str(".*");
        }
        // In theory regex compilation could fail if the pattern string
        // was long enough to exceed the default regex compilation size
        // limit. But this length would be approaching ~10MB or so. If
        // is does somehow fail, we'll just fall back to simple substring
        // search using `original`.
        QueryPattern {
            re: Regex::new(&pattern).ok(),
            original: literal_query_string.to_string(),
            original_is_exact: false,
        }
    }

    /// Create a new query
    pub fn exactly(symbol: &str) -> QueryPattern {
        QueryPattern {
            re: None,
            original: symbol.to_string(),
            original_is_exact: true,
        }
    }

    /// Create a new query pattern that matches all symbols.
    pub fn matches_all_symbols() -> QueryPattern {
        QueryPattern {
            re: None,
            original: String::new(),
            original_is_exact: false,
        }
    }

    fn is_match_symbol(&self, symbol: &SymbolInfo<'_>) -> bool {
        self.is_match_symbol_name(&symbol.name)
    }

    pub fn is_match_symbol_name(&self, symbol_name: &str) -> bool {
        if let Some(ref re) = self.re {
            re.is_match(symbol_name)
        } else if self.original_is_exact {
            symbol_name == self.original
        } else {
            // This is a degenerate case. The only way
            // we should get here is if the query string
            // was thousands (or more) characters long.
            // ... or, if "typed" text could not be found.
            symbol_name.contains(&self.original)
        }
    }

    /// Returns true when it is known that this pattern will return `true` for
    /// all inputs given to `QueryPattern::is_match_symbol_name`.
    ///
    /// This will never return `true` incorrectly, but it may return `false`
    /// incorrectly. That is, it's possible that this query will match all
    /// inputs but this still returns `false`.
    pub fn will_match_everything(&self) -> bool {
        self.re.is_none() && self.original.is_empty()
    }
}

impl From<&str> for QueryPattern {
    fn from(literal_query_string: &str) -> QueryPattern {
        QueryPattern::fuzzy(literal_query_string)
    }
}

impl Eq for QueryPattern {}

impl PartialEq for QueryPattern {
    fn eq(&self, rhs: &QueryPattern) -> bool {
        self.original == rhs.original
    }
}

/// A flat list of indexed symbols for a single file.
#[derive(Clone, Debug, Default, PartialEq, Eq, get_size2::GetSize)]
pub struct FlatSymbols {
    /// The symbols exported by a module.
    symbols: IndexVec<SymbolId, SymbolTree>,
    /// The names found in an `__all__` for a module.
    ///
    /// This is `None` if the module has no `__all__` at module
    /// scope.
    all_names: Option<FxHashSet<Name>>,
}

impl FlatSymbols {
    /// Get the symbol info for the symbol identified by the given ID.
    ///
    /// Returns `None` when the given ID does not reference a symbol in this
    /// collection.
    pub fn get(&self, id: SymbolId) -> Option<SymbolInfo<'_>> {
        self.symbols.get(id).map(Into::into)
    }

    /// Returns true if and only if this collection is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns the total number of symbols in this collection.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns an iterator over every symbol along with its ID.
    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.symbols
            .iter_enumerated()
            .map(|(id, symbol)| (id, symbol.into()))
    }

    /// Returns a sequence of symbols that matches the given query.
    pub fn search(&self, query: &QueryPattern) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.iter()
            .filter(|(_, symbol)| query.is_match_symbol(symbol))
    }

    /// Turns this flat sequence of symbols into a hierarchy of symbols.
    pub fn to_hierarchical(&self) -> HierarchicalSymbols {
        let mut children_ids: IndexVec<SymbolId, Vec<SymbolId>> = IndexVec::new();
        for (id, symbol) in self.symbols.iter_enumerated() {
            children_ids.push(vec![]);
            let Some(parent_id) = symbol.parent else {
                continue;
            };
            // OK because the symbol visitor guarantees that
            // all parents are ordered before their children.
            assert!(parent_id.index() < id.index());
            children_ids[parent_id].push(id);
        }

        // Now flatten our map of symbol ID to its children
        // IDs into a single vec that doesn't nest allocations.
        let mut symbols = IndexVec::new();
        let mut children: Vec<SymbolId> = vec![];
        let mut last_end: usize = 0;
        for (tree, child_symbol_ids) in self.symbols.iter().zip(children_ids) {
            let start = last_end;
            let end = start + child_symbol_ids.len();
            symbols.push(SymbolTreeWithChildren {
                tree: tree.clone(),
                children: start..end,
            });
            children.extend_from_slice(&child_symbol_ids);
            last_end = end;
        }

        HierarchicalSymbols { symbols, children }
    }
}

/// A collection of hierarchical indexed symbols for a single file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HierarchicalSymbols {
    symbols: IndexVec<SymbolId, SymbolTreeWithChildren>,
    children: Vec<SymbolId>,
}

impl HierarchicalSymbols {
    /// Get the symbol info for the symbol identified by the given ID.
    ///
    /// Returns `None` when the given ID does not reference a symbol in this
    /// collection.
    pub fn get(&self, id: SymbolId) -> Option<SymbolInfo<'_>> {
        self.symbols.get(id).map(Into::into)
    }

    /// Returns true if and only if this collection is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns the total number of symbols in this collection.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns an iterator over every top-level symbol along with its ID.
    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.symbols
            .iter_enumerated()
            .filter(|(_, symbol)| symbol.tree.parent.is_none())
            .map(|(id, symbol)| (id, symbol.into()))
    }

    /// Returns an iterator over the child symbols for the symbol
    /// identified by the given ID.
    ///
    /// Returns `None` when there aren't any children or when the given
    /// ID does not reference a symbol in this collection.
    pub fn children(&self, id: SymbolId) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.symbols
            .get(id)
            .into_iter()
            .flat_map(|symbol| self.children[symbol.children.clone()].iter())
            .copied()
            .map(|id| (id, SymbolInfo::from(&self.symbols[id])))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SymbolTreeWithChildren {
    tree: SymbolTree,
    /// The index range into `HierarchicalSymbols::children`
    /// corresponding to the children symbol IDs for this
    /// symbol.
    children: Range<usize>,
}

/// Uniquely identifies a symbol.
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct SymbolId;

/// Symbol information for IDE features like document outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolInfo<'a> {
    /// The name of the symbol
    pub name: Cow<'a, str>,
    /// The kind of symbol (function, class, variable, etc.)
    pub kind: SymbolKind,
    /// The range of the symbol name
    pub name_range: TextRange,
    /// The full range of the symbol (including body)
    pub full_range: TextRange,
}

impl SymbolInfo<'_> {
    pub fn to_owned(&self) -> SymbolInfo<'static> {
        SymbolInfo {
            name: Cow::Owned(self.name.to_string()),
            kind: self.kind,
            name_range: self.name_range,
            full_range: self.full_range,
        }
    }
}

impl<'a> From<&'a SymbolTree> for SymbolInfo<'a> {
    fn from(symbol: &'a SymbolTree) -> SymbolInfo<'a> {
        SymbolInfo {
            name: Cow::Borrowed(&symbol.name),
            kind: symbol.kind,
            name_range: symbol.name_range,
            full_range: symbol.full_range,
        }
    }
}

impl<'a> From<&'a SymbolTreeWithChildren> for SymbolInfo<'a> {
    fn from(symbol: &'a SymbolTreeWithChildren) -> SymbolInfo<'a> {
        SymbolInfo::from(&symbol.tree)
    }
}

/// The kind of symbol.
///
/// Note that this is computed on a best effort basis. The nature of
/// auto-import is that it tries to do a very low effort scan of a lot of code
/// very quickly. This means that it doesn't use things like type information
/// or completely resolve the definition of every symbol. So for example, we
/// might label a module as a variable, depending on how it was introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub enum SymbolKind {
    Module,
    Class,
    Method,
    Function,
    Variable,
    Constant,
    Property,
    Field,
    Constructor,
    Parameter,
    TypeParameter,
    Import,
}

impl SymbolKind {
    /// Returns the string representation of the symbol kind.
    pub fn to_string(self) -> &'static str {
        match self {
            SymbolKind::Module => "Module",
            SymbolKind::Class => "Class",
            SymbolKind::Method => "Method",
            SymbolKind::Function => "Function",
            SymbolKind::Variable => "Variable",
            SymbolKind::Constant => "Constant",
            SymbolKind::Property => "Property",
            SymbolKind::Field => "Field",
            SymbolKind::Constructor => "Constructor",
            SymbolKind::Parameter => "Parameter",
            SymbolKind::TypeParameter => "TypeParameter",
            SymbolKind::Import => "Import",
        }
    }

    /// Maps this to a "completion" kind if a sensible mapping exists.
    pub fn to_completion_kind(self) -> Option<CompletionKind> {
        Some(match self {
            SymbolKind::Module => CompletionKind::Module,
            SymbolKind::Class => CompletionKind::Class,
            SymbolKind::Method => CompletionKind::Method,
            SymbolKind::Function => CompletionKind::Function,
            SymbolKind::Variable => CompletionKind::Variable,
            SymbolKind::Constant => CompletionKind::Constant,
            SymbolKind::Property => CompletionKind::Property,
            SymbolKind::Field => CompletionKind::Field,
            SymbolKind::Constructor => CompletionKind::Constructor,
            SymbolKind::Parameter => CompletionKind::Variable,
            SymbolKind::TypeParameter => CompletionKind::TypeParameter,
            // Not quite sure what to do with this one. I guess
            // in theory the import should be "resolved" to its
            // underlying kind, but that seems expensive.
            SymbolKind::Import => return None,
        })
    }
}

/// Returns a flat list of symbols in the file given.
///
/// The flattened list includes parent/child information and can be
/// converted into a hierarchical collection of symbols.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn symbols_for_file(db: &dyn Db, file: File) -> FlatSymbols {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = SymbolVisitor::tree(db, file);
    visitor.visit_body(&module.syntax().body);
    visitor.into_flat_symbols()
}

/// Returns a flat list of *only global* symbols in the file given.
///
/// While callers can convert this into a hierarchical collection of
/// symbols, it won't result in anything meaningful since the flat list
/// returned doesn't include children.
#[salsa::tracked(
    returns(ref),
    cycle_initial=symbols_for_file_global_only_cycle_initial,
    heap_size=ruff_memory_usage::heap_size,
)]
pub(crate) fn symbols_for_file_global_only(db: &dyn Db, file: File) -> FlatSymbols {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = SymbolVisitor::globals(db, file);
    visitor.visit_body(&module.syntax().body);

    if file
        .path(db)
        .as_system_path()
        .is_none_or(|path| !db.project().is_file_included(db, path))
    {
        // Eagerly clear ASTs of third party files.
        parsed.clear();
    }
    visitor.into_flat_symbols()
}

fn symbols_for_file_global_only_cycle_initial(
    _db: &dyn Db,
    _id: salsa::Id,
    _file: File,
) -> FlatSymbols {
    FlatSymbols::default()
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
struct SymbolTree {
    parent: Option<SymbolId>,
    name: String,
    kind: SymbolKind,
    name_range: TextRange,
    full_range: TextRange,
    import_kind: Option<ImportKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum ImportKind {
    Normal,
    RedundantAlias,
    Wildcard,
}

/// An abstraction for managing module scope imports.
///
/// This is meant to recognize the following idioms for updating
/// `__all__` in module scope:
///
/// ```ignore
/// __all__ += submodule.__all__
/// __all__.extend(submodule.__all__)
/// ```
///
/// # Correctness
///
/// The approach used here is not correct 100% of the time.
/// For example, it is somewhat easy to defeat it:
///
/// ```ignore
/// from numpy import *
/// from importlib import resources
/// import numpy as np
/// np = resources
/// __all__ = []
/// __all__ += np.__all__
/// ```
///
/// In this example, `np` will still be resolved to the `numpy`
/// module instead of the `importlib.resources` module. Namely, this
/// abstraction doesn't track all definitions. This would result in a
/// silently incorrect `__all__`.
///
/// This abstraction does handle the case when submodules are imported.
/// Namely, we do get this case correct:
///
/// ```ignore
/// from importlib.resources import *
/// from importlib import resources
/// __all__ = []
/// __all__ += resources.__all__
/// ```
///
/// We do this by treating all imports in a `from ... import ...`
/// statement as *possible* modules. Then when we lookup `resources`,
/// we attempt to resolve it to an actual module. If that fails, then
/// we consider `__all__` invalid.
///
/// There are likely many many other cases that we don't handle as
/// well, which ty does (it has its own `__all__` parsing using types
/// to deal with this case). We can add handling for those as they
/// come up in real world examples.
///
/// # Performance
///
/// This abstraction recognizes that, compared to all possible imports,
/// it is very rare to use one of them to update `__all__`. Therefore,
/// we are careful not to do too much work up-front (like eagerly
/// manifesting `ModuleName` values).
#[derive(Clone, Debug, Default, get_size2::GetSize)]
struct Imports<'db> {
    /// A map from the name that a module is available
    /// under to its actual module name (and our level
    /// of certainty that it ought to be treated as a module).
    module_names: FxHashMap<&'db str, ImportModuleKind<'db>>,
}

impl<'db> Imports<'db> {
    /// Track the imports from the given `import ...` statement.
    fn add_import(&mut self, import: &'db ast::StmtImport) {
        for alias in &import.names {
            let asname = alias
                .asname
                .as_ref()
                .map(|ident| &ident.id)
                .unwrap_or(&alias.name.id);
            let module_name = ImportModuleName::Import(&alias.name.id);
            self.module_names
                .insert(asname, ImportModuleKind::Definitive(module_name));
        }
    }

    /// Track the imports from the given `from ... import ...` statement.
    fn add_import_from(&mut self, import_from: &'db ast::StmtImportFrom) {
        for alias in &import_from.names {
            if &alias.name == "*" {
                // FIXME: We'd ideally include the names
                // imported from the module, but we don't
                // want to do this eagerly. So supporting
                // this requires more infrastructure in
                // `Imports`.
                continue;
            }

            let asname = alias
                .asname
                .as_ref()
                .map(|ident| &ident.id)
                .unwrap_or(&alias.name.id);
            let module_name = ImportModuleName::ImportFrom {
                parent: import_from,
                child: &alias.name.id,
            };
            self.module_names
                .insert(asname, ImportModuleKind::Possible(module_name));
        }
    }

    /// Return the symbols exported by the module referred to by `name`.
    ///
    /// e.g., This can be used to resolve `__all__ += submodule.__all__`,
    /// where `name` is `submodule`.
    fn get_module_symbols(
        &self,
        db: &'db dyn Db,
        importing_file: File,
        name: &Name,
    ) -> Option<&'db FlatSymbols> {
        let module_name = match self.module_names.get(name.as_str())? {
            ImportModuleKind::Definitive(name) | ImportModuleKind::Possible(name) => {
                name.to_module_name(db, importing_file)?
            }
        };
        let module = resolve_module(db, importing_file, &module_name)?;
        Some(symbols_for_file_global_only(db, module.file(db)?))
    }
}

/// Describes the level of certainty that an import is a module.
///
/// For example, `import foo`, then `foo` is definitively a module.
/// But `from quux import foo`, then `quux.foo` is possibly a module.
#[derive(Debug, Clone, Copy, get_size2::GetSize)]
enum ImportModuleKind<'db> {
    Definitive(ImportModuleName<'db>),
    Possible(ImportModuleName<'db>),
}

/// A representation of something that can be turned into a
/// `ModuleName`.
///
/// We don't do this eagerly, and instead represent the constituent
/// pieces, in order to avoid the work needed to build a `ModuleName`.
/// In particular, it is somewhat rare for the visitor to need
/// to access the imports found in a module. At time of writing
/// (2025-12-10), this only happens when referencing a submodule
/// to augment an `__all__` definition. For example, as found in
/// `matplotlib`:
///
/// ```ignore
/// import numpy as np
/// __all__ = ['rand', 'randn', 'repmat']
/// __all__ += np.__all__
/// ```
///
/// This construct is somewhat rare and it would be sad to allocate a
/// `ModuleName` for every imported item unnecessarily.
#[derive(Debug, Clone, Copy, get_size2::GetSize)]
enum ImportModuleName<'db> {
    /// The `foo` in `import quux, foo as blah, baz`.
    Import(&'db Name),
    /// A possible module in a `from ... import ...` statement.
    ImportFrom {
        /// The `..foo` in `from ..foo import quux`.
        parent: &'db ast::StmtImportFrom,
        /// The `foo` in `from quux import foo`.
        child: &'db Name,
    },
}

impl<'db> ImportModuleName<'db> {
    /// Converts the lazy representation of a module name into an
    /// actual `ModuleName` that can be used for module resolution.
    fn to_module_name(self, db: &'db dyn Db, importing_file: File) -> Option<ModuleName> {
        match self {
            ImportModuleName::Import(name) => ModuleName::new(name),
            ImportModuleName::ImportFrom { parent, child } => {
                let mut module_name =
                    ModuleName::from_import_statement(db, importing_file, parent).ok()?;
                let child_module_name = ModuleName::new(child)?;
                module_name.extend(&child_module_name);
                Some(module_name)
            }
        }
    }
}

/// A visitor over all symbols in a single file.
///
/// This guarantees that child symbols have a symbol ID greater
/// than all of its parents.
#[allow(clippy::struct_excessive_bools)]
struct SymbolVisitor<'db> {
    db: &'db dyn Db,
    file: File,
    symbols: IndexVec<SymbolId, SymbolTree>,
    symbol_stack: Vec<SymbolId>,
    /// Track if we're currently inside a function at any point.
    ///
    /// This is true even when we're inside a class definition
    /// that is inside a class.
    in_function: bool,
    /// Track if we're currently inside a class at any point.
    ///
    /// This is true even when we're inside a function definition
    /// that is inside a class.
    in_class: bool,
    /// When enabled, the visitor should only try to extract
    /// symbols from a module that we believed form the "exported"
    /// interface for that module. i.e., `__all__` is only respected
    /// when this is enabled. It's otherwise ignored.
    exports_only: bool,
    /// The origin of an `__all__` variable, if found.
    all_origin: Option<DunderAllOrigin>,
    /// A set of names extracted from `__all__`.
    all_names: FxHashSet<Name>,
    /// A flag indicating whether the module uses unrecognized
    /// `__all__` idioms or there are any invalid elements in
    /// `__all__`.
    all_invalid: bool,
    /// A collection of imports found while visiting the AST.
    ///
    /// These are used to help resolve references to modules
    /// in some limited cases.
    imports: Imports<'db>,
}

impl<'db> SymbolVisitor<'db> {
    fn tree(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            symbols: IndexVec::new(),
            symbol_stack: vec![],
            in_function: false,
            in_class: false,
            exports_only: false,
            all_origin: None,
            all_names: FxHashSet::default(),
            all_invalid: false,
            imports: Imports::default(),
        }
    }

    fn globals(db: &'db dyn Db, file: File) -> Self {
        Self {
            exports_only: true,
            ..Self::tree(db, file)
        }
    }

    fn into_flat_symbols(mut self) -> FlatSymbols {
        // If `__all__` was found but wasn't recognized,
        // then we emit a diagnostic message indicating as such.
        if self.all_invalid {
            tracing::debug!("Invalid `__all__` in `{}`", self.file.path(self.db));
        }
        // We want to filter out some of the symbols we collected.
        // Specifically, to respect conventions around library
        // interface.
        //
        // But, we always assigned IDs to each symbol based on
        // their position in a sequence. So when we filter some
        // out, we need to remap the identifiers.
        //
        // We also want to deduplicate when `exports_only` is
        // `true`. In particular, dealing with `__all__` can
        // result in cycles, and we need to make sure our output
        // is stable for that reason.
        //
        // N.B. The remapping could be skipped when `exports_only` is
        // true, since in that case, none of the symbols have a parent
        // ID by construction.
        let mut remap = IndexVec::with_capacity(self.symbols.len());
        let mut seen = self.exports_only.then(FxHashSet::default);
        let mut new = IndexVec::with_capacity(self.symbols.len());
        for mut symbol in std::mem::take(&mut self.symbols) {
            // If we're deduplicating and we've already seen
            // this symbol, then skip it.
            //
            // FIXME: We should do this without copying every
            // symbol name. ---AG
            if let Some(ref mut seen) = seen {
                if !seen.insert(symbol.name.clone()) {
                    continue;
                }
            }
            if !self.is_part_of_library_interface(&symbol) {
                remap.push(None);
                continue;
            }

            if let Some(ref mut parent) = symbol.parent {
                // OK because the visitor guarantees that
                // all parents have IDs less than their
                // children. So its ID has already been
                // remapped.
                if let Some(new_parent) = remap[*parent] {
                    *parent = new_parent;
                } else {
                    // The parent symbol was dropped, so
                    // all of its children should be as
                    // well.
                    remap.push(None);
                    continue;
                }
            }
            let new_id = new.next_index();
            remap.push(Some(new_id));
            new.push(symbol);
        }
        FlatSymbols {
            symbols: new,
            all_names: self.all_origin.map(|_| self.all_names),
        }
    }

    fn visit_body(&mut self, body: &'db [ast::Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
        }
    }

    /// Add a new symbol and return its ID.
    fn add_symbol(&mut self, mut symbol: SymbolTree) -> SymbolId {
        if let Some(&parent_id) = self.symbol_stack.last() {
            symbol.parent = Some(parent_id);
        }
        // It's important that we push the symbol and allocate
        // an ID before visiting its child. This preserves the
        // guarantee that parent IDs are always less than their
        // children IDs.
        let symbol_id = self.symbols.next_index();
        self.symbols.push(symbol);
        symbol_id
    }

    /// Adds a symbol introduced via an assignment.
    fn add_assignment(&mut self, stmt: &ast::Stmt, name: &ast::ExprName) -> SymbolId {
        let kind = if Self::is_constant_name(name.id.as_str()) {
            SymbolKind::Constant
        } else if self
            .iter_symbol_stack()
            .any(|s| s.kind == SymbolKind::Class)
        {
            SymbolKind::Field
        } else {
            SymbolKind::Variable
        };

        let symbol = SymbolTree {
            parent: None,
            name: name.id.to_string(),
            kind,
            name_range: name.range(),
            full_range: stmt.range(),
            import_kind: None,
        };
        self.add_symbol(symbol)
    }

    /// Adds a symbol introduced via an import `stmt`.
    fn add_import_alias(&mut self, stmt: &ast::Stmt, alias: &ast::Alias) -> SymbolId {
        let name = alias.asname.as_ref().unwrap_or(&alias.name);
        let kind = if stmt.is_import_stmt() {
            SymbolKind::Module
        } else if Self::is_constant_name(name.as_str()) {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };
        let re_export = Some(
            if alias.asname.as_ref().map(ast::Identifier::as_str) == Some(alias.name.as_str()) {
                ImportKind::RedundantAlias
            } else {
                ImportKind::Normal
            },
        );
        self.add_symbol(SymbolTree {
            parent: None,
            name: name.id.to_string(),
            kind,
            name_range: name.range(),
            full_range: stmt.range(),
            import_kind: re_export,
        })
    }

    /// Extracts `__all__` names from the given assignment.
    ///
    /// If the assignment isn't for `__all__`, then this is a no-op.
    fn add_all_assignment(&mut self, targets: &[ast::Expr], value: Option<&ast::Expr>) {
        // We don't care about `__all__` unless we're
        // specifically looking for exported symbols.
        if !self.exports_only {
            return;
        }
        if self.in_function || self.in_class {
            return;
        }
        let Some(target) = targets.first() else {
            return;
        };
        if !is_dunder_all(target) {
            return;
        }

        let Some(value) = value else { return };
        match *value {
            // `__all__ = [...]`
            // `__all__ = (...)`
            ast::Expr::List(ast::ExprList { ref elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { ref elts, .. }) => {
                self.update_all_origin(DunderAllOrigin::CurrentModule);
                if !self.add_all_names(elts) {
                    self.all_invalid = true;
                }
            }
            _ => {
                self.all_invalid = true;
            }
        }
    }

    /// Extends the current set of names with the names from the
    /// given expression which currently must be a list/tuple/set of
    /// string-literal names. This currently does not support using a
    /// submodule's `__all__` variable.
    ///
    /// Returns `true` if the expression is a valid list/tuple/set or
    /// module `__all__`, `false` otherwise.
    ///
    /// N.B. Supporting all instances of `__all__ += submodule.__all__`
    /// and `__all__.extend(submodule.__all__)` is likely difficult
    /// in this context. Namely, `submodule` needs to be resolved
    /// to a particular module. ty proper can do this (by virtue
    /// of inferring the type of `submodule`). With that said, we
    /// could likely support a subset of cases here without too much
    /// ceremony. ---AG
    fn extend_all(&mut self, expr: &ast::Expr) -> bool {
        match expr {
            // `__all__ += [...]`
            // `__all__ += (...)`
            // `__all__ += {...}`
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. })
            | ast::Expr::Set(ast::ExprSet { elts, .. }) => self.add_all_names(elts),
            // `__all__ += module.__all__`
            // `__all__.extend(module.__all__)`
            ast::Expr::Attribute(ast::ExprAttribute { .. }) => {
                let Some(unqualified) = UnqualifiedName::from_expr(expr) else {
                    return false;
                };
                let Some((&attr, rest)) = unqualified.segments().split_last() else {
                    return false;
                };
                if attr != "__all__" {
                    return false;
                }
                let possible_module_name = Name::new(rest.join("."));
                let Some(symbols) =
                    self.imports
                        .get_module_symbols(self.db, self.file, &possible_module_name)
                else {
                    return false;
                };
                let Some(ref all) = symbols.all_names else {
                    return false;
                };
                self.all_names.extend(all.iter().cloned());
                true
            }
            _ => false,
        }
    }

    /// Processes a call idiom for `__all__` and updates the set of
    /// names accordingly.
    ///
    /// Returns `true` if the call idiom is recognized and valid,
    /// `false` otherwise.
    fn update_all_by_call_idiom(
        &mut self,
        function_name: &ast::Identifier,
        arguments: &ast::Arguments,
    ) -> bool {
        if arguments.len() != 1 {
            return false;
        }
        let Some(argument) = arguments.find_positional(0) else {
            return false;
        };
        match function_name.as_str() {
            // `__all__.extend([...])`
            // `__all__.extend(module.__all__)`
            "extend" => {
                if !self.extend_all(argument) {
                    return false;
                }
            }
            // `__all__.append(...)`
            "append" => {
                let Some(name) = create_all_name(argument) else {
                    return false;
                };
                self.all_names.insert(name);
            }
            // `__all__.remove(...)`
            "remove" => {
                let Some(name) = create_all_name(argument) else {
                    return false;
                };
                self.all_names.remove(&name);
            }
            _ => return false,
        }
        true
    }

    /// Adds all of the names exported from the module
    /// imported by `import_from`. i.e., This implements
    /// `from module import *` semantics.
    fn add_exported_from_wildcard(&mut self, import_from: &ast::StmtImportFrom) {
        let Some(symbols) = self.get_names_from_wildcard(import_from) else {
            self.all_invalid = true;
            return;
        };
        self.symbols
            .extend(symbols.symbols.iter().filter_map(|symbol| {
                // If there's no `__all__`, then names with an underscore
                // are never pulled in via a wildcard import. Otherwise,
                // we defer to `__all__` filtering.
                if symbols.all_names.is_none() && symbol.name.starts_with('_') {
                    return None;
                }
                let mut symbol = symbol.clone();
                symbol.import_kind = Some(ImportKind::Wildcard);
                Some(symbol)
            }));
        // If the imported module defines an `__all__` AND `__all__` is
        // in `__all__`, then the importer gets it too.
        if let Some(ref all) = symbols.all_names
            && all.contains("__all__")
        {
            self.update_all_origin(DunderAllOrigin::StarImport);
            self.all_names.extend(all.iter().cloned());
        }
    }

    /// Adds `__all__` from the module imported by `import_from`. i.e.,
    /// This implements `from module import __all__` semantics.
    fn add_all_from_import(&mut self, import_from: &ast::StmtImportFrom) {
        let Some(symbols) = self.get_names_from_wildcard(import_from) else {
            self.all_invalid = true;
            return;
        };
        // If the imported module defines an `__all__`,
        // then the importer gets it too.
        if let Some(ref all) = symbols.all_names {
            self.update_all_origin(DunderAllOrigin::ExternalModule);
            self.all_names.extend(all.iter().cloned());
        }
    }

    /// Returns the exported symbols (along with `__all__`) from the
    /// module imported in `import_from`.
    fn get_names_from_wildcard(
        &self,
        import_from: &ast::StmtImportFrom,
    ) -> Option<&'db FlatSymbols> {
        let module_name =
            ModuleName::from_import_statement(self.db, self.file, import_from).ok()?;
        let module = resolve_module(self.db, self.file, &module_name)?;
        Some(symbols_for_file_global_only(self.db, module.file(self.db)?))
    }

    /// Add valid names from `__all__` to the set of existing `__all__`
    /// names.
    ///
    /// Returns `false` if any of the names are invalid.
    fn add_all_names(&mut self, exprs: &[ast::Expr]) -> bool {
        for expr in exprs {
            let Some(name) = create_all_name(expr) else {
                return false;
            };
            self.all_names.insert(name);
        }
        true
    }

    /// Updates the origin of `__all__` in the current module.
    ///
    /// This will clear existing names if the origin is changed to
    /// mimic the behavior of overriding `__all__` in the current
    /// module.
    fn update_all_origin(&mut self, origin: DunderAllOrigin) {
        if self.all_origin.is_some() {
            self.all_names.clear();
        }
        self.all_origin = Some(origin);
    }

    fn push_symbol(&mut self, symbol: SymbolTree) {
        let symbol_id = self.add_symbol(symbol);
        self.symbol_stack.push(symbol_id);
    }

    fn pop_symbol(&mut self) {
        self.symbol_stack.pop().unwrap();
    }

    fn iter_symbol_stack(&self) -> impl Iterator<Item = &SymbolTree> {
        self.symbol_stack
            .iter()
            .copied()
            .map(|id| &self.symbols[id])
    }

    fn is_constant_name(name: &str) -> bool {
        name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
    }

    /// This routine determines whether the given symbol should be
    /// considered part of the public API of this module. The given
    /// symbol should defined or imported into this module.
    ///
    /// See: <https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols>
    fn is_part_of_library_interface(&self, symbol: &SymbolTree) -> bool {
        // If this is a child of something else, then we always
        // defer its visibility to the parent.
        if symbol.parent.is_some() {
            return true;
        }

        // When there's no `__all__`, we use conventions to determine
        // if a name should be part of the exported API of a module
        // or not. When there is `__all__`, we currently follow it
        // strictly.
        //
        // If `__all__` is somehow invalid, ignore it and fall
        // through as-if `__all__` didn't exist.
        if self.all_origin.is_some() && !self.all_invalid {
            return self.all_names.contains(&*symbol.name);
        }

        // "Imported symbols are considered private by default. A fixed
        // set of import forms re-export imported symbols." Specifically:
        //
        // * `import X as X`
        // * `from Y import X as X`
        // * `from Y import *`
        if let Some(kind) = symbol.import_kind {
            return match kind {
                ImportKind::RedundantAlias | ImportKind::Wildcard => true,
                ImportKind::Normal => false,
            };
        }
        // "Symbols whose names begin with an underscore (but are not
        // dunder names) are considered private."
        //
        // ... however, we currently include these as part of the public
        // API. The only extant (2025-12-03) consumer is completions, and
        // completions will rank these names lower than others.
        if symbol.name.starts_with('_')
            && !(symbol.name.starts_with("__") && symbol.name.ends_with("__"))
        {
            return true;
        }
        // ... otherwise, it's exported!
        true
    }
}

impl<'db> SourceOrderVisitor<'db> for SymbolVisitor<'db> {
    fn visit_stmt(&mut self, stmt: &'db ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(func_def) => {
                let kind = if self
                    .iter_symbol_stack()
                    .any(|s| s.kind == SymbolKind::Class)
                {
                    if func_def.name.as_str() == "__init__" {
                        SymbolKind::Constructor
                    } else {
                        SymbolKind::Method
                    }
                } else {
                    SymbolKind::Function
                };

                let symbol = SymbolTree {
                    parent: None,
                    name: func_def.name.to_string(),
                    kind,
                    name_range: func_def.name.range(),
                    full_range: stmt.range(),
                    import_kind: None,
                };

                if self.exports_only {
                    self.add_symbol(symbol);
                    // If global_only, don't walk function bodies
                    return;
                }

                self.push_symbol(symbol);

                // Mark that we're entering a function scope
                let was_in_function = self.in_function;
                self.in_function = true;

                source_order::walk_stmt(self, stmt);

                // Restore the previous function scope state
                self.in_function = was_in_function;

                self.pop_symbol();
            }
            ast::Stmt::ClassDef(class_def) => {
                let symbol = SymbolTree {
                    parent: None,
                    name: class_def.name.to_string(),
                    kind: SymbolKind::Class,
                    name_range: class_def.name.range(),
                    full_range: stmt.range(),
                    import_kind: None,
                };

                if self.exports_only {
                    self.add_symbol(symbol);
                    // If global_only, don't walk class bodies
                    return;
                }

                // Mark that we're entering a class scope
                let was_in_class = self.in_class;
                self.in_class = true;

                self.push_symbol(symbol);
                source_order::walk_stmt(self, stmt);
                self.pop_symbol();

                // Restore the previous class scope state
                self.in_class = was_in_class;
            }
            ast::Stmt::Assign(assign) => {
                self.add_all_assignment(&assign.targets, Some(&assign.value));

                // Include assignments only when we're in global or class scope
                if self.in_function {
                    return;
                }
                for target in &assign.targets {
                    let ast::Expr::Name(name) = target else {
                        continue;
                    };
                    self.add_assignment(stmt, name);
                }
            }
            ast::Stmt::AnnAssign(ann_assign) => {
                self.add_all_assignment(
                    std::slice::from_ref(&ann_assign.target),
                    ann_assign.value.as_deref(),
                );

                // Include assignments only when we're in global or class scope
                if self.in_function {
                    return;
                }
                let ast::Expr::Name(name) = &*ann_assign.target else {
                    return;
                };
                self.add_assignment(stmt, name);
            }
            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target, op, value, ..
            }) => {
                // We don't care about `__all__` unless we're
                // specifically looking for exported symbols.
                if !self.exports_only {
                    return;
                }

                if self.all_origin.is_none() {
                    // We can't update `__all__` if it doesn't already
                    // exist.
                    return;
                }
                if !is_dunder_all(target) {
                    return;
                }
                // Anything other than `+=` is not valid.
                if !matches!(op, ast::Operator::Add) {
                    self.all_invalid = true;
                    return;
                }
                if !self.extend_all(value) {
                    self.all_invalid = true;
                }
            }
            ast::Stmt::Expr(expr) => {
                // We don't care about `__all__` unless we're
                // specifically looking for exported symbols.
                if !self.exports_only {
                    return;
                }

                if self.all_origin.is_none() {
                    // We can't update `__all__` if it doesn't already exist.
                    return;
                }
                let Some(ast::ExprCall {
                    func, arguments, ..
                }) = expr.value.as_call_expr()
                else {
                    return;
                };
                let Some(ast::ExprAttribute {
                    value,
                    attr,
                    ctx: ast::ExprContext::Load,
                    ..
                }) = func.as_attribute_expr()
                else {
                    return;
                };
                if !is_dunder_all(value) {
                    return;
                }
                if !self.update_all_by_call_idiom(attr, arguments) {
                    self.all_invalid = true;
                }

                source_order::walk_stmt(self, stmt);
            }
            ast::Stmt::Import(import) => {
                // We ignore any names introduced by imports
                // unless we're specifically looking for the
                // set of exported symbols.
                if !self.exports_only {
                    return;
                }
                // We only consider imports in global scope.
                if self.in_function {
                    return;
                }
                self.imports.add_import(import);
                for alias in &import.names {
                    self.add_import_alias(stmt, alias);
                }
            }
            ast::Stmt::ImportFrom(import_from) => {
                // We ignore any names introduced by imports
                // unless we're specifically looking for the
                // set of exported symbols.
                if !self.exports_only {
                    return;
                }
                // We only consider imports in global scope.
                if self.in_function {
                    return;
                }
                self.imports.add_import_from(import_from);
                for alias in &import_from.names {
                    if &alias.name == "*" {
                        self.add_exported_from_wildcard(import_from);
                    } else {
                        if &alias.name == "__all__"
                            && alias
                                .asname
                                .as_ref()
                                .is_none_or(|asname| asname == "__all__")
                        {
                            self.add_all_from_import(import_from);
                        }
                        self.add_import_alias(stmt, alias);
                    }
                }
            }
            // FIXME: We don't currently try to evaluate `if`
            // statements. We just assume that all `if` statements are
            // always `True`. This applies to symbols in general but
            // also `__all__`.
            _ => {
                source_order::walk_stmt(self, stmt);
            }
        }
    }

    // TODO: We might consider handling walrus expressions
    // here, since they can be used to introduce new names.
    fn visit_expr(&mut self, _expr: &ast::Expr) {}
}

/// Represents where an `__all__` has been defined.
#[derive(Debug, Clone)]
enum DunderAllOrigin {
    /// The `__all__` variable is defined in the current module.
    CurrentModule,
    /// The `__all__` variable is imported from another module.
    ExternalModule,
    /// The `__all__` variable is imported from a module via a `*`-import.
    StarImport,
}

/// Checks if the given expression is a name expression for `__all__`.
fn is_dunder_all(expr: &ast::Expr) -> bool {
    matches!(expr, ast::Expr::Name(ast::ExprName { id, .. }) if id == "__all__")
}

/// Create and return a string representing a name from the given
/// expression, or `None` if it is an invalid expression for a
/// `__all__` element.
fn create_all_name(expr: &ast::Expr) -> Option<Name> {
    Some(expr.as_string_literal_expr()?.value.to_str().into())
}

#[cfg(test)]
mod tests {
    use camino::Utf8Component;
    use insta::internals::SettingsBindDropGuard;

    use ruff_db::Db;
    use ruff_db::files::{FileRootKind, system_path_to_file};
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_python_trivia::textwrap::dedent;
    use ty_project::{ProjectMetadata, TestDb};

    use super::symbols_for_file_global_only;

    #[test]
    fn various_yes() {
        assert!(matches("", ""));
        assert!(matches("", "a"));
        assert!(matches("", "abc"));

        assert!(matches("a", "a"));
        assert!(matches("a", "abc"));
        assert!(matches("a", "xaz"));
        assert!(matches("a", "xza"));

        assert!(matches("abc", "abc"));
        assert!(matches("abc", "axbyc"));
        assert!(matches("abc", "waxbycz"));
        assert!(matches("abc", "WAXBYCZ"));
        assert!(matches("ABC", "waxbycz"));
        assert!(matches("ABC", "WAXBYCZ"));
        assert!(matches("aBc", "wAXbyCZ"));

        assert!(matches("δ", "Δ"));
        assert!(matches("δΘπ", "ΔθΠ"));
    }

    #[test]
    fn various_no() {
        assert!(!matches("a", ""));
        assert!(!matches("abc", "bac"));
        assert!(!matches("abcd", "abc"));
        assert!(!matches("δΘπ", "θΔΠ"));
    }

    #[test]
    fn exports_simple() {
        insta::assert_snapshot!(
            public_test("\
FOO = 1
foo = 1
frob: int = 1
class Foo:
    BAR = 1
def quux():
    baz = 1
").exports(),
            @r"
        FOO :: Constant
        foo :: Variable
        frob :: Variable
        Foo :: Class
        quux :: Function
        ",
        );
    }

    /// The typing spec says that names beginning with an underscore
    /// ought to be considered unexported[1]. However, at present, we
    /// currently include them in completions but rank them lower than
    /// non-underscore names. So this tests that we return underscore
    /// names.
    ///
    /// [1]: https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols
    #[test]
    fn exports_underscore() {
        insta::assert_snapshot!(
            public_test("\
_foo = 1
").exports(),
            @r"
        _foo :: Variable
        ",
        );
    }

    #[test]
    fn exports_conditional_true() {
        insta::assert_snapshot!(
            public_test("\
foo = 1
if True:
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_conditional_false() {
        // FIXME: This shouldn't include `bar`.
        insta::assert_snapshot!(
            public_test("\
foo = 1
if False:
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_conditional_sys_version() {
        // FIXME: This shouldn't include `bar`.
        insta::assert_snapshot!(
            public_test("\
import sys

foo = 1
if sys.version < (3, 5):
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_type_checking() {
        insta::assert_snapshot!(
            public_test("\
from typing import TYPE_CHECKING

foo = 1
if TYPE_CHECKING:
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_conditional_always_else() {
        // FIXME: This shouldn't include `bar`.
        insta::assert_snapshot!(
            public_test("\
foo = 1
bar = 1
if True:
    __all__ = ['foo']
else:
    __all__ = ['foo', 'bar']
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_all_overwrites_previous() {
        insta::assert_snapshot!(
            public_test("\
foo = 1
bar = 1
__all__ = ['foo']
__all__ = ['foo', 'bar']
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_import_no_reexport() {
        insta::assert_snapshot!(
            public_test("\
import collections
").exports(),
            @r"",
        );
    }

    #[test]
    fn exports_import_as_no_reexport() {
        insta::assert_snapshot!(
            public_test("\
import numpy as np
").exports(),
            @r"",
        );
    }

    #[test]
    fn exports_from_import_no_reexport() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
").exports(),
            @r"",
        );
    }

    #[test]
    fn exports_from_import_as_no_reexport() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict as dd
").exports(),
            @r"",
        );
    }

    #[test]
    fn exports_import_reexport() {
        insta::assert_snapshot!(
            public_test("\
import numpy as numpy
").exports(),
            @"numpy :: Module",
        );
    }

    #[test]
    fn exports_from_import_reexport() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict as defaultdict
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_assignment() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = ['defaultdict']
").exports(),
            @"defaultdict :: Variable",
        );

        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = ('defaultdict',)
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_annotated_assignment() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__: list[str] = ['defaultdict']
").exports(),
            @"defaultdict :: Variable",
        );

        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__: tuple[str, ...] = ('defaultdict',)
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_augmented_assignment() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__ += ['defaultdict']
").exports(),
            @"defaultdict :: Variable",
        );

        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__ += ('defaultdict',)
").exports(),
            @"defaultdict :: Variable",
        );

        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__ += {'defaultdict'}
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_invalid_augmented_assignment() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ += ['defaultdict']
").exports(),
            @"",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_extend() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__.extend(['defaultdict'])
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_invalid_extend() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__.extend(['defaultdict'])
").exports(),
            @r"",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_append() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__.append('defaultdict')
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_plus_equals() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__ += ['defaultdict']
").exports(),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_star_equals() {
        // Confirm that this doesn't work. Only `__all__ += ...` should
        // be recognized. This makes the symbol visitor consider
        // `__all__` invalid and thus ignore it. And this in turn lets
        // `__all__` be exported. This seems like a somewhat degenerate
        // case, but is a consequence of us treating sunder and dunder
        // symbols as exported when `__all__` isn't present.
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__ *= ['defaultdict']
").exports(),
            @"__all__ :: Variable",
        );
    }

    #[test]
    fn exports_from_import_all_reexport_remove() {
        insta::assert_snapshot!(
            public_test("\
from collections import defaultdict
__all__ = []
__all__.remove('defaultdict')
").exports(),
            @"",
        );
    }

    #[test]
    fn exports_nested_all() {
        insta::assert_snapshot!(
            public_test(r#"\
bar = 1
baz = 1
__all__ = []

def foo():
    __all__.append("bar")

class X:
    def method(self):
        __all__.extend(["baz"])
"#).exports(),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_simple_no_all() {
        let test = PublicTestBuilder::default()
            .source("foo.py", "ZQZQZQ = 1")
            .source("test.py", "from foo import *")
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"ZQZQZQ :: Constant",
        );
    }

    #[test]
    fn wildcard_reexport_single_underscore_no_all() {
        let test = PublicTestBuilder::default()
            .source("foo.py", "_ZQZQZQ = 1")
            .source("test.py", "from foo import *")
            .build();
        // Without `__all__` present, a wildcard import won't include
        // names starting with an underscore at runtime. So `_ZQZQZQ`
        // should not be present here.
        // See: <https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers>
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_double_underscore_no_all() {
        let test = PublicTestBuilder::default()
            .source("foo.py", "__ZQZQZQ = 1")
            .source("test.py", "from foo import *")
            .build();
        // Without `__all__` present, a wildcard import won't include
        // names starting with an underscore at runtime. So `__ZQZQZQ`
        // should not be present here.
        // See: <https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers>
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_normal_import_no_all() {
        let test = PublicTestBuilder::default()
            .source("foo.py", "import collections")
            .source("test.py", "from foo import *")
            .build();
        // We specifically test for the absence of `collections`
        // here. That is, `from foo import *` will import
        // `collections` at runtime, but we don't consider it part
        // of the exported interface of `foo`.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_redundant_import_no_all() {
        let test = PublicTestBuilder::default()
            .source("foo.py", "import collections as collections")
            .source("test.py", "from foo import *")
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"collections :: Module",
        );
    }

    #[test]
    fn wildcard_reexport_normal_from_import_no_all() {
        let test = PublicTestBuilder::default()
            .source("foo.py", "from collections import defaultdict")
            .source("test.py", "from foo import *")
            .build();
        // We specifically test for the absence of `defaultdict`
        // here. That is, `from foo import *` will import
        // `defaultdict` at runtime, but we don't consider it part
        // of the exported interface of `foo`.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_redundant_from_import_no_all() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "from collections import defaultdict as defaultdict",
            )
            .source("test.py", "from foo import *")
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"defaultdict :: Variable",
        );
    }

    #[test]
    fn wildcard_reexport_all_simple() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['ZQZQZQ']
            ",
            )
            .source("test.py", "from foo import *")
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"ZQZQZQ :: Constant",
        );
    }

    #[test]
    fn wildcard_reexport_all_simple_include_all() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['__all__', 'ZQZQZQ']
            ",
            )
            .source("test.py", "from foo import *")
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        ZQZQZQ :: Constant
        __all__ :: Variable
        ",
        );
    }

    #[test]
    fn wildcard_reexport_all_empty() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = []
            ",
            )
            .source("test.py", "from foo import *")
            .build();
        // Nothing is exported because `__all__` is defined
        // and also empty.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_all_empty_not_applies_to_importer() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = []
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 TRICKSY = 1",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"TRICKSY :: Constant",
        );
    }

    #[test]
    fn wildcard_reexport_all_include_all_applies_to_importer() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['__all__']
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 TRICKSY = 1",
            )
            .build();
        // TRICKSY should specifically be absent because
        // `__all__` is defined in `test.py` (via a wildcard
        // import) and does not itself include `TRICKSY`.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"__all__ :: Variable",
        );
    }

    #[test]
    fn wildcard_reexport_all_empty_then_added_to() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = []
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 TRICKSY = 1
                 __all__.append('TRICKSY')",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"TRICKSY :: Constant",
        );
    }

    #[test]
    fn wildcard_reexport_all_include_all_then_added_to() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['__all__']
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 TRICKSY = 1
                 __all__.append('TRICKSY')",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        __all__ :: Variable
        TRICKSY :: Constant
        ",
        );
    }

    /// Tests that a `from module import *` doesn't bring an
    /// `__all__` into scope if `module` doesn't provide an
    /// `__all__` that includes `__all__` AND this causes
    /// `__all__.append` to fail in the importing module
    /// (because it isn't defined).
    #[test]
    fn wildcard_reexport_all_empty_then_added_to_incorrect() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = []
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 from collections import defaultdict
                 __all__.append('defaultdict')",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_all_include_all_then_added_to_correct() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['__all__']
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 from collections import defaultdict
                 __all__.append('defaultdict')",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        __all__ :: Variable
        defaultdict :: Variable
        ",
        );
    }

    #[test]
    fn wildcard_reexport_all_non_empty_but_non_existent() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['TRICKSY']
            ",
            )
            .source("test.py", "from foo import *")
            .build();
        // `TRICKSY` isn't actually a valid symbol,
        // and `ZQZQZQ` isn't in `__all__`, so we get
        // no symbols here.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn wildcard_reexport_all_include_all_and_non_existent() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['__all__', 'TRICKSY']
            ",
            )
            .source("test.py", "from foo import *")
            .build();
        // Note that this example will actually result in a runtime
        // error since `TRICKSY` doesn't exist in `foo.py` and
        // `from foo import *` will try to import it anyway.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"__all__ :: Variable",
        );
    }

    #[test]
    fn wildcard_reexport_all_not_applies_to_importer() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['TRICKSY']
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 TRICKSY = 1",
            )
            .build();
        // Note that this example will actually result in a runtime
        // error since `TRICKSY` doesn't exist in `foo.py` and
        // `from foo import *` will try to import it anyway.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"TRICKSY :: Constant",
        );
    }

    #[test]
    fn wildcard_reexport_all_include_all_with_others_applies_to_importer() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['__all__', 'TRICKSY']
            ",
            )
            .source(
                "test.py",
                "from foo import *
                 TRICKSY = 1",
            )
            .build();
        // Note that this example will actually result in a runtime
        // error since `TRICKSY` doesn't exist in `foo.py` and
        // `from foo import *` will try to import it anyway.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        __all__ :: Variable
        TRICKSY :: Constant
        ",
        );
    }

    #[test]
    fn explicit_reexport_all_empty_applies_to_importer() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = []
            ",
            )
            .source(
                "test.py",
                "from foo import __all__ as __all__
                 TRICKSY = 1",
            )
            .build();
        // `__all__` is imported from `foo.py` but it's
        // empty, so `TRICKSY` is not part of the exported
        // API.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn explicit_reexport_all_empty_then_added_to() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = []
            ",
            )
            .source(
                "test.py",
                "from foo import __all__
                 TRICKSY = 1
                 __all__.append('TRICKSY')",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"TRICKSY :: Constant",
        );
    }

    #[test]
    fn explicit_reexport_all_non_empty_but_non_existent() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['TRICKSY']
            ",
            )
            .source("test.py", "from foo import __all__ as __all__")
            .build();
        // `TRICKSY` is not a valid symbol, so it's not considered
        // part of the exports of `test`.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"",
        );
    }

    #[test]
    fn explicit_reexport_all_applies_to_importer() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                ZQZQZQ = 1
                __all__ = ['TRICKSY']
            ",
            )
            .source(
                "test.py",
                "from foo import __all__
                 TRICKSY = 1",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @"TRICKSY :: Constant",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_statement_plus_equals() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "import foo
                 from foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__ += foo.__all__
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_statement_extend() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "import foo
                 from foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__.extend(foo.__all__)
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_statement_alias() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "import foo as blah
                 from foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__ += blah.__all__
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_statement_nested_alias() {
        let test = PublicTestBuilder::default()
            .source("parent/__init__.py", "")
            .source(
                "parent/foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "import parent.foo as blah
                 from parent.foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__ += blah.__all__
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_from_statement_plus_equals() {
        let test = PublicTestBuilder::default()
            .source("parent/__init__.py", "")
            .source(
                "parent/foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "from parent import foo
                 from parent.foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__ += foo.__all__
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_from_statement_nested_module_reference() {
        let test = PublicTestBuilder::default()
            .source("parent/__init__.py", "")
            .source(
                "parent/foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "import parent.foo
                 from parent.foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__ += parent.foo.__all__
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_from_statement_extend() {
        let test = PublicTestBuilder::default()
            .source("parent/__init__.py", "")
            .source(
                "parent/foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "import parent.foo
                 from parent.foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__.extend(parent.foo.__all__)
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_from_statement_alias() {
        let test = PublicTestBuilder::default()
            .source("parent/__init__.py", "")
            .source(
                "parent/foo.py",
                "
                _ZQZQZQ = 1
                __all__ = ['_ZQZQZQ']
            ",
            )
            .source(
                "test.py",
                "from parent import foo as blah
                 from parent.foo import *
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__ += blah.__all__
                ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZQZQZQ :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_cycle1() {
        let test = PublicTestBuilder::default()
            .source(
                "a.py",
                "from b import *
                 import b
                _ZAZAZA = 1
                 __all__ = ['_ZAZAZA']
                 __all__ += b.__all__
                ",
            )
            .source(
                "b.py",
                "
                from a import *
                import a
                _ZBZBZB = 1
                __all__ = ['_ZBZBZB']
                __all__ += a.__all__
            ",
            )
            .build();
        insta::assert_snapshot!(
            test.exports_for("a.py"),
            @r"
        _ZBZBZB :: Constant
        _ZAZAZA :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_statement_failure1() {
        let test = PublicTestBuilder::default()
            .source(
                "foo.py",
                "
                _ZFZFZF = 1
                __all__ = ['_ZFZFZF']
            ",
            )
            .source(
                "bar.py",
                "
                _ZBZBZB = 1
                __all__ = ['_ZBZBZB']
            ",
            )
            .source(
                "test.py",
                "import foo
                 import bar
                 from foo import *
                 from bar import *

                 foo = bar
                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__.extend(foo.__all__)
                ",
            )
            .build();
        // In this test, we resolve `foo.__all__` to the `__all__`
        // attribute in module `foo` instead of in `bar`. This is
        // because we don't track redefinitions of imports (as of
        // 2025-12-11). Handling this correctly would mean exporting
        // `_ZBZBZB` instead of `_ZFZFZF`.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZFZFZF :: Constant
        _ZYZYZY :: Constant
        ",
        );
    }

    #[test]
    fn reexport_and_extend_from_submodule_import_statement_failure2() {
        let test = PublicTestBuilder::default()
            .source(
                "parent/__init__.py",
                "import parent.foo as foo
                 __all__ = ['foo']
                ",
            )
            .source(
                "parent/foo.py",
                "
                _ZFZFZF = 1
                __all__ = ['_ZFZFZF']
            ",
            )
            .source(
                "test.py",
                "from parent.foo import *
                 from parent import *

                 _ZYZYZY = 1
                 __all__ = ['_ZYZYZY']
                 __all__.extend(foo.__all__)
                ",
            )
            .build();
        // This is not quite right either because we end up
        // considering the `__all__` in `test.py` to be invalid.
        // Namely, we don't pick up the `foo` that is in scope
        // from the `from parent import *` import. The correct
        // answer should just be `_ZFZFZF` and `_ZYZYZY`.
        insta::assert_snapshot!(
            test.exports_for("test.py"),
            @r"
        _ZFZFZF :: Constant
        foo :: Module
        _ZYZYZY :: Constant
        __all__ :: Variable
        ",
        );
    }

    fn matches(query: &str, symbol: &str) -> bool {
        super::QueryPattern::fuzzy(query).is_match_symbol_name(symbol)
    }

    fn public_test(code: &str) -> PublicTest {
        PublicTestBuilder::default().source("test.py", code).build()
    }

    struct PublicTest {
        db: TestDb,
        _insta_settings_guard: SettingsBindDropGuard,
    }

    impl PublicTest {
        /// Returns the exports from `test.py`.
        ///
        /// This is, conventionally, the default module file path used. For
        /// example, it's used by the `public_test` convenience constructor.
        fn exports(&self) -> String {
            self.exports_for("test.py")
        }

        /// Returns the exports from the module at the given path.
        ///
        /// The path given must have been written to this test's salsa DB.
        fn exports_for(&self, path: impl AsRef<SystemPath>) -> String {
            let file = system_path_to_file(&self.db, path.as_ref()).unwrap();
            let symbols = symbols_for_file_global_only(&self.db, file);
            symbols
                .iter()
                .map(|(_, symbol)| {
                    format!("{name} :: {kind:?}", name = symbol.name, kind = symbol.kind)
                })
                .collect::<Vec<String>>()
                .join("\n")
        }
    }

    #[derive(Default)]
    struct PublicTestBuilder {
        /// A list of source files, corresponding to the
        /// file's path and its contents.
        sources: Vec<Source>,
    }

    impl PublicTestBuilder {
        pub(super) fn build(&self) -> PublicTest {
            let mut db = TestDb::new(ProjectMetadata::new(
                "test".into(),
                SystemPathBuf::from("/"),
            ));

            db.init_program().unwrap();

            for Source { path, contents } in &self.sources {
                db.write_file(path, contents)
                    .expect("write to memory file system to be successful");

                // Add a root for the top-most component.
                let top = path.components().find_map(|c| match c {
                    Utf8Component::Normal(c) => Some(c),
                    _ => None,
                });
                if let Some(top) = top {
                    let top = SystemPath::new(top);
                    if db.system().is_directory(top) {
                        db.files()
                            .try_add_root(&db, top, FileRootKind::LibrarySearchPath);
                    }
                }
            }

            // N.B. We don't set anything custom yet, but we leave
            // this here for when we invevitable add a filter.
            let insta_settings = insta::Settings::clone_current();
            let insta_settings_guard = insta_settings.bind_to_scope();
            PublicTest {
                db,
                _insta_settings_guard: insta_settings_guard,
            }
        }

        pub(super) fn source(
            &mut self,
            path: impl Into<SystemPathBuf>,
            contents: impl AsRef<str>,
        ) -> &mut PublicTestBuilder {
            let path = path.into();
            let contents = dedent(contents.as_ref()).into_owned();
            self.sources.push(Source { path, contents });
            self
        }
    }

    struct Source {
        path: SystemPathBuf,
        contents: String,
    }
}

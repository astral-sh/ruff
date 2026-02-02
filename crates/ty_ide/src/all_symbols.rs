use ruff_db::files::File;
use ruff_python_ast::name::Name;
use ty_module_resolver::{Module, ModuleName, all_modules, resolve_real_shadowable_module};
use ty_project::Db;

use crate::{
    SymbolKind,
    symbols::{ImportedFrom, QueryPattern, SymbolInfo, symbols_for_file_global_only},
};

/// Get all symbols matching the query string.
///
/// Returns symbols from all files in the workspace and dependencies, filtered
/// by the query.
pub fn all_symbols<'db>(
    db: &'db dyn Db,
    importing_from: File,
    query: &QueryPattern,
) -> Vec<AllSymbolInfo<'db>> {
    // If the query is empty, return immediately to avoid expensive file scanning
    if query.will_match_everything() {
        return Vec::new();
    }

    let all_symbols_span = tracing::debug_span!("all_symbols");
    let _span = all_symbols_span.enter();

    let typing_extensions = ModuleName::new_static("typing_extensions").unwrap();
    let is_typing_extensions_available = importing_from.is_stub(db)
        || resolve_real_shadowable_module(db, importing_from, &typing_extensions).is_some();

    let results = std::sync::Mutex::new(Vec::new());
    {
        let modules = all_modules(db);
        let db = db.dyn_clone();
        let all_symbols_span = &all_symbols_span;
        let results = &results;
        let query = &query;

        rayon::scope(move |s| {
            // For each file, extract symbols and add them to results
            for module in modules {
                let db = db.dyn_clone();
                let Some(file) = module.file(&*db) else {
                    continue;
                };

                // By convention, modules starting with an underscore
                // are generally considered unexported. However, we
                // should consider first party modules fair game.
                //
                // Note that we apply this recursively. e.g.,
                // `numpy._core.multiarray` is considered private
                // because it's a child of `_core`.
                if module.name(&*db).components().any(|c| c.starts_with('_'))
                    && module
                        .search_path(&*db)
                        .is_none_or(|sp| !sp.is_first_party())
                {
                    continue;
                }
                // TODO: also make it available in `TYPE_CHECKING` blocks
                // (we'd need https://github.com/astral-sh/ty/issues/1553 to do this well)
                if !is_typing_extensions_available && module.name(&*db) == &typing_extensions {
                    continue;
                }
                s.spawn(move |_| {
                    let symbols_for_file_span = tracing::debug_span!(
                        parent: all_symbols_span,
                        "symbols_for_file_global_only",
                        path = %file.path(&*db),
                    );
                    let _entered = symbols_for_file_span.entered();

                    let mut symbols = vec![];
                    if query.is_match_symbol_name(module.name(&*db)) {
                        symbols.push(AllSymbolInfo::from_module(&*db, module, file));
                    }
                    for (_, symbol) in symbols_for_file_global_only(&*db, file).search(query) {
                        symbols.push(AllSymbolInfo::from_non_module_symbol(
                            &*db,
                            symbol.to_owned(),
                            module,
                            file,
                        ));
                    }
                    results.lock().unwrap().extend(symbols);
                });
            }
        });
    }
    merge::merge(db, results.into_inner().unwrap())
}

/// A symbol found in the workspace and dependencies, including the
/// file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllSymbolInfo<'db> {
    /// The symbol information.
    ///
    /// When absent, this implies the symbol is the module itself.
    symbol: Option<SymbolInfo<'static>>,
    /// The fully qualified name of this symbol.
    qualified: Name,
    /// The module containing the symbol.
    module: Module<'db>,
    /// The file containing the symbol.
    ///
    /// This `File` is guaranteed to be the same
    /// as the `File` underlying `module`.
    file: File,
}

impl<'db> AllSymbolInfo<'db> {
    fn from_non_module_symbol(
        db: &'_ dyn Db,
        symbol: SymbolInfo<'static>,
        module: Module<'db>,
        file: File,
    ) -> AllSymbolInfo<'db> {
        let qualified = Name::from(compact_str::format_compact!(
            "{module_name}.{name}",
            module_name = module.name(db),
            name = symbol.name,
        ));
        AllSymbolInfo {
            symbol: Some(symbol),
            qualified,
            module,
            file,
        }
    }

    /// Creates a new symbol referencing an entire module.
    fn from_module(db: &'_ dyn Db, module: Module<'db>, file: File) -> AllSymbolInfo<'db> {
        AllSymbolInfo {
            symbol: None,
            qualified: module.name(db).as_str().into(),
            module,
            file,
        }
    }

    /// Returns the name of this symbol as it exists in a file.
    ///
    /// When absent, there is no concrete symbol in a module
    /// somewhere. Instead, this represents importing a module.
    /// In this case, if the caller needs a symbol name, they
    /// should use `AllSymbolInfo::module().name()`.
    pub fn name_in_file(&self) -> Option<&str> {
        self.symbol.as_ref().map(|symbol| &*symbol.name)
    }

    /// Returns the fully qualified name for this symbol.
    ///
    /// This includes the full absolute module name for the symbol.
    ///
    /// When this symbol corresponds to a module, then this is
    /// just the full absolute module name itself.
    pub fn qualified(&self) -> &str {
        &self.qualified
    }

    /// Returns the "kind" of this symbol.
    ///
    /// The kind of a symbol in the context of auto-import is
    /// determined on a best effort basis. It may be imprecise
    /// in some cases, e.g., reporting a module as a variable.
    pub fn kind(&self) -> SymbolKind {
        self.symbol
            .as_ref()
            .map(|symbol| symbol.kind)
            .unwrap_or(SymbolKind::Module)
    }

    /// Returns the module this symbol is exported from.
    pub fn module(&self) -> Module<'db> {
        self.module
    }

    /// Returns the `File` corresponding to the module.
    ///
    /// This is always equivalent to
    /// `AllSymbolInfo::module().file().unwrap()`.
    pub fn file(&self) -> File {
        self.file
    }

    /// Returns the module that this symbol was re-exported from.
    ///
    /// This is only available for symbols that have been imported
    /// into `Self::module()` *and* are determined to be re-exports.
    pub(crate) fn imported_from(&self) -> Option<&ImportedFrom> {
        self.symbol
            .as_ref()
            .and_then(|symbol| symbol.imported_from.as_ref())
    }
}

mod merge {
    use rustc_hash::FxHashSet;
    use ty_module_resolver::ModuleName;
    use ty_project::Db;

    use super::AllSymbolInfo;

    /// This merges symbols within a package hierarchy, preferring symbols
    /// in the top-most module for a particular package.
    ///
    /// The implementation works by identifying chains of symbols within
    /// the same package that have the same name. The chain is formed
    /// by metadata attached to the symbol indicating whether it is a
    /// re-export, and if so, the module it is being re-exported from.
    ///
    /// Here's a real example to gain some intuition. The `pandas`
    /// library, as of 2026-01-13, defines its `pandas.read_csv` function
    /// in `pandas.io.parsers.readers`. It is then re-exported in
    /// `pandas.io.parsers`. Then re-exported again in `pandas.io.api`. And
    /// finally re-exported once more in the top-level `pandas` module. We
    /// want to merge all of these into the symbol identified in `pandas`.
    /// The "merge" here mostly just refers to preferring the `SymbolKind`
    /// associated with the original definition and not the re-export.
    /// (Because the `SymbolKind` on a re-export is necessarily imprecise.)
    ///
    /// Ultimately, this is a heuristic, as the following points convey:
    ///
    /// * One thing we specifically and intentionally do no handle here
    ///   is the case where a symbol is re-exported under a different name.
    ///   We could adapt the logic here to deal with that, but at time of
    ///   writing (2026-01-13), it wasn't clear whether we actually want to
    ///   do this or not. We might *want* to include both symbols.
    /// * Another thing we might consider doing differently here is to not
    ///   completely remove symbols deeper in the package hierarchy, and
    ///   instead just expose the fact that they are redundant somehow on
    ///   `AllSymbolInfo`. Then callers (like for completions) might just
    ///   choose to rank the redundant symbols much lower. But for now, we
    ///   just remove them entirely.
    /// * Also, we only consider symbols within the same package hierarchy.
    ///   For example, if `pandas` re-exports a symbol from `numpy`, then
    ///   we include both, but do merge relevant information from the
    ///   originating symbol into the re-export.
    pub(super) fn merge<'db>(
        db: &'db dyn Db,
        mut symbols: Vec<AllSymbolInfo<'db>>,
    ) -> Vec<AllSymbolInfo<'db>> {
        symbols.sort_unstable_by(|s1, s2| cmp(db, s1, s2));

        let mut group = Group::default();
        let mut merged = Vec::with_capacity(symbols.len());
        let mut it = symbols.into_iter().peekable();
        while let Some(sym) = it.next() {
            let Some(name) = sym.name_in_file() else {
                merged.extend(group.merge(db));
                group.clear();
                // A symbol without a name in a file is just
                // a module, and is never a re-export. So add
                // it as-is without ceremony.
                merged.push(sym);
                continue;
            };
            // If the name matches our current group (when non-empty), then
            // just add it and keep on collecting other symbols.
            if group.name().is_some_and(|group_name| group_name == name) {
                group.add(sym);
                continue;
            }

            // At this point, the symbol either doesn't
            // match the group or the group is empty. So
            // we know we're done with the group.
            if !group.is_empty() {
                merged.extend(group.merge(db));
                group.clear();
            }

            // As an optimization, if we know there isn't
            // an adjacent name or doesn't match this
            // one, then we know this symbol will never
            // be part of a group with size greater than 1.
            if it
                .peek()
                .and_then(|sym| sym.name_in_file())
                .map(|next_name| next_name != name)
                .unwrap_or(true)
            {
                merged.push(sym);
                continue;
            }

            // We know we're going to have a possibly
            // interesting group to deal with at this point.
            assert!(group.is_empty());
            group.add(sym);
        }
        merged.extend(group.merge(db));

        // The above might change the ordering of the symbols,
        // so re-sort them. If this ends up being a perf problem,
        // the above code should be adapted to avoid changing
        // the order. ---AG
        merged.sort_unstable_by(|s1, s2| cmp(db, s1, s2));
        merged
    }

    /// A *candidate* group of symbols that may form a re-export chain.
    ///
    /// Some, all or none of these symbols may get merged together.
    #[derive(Default)]
    struct Group<'db> {
        /// Symbols that are not re-exports. i.e., The original
        /// definitions.
        origin: Vec<AllSymbolInfo<'db>>,
        /// Symbols that are re-exports. i.e., Symbols that have
        /// been introduced into scope via an `import` or
        /// `from ... import` statement.
        ///
        /// Some, all or none of these may be a re-export from a
        /// symbol in `origin`.
        reexport: Vec<AllSymbolInfo<'db>>,
        /// The `origin` symbols to keep. By default, we keep all
        /// of them. If a re-export takes precedence (because it's
        /// higher in the module hierarchy), then the corresponding
        /// element here is set to `false`.
        origin_keep: Vec<bool>,
        /// The `reexport` symbols to keep.
        reexport_keep: Vec<bool>,
        /// The full set of re-exports discovered for a single
        /// `origin` symbol.
        all_reexports: FxHashSet<usize>,
        /// A stack of module names to look for re-exports.
        ///
        /// This is used to avoid explicit recursion.
        ///
        /// Specifically, an entry in this stack implies that
        /// we want to look for all symbols in `reexport` for
        /// which the module it was *imported from* matches the
        /// module name entry in this stack.
        ///
        /// This stack is bootstrapped with the name of the module
        /// containing a symbol from `origin`. By looking for
        /// all re-exports imported from its module name, and then
        /// doing that recursively for each re-export, we find
        /// the full set of re-exports for the original symbol.
        stack: Vec<&'db ModuleName>,
    }

    impl<'db> Group<'db> {
        /// Adds the given symbol to this group.
        ///
        /// # Panics
        ///
        /// When this group is non-empty, it is the caller's
        /// responsibility to ensure that the symbol's base
        /// name matches the base name of all other symbols
        /// in this group.
        fn add(&mut self, sym: AllSymbolInfo<'db>) {
            assert!(
                self.name().is_none_or(
                    |name| name == sym.name_in_file().unwrap_or_else(|| sym.qualified())
                ),
                "expected name of symbol provided to match the name of other symbols in this group",
            );
            if sym.imported_from().is_none() {
                self.origin.push(sym);
                self.origin_keep.push(true);
            } else {
                self.reexport.push(sym);
                self.reexport_keep.push(true);
            }
        }

        /// Clears state such that this group is
        /// indistinguishable from an empty group.
        fn clear(&mut self) {
            self.origin.clear();
            self.origin_keep.clear();
            self.reexport.clear();
            self.reexport_keep.clear();
        }

        /// Possibly merge the symbols in this group and return them.
        ///
        /// The iterator returned may not contain all symbols in this group.
        /// Some of them may have been dropped if they are considered redundant.
        fn merge(&mut self, db: &'db dyn Db) -> impl Iterator<Item = AllSymbolInfo<'db>> {
            for origin_index in 0..self.origin.len() {
                self.populate_all_reexports(db, origin_index);

                let origin = &self.origin[origin_index];
                let origin_module_name = origin.module().name(db);
                let top = origin_module_name.first_component();
                let mut min_reexport_len = None;
                for &reexport_index in &self.all_reexports {
                    let reexport = &mut self.reexport[reexport_index];
                    // Merge the kind from the original symbol into the
                    // reexported symbol. We do this unconditionally since
                    // it is applicable in all cases, even when the symbols
                    // are in different package hierarchies.
                    //
                    // The reason why we do this unconditionally is that
                    // reexport symbols usually do not have a very specific
                    // kind, and are typically just assigned a kind of
                    // "variable." This is because the symbol extraction
                    // reports these symbols as-is by inspecting import
                    // statements and doesn't follow them to their original
                    // definition to discover their "true" kind. But that's
                    // exactly what we're doing here! We have the original
                    // definition. So assign the kind from the original,
                    // even when it crosses package hierarchies.
                    if let (Some(origin_sym), &mut Some(ref mut reexport_sym)) =
                        (&origin.symbol, &mut reexport.symbol)
                    {
                        reexport_sym.kind = origin_sym.kind;
                    }

                    // Now we want to find the shortest (in terms of
                    // module components) len of all the re-exports.
                    // This is in service of eliminating all re-exports
                    // with a greater length, so we specifically don't
                    // consider re-exports that cross a module boundary.
                    let reexport_module_name = reexport.module().name(db);
                    if top != reexport_module_name.first_component() {
                        continue;
                    }
                    let len = reexport_module_name.components().count();
                    if min_reexport_len.map(|min| len < min).unwrap_or(true) {
                        min_reexport_len = Some(len);
                    }
                }

                let Some(mut min_reexport_len) = min_reexport_len else {
                    // When we couldn't find *any* other re-exports
                    // within the same package hierarchy as the original
                    // definition, then we're done.
                    continue;
                };
                let origin_len = origin_module_name.components().count();
                min_reexport_len = std::cmp::min(min_reexport_len, origin_len);
                // At this point, we want to keep all symbols that
                // are equivalent to the minimum length we found. This
                // may include or exclude the original definition.
                if origin_len > min_reexport_len {
                    self.origin_keep[origin_index] = false;
                }
                for &reexport_index in &self.all_reexports {
                    let reexport = &self.reexport[reexport_index];
                    if reexport.module().name(db).components().count() > min_reexport_len {
                        self.reexport_keep[reexport_index] = false;
                    }
                }
            }

            let origin = self
                .origin
                .drain(..)
                .enumerate()
                .filter(|&(i, _)| self.origin_keep[i])
                .map(|(_, sym)| sym);
            let reexport = self
                .reexport
                .drain(..)
                .enumerate()
                .filter(|&(i, _)| self.reexport_keep[i])
                .map(|(_, sym)| sym);
            origin.chain(reexport)
        }

        /// Populate `self.all_reexports` with indices into
        /// `self.reexport` for all symbols that are a re-export
        /// of `self.origin[origin_index]`. This includes the full
        /// transitive closure of those symbols.
        fn populate_all_reexports(&mut self, db: &'db dyn Db, origin_index: usize) {
            self.all_reexports.clear();
            self.stack.clear();
            self.stack.push(self.origin[origin_index].module().name(db));
            // We specifically do not use recursion here to avoid
            // stack usage proportional to the size of a re-export
            // chain in user provided code.
            while let Some(module_name) = self.stack.pop() {
                // Finds all of the symbols re-exported *directly from* `module_name`.
                let reexports_for_module_name = self
                    .reexport
                    .iter()
                    .enumerate()
                    .filter(move |&(_, sym)| {
                        sym.imported_from().unwrap().module_name() == module_name
                    })
                    .map(|(i, _)| i);
                // Now find, recursively, all symbols that
                // are re-exports of the re-exports (being
                // careful not to repeat work).
                for i in reexports_for_module_name {
                    if self.all_reexports.insert(i) {
                        self.stack.push(self.reexport[i].module().name(db));
                    }
                }
            }
        }

        /// Returns true when this group is empty.
        fn is_empty(&self) -> bool {
            self.origin.is_empty() && self.reexport.is_empty()
        }

        /// Returns the base symbol name for this group.
        ///
        /// Callers must ensure that all symbols in this
        /// group have the same base name.
        ///
        /// This returns `None` if and only if this group is empty.
        fn name(&self) -> Option<&str> {
            self.origin
                .first()
                .or_else(|| self.reexport.first())
                .map(|sym| sym.name_in_file().unwrap_or_else(|| sym.qualified()))
        }
    }

    fn cmp<'a>(
        db: &'a dyn Db,
        sym1: &'a AllSymbolInfo<'_>,
        sym2: &'a AllSymbolInfo<'_>,
    ) -> std::cmp::Ordering {
        let name1 = sym1
            .name_in_file()
            .unwrap_or_else(|| sym1.module().name(db).as_str());
        let name2 = sym2
            .name_in_file()
            .unwrap_or_else(|| sym2.module().name(db).as_str());
        let ord = name1.cmp(name2);
        if !ord.is_eq() {
            return ord;
        }

        let mod1 = sym1.module().name(db).as_str();
        let mod2 = sym2.module().name(db).as_str();
        let ord = mod1.cmp(mod2);
        if !ord.is_eq() {
            return ord;
        }

        let path1 = sym1.file.path(db).as_str();
        let path2 = sym2.file.path(db).as_str();
        path1.cmp(path2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::CursorTest;
    use crate::tests::IntoDiagnostic;
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };

    /// Tests that we merge redundant re-exports in a real world use case.
    ///
    /// This mimics how pandas exports `read_csv` in its top-level module
    /// but where it is actually defined in a deeper module. Note though
    /// that we use `zqzqzq` instead of `read_csv` to make our fuzzy query
    /// return only exactly what we care about.
    #[test]
    fn pandas_read_csv_merged_all() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source(
                "pandas/__init__.py",
                "
from pandas.io.api import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source("pandas/io/__init__.py", "")
            .source(
                "pandas/io/api.py",
                "
from pandas.io.parsers import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source(
                "pandas/io/parsers/__init__.py",
                "
from pandas.io.parsers.readers import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source(
                "pandas/io/parsers/readers.py",
                "
def zqzqzq():
    pass
",
            )
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/__init__.py:2:27
          |
        2 | from pandas.io.api import zqzqzq
          |                           ^^^^^^
        3 | __all__ = ['zqzqzq']
          |
        info: Function zqzqzq
        ");
    }

    /// Like `pandas_read_csv_merged_all`, but is a sanity
    /// check that re-exports via a redundant alias work too.
    #[test]
    fn pandas_read_csv_merged_redundant_export() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source(
                "pandas/__init__.py",
                "
from pandas.io.api import zqzqzq as zqzqzq
",
            )
            .source("pandas/io/__init__.py", "")
            .source(
                "pandas/io/api.py",
                "
from pandas.io.parsers import zqzqzq as zqzqzq
",
            )
            .source(
                "pandas/io/parsers/__init__.py",
                "
from pandas.io.parsers.readers import zqzqzq as zqzqzq
",
            )
            .source(
                "pandas/io/parsers/readers.py",
                "
def zqzqzq():
    pass
",
            )
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/__init__.py:2:37
          |
        2 | from pandas.io.api import zqzqzq as zqzqzq
          |                                     ^^^^^^
          |
        info: Function zqzqzq
        ");
    }

    /// Like `pandas_read_csv_merged_all`, but is a sanity
    /// check that re-exports via a wild card work too.
    #[test]
    fn pandas_read_csv_merged_wildcard() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source(
                "pandas/__init__.py",
                "
from pandas.io.api import *
",
            )
            .source("pandas/io/__init__.py", "")
            .source(
                "pandas/io/api.py",
                "
from pandas.io.parsers import *
",
            )
            .source(
                "pandas/io/parsers/__init__.py",
                "
from pandas.io.parsers.readers import *
",
            )
            .source(
                "pandas/io/parsers/readers.py",
                "
def zqzqzq():
    pass
",
            )
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/__init__.py:2:5
          |
        2 | from pandas.io.api import *
          |     ^^^^^^
          |
        info: Function zqzqzq
        ");
    }

    /// This tests that when we have multiple re-exports
    /// available at the same "module" depth in the same
    /// package, then we include both of them.
    ///
    /// In this test, that's `pandas.io.api.zqzqzq` and
    /// `pandas.io.parsers.zqzqzq`.
    ///
    /// Notice that the original definition,
    /// `pandas.io.parsers.readers.zqzqzq` is excluded
    /// since it is at a deeper level of nesting than
    /// the re-exports.
    #[test]
    fn redundant_reexported_merged_multiple_symbols_retained() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source("pandas/__init__.py", "")
            .source("pandas/io/__init__.py", "")
            .source(
                "pandas/io/api.py",
                "
from pandas.io.parsers import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source(
                "pandas/io/parsers/__init__.py",
                "
from pandas.io.parsers.readers import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source(
                "pandas/io/parsers/readers.py",
                "
def zqzqzq():
    pass
",
            )
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/io/api.py:2:31
          |
        2 | from pandas.io.parsers import zqzqzq
          |                               ^^^^^^
        3 | __all__ = ['zqzqzq']
          |
        info: Function zqzqzq

        info[all-symbols]: AllSymbolInfo
         --> pandas/io/parsers/__init__.py:2:39
          |
        2 | from pandas.io.parsers.readers import zqzqzq
          |                                       ^^^^^^
        3 | __all__ = ['zqzqzq']
          |
        info: Function zqzqzq
        ");
    }

    /// This test inverts `pandas_read_csv_merged_all` such that
    /// the original definition is at the top-level and the re-exports
    /// are beneath it.
    ///
    /// In this case, we drop all of the re-exports and retain the
    /// original definition (because it is at the top of the package
    /// hierarchy).
    #[test]
    fn redundant_reexported_merged_definition_at_top_level() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source(
                "pandas/__init__.py",
                "
def zqzqzq():
    pass
",
            )
            .source("pandas/io/__init__.py", "")
            .source(
                "pandas/io/api.py",
                "
from pandas import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source(
                "pandas/io/parsers/__init__.py",
                "
from pandas.io.api import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .source(
                "pandas/io/parsers/readers.py",
                "
from pandas.io.parsers import zqzqzq
__all__ = ['zqzqzq']
",
            )
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/__init__.py:2:5
          |
        2 | def zqzqzq():
          |     ^^^^^^
        3 |     pass
          |
        info: Function zqzqzq
        ");
    }

    /// This test ensures that we don't drop re-exports when
    /// their top-level modules are different. That is, we only
    /// consider re-exports redundant when they are within the
    /// same package hierarchy.
    #[test]
    fn redundant_reexported_not_merged_across_top_level_modules() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source(
                "pandas/__init__.py",
                "
def zqzqzq():
    pass
",
            )
            .source("sub1/__init__.py", "")
            .source("sub1/sub2/__init__.py", "")
            .source("sub1/sub2/sub3.py", "from pandas import zqzqzq as zqzqzq")
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/__init__.py:2:5
          |
        2 | def zqzqzq():
          |     ^^^^^^
        3 |     pass
          |
        info: Function zqzqzq

        info[all-symbols]: AllSymbolInfo
         --> sub1/sub2/sub3.py:1:30
          |
        1 | from pandas import zqzqzq as zqzqzq
          |                              ^^^^^^
          |
        info: Function zqzqzq
        ");
    }

    /// This test is like `redundant_reexported_not_merged_across_top_level_modules`,
    /// but it includes an additional re-export with a module root
    /// (`sub1`) different from the original definition (`pandas`).
    ///
    /// At time of writing (2026-01-13), the redundant re-exports from
    /// `sub1` are not merged together. Arguably they should be, but it
    /// requires more work to get right.
    #[test]
    fn redundant_reexported_some_merged_across_top_level_modules() {
        let test = CursorTest::builder()
            .source("main.py", "<CURSOR>")
            .source(
                "pandas/__init__.py",
                "
def zqzqzq():
    pass
",
            )
            .source("sub1/__init__.py", "from pandas import zqzqzq as zqzqzq")
            .source("sub1/sub2/__init__.py", "")
            .source("sub1/sub2/sub3.py", "from pandas import zqzqzq as zqzqzq")
            .build();

        assert_snapshot!(test.all_symbols("zqzqzq"), @r"
        info[all-symbols]: AllSymbolInfo
         --> pandas/__init__.py:2:5
          |
        2 | def zqzqzq():
          |     ^^^^^^
        3 |     pass
          |
        info: Function zqzqzq

        info[all-symbols]: AllSymbolInfo
         --> sub1/__init__.py:1:30
          |
        1 | from pandas import zqzqzq as zqzqzq
          |                              ^^^^^^
          |
        info: Function zqzqzq

        info[all-symbols]: AllSymbolInfo
         --> sub1/sub2/sub3.py:1:30
          |
        1 | from pandas import zqzqzq as zqzqzq
          |                              ^^^^^^
          |
        info: Function zqzqzq
        ");
    }

    #[test]
    fn all_symbols_multi_file() {
        // We use odd symbol names here so that we can
        // write queries that target them specifically
        // and (hopefully) nothing else.
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def abcdefghijklmnop():
    '''A helpful utility function'''
    pass
",
            )
            .source(
                "models.py",
                "
class Abcdefghijklmnop:
    '''A data model class'''
    def __init__(self):
        pass
",
            )
            .source(
                "constants.py",
                "
ABCDEFGHIJKLMNOP = 'https://api.example.com'
<CURSOR>",
            )
            .build();

        assert_snapshot!(test.all_symbols("acegikmo"), @"
        info[all-symbols]: AllSymbolInfo
         --> constants.py:2:1
          |
        2 | ABCDEFGHIJKLMNOP = 'https://api.example.com'
          | ^^^^^^^^^^^^^^^^
          |
        info: Constant ABCDEFGHIJKLMNOP

        info[all-symbols]: AllSymbolInfo
         --> models.py:2:7
          |
        2 | class Abcdefghijklmnop:
          |       ^^^^^^^^^^^^^^^^
        3 |     '''A data model class'''
        4 |     def __init__(self):
          |
        info: Class Abcdefghijklmnop

        info[all-symbols]: AllSymbolInfo
         --> utils.py:2:5
          |
        2 | def abcdefghijklmnop():
          |     ^^^^^^^^^^^^^^^^
        3 |     '''A helpful utility function'''
        4 |     pass
          |
        info: Function abcdefghijklmnop
        ");
    }

    impl CursorTest {
        fn all_symbols(&self, query: &str) -> String {
            let symbols = all_symbols(&self.db, self.cursor.file, &QueryPattern::fuzzy(query));

            if symbols.is_empty() {
                return "No symbols found".to_string();
            }

            self.render_diagnostics(symbols.into_iter().map(|symbol_info| AllSymbolDiagnostic {
                db: &self.db,
                symbol_info,
            }))
        }
    }

    struct AllSymbolDiagnostic<'db> {
        db: &'db dyn Db,
        symbol_info: AllSymbolInfo<'db>,
    }

    impl IntoDiagnostic for AllSymbolDiagnostic<'_> {
        fn into_diagnostic(self) -> Diagnostic {
            let symbol_kind_str = self.symbol_info.kind().to_string();

            let info_text = format!(
                "{} {}",
                symbol_kind_str,
                self.symbol_info.name_in_file().unwrap_or_else(|| self
                    .symbol_info
                    .module()
                    .name(self.db)
                    .as_str())
            );

            let sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, info_text);

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("all-symbols")),
                Severity::Info,
                "AllSymbolInfo".to_string(),
            );

            let mut span = Span::from(self.symbol_info.file());
            if let Some(ref symbol) = self.symbol_info.symbol {
                span = span.with_range(symbol.name_range);
            }
            main.annotate(Annotation::primary(span));
            main.sub(sub);

            main
        }
    }
}

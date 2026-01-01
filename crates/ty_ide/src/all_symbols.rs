use ruff_db::files::File;
use ty_module_resolver::{Module, ModuleName, all_modules, resolve_real_shadowable_module};
use ty_project::Db;

use crate::{
    SymbolKind,
    symbols::{QueryPattern, SymbolInfo, symbols_for_file_global_only},
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
                    let symbols_for_file_span = tracing::debug_span!(parent: all_symbols_span, "symbols_for_file_global_only", ?file);
                    let _entered = symbols_for_file_span.entered();

                    if query.is_match_symbol_name(module.name(&*db)) {
                        results.lock().unwrap().push(AllSymbolInfo {
                            symbol: None,
                            module,
                            file,
                        });
                    }
                    for (_, symbol) in symbols_for_file_global_only(&*db, file).search(query) {
                        // It seems like we could do better here than
                        // locking `results` for every single symbol,
                        // but this works pretty well as it is.
                        results.lock().unwrap().push(AllSymbolInfo {
                            symbol: Some(symbol.to_owned()),
                            module,
                            file,
                        });
                    }
                });
            }
        });
    }

    let mut results = results.into_inner().unwrap();
    results.sort_by(|s1, s2| {
        let key1 = (
            s1.name_in_file()
                .unwrap_or_else(|| s1.module().name(db).as_str()),
            s1.file.path(db).as_str(),
        );
        let key2 = (
            s2.name_in_file()
                .unwrap_or_else(|| s2.module().name(db).as_str()),
            s2.file.path(db).as_str(),
        );
        key1.cmp(&key2)
    });
    results
}

/// A symbol found in the workspace and dependencies, including the
/// file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllSymbolInfo<'db> {
    /// The symbol information.
    ///
    /// When absent, this implies the symbol is the module itself.
    symbol: Option<SymbolInfo<'static>>,
    /// The module containing the symbol.
    module: Module<'db>,
    /// The file containing the symbol.
    ///
    /// This `File` is guaranteed to be the same
    /// as the `File` underlying `module`.
    file: File,
}

impl<'db> AllSymbolInfo<'db> {
    /// Returns the name of this symbol as it exists in a file.
    ///
    /// When absent, there is no concrete symbol in a module
    /// somewhere. Instead, this represents importing a module.
    /// In this case, if the caller needs a symbol name, they
    /// should use `AllSymbolInfo::module().name()`.
    pub fn name_in_file(&self) -> Option<&str> {
        self.symbol.as_ref().map(|symbol| &*symbol.name)
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

    #[test]
    fn test_all_symbols_multi_file() {
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

        assert_snapshot!(test.all_symbols("acegikmo"), @r"
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

use ruff_db::files::File;
use ty_project::Db;
use ty_python_semantic::{Module, ModuleName, all_modules, resolve_real_shadowable_module};

use crate::symbols::{QueryPattern, SymbolInfo, symbols_for_file_global_only};

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

    let typing_extensions = ModuleName::new("typing_extensions").unwrap();
    let is_typing_extensions_available = importing_from.is_stub(db)
        || resolve_real_shadowable_module(db, importing_from, &typing_extensions).is_some();

    let results = std::sync::Mutex::new(Vec::new());
    {
        let modules = all_modules(db);
        let db = db.dyn_clone();
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
                    for (_, symbol) in symbols_for_file_global_only(&*db, file).search(query) {
                        // It seems like we could do better here than
                        // locking `results` for every single symbol,
                        // but this works pretty well as it is.
                        results.lock().unwrap().push(AllSymbolInfo {
                            symbol: symbol.to_owned(),
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
        let key1 = (&s1.symbol.name, s1.file.path(db).as_str());
        let key2 = (&s2.symbol.name, s2.file.path(db).as_str());
        key1.cmp(&key2)
    });
    results
}

/// A symbol found in the workspace and dependencies, including the
/// file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllSymbolInfo<'db> {
    /// The symbol information.
    pub symbol: SymbolInfo<'static>,
    /// The module containing the symbol.
    pub module: Module<'db>,
    /// The file containing the symbol.
    ///
    /// This `File` is guaranteed to be the same
    /// as the `File` underlying `module`.
    pub file: File,
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

            self.render_diagnostics(symbols.into_iter().map(AllSymbolDiagnostic::new))
        }
    }

    struct AllSymbolDiagnostic<'db> {
        symbol_info: AllSymbolInfo<'db>,
    }

    impl<'db> AllSymbolDiagnostic<'db> {
        fn new(symbol_info: AllSymbolInfo<'db>) -> Self {
            Self { symbol_info }
        }
    }

    impl IntoDiagnostic for AllSymbolDiagnostic<'_> {
        fn into_diagnostic(self) -> Diagnostic {
            let symbol_kind_str = self.symbol_info.symbol.kind.to_string();

            let info_text = format!("{} {}", symbol_kind_str, self.symbol_info.symbol.name);

            let sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, info_text);

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("all-symbols")),
                Severity::Info,
                "AllSymbolInfo".to_string(),
            );
            main.annotate(Annotation::primary(
                Span::from(self.symbol_info.file).with_range(self.symbol_info.symbol.name_range),
            ));
            main.sub(sub);

            main
        }
    }
}

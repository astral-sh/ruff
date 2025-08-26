use crate::symbols::{QueryPattern, SymbolInfo, symbols_for_file_global_only};
use ruff_db::files::File;
use ty_project::Db;

/// Get all symbols matching the query string.
///
/// Returns symbols from all files in the workspace and dependencies, filtered
/// by the query.
pub fn all_symbols(db: &dyn Db, query: &str) -> Vec<AllSymbolInfo> {
    // If the query is empty, return immediately to avoid expensive file scanning
    if query.is_empty() {
        return Vec::new();
    }

    let project = db.project();

    let query = QueryPattern::new(query);
    let files = project.files(db);
    let results = std::sync::Mutex::new(Vec::new());
    {
        let db = db.dyn_clone();
        let files = &files;
        let results = &results;
        let query = &query;

        rayon::scope(move |s| {
            // For each file, extract symbols and add them to results
            for file in files.iter() {
                let db = db.dyn_clone();
                s.spawn(move |_| {
                    for (_, symbol) in symbols_for_file_global_only(&*db, *file).search(query) {
                        // It seems like we could do better here than
                        // locking `results` for every single symbol,
                        // but this works pretty well as it is.
                        results.lock().unwrap().push(AllSymbolInfo {
                            symbol: symbol.to_owned(),
                            file: *file,
                        });
                    }
                });
            }
        });
    }

    results.into_inner().unwrap()
}

/// A symbol found in the workspace and dependencies, including the
/// file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllSymbolInfo {
    /// The symbol information
    pub symbol: SymbolInfo<'static>,
    /// The file containing the symbol
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

        assert_snapshot!(test.all_symbols("ufunc"), @r"
        info[all-symbols]: AllSymbolInfo
         --> utils.py:2:5
          |
        2 | ABCDEFGHIJKLMNOP = 'https://api.example.com'
          | ^^^^^^^^^^^^^^^^
          |
        info: Function utility_function
        ");

        assert_snapshot!(test.all_symbols("data"), @r"
        info[all-symbols]: AllSymbolInfo
         --> models.py:2:7
          |
        2 | class Abcdefghijklmnop:
          |       ^^^^^^^^^^^^^^^^
        3 |     '''A data model class'''
        4 |     def __init__(self):
          |
        info: Class DataModel
        ");

        info[all-symbols]: AllSymbolInfo
         --> constants.py:2:1
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
            let symbols = all_symbols(&self.db, query);

            if symbols.is_empty() {
                return "No symbols found".to_string();
            }

            self.render_diagnostics(symbols.into_iter().map(AllSymbolDiagnostic::new))
        }
    }

    struct AllSymbolDiagnostic {
        symbol_info: AllSymbolInfo,
    }

    impl AllSymbolDiagnostic {
        fn new(symbol_info: AllSymbolInfo) -> Self {
            Self { symbol_info }
        }
    }

    impl IntoDiagnostic for AllSymbolDiagnostic {
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

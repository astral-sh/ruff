use crate::symbols::{QueryPattern, SymbolInfo, symbols_for_file_global_only};
use ruff_db::files::File;
use ty_project::Db;

/// Get all workspace symbols matching the query string.
/// Returns symbols from all files in the workspace, filtered by the query.
pub fn workspace_symbols(db: &dyn Db, query: &str) -> Vec<WorkspaceSymbolInfo> {
    // If the query is empty, return immediately to avoid expensive file scanning
    if query.is_empty() {
        return Vec::new();
    }

    let project = db.project();

    let query = QueryPattern::fuzzy(query);
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
                        results.lock().unwrap().push(WorkspaceSymbolInfo {
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

/// A symbol found in the workspace, including the file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSymbolInfo {
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
    fn test_workspace_symbols_multi_file() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def utility_function():
    '''A helpful utility function'''
    pass
",
            )
            .source(
                "models.py",
                "
class DataModel:
    '''A data model class'''
    def __init__(self):
        pass
",
            )
            .source(
                "constants.py",
                "
API_BASE_URL = 'https://api.example.com'
<CURSOR>",
            )
            .build();

        assert_snapshot!(test.workspace_symbols("ufunc"), @r"
        info[workspace-symbols]: WorkspaceSymbolInfo
         --> utils.py:2:5
          |
        2 | def utility_function():
          |     ^^^^^^^^^^^^^^^^
        3 |     '''A helpful utility function'''
        4 |     pass
          |
        info: Function utility_function
        ");

        assert_snapshot!(test.workspace_symbols("data"), @r"
        info[workspace-symbols]: WorkspaceSymbolInfo
         --> models.py:2:7
          |
        2 | class DataModel:
          |       ^^^^^^^^^
        3 |     '''A data model class'''
        4 |     def __init__(self):
          |
        info: Class DataModel
        ");

        assert_snapshot!(test.workspace_symbols("apibase"), @r"
        info[workspace-symbols]: WorkspaceSymbolInfo
         --> constants.py:2:1
          |
        2 | API_BASE_URL = 'https://api.example.com'
          | ^^^^^^^^^^^^
          |
        info: Constant API_BASE_URL
        ");
    }

    impl CursorTest {
        fn workspace_symbols(&self, query: &str) -> String {
            let symbols = workspace_symbols(&self.db, query);

            if symbols.is_empty() {
                return "No symbols found".to_string();
            }

            self.render_diagnostics(symbols.into_iter().map(WorkspaceSymbolDiagnostic::new))
        }
    }

    struct WorkspaceSymbolDiagnostic {
        symbol_info: WorkspaceSymbolInfo,
    }

    impl WorkspaceSymbolDiagnostic {
        fn new(symbol_info: WorkspaceSymbolInfo) -> Self {
            Self { symbol_info }
        }
    }

    impl IntoDiagnostic for WorkspaceSymbolDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let symbol_kind_str = self.symbol_info.symbol.kind.to_string();

            let info_text = format!("{} {}", symbol_kind_str, self.symbol_info.symbol.name);

            let sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, info_text);

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("workspace-symbols")),
                Severity::Info,
                "WorkspaceSymbolInfo".to_string(),
            );
            main.annotate(Annotation::primary(
                Span::from(self.symbol_info.file).with_range(self.symbol_info.symbol.name_range),
            ));
            main.sub(sub);

            main
        }
    }
}

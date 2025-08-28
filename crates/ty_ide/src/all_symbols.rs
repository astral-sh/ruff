use ruff_db::files::File;
use ty_project::Db;
use ty_python_semantic::all_modules;

use crate::symbols::{QueryPattern, SymbolInfo, symbols_for_file_global_only};

/// Get all symbols matching the query string.
///
/// Returns symbols from all files in the workspace and dependencies, filtered
/// by the query.
pub fn all_symbols(db: &dyn Db, query: &str) -> Vec<AllSymbolInfo> {
    // If the query is empty, return immediately to avoid expensive file scanning
    if query.is_empty() {
        return Vec::new();
    }

    let query = QueryPattern::new(query);
    let results = std::sync::Mutex::new(Vec::new());
    {
        let modules = all_modules(&*db);
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
                s.spawn(move |_| {
                    for (_, symbol) in symbols_for_file_global_only(&*db, file).search(query) {
                        // It seems like we could do better here than
                        // locking `results` for every single symbol,
                        // but this works pretty well as it is.
                        results.lock().unwrap().push(AllSymbolInfo {
                            symbol: symbol.to_owned(),
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

        assert_snapshot!(test.all_symbols("ulfunc"), @r#"
        info[all-symbols]: AllSymbolInfo
           --> stdlib/types.pyi:645:7
            |
        644 | @final
        645 | class BuiltinFunctionType:
            |       ^^^^^^^^^^^^^^^^^^^
        646 |     @property
        647 |     def __self__(self) -> object | ModuleType: ...
            |
        info: Class BuiltinFunctionType

        info[all-symbols]: AllSymbolInfo
          --> stdlib/trace.pyi:33:1
           |
        31 | _T = TypeVar("_T")
        32 | _P = ParamSpec("_P")
        33 | _FileModuleFunction: TypeAlias = tuple[str, str | None, str]
           | ^^^^^^^^^^^^^^^^^^^
        34 |
        35 | class CoverageResults:
           |
        info: Variable _FileModuleFunction

        info[all-symbols]: AllSymbolInfo
         --> utils.py:2:5
          |
        2 | def utility_function():
          |     ^^^^^^^^^^^^^^^^
        3 |     '''A helpful utility function'''
        4 |     pass
          |
        info: Function utility_function
        "#);

        assert_snapshot!(test.all_symbols("datamod"), @r#"
        info[all-symbols]: AllSymbolInfo
         --> models.py:2:7
          |
        2 | class DataModel:
          |       ^^^^^^^^^
        3 |     '''A data model class'''
        4 |     def __init__(self):
          |
        info: Class DataModel

        info[all-symbols]: AllSymbolInfo
          --> stdlib/py_compile.pyi:53:5
           |
        51 |     UNCHECKED_HASH = 3
        52 |
        53 | def _get_default_invalidation_mode() -> PycInvalidationMode: ...
           |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        54 | def compile(
        55 |     file: AnyStr,
           |
        info: Function _get_default_invalidation_mode

        info[all-symbols]: AllSymbolInfo
           --> stdlib/abc.pyi:139:9
            |
        138 | if sys.version_info >= (3, 10):
        139 |     def update_abstractmethods(cls: type[_T]) -> type[_T]:
            |         ^^^^^^^^^^^^^^^^^^^^^^
        140 |         """Recalculate the set of abstract methods of an abstract class.
            |
        info: Function update_abstractmethods
        "#);

        assert_snapshot!(test.all_symbols("apibase"), @r"
        info[all-symbols]: AllSymbolInfo
           --> stdlib/_ssl.pyi:313:1
            |
        311 | ALERT_DESCRIPTION_UNRECOGNIZED_NAME: Final = 112
        312 | ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE: Final = 113
        313 | ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE: Final = 114
            | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        314 | ALERT_DESCRIPTION_UNKNOWN_PSK_IDENTITY: Final = 115
            |
        info: Constant ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE

        info[all-symbols]: AllSymbolInfo
           --> stdlib/ssl.pyi:410:1
            |
        408 | ALERT_DESCRIPTION_ACCESS_DENIED: Final = AlertDescription.ALERT_DESCRIPTION_ACCESS_DENIED
        409 | ALERT_DESCRIPTION_BAD_CERTIFICATE: Final = AlertDescription.ALERT_DESCRIPTION_BAD_CERTIFICATE
        410 | ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE: Final = AlertDescription.ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE
            | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        411 | ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE: Final = AlertDescription.ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE
        412 | ALERT_DESCRIPTION_BAD_RECORD_MAC: Final = AlertDescription.ALERT_DESCRIPTION_BAD_RECORD_MAC
            |
        info: Constant ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE

        info[all-symbols]: AllSymbolInfo
           --> stdlib/_ssl.pyi:312:1
            |
        310 | ALERT_DESCRIPTION_CERTIFICATE_UNOBTAINABLE: Final = 111
        311 | ALERT_DESCRIPTION_UNRECOGNIZED_NAME: Final = 112
        312 | ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE: Final = 113
            | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        313 | ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE: Final = 114
        314 | ALERT_DESCRIPTION_UNKNOWN_PSK_IDENTITY: Final = 115
            |
        info: Constant ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE

        info[all-symbols]: AllSymbolInfo
           --> stdlib/ssl.pyi:411:1
            |
        409 | ALERT_DESCRIPTION_BAD_CERTIFICATE: Final = AlertDescription.ALERT_DESCRIPTION_BAD_CERTIFICATE
        410 | ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE: Final = AlertDescription.ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE
        411 | ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE: Final = AlertDescription.ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE
            | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        412 | ALERT_DESCRIPTION_BAD_RECORD_MAC: Final = AlertDescription.ALERT_DESCRIPTION_BAD_RECORD_MAC
        413 | ALERT_DESCRIPTION_CERTIFICATE_EXPIRED: Final = AlertDescription.ALERT_DESCRIPTION_CERTIFICATE_EXPIRED
            |
        info: Constant ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE

        info[all-symbols]: AllSymbolInfo
         --> constants.py:2:1
          |
        2 | API_BASE_URL = 'https://api.example.com'
          | ^^^^^^^^^^^^
          |
        info: Constant API_BASE_URL
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

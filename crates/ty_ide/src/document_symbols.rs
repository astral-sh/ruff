use crate::symbols::{FlatSymbols, symbols_for_file};
use ruff_db::files::File;
use ty_project::Db;

/// Get all document symbols for a file with the given options.
pub fn document_symbols(db: &dyn Db, file: File) -> &FlatSymbols {
    symbols_for_file(db, file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{HierarchicalSymbols, SymbolId, SymbolInfo};
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };

    #[test]
    fn test_document_symbols_simple() {
        let test = cursor_test(
            "
def hello():
    pass

class World:
    def method(self):
        pass
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @r"
        info[document-symbols]: SymbolInfo
         --> main.py:2:5
          |
        2 | def hello():
          |     ^^^^^
        3 |     pass
          |
        info: Function hello

        info[document-symbols]: SymbolInfo
         --> main.py:5:7
          |
        3 |     pass
        4 |
        5 | class World:
          |       ^^^^^
        6 |     def method(self):
        7 |         pass
          |
        info: Class World

        info[document-symbols]: SymbolInfo
         --> main.py:6:9
          |
        5 | class World:
        6 |     def method(self):
          |         ^^^^^^
        7 |         pass
          |
        info: Method method
        ");
    }

    #[test]
    fn test_document_symbols_complex() {
        let test = cursor_test(
            "
import os
from typing import List

CONSTANT = 42
variable = 'hello'
typed_global: str = 'typed'
annotated_only: int

class MyClass:
    class_var = 100
    typed_class_var: str = 'class_typed'
    annotated_class_var: float

    def __init__(self):
        self.instance_var = 0

    def public_method(self):
        return self.instance_var

    def _private_method(self):
        pass

def standalone_function():
    local_var = 10
    return local_var
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @r"
        info[document-symbols]: SymbolInfo
         --> main.py:5:1
          |
        3 | from typing import List
        4 |
        5 | CONSTANT = 42
          | ^^^^^^^^
        6 | variable = 'hello'
        7 | typed_global: str = 'typed'
          |
        info: Constant CONSTANT

        info[document-symbols]: SymbolInfo
         --> main.py:6:1
          |
        5 | CONSTANT = 42
        6 | variable = 'hello'
          | ^^^^^^^^
        7 | typed_global: str = 'typed'
        8 | annotated_only: int
          |
        info: Variable variable

        info[document-symbols]: SymbolInfo
         --> main.py:7:1
          |
        5 | CONSTANT = 42
        6 | variable = 'hello'
        7 | typed_global: str = 'typed'
          | ^^^^^^^^^^^^
        8 | annotated_only: int
          |
        info: Variable typed_global

        info[document-symbols]: SymbolInfo
          --> main.py:8:1
           |
         6 | variable = 'hello'
         7 | typed_global: str = 'typed'
         8 | annotated_only: int
           | ^^^^^^^^^^^^^^
         9 |
        10 | class MyClass:
           |
        info: Variable annotated_only

        info[document-symbols]: SymbolInfo
          --> main.py:10:7
           |
         8 | annotated_only: int
         9 |
        10 | class MyClass:
           |       ^^^^^^^
        11 |     class_var = 100
        12 |     typed_class_var: str = 'class_typed'
           |
        info: Class MyClass

        info[document-symbols]: SymbolInfo
          --> main.py:11:5
           |
        10 | class MyClass:
        11 |     class_var = 100
           |     ^^^^^^^^^
        12 |     typed_class_var: str = 'class_typed'
        13 |     annotated_class_var: float
           |
        info: Field class_var

        info[document-symbols]: SymbolInfo
          --> main.py:12:5
           |
        10 | class MyClass:
        11 |     class_var = 100
        12 |     typed_class_var: str = 'class_typed'
           |     ^^^^^^^^^^^^^^^
        13 |     annotated_class_var: float
           |
        info: Field typed_class_var

        info[document-symbols]: SymbolInfo
          --> main.py:13:5
           |
        11 |     class_var = 100
        12 |     typed_class_var: str = 'class_typed'
        13 |     annotated_class_var: float
           |     ^^^^^^^^^^^^^^^^^^^
        14 |
        15 |     def __init__(self):
           |
        info: Field annotated_class_var

        info[document-symbols]: SymbolInfo
          --> main.py:15:9
           |
        13 |     annotated_class_var: float
        14 |
        15 |     def __init__(self):
           |         ^^^^^^^^
        16 |         self.instance_var = 0
           |
        info: Constructor __init__

        info[document-symbols]: SymbolInfo
          --> main.py:18:9
           |
        16 |         self.instance_var = 0
        17 |
        18 |     def public_method(self):
           |         ^^^^^^^^^^^^^
        19 |         return self.instance_var
           |
        info: Method public_method

        info[document-symbols]: SymbolInfo
          --> main.py:21:9
           |
        19 |         return self.instance_var
        20 |
        21 |     def _private_method(self):
           |         ^^^^^^^^^^^^^^^
        22 |         pass
           |
        info: Method _private_method

        info[document-symbols]: SymbolInfo
          --> main.py:24:5
           |
        22 |         pass
        23 |
        24 | def standalone_function():
           |     ^^^^^^^^^^^^^^^^^^^
        25 |     local_var = 10
        26 |     return local_var
           |
        info: Function standalone_function
        ");
    }

    #[test]
    fn test_document_symbols_nested() {
        let test = cursor_test(
            "
class OuterClass:
    OUTER_CONSTANT = 100

    def outer_method(self):
        return self.OUTER_CONSTANT

    class InnerClass:
        def inner_method(self):
            pass
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @r"
        info[document-symbols]: SymbolInfo
         --> main.py:2:7
          |
        2 | class OuterClass:
          |       ^^^^^^^^^^
        3 |     OUTER_CONSTANT = 100
          |
        info: Class OuterClass

        info[document-symbols]: SymbolInfo
         --> main.py:3:5
          |
        2 | class OuterClass:
        3 |     OUTER_CONSTANT = 100
          |     ^^^^^^^^^^^^^^
        4 |
        5 |     def outer_method(self):
          |
        info: Constant OUTER_CONSTANT

        info[document-symbols]: SymbolInfo
         --> main.py:5:9
          |
        3 |     OUTER_CONSTANT = 100
        4 |
        5 |     def outer_method(self):
          |         ^^^^^^^^^^^^
        6 |         return self.OUTER_CONSTANT
          |
        info: Method outer_method

        info[document-symbols]: SymbolInfo
          --> main.py:8:11
           |
         6 |         return self.OUTER_CONSTANT
         7 |
         8 |     class InnerClass:
           |           ^^^^^^^^^^
         9 |         def inner_method(self):
        10 |             pass
           |
        info: Class InnerClass

        info[document-symbols]: SymbolInfo
          --> main.py:9:13
           |
         8 |     class InnerClass:
         9 |         def inner_method(self):
           |             ^^^^^^^^^^^^
        10 |             pass
           |
        info: Method inner_method
        ");
    }

    impl CursorTest {
        fn document_symbols(&self) -> String {
            let symbols = document_symbols(&self.db, self.cursor.file).to_hierarchical();

            if symbols.is_empty() {
                return "No symbols found".to_string();
            }

            self.render_diagnostics(symbols.iter().flat_map(|(id, symbol)| {
                symbol_to_diagnostics(&symbols, id, symbol, self.cursor.file)
            }))
        }
    }

    fn symbol_to_diagnostics<'db>(
        symbols: &'db HierarchicalSymbols,
        id: SymbolId,
        symbol: SymbolInfo<'db>,
        file: File,
    ) -> Vec<DocumentSymbolDiagnostic<'db>> {
        // Output the symbol and recursively output all child symbols
        let mut diagnostics = vec![DocumentSymbolDiagnostic::new(symbol, file)];

        for (child_id, child) in symbols.children(id) {
            diagnostics.extend(symbol_to_diagnostics(symbols, child_id, child, file));
        }

        diagnostics
    }
    struct DocumentSymbolDiagnostic<'db> {
        symbol: SymbolInfo<'db>,
        file: File,
    }

    impl<'db> DocumentSymbolDiagnostic<'db> {
        fn new(symbol: SymbolInfo<'db>, file: File) -> Self {
            Self { symbol, file }
        }
    }

    impl IntoDiagnostic for DocumentSymbolDiagnostic<'_> {
        fn into_diagnostic(self) -> Diagnostic {
            let symbol_kind_str = self.symbol.kind.to_string();

            let info_text = format!("{} {}", symbol_kind_str, self.symbol.name);

            let sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, info_text);

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("document-symbols")),
                Severity::Info,
                "SymbolInfo".to_string(),
            );
            main.annotate(Annotation::primary(
                Span::from(self.file).with_range(self.symbol.name_range),
            ));
            main.sub(sub);

            main
        }
    }
}

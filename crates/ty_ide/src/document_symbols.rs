use crate::symbols::{FlatSymbols, symbols_for_file};
use ty_project::Db;
use ty_python_core::ProgramFile;

/// Get all document symbols for a file with the given options.
pub fn document_symbols<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> &'db FlatSymbols {
    symbols_for_file(db, file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{HierarchicalSymbols, SymbolId, SymbolInfo, SymbolKind};
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };
    use ruff_db::files::File;

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

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:2:5
          |
        2 | def hello():
          |     ^^^^^
        info: Function hello

        info[document-symbols]: SymbolInfo
         --> main.py:5:7
          |
        5 | class World:
          |       ^^^^^
        info: Class World

        info[document-symbols]: SymbolInfo
         --> main.py:6:9
          |
        6 |     def method(self):
          |         ^^^^^^
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

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:5:1
          |
        5 | CONSTANT = 42
          | ^^^^^^^^
        info: Constant CONSTANT

        info[document-symbols]: SymbolInfo
         --> main.py:6:1
          |
        6 | variable = 'hello'
          | ^^^^^^^^
        info: Variable variable

        info[document-symbols]: SymbolInfo
         --> main.py:7:1
          |
        7 | typed_global: str = 'typed'
          | ^^^^^^^^^^^^
        info: Variable typed_global

        info[document-symbols]: SymbolInfo
         --> main.py:8:1
          |
        8 | annotated_only: int
          | ^^^^^^^^^^^^^^
        info: Variable annotated_only

        info[document-symbols]: SymbolInfo
          --> main.py:10:7
           |
        10 | class MyClass:
           |       ^^^^^^^
        info: Class MyClass

        info[document-symbols]: SymbolInfo
          --> main.py:11:5
           |
        11 |     class_var = 100
           |     ^^^^^^^^^
        info: Field class_var

        info[document-symbols]: SymbolInfo
          --> main.py:12:5
           |
        12 |     typed_class_var: str = 'class_typed'
           |     ^^^^^^^^^^^^^^^
        info: Field typed_class_var

        info[document-symbols]: SymbolInfo
          --> main.py:13:5
           |
        13 |     annotated_class_var: float
           |     ^^^^^^^^^^^^^^^^^^^
        info: Field annotated_class_var

        info[document-symbols]: SymbolInfo
          --> main.py:15:9
           |
        15 |     def __init__(self):
           |         ^^^^^^^^
        info: Constructor __init__

        info[document-symbols]: SymbolInfo
          --> main.py:18:9
           |
        18 |     def public_method(self):
           |         ^^^^^^^^^^^^^
        info: Method public_method

        info[document-symbols]: SymbolInfo
          --> main.py:21:9
           |
        21 |     def _private_method(self):
           |         ^^^^^^^^^^^^^^^
        info: Method _private_method

        info[document-symbols]: SymbolInfo
          --> main.py:24:5
           |
        24 | def standalone_function():
           |     ^^^^^^^^^^^^^^^^^^^
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

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:2:7
          |
        2 | class OuterClass:
          |       ^^^^^^^^^^
        info: Class OuterClass

        info[document-symbols]: SymbolInfo
         --> main.py:3:5
          |
        3 |     OUTER_CONSTANT = 100
          |     ^^^^^^^^^^^^^^
        info: Constant OUTER_CONSTANT

        info[document-symbols]: SymbolInfo
         --> main.py:5:9
          |
        5 |     def outer_method(self):
          |         ^^^^^^^^^^^^
        info: Method outer_method

        info[document-symbols]: SymbolInfo
         --> main.py:8:11
          |
        8 |     class InnerClass:
          |           ^^^^^^^^^^
        info: Class InnerClass

        info[document-symbols]: SymbolInfo
         --> main.py:9:13
          |
        9 |         def inner_method(self):
          |             ^^^^^^^^^^^^
        info: Method inner_method
        ");
    }

    #[test]
    fn test_document_symbols_type_alias() {
        let test = cursor_test(
            "
type IntList = list[int]

class Aliases:
    type Item = int
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:2:6
          |
        2 | type IntList = list[int]
          |      ^^^^^^^
        info: Variable IntList

        info[document-symbols]: SymbolInfo
         --> main.py:4:7
          |
        4 | class Aliases:
          |       ^^^^^^^
        info: Class Aliases

        info[document-symbols]: SymbolInfo
         --> main.py:5:10
          |
        5 |     type Item = int
          |          ^^^^
        info: Variable Item
        ");
    }

    #[test]
    fn document_symbols_with_statement_targets() {
        let test = cursor_test(
            "
from contextlib import nullcontext

with nullcontext() as module_target, nullcontext((1, 2)) as (left, right):
    body_target = 1

class C:
    with nullcontext() as class_target:
        body_field = 1

def function():
    with nullcontext() as local_target:
        pass
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:4:23
          |
        4 | with nullcontext() as module_target, nullcontext((1, 2)) as (left, right):
          |                       ^^^^^^^^^^^^^
        info: Variable module_target

        info[document-symbols]: SymbolInfo
         --> main.py:4:62
          |
        4 | with nullcontext() as module_target, nullcontext((1, 2)) as (left, right):
          |                                                              ^^^^
        info: Variable left

        info[document-symbols]: SymbolInfo
         --> main.py:4:68
          |
        4 | with nullcontext() as module_target, nullcontext((1, 2)) as (left, right):
          |                                                                    ^^^^^
        info: Variable right

        info[document-symbols]: SymbolInfo
         --> main.py:5:5
          |
        5 |     body_target = 1
          |     ^^^^^^^^^^^
        info: Variable body_target

        info[document-symbols]: SymbolInfo
         --> main.py:7:7
          |
        7 | class C:
          |       ^
        info: Class C

        info[document-symbols]: SymbolInfo
         --> main.py:8:27
          |
        8 |     with nullcontext() as class_target:
          |                           ^^^^^^^^^^^^
        info: Field class_target

        info[document-symbols]: SymbolInfo
         --> main.py:9:9
          |
        9 |         body_field = 1
          |         ^^^^^^^^^^
        info: Field body_field

        info[document-symbols]: SymbolInfo
          --> main.py:11:5
           |
        11 | def function():
           |     ^^^^^^^^
        info: Function function
        ");
    }

    #[test]
    fn document_symbols_augmented_assignment_targets() {
        let test = cursor_test(
            "
items = [1]
items[(index := 0)] += 1
(obj := factory()).value += 1
items += (rhs := [1])
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:2:1
          |
        2 | items = [1]
          | ^^^^^
        info: Variable items

        info[document-symbols]: SymbolInfo
         --> main.py:3:8
          |
        3 | items[(index := 0)] += 1
          |        ^^^^^
        info: Variable index

        info[document-symbols]: SymbolInfo
         --> main.py:4:2
          |
        4 | (obj := factory()).value += 1
          |  ^^^
        info: Variable obj

        info[document-symbols]: SymbolInfo
         --> main.py:5:11
          |
        5 | items += (rhs := [1])
          |           ^^^
        info: Variable rhs
        ");
    }

    #[test]
    fn document_symbols_store_context_targets() {
        let test = cursor_test(
            "
first, *rest, LAST = values

for loop_left, [loop_right, *loop_rest] in rows:
    loop_body = 1

with manager() as [with_left, *with_rest], manager() as WITH_CONSTANT:
    with_body = 1

captured = (walrus := 1)

def function():
    function_local = 1
    with manager() as function_target:
        pass
<CURSOR>",
        );

        let symbols = document_symbols(&test.db, test.program_file(test.cursor.file))
            .iter()
            .map(|(_, symbol)| (symbol.name.into_owned(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            symbols,
            [
                ("first", SymbolKind::Variable),
                ("rest", SymbolKind::Variable),
                ("LAST", SymbolKind::Constant),
                ("loop_left", SymbolKind::Variable),
                ("loop_right", SymbolKind::Variable),
                ("loop_rest", SymbolKind::Variable),
                ("loop_body", SymbolKind::Variable),
                ("with_left", SymbolKind::Variable),
                ("with_rest", SymbolKind::Variable),
                ("WITH_CONSTANT", SymbolKind::Constant),
                ("with_body", SymbolKind::Variable),
                ("captured", SymbolKind::Variable),
                ("walrus", SymbolKind::Variable),
                ("function", SymbolKind::Function),
            ]
            .map(|(name, kind)| (name.to_owned(), kind))
        );
    }

    #[test]
    fn document_symbols_comprehension_and_lambda_scopes() {
        let test = cursor_test(
            "
result = [item for item in values if (leaked := item)]
generator = (other for other in values)
lambda_value = lambda: (lambda_local := 1)
<CURSOR>",
        );

        let names = document_symbols(&test.db, test.program_file(test.cursor.file))
            .iter()
            .map(|(_, symbol)| symbol.name.into_owned())
            .collect::<Vec<_>>();

        assert_eq!(names, ["result", "leaked", "generator", "lambda_value"]);
    }

    #[test]
    fn document_symbols_function_and_class_header_bindings() {
        let test = cursor_test(
            "
@(function_decorator := decorate)
def function(value=(default_value := 1)):
    function_local = 1

@(class_decorator := decorate)
class Example((class_base := Base)):
    class_field = 1
<CURSOR>",
        );

        let symbols = document_symbols(&test.db, test.program_file(test.cursor.file))
            .iter()
            .map(|(_, symbol)| (symbol.name.into_owned(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            symbols,
            [
                ("function_decorator", SymbolKind::Variable),
                ("function", SymbolKind::Function),
                ("default_value", SymbolKind::Variable),
                ("class_decorator", SymbolKind::Variable),
                ("Example", SymbolKind::Class),
                ("class_base", SymbolKind::Variable),
                ("class_field", SymbolKind::Field),
            ]
            .map(|(name, kind)| (name.to_owned(), kind))
        );
    }

    impl CursorTest {
        fn document_symbols(&self) -> String {
            let symbols =
                document_symbols(&self.db, self.program_file(self.cursor.file)).to_hierarchical();

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

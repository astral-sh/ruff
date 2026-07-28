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
    fn document_symbols_for_statement_targets() {
        let test = cursor_test(
            "
for module_target in ():
    body_target = 1
else:
    fallback_target = 2

for [left, *middle, right] in ():
    pass

class C:
    for class_target in ():
        body_field = 1

def function():
    for local_target in ():
        pass
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @"
        info[document-symbols]: SymbolInfo
         --> main.py:2:5
          |
        2 | for module_target in ():
          |     ^^^^^^^^^^^^^
        info: Variable module_target

        info[document-symbols]: SymbolInfo
         --> main.py:3:5
          |
        3 |     body_target = 1
          |     ^^^^^^^^^^^
        info: Variable body_target

        info[document-symbols]: SymbolInfo
         --> main.py:5:5
          |
        5 |     fallback_target = 2
          |     ^^^^^^^^^^^^^^^
        info: Variable fallback_target

        info[document-symbols]: SymbolInfo
         --> main.py:7:6
          |
        7 | for [left, *middle, right] in ():
          |      ^^^^
        info: Variable left

        info[document-symbols]: SymbolInfo
         --> main.py:7:13
          |
        7 | for [left, *middle, right] in ():
          |             ^^^^^^
        info: Variable middle

        info[document-symbols]: SymbolInfo
         --> main.py:7:21
          |
        7 | for [left, *middle, right] in ():
          |                     ^^^^^
        info: Variable right

        info[document-symbols]: SymbolInfo
          --> main.py:10:7
           |
        10 | class C:
           |       ^
        info: Class C

        info[document-symbols]: SymbolInfo
          --> main.py:11:9
           |
        11 |     for class_target in ():
           |         ^^^^^^^^^^^^
        info: Field class_target

        info[document-symbols]: SymbolInfo
          --> main.py:12:9
           |
        12 |         body_field = 1
           |         ^^^^^^^^^^
        info: Field body_field

        info[document-symbols]: SymbolInfo
          --> main.py:14:5
           |
        14 | def function():
           |     ^^^^^^^^
        info: Function function
        ");
    }

    #[test]
    fn document_symbols_match_pattern_bindings() {
        let test = cursor_test(
            "
match subject:
    case [first, *middle, last] as sequence:
        body_target = 1
    case {\"key\": mapping_value, **remaining}:
        fallback_target = 2
    case Point(positional, named=keyword):
        pass
    case (0 as alternative) | (1 as alternative):
        pass

class C:
    match subject:
        case class_capture:
            body_field = 1

def function():
    match subject:
        case local_capture:
            pass
<CURSOR>",
        );

        assert_snapshot!(test.document_symbols(), @r#"
        info[document-symbols]: SymbolInfo
         --> main.py:3:11
          |
        3 |     case [first, *middle, last] as sequence:
          |           ^^^^^
        info: Variable first

        info[document-symbols]: SymbolInfo
         --> main.py:3:19
          |
        3 |     case [first, *middle, last] as sequence:
          |                   ^^^^^^
        info: Variable middle

        info[document-symbols]: SymbolInfo
         --> main.py:3:27
          |
        3 |     case [first, *middle, last] as sequence:
          |                           ^^^^
        info: Variable last

        info[document-symbols]: SymbolInfo
         --> main.py:3:36
          |
        3 |     case [first, *middle, last] as sequence:
          |                                    ^^^^^^^^
        info: Variable sequence

        info[document-symbols]: SymbolInfo
         --> main.py:4:9
          |
        4 |         body_target = 1
          |         ^^^^^^^^^^^
        info: Variable body_target

        info[document-symbols]: SymbolInfo
         --> main.py:5:18
          |
        5 |     case {"key": mapping_value, **remaining}:
          |                  ^^^^^^^^^^^^^
        info: Variable mapping_value

        info[document-symbols]: SymbolInfo
         --> main.py:5:35
          |
        5 |     case {"key": mapping_value, **remaining}:
          |                                   ^^^^^^^^^
        info: Variable remaining

        info[document-symbols]: SymbolInfo
         --> main.py:6:9
          |
        6 |         fallback_target = 2
          |         ^^^^^^^^^^^^^^^
        info: Variable fallback_target

        info[document-symbols]: SymbolInfo
         --> main.py:7:16
          |
        7 |     case Point(positional, named=keyword):
          |                ^^^^^^^^^^
        info: Variable positional

        info[document-symbols]: SymbolInfo
         --> main.py:7:34
          |
        7 |     case Point(positional, named=keyword):
          |                                  ^^^^^^^
        info: Variable keyword

        info[document-symbols]: SymbolInfo
         --> main.py:9:16
          |
        9 |     case (0 as alternative) | (1 as alternative):
          |                ^^^^^^^^^^^
        info: Variable alternative

        info[document-symbols]: SymbolInfo
         --> main.py:9:37
          |
        9 |     case (0 as alternative) | (1 as alternative):
          |                                     ^^^^^^^^^^^
        info: Variable alternative

        info[document-symbols]: SymbolInfo
          --> main.py:12:7
           |
        12 | class C:
           |       ^
        info: Class C

        info[document-symbols]: SymbolInfo
          --> main.py:14:14
           |
        14 |         case class_capture:
           |              ^^^^^^^^^^^^^
        info: Field class_capture

        info[document-symbols]: SymbolInfo
          --> main.py:15:13
           |
        15 |             body_field = 1
           |             ^^^^^^^^^^
        info: Field body_field

        info[document-symbols]: SymbolInfo
          --> main.py:17:5
           |
        17 | def function():
           |     ^^^^^^^^
        info: Function function
        "#);
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

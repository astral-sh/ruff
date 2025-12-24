use crate::goto::find_goto_target;
use crate::references::{ReferencesMode, references};
use crate::{Db, ReferenceTarget};
use ruff_db::files::File;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::SemanticModel;

/// Returns the range of the symbol if it can be renamed, None if not.
pub fn can_rename(db: &dyn Db, file: File, offset: TextSize) -> Option<ruff_text_size::TextRange> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);

    // Get the definitions for the symbol at the offset
    let goto_target = find_goto_target(&model, &module, offset)?;

    // Don't allow renaming of import module components
    if matches!(
        goto_target,
        crate::goto::GotoTarget::ImportModuleComponent { .. }
    ) {
        return None;
    }

    let current_file_in_project = is_file_in_project(db, file);

    let definition_targets = goto_target
        .get_definition_targets(&model, ReferencesMode::Rename.to_import_alias_resolution())?
        .declaration_targets(db)?;

    for target in &definition_targets {
        let target_file = target.file();

        // If definition is outside the project, refuse rename
        if !is_file_in_project(db, target_file) {
            return None;
        }

        // If current file is not in project and any definition is outside current file, refuse rename
        if !current_file_in_project && target_file != file {
            return None;
        }
    }

    Some(goto_target.range())
}

/// Perform a rename operation on the symbol at the given position.
/// Returns all locations that need to be updated with the new name.
pub fn rename(
    db: &dyn Db,
    file: File,
    offset: TextSize,
    new_name: &str,
) -> Option<Vec<ReferenceTarget>> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);

    // Get the definitions for the symbol at the offset
    let goto_target = find_goto_target(&model, &module, offset)?;

    // Clients shouldn't call us with an empty new name, but just in case...
    if new_name.is_empty() {
        return None;
    }

    // Determine if we should do a multi-file rename or single-file rename
    // based on whether the current file is part of the project
    let current_file_in_project = is_file_in_project(db, file);

    // Choose the appropriate rename mode:
    // - If current file is in project, do multi-file rename
    // - If current file is not in project, limit to single-file rename
    let rename_mode = if current_file_in_project {
        ReferencesMode::RenameMultiFile
    } else {
        ReferencesMode::Rename
    };

    // Find all references that need to be renamed
    references(db, file, &goto_target, rename_mode)
}

/// Helper function to check if a file is included in the project.
fn is_file_in_project(db: &dyn Db, file: File) -> bool {
    file.path(db).is_system_virtual_path() || db.project().files(db).contains(&file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    impl CursorTest {
        fn prepare_rename(&self) -> String {
            let Some(range) = salsa::attach(&self.db, || {
                can_rename(&self.db, self.cursor.file, self.cursor.offset)
            }) else {
                return "Cannot rename".to_string();
            };

            format!("Can rename symbol at range {range:?}")
        }

        fn rename(&self, new_name: &str) -> String {
            let rename_results = salsa::attach(&self.db, || {
                can_rename(&self.db, self.cursor.file, self.cursor.offset)?;

                rename(&self.db, self.cursor.file, self.cursor.offset, new_name)
            });

            let Some(rename_results) = rename_results else {
                return "Cannot rename".to_string();
            };

            if rename_results.is_empty() {
                return "No locations to rename".to_string();
            }

            // Create a single diagnostic with multiple annotations
            let rename_diagnostic = RenameResultSet {
                locations: rename_results
                    .into_iter()
                    .map(|ref_item| FileRange::new(ref_item.file(), ref_item.range()))
                    .collect(),
            };

            self.render_diagnostics([rename_diagnostic])
        }
    }

    struct RenameResultSet {
        locations: Vec<FileRange>,
    }

    impl IntoDiagnostic for RenameResultSet {
        fn into_diagnostic(self) -> Diagnostic {
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("rename")),
                Severity::Info,
                format!("Rename symbol (found {} locations)", self.locations.len()),
            );

            // Add the first location as primary annotation (the symbol being renamed)
            if let Some(first_location) = self.locations.first() {
                main.annotate(Annotation::primary(
                    Span::from(first_location.file()).with_range(first_location.range()),
                ));

                // Add remaining locations as secondary annotations
                for location in &self.locations[1..] {
                    main.annotate(Annotation::secondary(
                        Span::from(location.file()).with_range(location.range()),
                    ));
                }
            }

            main
        }
    }

    #[test]
    fn prepare_rename_parameter() {
        let test = cursor_test(
            "
def func(<CURSOR>value: int) -> int:
    value *= 2
    return value

value = 0
",
        );

        assert_snapshot!(test.prepare_rename(), @"Can rename symbol at range 10..15");
    }

    #[test]
    fn rename_parameter() {
        let test = cursor_test(
            "
def func(<CURSOR>value: int) -> int:
    value *= 2
    return value

func(value=42)
",
        );

        assert_snapshot!(test.rename("number"), @r"
        info[rename]: Rename symbol (found 4 locations)
         --> main.py:2:10
          |
        2 | def func(value: int) -> int:
          |          ^^^^^
        3 |     value *= 2
          |     -----
        4 |     return value
          |            -----
        5 |
        6 | func(value=42)
          |      -----
          |
        ");
    }

    #[test]
    fn rename_function() {
        let test = cursor_test(
            "
def fu<CURSOR>nc():
    pass

result1 = func()
x = func
",
        );

        assert_snapshot!(test.rename("calculate"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:5
          |
        2 | def func():
          |     ^^^^
        3 |     pass
        4 |
        5 | result1 = func()
          |           ----
        6 | x = func
          |     ----
          |
        ");
    }

    #[test]
    fn rename_class() {
        let test = cursor_test(
            "
class My<CURSOR>Class:
    def __init__(self):
        pass

obj1 = MyClass()
cls = MyClass
",
        );

        assert_snapshot!(test.rename("MyNewClass"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         pass
        5 |
        6 | obj1 = MyClass()
          |        -------
        7 | cls = MyClass
          |       -------
          |
        ");
    }

    #[test]
    fn rename_invalid_name() {
        let test = cursor_test(
            "
def fu<CURSOR>nc():
    pass
",
        );

        assert_snapshot!(test.rename(""), @"Cannot rename");
        assert_snapshot!(test.rename("valid_name"), @r"
        info[rename]: Rename symbol (found 1 locations)
         --> main.py:2:5
          |
        2 | def func():
          |     ^^^^
        3 |     pass
          |
        ");
    }

    #[test]
    fn multi_file_function_rename() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def fu<CURSOR>nc(x):
    return x * 2
",
            )
            .source(
                "module.py",
                "
from utils import func

def test(data):
    return func(data)
",
            )
            .source(
                "app.py",
                "
from utils import helper_function

class DataProcessor:
    def __init__(self):
        self.multiplier = helper_function

    def process(self, value):
        return helper_function(value)
",
            )
            .build();

        assert_snapshot!(test.rename("utility_function"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> utils.py:2:5
          |
        2 | def func(x):
          |     ^^^^
        3 |     return x * 2
          |
         ::: module.py:2:19
          |
        2 | from utils import func
          |                   ----
        3 |
        4 | def test(data):
        5 |     return func(data)
          |            ----
          |
        ");
    }

    #[test]
    fn rename_string_annotation1() {
        let test = cursor_test(
            r#"
        a: "MyCla<CURSOR>ss" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:5
          |
        2 | a: "MyClass" = 1
          |     ^^^^^^^
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn rename_string_annotation2() {
        let test = cursor_test(
            r#"
        a: "None | MyCl<CURSOR>ass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn rename_string_annotation3() {
        let test = cursor_test(
            r#"
        a: "None |<CURSOR> MyClass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @"Cannot rename");
    }

    #[test]
    fn rename_string_annotation4() {
        let test = cursor_test(
            r#"
        a: "None | MyClass<CURSOR>" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn rename_string_annotation5() {
        let test = cursor_test(
            r#"
        a: "None | MyClass"<CURSOR> = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @"Cannot rename");
    }

    #[test]
    fn rename_string_annotation_dangling1() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass |" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @"Cannot rename");
    }

    #[test]
    fn rename_string_annotation_dangling2() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass | No" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:5
          |
        2 | a: "MyClass | No" = 1
          |     ^^^^^^^
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn rename_string_annotation_dangling3() {
        let test = cursor_test(
            r#"
        a: "MyClass | N<CURSOR>o" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.rename("MyNewClass"), @"Cannot rename");
    }

    #[test]
    fn rename_match_name_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:4:22
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ab]:
          |                      ^^
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn rename_match_name_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:4:22
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ab]:
          |                      ^^
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn rename_match_rest_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:4:23
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", *ab]:
          |                       ^^
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn rename_match_rest_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:4:23
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", *ab]:
          |                       ^^
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn rename_match_as_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:4:37
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     ^^
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn rename_match_as_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:4:37
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     ^^
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn rename_match_keyword_stmt() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=a<CURSOR>b):
                        x = ab
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 2 locations)
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |                              ^^
        11 |             x = ab
           |                 --
           |
        ");
    }

    #[test]
    fn rename_match_keyword_binding() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=ab):
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 2 locations)
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |                              ^^
        11 |             x = ab
           |                 --
           |
        ");
    }

    #[test]
    fn rename_match_class_name() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Cl<CURSOR>ick(x, button=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r#"
        info[rename]: Rename symbol (found 3 locations)
          --> main.py:2:7
           |
         2 | class Click:
           |       ^^^^^
         3 |     __match_args__ = ("position", "button")
         4 |     def __init__(self, pos, btn):
           |
          ::: main.py:8:20
           |
         6 |         self.button: str = btn
         7 |
         8 | def my_func(event: Click):
           |                    -----
         9 |     match event:
        10 |         case Click(x, button=ab):
           |              -----
        11 |             x = ab
           |
        "#);
    }

    #[test]
    fn rename_match_class_field_name() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, but<CURSOR>ton=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.rename("XY"), @"Cannot rename");
    }

    #[test]
    fn rename_typevar_name_stmt() {
        let test = cursor_test(
            r#"
            type Alias1[A<CURSOR>B: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             ^^                      --       --
          |
        ");
    }

    #[test]
    fn rename_typevar_name_binding() {
        let test = cursor_test(
            r#"
            type Alias1[AB: int = bool] = tuple[A<CURSOR>B, list[AB]]
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             ^^                      --       --
          |
        ");
    }

    #[test]
    fn rename_typevar_spec_stmt() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**A<CURSOR>B = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:3:15
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               ^^                          --        --
          |
        ");
    }

    #[test]
    fn rename_typevar_spec_binding() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[A<CURSOR>B, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:3:15
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               ^^                          --        --
          |
        ");
    }

    #[test]
    fn rename_typevar_tuple_stmt() {
        let test = cursor_test(
            r#"
            type Alias3[*A<CURSOR>B = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              ^^                      --          --
          |
        ");
    }

    #[test]
    fn rename_typevar_tuple_binding() {
        let test = cursor_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*A<CURSOR>B], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.rename("XY"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              ^^                      --          --
          |
        ");
    }

    #[test]
    fn cannot_rename_import_module_component() {
        // Test that we cannot rename parts of module names in import statements
        let test = cursor_test(
            "
import <CURSOR>os.path
x = os.path.join('a', 'b')
",
        );

        assert_snapshot!(test.prepare_rename(), @"Cannot rename");
    }

    #[test]
    fn cannot_rename_from_import_module_component() {
        // Test that we cannot rename parts of module names in from import statements
        let test = cursor_test(
            "
from os.<CURSOR>path import join
result = join('a', 'b')
",
        );

        assert_snapshot!(test.prepare_rename(), @"Cannot rename");
    }

    #[test]
    fn cannot_rename_external_file() {
        // This test verifies that we cannot rename a symbol when it's defined in a file
        // that's outside the project (like a standard library function)
        let test = cursor_test(
            "
import os
x = <CURSOR>os.path.join('a', 'b')
",
        );

        assert_snapshot!(test.prepare_rename(), @"Cannot rename");
    }

    #[test]
    fn rename_alias_at_import_statement() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def test(): pass
",
            )
            .source(
                "main.py",
                "
from utils import test as <CURSOR>alias
result = alias()
",
            )
            .build();

        assert_snapshot!(test.rename("new_alias"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:27
          |
        2 | from utils import test as alias
          |                           ^^^^^
        3 | result = alias()
          |          -----
          |
        ");
    }

    #[test]
    fn rename_alias_at_usage_site() {
        // Test renaming an alias when the cursor is on the alias in the usage statement
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def test(): pass
",
            )
            .source(
                "main.py",
                "
from utils import test as alias
result = <CURSOR>alias()
",
            )
            .build();

        assert_snapshot!(test.rename("new_alias"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:27
          |
        2 | from utils import test as alias
          |                           ^^^^^
        3 | result = alias()
          |          -----
          |
        ");
    }

    #[test]
    fn rename_across_import_chain_with_mixed_aliases() {
        // Test renaming a symbol that's imported across multiple files with mixed alias patterns
        // File 1 (source.py): defines the original function
        // File 2 (middle.py): imports without alias from source.py
        // File 3 (consumer.py): imports with alias from middle.py
        let test = CursorTest::builder()
            .source(
                "source.py",
                "
def original_func<CURSOR>tion():
    return 'Hello from source'
",
            )
            .source(
                "middle.py",
                "
from source import original_function

def wrapper():
    return original_function()

result = original_function()
",
            )
            .source(
                "consumer.py",
                "
from middle import original_function as func_alias

def process():
    return func_alias()

value1 = func_alias()
",
            )
            .build();

        assert_snapshot!(test.rename("renamed_function"), @r"
        info[rename]: Rename symbol (found 5 locations)
         --> source.py:2:5
          |
        2 | def original_function():
          |     ^^^^^^^^^^^^^^^^^
        3 |     return 'Hello from source'
          |
         ::: consumer.py:2:20
          |
        2 | from middle import original_function as func_alias
          |                    -----------------
        3 |
        4 | def process():
          |
         ::: middle.py:2:20
          |
        2 | from source import original_function
          |                    -----------------
        3 |
        4 | def wrapper():
        5 |     return original_function()
          |            -----------------
        6 |
        7 | result = original_function()
          |          -----------------
          |
        ");
    }

    #[test]
    fn rename_alias_in_import_chain() {
        let test = CursorTest::builder()
            .source(
                "file1.py",
                "
def func1(): pass
",
            )
            .source(
                "file2.py",
                "
from file1 import func1 as func2

func2()
",
            )
            .source(
                "file3.py",
                "
from file2 import func2

class App:
    def run(self):
        return fu<CURSOR>nc2()
",
            )
            .build();

        assert_snapshot!(test.rename("new_util_name"), @r"
        info[rename]: Rename symbol (found 4 locations)
         --> file3.py:2:19
          |
        2 | from file2 import func2
          |                   ^^^^^
        3 |
        4 | class App:
        5 |     def run(self):
        6 |         return func2()
          |                -----
          |
         ::: file2.py:2:28
          |
        2 | from file1 import func1 as func2
          |                            -----
        3 |
        4 | func2()
          | -----
          |
        ");
    }

    #[test]
    fn cannot_rename_keyword() {
        // Test that we cannot rename Python keywords like "None"
        let test = cursor_test(
            "
def process_value(value):
    if value is <CURSOR>None:
        return 'empty'
    return str(value)
",
        );

        assert_snapshot!(test.prepare_rename(), @"Cannot rename");
    }

    #[test]
    fn cannot_rename_builtin_type() {
        // Test that we cannot rename Python builtin types like "int"
        let test = cursor_test(
            "
def convert_to_number(value):
    return <CURSOR>int(value)
",
        );

        assert_snapshot!(test.prepare_rename(), @"Cannot rename");
    }

    #[test]
    fn rename_keyword_argument() {
        // Test renaming a keyword argument and its corresponding parameter
        let test = cursor_test(
            "
def func(x, y=5):
    return x + y

result = func(10, <CURSOR>y=20)
",
        );

        assert_snapshot!(test.rename("z"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:13
          |
        2 | def func(x, y=5):
          |             ^
        3 |     return x + y
          |                -
        4 |
        5 | result = func(10, y=20)
          |                   -
          |
        ");
    }

    #[test]
    fn rename_parameter_with_keyword_argument() {
        // Test renaming a parameter and its corresponding keyword argument
        let test = cursor_test(
            "
def func(x, <CURSOR>y=5):
    return x + y

result = func(10, y=20)
",
        );

        assert_snapshot!(test.rename("z"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:13
          |
        2 | def func(x, y=5):
          |             ^
        3 |     return x + y
          |                -
        4 |
        5 | result = func(10, y=20)
          |                   -
          |
        ");
    }

    #[test]
    fn import_alias() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                import warnings
                import warnings as <CURSOR>abc

                x = abc
                y = warnings
            "#,
            )
            .build();

        assert_snapshot!(test.rename("z"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:3:20
          |
        2 | import warnings
        3 | import warnings as abc
          |                    ^^^
        4 |
        5 | x = abc
          |     ---
        6 | y = warnings
          |
        ");
    }

    #[test]
    fn import_alias_to_first_party_definition() {
        let test = CursorTest::builder()
            .source("lib.py", "def deprecated(): pass")
            .source(
                "main.py",
                r#"
                import lib as lib2<CURSOR>

                x = lib2
            "#,
            )
            .build();

        assert_snapshot!(test.rename("z"), @r"
            info[rename]: Rename symbol (found 2 locations)
             --> main.py:2:15
              |
            2 | import lib as lib2
              |               ^^^^
            3 |
            4 | x = lib2
              |     ----
              |
        ");
    }

    #[test]
    fn imported_first_party_definition() {
        let test = CursorTest::builder()
            .source("lib.py", "def deprecated(): pass")
            .source(
                "main.py",
                r#"
                from lib import deprecated<CURSOR>

                x = deprecated
            "#,
            )
            .build();

        assert_snapshot!(test.rename("z"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:17
          |
        2 | from lib import deprecated
          |                 ^^^^^^^^^^
        3 |
        4 | x = deprecated
          |     ----------
          |
         ::: lib.py:1:5
          |
        1 | def deprecated(): pass
          |     ----------
          |
        ");
    }

    #[test]
    fn import_alias_use() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                import warnings
                import warnings as abc

                x = abc<CURSOR>
                y = warnings
            "#,
            )
            .build();

        assert_snapshot!(test.rename("z"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:3:20
          |
        2 | import warnings
        3 | import warnings as abc
          |                    ^^^
        4 |
        5 | x = abc
          |     ---
        6 | y = warnings
          |
        ");
    }

    #[test]
    fn rename_submodule_import_from_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.submod import val

                x = sub<CURSOR>pkg
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // TODO(submodule-imports): we should refuse to rename this (it's the name of a module)
        assert_snapshot!(test.rename("mypkg"), @r"
        info[rename]: Rename symbol (found 1 locations)
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
        4 | x = subpkg
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn rename_submodule_import_from_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .sub<CURSOR>pkg.submod import val

                x = subpkg
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // Refusing to rename is correct
        assert_snapshot!(test.rename("mypkg"), @"Cannot rename");
    }

    #[test]
    fn rename_submodule_import_from_wrong_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.submod import val

                x = sub<CURSOR>mod
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // Refusing to rename is good/fine here, it's an undefined reference
        assert_snapshot!(test.rename("mypkg"), @"Cannot rename");
    }

    #[test]
    fn rename_submodule_import_from_wrong_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.sub<CURSOR>mod import val

                x = submod
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // Refusing to rename is good here, it's a module name
        assert_snapshot!(test.rename("mypkg"), @"Cannot rename");
    }

    #[test]
    fn rename_submodule_import_from_confusing_shadowed_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .sub<CURSOR>pkg import subpkg

                x = subpkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // Refusing to rename is good here, it's the name of a module
        assert_snapshot!(test.rename("mypkg"), @"Cannot rename");
    }

    #[test]
    fn rename_submodule_import_from_confusing_real_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg import sub<CURSOR>pkg

                x = subpkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // Renaming the integer is correct
        assert_snapshot!(test.rename("mypkg"), @r"
        info[rename]: Rename symbol (found 3 locations)
         --> mypackage/__init__.py:2:21
          |
        2 | from .subpkg import subpkg
          |                     ^^^^^^
        3 |
        4 | x = subpkg
          |     ------
          |
         ::: mypackage/subpkg/__init__.py:2:1
          |
        2 | subpkg: int = 10
          | ------
          |
        ");
    }

    #[test]
    fn rename_submodule_import_from_confusing_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg import subpkg

                x = sub<CURSOR>pkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // TODO(submodule-imports): this is incorrect, we should rename the `subpkg` int
        // and the RHS of the import statement (but *not* rename the LHS).
        //
        // However us being cautious here *would* be good as the rename will actually
        // result in a `subpkg` variable still existing in this code, as the import's LHS
        // `DefinitionKind::ImportFromSubmodule` would stop being overwritten by the RHS!
        assert_snapshot!(test.rename("mypkg"), @r"
        info[rename]: Rename symbol (found 1 locations)
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg import subpkg
        3 |
        4 | x = subpkg
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn rename_overloaded_function() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                from typing import overload, Any

                @overload
                def test<CURSOR>() -> None: ...
                @overload
                def test(a: str) -> str: ...
                @overload
                def test(a: int) -> int: ...

                def test(a: Any) -> Any:
                    return a
                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import test

                test("test")
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 3 locations)
         --> lib.py:5:5
          |
        4 | @overload
        5 | def test() -> None: ...
          |     ^^^^
        6 | @overload
        7 | def test(a: str) -> str: ...
          |
         ::: main.py:2:17
          |
        2 | from lib import test
          |                 ----
        3 |
        4 | test("test")
          | ----
          |
        "#);
    }

    #[test]
    fn rename_overloaded_method() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                from typing import overload, Any

                class Test:
                    @overload
                    def test<CURSOR>() -> None: ...
                    @overload
                    def test(a: str) -> str: ...
                    @overload
                    def test(a: int) -> int: ...

                    def test(a: Any) -> Any:
                        return a

                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import Test

                Test().test("test")
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 2 locations)
         --> lib.py:6:9
          |
        4 | class Test:
        5 |     @overload
        6 |     def test() -> None: ...
          |         ^^^^
        7 |     @overload
        8 |     def test(a: str) -> str: ...
          |
         ::: main.py:4:8
          |
        2 | from lib import Test
        3 |
        4 | Test().test("test")
          |        ----
          |
        "#);
    }

    #[test]
    fn rename_overloaded_function_usage() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                from typing import overload, Any

                @overload
                def test() -> None: ...
                @overload
                def test(a: str) -> str: ...
                @overload
                def test(a: int) -> int: ...

                def test(a: Any) -> Any:
                    return a
                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import test

                test<CURSOR>("test")
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:17
          |
        2 | from lib import test
          |                 ^^^^
        3 |
        4 | test("test")
          | ----
          |
         ::: lib.py:5:5
          |
        4 | @overload
        5 | def test() -> None: ...
          |     ----
        6 | @overload
        7 | def test(a: str) -> str: ...
          |
        "#);
    }

    #[test]
    fn rename_property() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                class Foo:
                    @property
                    def my_property<CURSOR>(self) -> int:
                        return 42
                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import Foo

                print(Foo().my_property)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> lib.py:4:9
          |
        2 | class Foo:
        3 |     @property
        4 |     def my_property(self) -> int:
          |         ^^^^^^^^^^^
        5 |         return 42
          |
         ::: main.py:4:13
          |
        2 | from lib import Foo
        3 |
        4 | print(Foo().my_property)
          |             -----------
          |
        ");
    }

    // TODO: this should rename the name of the function decorated with
    // `@my_property.setter` as well as the getter function name
    #[test]
    fn rename_property_with_setter() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                class Foo:
                    @property
                    def my_property<CURSOR>(self) -> int:
                        return 42

                    @my_property.setter
                    def my_property(self, value: int) -> None:
                        pass
                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import Foo

                print(Foo().my_property)
                Foo().my_property = 56
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r"
        info[rename]: Rename symbol (found 4 locations)
         --> lib.py:4:9
          |
        2 | class Foo:
        3 |     @property
        4 |     def my_property(self) -> int:
          |         ^^^^^^^^^^^
        5 |         return 42
        6 |
        7 |     @my_property.setter
          |      -----------
        8 |     def my_property(self, value: int) -> None:
        9 |         pass
          |
         ::: main.py:4:13
          |
        2 | from lib import Foo
        3 |
        4 | print(Foo().my_property)
          |             -----------
        5 | Foo().my_property = 56
          |       -----------
          |
        ");
    }

    // TODO: this should rename the name of the function decorated with
    // `@my_property.deleter` as well as the getter function name
    #[test]
    fn rename_property_with_deleter() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                class Foo:
                    @property
                    def my_property<CURSOR>(self) -> int:
                        return 42

                    @my_property.deleter
                    def my_property(self) -> None:
                        pass
                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import Foo

                print(Foo().my_property)
                del Foo().my_property
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r"
        info[rename]: Rename symbol (found 4 locations)
         --> lib.py:4:9
          |
        2 | class Foo:
        3 |     @property
        4 |     def my_property(self) -> int:
          |         ^^^^^^^^^^^
        5 |         return 42
        6 |
        7 |     @my_property.deleter
          |      -----------
        8 |     def my_property(self) -> None:
        9 |         pass
          |
         ::: main.py:4:13
          |
        2 | from lib import Foo
        3 |
        4 | print(Foo().my_property)
          |             -----------
        5 | del Foo().my_property
          |           -----------
          |
        ");
    }

    // TODO: this should rename the name of the functions decorated with
    // `@my_property.deleter` and `@my_property.deleter` as well as the
    // getter function name
    #[test]
    fn rename_property_with_setter_and_deleter() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
                class Foo:
                    @property
                    def my_property<CURSOR>(self) -> int:
                        return 42

                    @my_property.setter
                    def my_property(self, value: int) -> None:
                        pass

                    @my_property.deleter
                    def my_property(self) -> None:
                        pass
                "#,
            )
            .source(
                "main.py",
                r#"
                from lib import Foo

                print(Foo().my_property)
                Foo().my_property = 56
                del Foo().my_property
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r"
        info[rename]: Rename symbol (found 6 locations)
          --> lib.py:4:9
           |
         2 | class Foo:
         3 |     @property
         4 |     def my_property(self) -> int:
           |         ^^^^^^^^^^^
         5 |         return 42
         6 |
         7 |     @my_property.setter
           |      -----------
         8 |     def my_property(self, value: int) -> None:
         9 |         pass
        10 |
        11 |     @my_property.deleter
           |      -----------
        12 |     def my_property(self) -> None:
        13 |         pass
           |
          ::: main.py:4:13
           |
         2 | from lib import Foo
         3 |
         4 | print(Foo().my_property)
           |             -----------
         5 | Foo().my_property = 56
           |       -----------
         6 | del Foo().my_property
           |           -----------
           |
        ");
    }

    #[test]
    fn rename_single_dispatch_function() {
        let test = CursorTest::builder()
            .source(
                "foo.py",
                r#"
                from functools import singledispatch

                @singledispatch
                def f<CURSOR>(x: object):
                    raise NotImplementedError

                @f.register
                def _(x: int) -> str:
                    return "int"

                @f.register
                def _(x: str) -> int:
                    return int(x)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 3 locations)
          --> foo.py:5:5
           |
         4 | @singledispatch
         5 | def f(x: object):
           |     ^
         6 |     raise NotImplementedError
         7 |
         8 | @f.register
           |  -
         9 | def _(x: int) -> str:
        10 |     return "int"
        11 |
        12 | @f.register
           |  -
        13 | def _(x: str) -> int:
        14 |     return int(x)
           |
        "#);
    }

    #[test]
    fn rename_single_dispatch_function_stacked_register() {
        let test = CursorTest::builder()
            .source(
                "foo.py",
                r#"
                from functools import singledispatch

                @singledispatch
                def f<CURSOR>(x):
                    raise NotImplementedError

                @f.register(int)
                @f.register(float)
                def _(x) -> float:
                    return "int"

                @f.register(str)
                def _(x) -> int:
                    return int(x)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 4 locations)
          --> foo.py:5:5
           |
         4 | @singledispatch
         5 | def f(x):
           |     ^
         6 |     raise NotImplementedError
         7 |
         8 | @f.register(int)
           |  -
         9 | @f.register(float)
           |  -
        10 | def _(x) -> float:
        11 |     return "int"
        12 |
        13 | @f.register(str)
           |  -
        14 | def _(x) -> int:
        15 |     return int(x)
           |
        "#);
    }

    #[test]
    fn rename_single_dispatchmethod() {
        let test = CursorTest::builder()
            .source(
                "foo.py",
                r#"
                from functools import singledispatchmethod

                class Foo:
                    @singledispatchmethod
                    def f<CURSOR>(self, x: object):
                        raise NotImplementedError

                    @f.register
                    def _(self, x: str) -> float:
                        return "int"

                    @f.register
                    def _(self, x: str) -> int:
                        return int(x)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 3 locations)
          --> foo.py:6:9
           |
         4 | class Foo:
         5 |     @singledispatchmethod
         6 |     def f(self, x: object):
           |         ^
         7 |         raise NotImplementedError
         8 |
         9 |     @f.register
           |      -
        10 |     def _(self, x: str) -> float:
        11 |         return "int"
        12 |
        13 |     @f.register
           |      -
        14 |     def _(self, x: str) -> int:
        15 |         return int(x)
           |
        "#);
    }

    #[test]
    fn rename_single_dispatchmethod_staticmethod() {
        let test = CursorTest::builder()
            .source(
                "foo.py",
                r#"
                from functools import singledispatchmethod

                class Foo:
                    @singledispatchmethod
                    @staticmethod
                    def f<CURSOR>(self, x):
                        raise NotImplementedError

                    @f.register(str)
                    @staticmethod
                    def _(x: int) -> str:
                        return "int"

                    @f.register
                    @staticmethod
                    def _(x: str) -> int:
                        return int(x)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 3 locations)
          --> foo.py:7:9
           |
         5 |     @singledispatchmethod
         6 |     @staticmethod
         7 |     def f(self, x):
           |         ^
         8 |         raise NotImplementedError
         9 |
        10 |     @f.register(str)
           |      -
        11 |     @staticmethod
        12 |     def _(x: int) -> str:
        13 |         return "int"
        14 |
        15 |     @f.register
           |      -
        16 |     @staticmethod
        17 |     def _(x: str) -> int:
           |
        "#);
    }

    #[test]
    fn rename_single_dispatchmethod_classmethod() {
        let test = CursorTest::builder()
            .source(
                "foo.py",
                r#"
                from functools import singledispatchmethod

                class Foo:
                    @singledispatchmethod
                    @classmethod
                    def f<CURSOR>(cls, x):
                        raise NotImplementedError

                    @f.register(str)
                    @classmethod
                    def _(cls, x) -> str:
                        return "int"

                    @f.register(int)
                    @f.register(float)
                    @staticmethod
                    def _(cls, x) -> int:
                        return int(x)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 4 locations)
          --> foo.py:7:9
           |
         5 |     @singledispatchmethod
         6 |     @classmethod
         7 |     def f(cls, x):
           |         ^
         8 |         raise NotImplementedError
         9 |
        10 |     @f.register(str)
           |      -
        11 |     @classmethod
        12 |     def _(cls, x) -> str:
        13 |         return "int"
        14 |
        15 |     @f.register(int)
           |      -
        16 |     @f.register(float)
           |      -
        17 |     @staticmethod
        18 |     def _(cls, x) -> int:
           |
        "#);
    }

    #[test]
    fn rename_attribute() {
        let test = CursorTest::builder()
            .source(
                "foo.py",
                r#"
                class Test:
                    attribute<CURSOR>: str

                    def __init__(self, value: str):
                        self.attribute = value

                class Child(Test):
                    def test(self):
                        return self.attribute


                c = Child("test")

                print(c.attribute)
                c.attribute = "new_value"
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 5 locations)
          --> foo.py:3:5
           |
         2 | class Test:
         3 |     attribute: str
           |     ^^^^^^^^^
         4 |
         5 |     def __init__(self, value: str):
         6 |         self.attribute = value
           |              ---------
         7 |
         8 | class Child(Test):
         9 |     def test(self):
        10 |         return self.attribute
           |                     ---------
           |
          ::: foo.py:15:9
           |
        13 | c = Child("test")
        14 |
        15 | print(c.attribute)
           |         ---------
        16 | c.attribute = "new_value"
           |   ---------
           |
        "#);
    }

    // TODO: This should rename all attribute usages
    // Note: Pylance only renames the assignment in `__init__`.
    #[test]
    fn rename_implicit_attribute() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                class Test:
                    def __init__(self, value: str):
                        self.<CURSOR>attribute = value

                class Child(Test):
                    def __init__(self, value: str):
                        super().__init__(value)
                        self.attribute = value + "child"

                    def test(self):
                        return self.attribute


                c = Child("test")

                print(c.attribute)
                c.attribute = "new_value"
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r"
        info[rename]: Rename symbol (found 1 locations)
         --> main.py:4:14
          |
        2 | class Test:
        3 |     def __init__(self, value: str):
        4 |         self.attribute = value
          |              ^^^^^^^^^
        5 |
        6 | class Child(Test):
          |
        ");
    }

    // TODO: Should not rename the first declaration
    #[test]
    fn rename_redeclarations() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                a: str = "test"

                a: int = 10

                print(a<CURSOR>)
                "#,
            )
            .build();

        assert_snapshot!(test.rename("better_name"), @r#"
        info[rename]: Rename symbol (found 3 locations)
         --> main.py:2:1
          |
        2 | a: str = "test"
          | ^
        3 |
        4 | a: int = 10
          | -
        5 |
        6 | print(a)
          |       -
          |
        "#);
    }
}

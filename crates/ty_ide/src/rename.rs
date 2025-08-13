use crate::goto::find_goto_target;
use crate::references::{ReferencesMode, references};
use crate::{Db, ReferenceTarget};
use ruff_db::files::File;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::ImportAliasResolution;

/// Returns the range of the symbol if it can be renamed, None if not.
pub fn can_rename(db: &dyn Db, file: File, offset: TextSize) -> Option<ruff_text_size::TextRange> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);

    // Get the definitions for the symbol at the offset
    let goto_target = find_goto_target(&module, offset)?;

    // Don't allow renaming of import module components
    if matches!(
        goto_target,
        crate::goto::GotoTarget::ImportModuleComponent { .. }
    ) {
        return None;
    }

    let current_file_in_project = is_file_in_project(db, file);

    if let Some(definition_targets) = goto_target
        .get_definition_targets(file, db, ImportAliasResolution::PreserveAliases)
        .and_then(|definitions| definitions.declaration_targets(db))
    {
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
    } else {
        // No definition targets found. This happens for keywords, so refuse rename
        return None;
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

    // Get the definitions for the symbol at the offset
    let goto_target = find_goto_target(&module, offset)?;

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
    db.project().files(db).contains(&file)
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
            let Some(range) = can_rename(&self.db, self.cursor.file, self.cursor.offset) else {
                return "Cannot rename".to_string();
            };

            format!("Can rename symbol at range {range:?}")
        }

        fn rename(&self, new_name: &str) -> String {
            let Some(rename_results) =
                rename(&self.db, self.cursor.file, self.cursor.offset, new_name)
            else {
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
    fn test_prepare_rename_parameter() {
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
    fn test_rename_parameter() {
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
    fn test_rename_function() {
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
    fn test_rename_class() {
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
    fn test_rename_invalid_name() {
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
    fn test_multi_file_function_rename() {
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
    fn test_cannot_rename_import_module_component() {
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
    fn test_cannot_rename_from_import_module_component() {
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
    fn test_cannot_rename_external_file() {
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
    fn test_rename_alias_at_import_statement() {
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
from utils import test as test_<CURSOR>alias
result = test_alias()
",
            )
            .build();

        assert_snapshot!(test.rename("new_alias"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:27
          |
        2 | from utils import test as test_alias
          |                           ^^^^^^^^^^
        3 | result = test_alias()
          |          ----------
          |
        ");
    }

    #[test]
    fn test_rename_alias_at_usage_site() {
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
from utils import test as test_alias
result = test_<CURSOR>alias()
",
            )
            .build();

        assert_snapshot!(test.rename("new_alias"), @r"
        info[rename]: Rename symbol (found 2 locations)
         --> main.py:2:27
          |
        2 | from utils import test as test_alias
          |                           ^^^^^^^^^^
        3 | result = test_alias()
          |          ----------
          |
        ");
    }

    #[test]
    fn test_rename_across_import_chain_with_mixed_aliases() {
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
    fn test_rename_alias_in_import_chain() {
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
    fn test_cannot_rename_keyword() {
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
    fn test_cannot_rename_builtin_type() {
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
    fn test_rename_keyword_argument() {
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
    fn test_rename_parameter_with_keyword_argument() {
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
}

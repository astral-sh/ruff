//! Presents collected pytest tests as editor items.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::identifier::Identifier;
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::DefinitionKind;
use ty_python_core::{ProgramFile, semantic_index};
use ty_python_semantic::pytest_tests_in_file;

use crate::{Db, FxIndexMap};

/// Returns collected pytest test functions and their containing classes in source order.
///
/// Collection follows pytest's default conventions, including `unittest.TestCase` methods.
/// Class items are included only when they contain a collected test, either directly or in a nested
/// class.
///
/// Multiple possible definitions of the same target produce one item, using the first definition's
/// source location. Methods from alternative class definitions are combined under that class item.
fn discover_pytest_tests<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
) -> Vec<DiscoveredPytestTest> {
    let tests = pytest_tests_in_file(db, file);
    if tests.is_empty() {
        return Vec::new();
    }

    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);
    let mut collector = TestCollector::default();

    for test in tests {
        let binding = test.binding();
        let Some(symbol) = binding.place(db).as_symbol() else {
            continue;
        };
        let name = index
            .place_table(binding.file_scope(db))
            .symbol(symbol)
            .name();
        let range = match binding.kind(db) {
            DefinitionKind::ImportFrom(import) => import.alias(&module).identifier(),
            _ => binding.focus_range(db, &module).range(),
        };

        // Track the identifier of the parent class.
        let mut parent: Option<String> = None;

        // Add enclosing classes to the list of test items.
        // Start from the outermost class so that each item's parent is added before the item itself.
        for class in test.enclosing_classes(db, &module).into_iter().rev() {
            parent = Some(collector.insert(
                &class.name,
                class.name.range(),
                DiscoveredPytestTestKind::Class,
                parent.as_deref(),
            ));
        }

        collector.insert(
            name.as_str(),
            range,
            DiscoveredPytestTestKind::Function,
            parent.as_deref(),
        );
    }

    collector.into_vec()
}

/// A collected pytest test function or a class containing collected pytest tests.
#[derive(Debug, PartialEq, Eq)]
struct DiscoveredPytestTest {
    /// File-relative pytest target, such as `TestUsers::test_lookup`.
    ///
    /// This identifier is unchanged by edits that only move the item's source location.
    id: String,
    /// Whether this item is a class or a function/method.
    kind: DiscoveredPytestTestKind,
    /// The source range of the test binding or class name.
    range: TextRange,
    /// The collected test name or class name shown in the editor.
    /// For `TestUsers::test_lookup`, this is `test_lookup`.
    label: String,
    /// The containing class's identifier, or `None` for a module-level item.
    /// For `TestUsers::test_lookup`, this is `Some("TestUsers")`.
    parent: Option<String>,
}

/// Whether an editor item represents a test class or a test function/method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoveredPytestTestKind {
    /// A class containing collected tests.
    Class,
    /// A collected function or method, including `unittest.TestCase` methods.
    Function,
}

/// Collects one item per pytest target in discovery order.
#[derive(Default)]
struct TestCollector {
    items: FxIndexMap<String, DiscoveredPytestTest>,
}

impl TestCollector {
    /// Inserts an item if its target is new and returns the target identifier.
    fn insert(
        &mut self,
        name: &str,
        range: TextRange,
        kind: DiscoveredPytestTestKind,
        parent: Option<&str>,
    ) -> String {
        let id = parent.map_or_else(|| name.to_string(), |parent| format!("{parent}::{name}"));
        self.items
            .entry(id.clone())
            .or_insert_with(|| DiscoveredPytestTest {
                id: id.clone(),
                kind,
                range,
                label: name.to_string(),
                parent: parent.map(str::to_owned),
            });
        id
    }

    fn into_vec(self) -> Vec<DiscoveredPytestTest> {
        self.items.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
    use ruff_db::files::File;
    use ruff_db::source::source_text;

    use super::{DiscoveredPytestTest, discover_pytest_tests};
    use crate::tests::{CursorTest, IntoDiagnostic};

    #[test]
    fn discovers_functions_and_test_classes() {
        let test = discovery_test(
            r#"
import unittest

def test_module(): ...

class TestUsers:
    def test_lookup(self): ...
    def test_update(self): ...

class UserCase(unittest.TestCase):
    def test_unit(self): ...
"#,
        );

        assert_snapshot!(discovered_tests(&test), @"
        info[test-discovery]: Discovered pytest tests
          --> test_users.py:4:5
           |
         4 | def test_module(): ...
           |     ^^^^^^^^^^^
         5 |
         6 | class TestUsers:
           |       ^^^^^^^^^
         7 |     def test_lookup(self): ...
           |         ^^^^^^^^^^^
         8 |     def test_update(self): ...
           |         ^^^^^^^^^^^
         9 |
        10 | class UserCase(unittest.TestCase):
           |       ^^^^^^^^
        11 |     def test_unit(self): ...
           |         ^^^^^^^^^
        ");
    }

    #[test]
    fn uses_binding_names_and_locations() {
        let mut test = discovery_test(
            r#"
from helpers import check as test_imported
from helpers import test_exported

def helper(): ...
test_alias = helper
test_second_alias = helper

class TestUsers:
    test_method = test_imported
"#,
        );
        test.write_file(
            "helpers.py",
            r#"
def check(): ...
def test_exported(): ...
"#,
        )
        .expect("writing imported test functions should succeed");

        assert_snapshot!(discovered_tests(&test), @"
        info[test-discovery]: Discovered pytest tests
          --> test_users.py:2:30
           |
         2 | from helpers import check as test_imported
           |                              ^^^^^^^^^^^^^
         3 | from helpers import test_exported
           |                     ^^^^^^^^^^^^^
         4 |
         5 | def helper(): ...
         6 | test_alias = helper
           | ^^^^^^^^^^
         7 | test_second_alias = helper
           | ^^^^^^^^^^^^^^^^^
         8 |
         9 | class TestUsers:
           |       ^^^^^^^^^
        10 |     test_method = test_imported
           |     ^^^^^^^^^^^
        ");
    }

    #[test]
    fn preserves_nested_class_paths() {
        let test = discovery_test(
            r#"
class TestUsers[T]:
    class TestPermissions:
        def test_read[U](self): ...
        def test_write(self): ...

class TestGroups:
    class TestPermissions:
        def test_read(self): ...
"#,
        );

        assert_snapshot!(discovered_tests(&test), @"
        info[test-discovery]: Discovered pytest tests
         --> test_users.py:2:7
          |
        2 | class TestUsers[T]:
          |       ^^^^^^^^^
        3 |     class TestPermissions:
          |           ^^^^^^^^^^^^^^^
        4 |         def test_read[U](self): ...
          |             ^^^^^^^^^
        5 |         def test_write(self): ...
          |             ^^^^^^^^^^
        6 |
        7 | class TestGroups:
          |       ^^^^^^^^^^
        8 |     class TestPermissions:
          |           ^^^^^^^^^^^^^^^
        9 |         def test_read(self): ...
          |             ^^^^^^^^^
        ");
    }

    #[test]
    fn omits_classes_without_collected_tests() {
        let test = discovery_test(
            r#"
class TestEmpty: ...
"#,
        );

        assert_snapshot!(discovered_tests(&test), @"No tests found");
    }

    #[test]
    fn coalesces_conditional_definitions() {
        let test = discovery_test(
            r#"
import os

if os.getenv("TEST_MODE"):
    def test_choice(): ...

    class TestChoice:
        def test_shared(self): ...
        def test_first(self): ...
else:
    def test_choice(): ...

    class TestChoice:
        def test_shared(self): ...
        def test_second(self): ...
"#,
        );

        assert_snapshot!(discovered_tests(&test), @"
        info[test-discovery]: Discovered pytest tests
          --> test_users.py:5:9
           |
         5 |     def test_choice(): ...
           |         ^^^^^^^^^^^
         6 |
         7 |     class TestChoice:
           |           ^^^^^^^^^^
         8 |         def test_shared(self): ...
           |             ^^^^^^^^^^^
         9 |         def test_first(self): ...
           |             ^^^^^^^^^^
           |
          ::: test_users.py:15:13
           |
        15 |         def test_second(self): ...
           |             ^^^^^^^^^^^
        ");
    }

    fn discovery_test(source: &str) -> CursorTest {
        CursorTest::builder()
            .source(
                "test_users.py",
                format!(
                    r#"{source}
<CURSOR>"#
                ),
            )
            .build()
    }

    fn discovered_tests(test: &CursorTest) -> String {
        let items = discover_pytest_tests(&test.db, test.program_file(test.cursor.file));
        if items.is_empty() {
            return "No tests found".to_owned();
        }

        let source = source_text(&test.db, test.cursor.file);
        for item in &items {
            assert_eq!(&source[item.range], item.label);
        }

        test.render_diagnostics([DiscoveredPytestTestsDiagnostic {
            file: test.cursor.file,
            items,
        }])
    }

    struct DiscoveredPytestTestsDiagnostic {
        file: File,
        items: Vec<DiscoveredPytestTest>,
    }

    impl IntoDiagnostic for DiscoveredPytestTestsDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::lint("test-discovery"),
                Severity::Info,
                "Discovered pytest tests",
            );

            for item in self.items {
                diagnostic.annotate(Annotation::primary(
                    Span::from(self.file).with_range(item.range),
                ));
            }
            diagnostic
        }
    }
}

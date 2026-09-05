//! Presents collected pytest tests as editor items.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::identifier::Identifier;
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::DefinitionKind;
use ty_python_core::{ProgramFile, semantic_index};
use ty_python_semantic::pytest_tests_in_file;

use crate::{Db, FxIndexMap};

/// Returns collected test functions and their containing classes in source order.
///
/// Collection follows pytest's default conventions, including `unittest.TestCase` methods.
/// Class items are included only when they contain a collected test, either directly or in a nested
/// class.
///
/// Multiple possible definitions of the same target produce one item, using the first definition's
/// source location. Methods from alternative class definitions are combined under that class item.
fn discover_tests<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> Vec<DiscoveredTest> {
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

        // `pytest_tests_in_file` has already established that the test is
        // eligible for collection even when it is nested in a class hierarchy,
        // so we can simply retrieve that nested class hierarchy if it exists.
        let enclosing_classes = index
            .ancestor_scopes(binding.file_scope(db))
            .filter_map(|(_, scope)| scope.node().as_class().map(|class| class.node(&module)))
            .collect::<Vec<_>>();

        // Track the identifier of the parent class.
        let mut parent: Option<String> = None;

        // Add enclosing classes to the list of test items.
        // Start from the outermost class so that each item's parent is added before the item itself.
        for class in enclosing_classes.into_iter().rev() {
            parent = Some(collector.insert(
                &class.name,
                class.name.range(),
                DiscoveredTestKind::Class,
                parent.as_deref(),
            ));
        }

        collector.insert(
            name.as_str(),
            range,
            DiscoveredTestKind::Function,
            parent.as_deref(),
        );
    }

    collector.into_vec()
}

/// A collected test function or a class containing collected tests.
#[derive(Debug, PartialEq, Eq)]
struct DiscoveredTest {
    /// File-relative pytest target, such as `TestUsers::test_lookup`.
    ///
    /// This identifier is unchanged by edits that only move the item's source location.
    id: String,
    /// Whether this item is a class or a function/method.
    kind: DiscoveredTestKind,
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
enum DiscoveredTestKind {
    /// A class containing collected tests.
    Class,
    /// A collected function or method, including `unittest.TestCase` methods.
    Function,
}

/// Collects one item per pytest target in discovery order.
#[derive(Default)]
struct TestCollector {
    items: FxIndexMap<String, DiscoveredTest>,
}

impl TestCollector {
    /// Inserts an item if its target is new and returns the target identifier.
    fn insert(
        &mut self,
        name: &str,
        range: TextRange,
        kind: DiscoveredTestKind,
        parent: Option<&str>,
    ) -> String {
        let id = parent.map_or_else(|| name.to_string(), |parent| format!("{parent}::{name}"));
        self.items
            .entry(id.clone())
            .or_insert_with(|| DiscoveredTest {
                id: id.clone(),
                kind,
                range,
                label: name.to_string(),
                parent: parent.map(str::to_owned),
            });
        id
    }

    fn into_vec(self) -> Vec<DiscoveredTest> {
        self.items.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
    use ruff_db::files::File;
    use ruff_db::source::source_text;

    use super::{DiscoveredTest, DiscoveredTestKind, discover_tests};
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
        info[test-discovery]: function: test_module
         --> test_users.py:4:5
          |
        4 | def test_module(): ...
          |     ^^^^^^^^^^^

        info[test-discovery]: class: TestUsers
         --> test_users.py:6:7
          |
        6 | class TestUsers:
          |       ^^^^^^^^^

        info[test-discovery]: function: TestUsers::test_lookup
         --> test_users.py:7:9
          |
        7 |     def test_lookup(self): ...
          |         ^^^^^^^^^^^

        info[test-discovery]: function: TestUsers::test_update
         --> test_users.py:8:9
          |
        8 |     def test_update(self): ...
          |         ^^^^^^^^^^^

        info[test-discovery]: class: UserCase
          --> test_users.py:10:7
           |
        10 | class UserCase(unittest.TestCase):
           |       ^^^^^^^^

        info[test-discovery]: function: UserCase::test_unit
          --> test_users.py:11:9
           |
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
        info[test-discovery]: function: test_imported
         --> test_users.py:2:30
          |
        2 | from helpers import check as test_imported
          |                              ^^^^^^^^^^^^^

        info[test-discovery]: function: test_exported
         --> test_users.py:3:21
          |
        3 | from helpers import test_exported
          |                     ^^^^^^^^^^^^^

        info[test-discovery]: function: test_alias
         --> test_users.py:6:1
          |
        6 | test_alias = helper
          | ^^^^^^^^^^

        info[test-discovery]: function: test_second_alias
         --> test_users.py:7:1
          |
        7 | test_second_alias = helper
          | ^^^^^^^^^^^^^^^^^

        info[test-discovery]: class: TestUsers
         --> test_users.py:9:7
          |
        9 | class TestUsers:
          |       ^^^^^^^^^

        info[test-discovery]: function: TestUsers::test_method
          --> test_users.py:10:5
           |
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
        info[test-discovery]: class: TestUsers
         --> test_users.py:2:7
          |
        2 | class TestUsers[T]:
          |       ^^^^^^^^^

        info[test-discovery]: class: TestUsers::TestPermissions
         --> test_users.py:3:11
          |
        3 |     class TestPermissions:
          |           ^^^^^^^^^^^^^^^

        info[test-discovery]: function: TestUsers::TestPermissions::test_read
         --> test_users.py:4:13
          |
        4 |         def test_read[U](self): ...
          |             ^^^^^^^^^

        info[test-discovery]: function: TestUsers::TestPermissions::test_write
         --> test_users.py:5:13
          |
        5 |         def test_write(self): ...
          |             ^^^^^^^^^^

        info[test-discovery]: class: TestGroups
         --> test_users.py:7:7
          |
        7 | class TestGroups:
          |       ^^^^^^^^^^

        info[test-discovery]: class: TestGroups::TestPermissions
         --> test_users.py:8:11
          |
        8 |     class TestPermissions:
          |           ^^^^^^^^^^^^^^^

        info[test-discovery]: function: TestGroups::TestPermissions::test_read
         --> test_users.py:9:13
          |
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
        info[test-discovery]: function: test_choice
         --> test_users.py:5:9
          |
        5 |     def test_choice(): ...
          |         ^^^^^^^^^^^

        info[test-discovery]: class: TestChoice
         --> test_users.py:7:11
          |
        7 |     class TestChoice:
          |           ^^^^^^^^^^

        info[test-discovery]: function: TestChoice::test_shared
         --> test_users.py:8:13
          |
        8 |         def test_shared(self): ...
          |             ^^^^^^^^^^^

        info[test-discovery]: function: TestChoice::test_first
         --> test_users.py:9:13
          |
        9 |         def test_first(self): ...
          |             ^^^^^^^^^^

        info[test-discovery]: function: TestChoice::test_second
          --> test_users.py:15:13
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
        let items = discover_tests(&test.db, test.program_file(test.cursor.file));
        if items.is_empty() {
            return "No tests found".to_owned();
        }

        let source = source_text(&test.db, test.cursor.file);
        for item in &items {
            assert_eq!(&source[item.range], item.label);
        }

        test.render_diagnostics(items.into_iter().map(|item| DiscoveredTestDiagnostic {
            file: test.cursor.file,
            item,
        }))
    }

    struct DiscoveredTestDiagnostic {
        file: File,
        item: DiscoveredTest,
    }

    impl IntoDiagnostic for DiscoveredTestDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let kind = match self.item.kind {
                DiscoveredTestKind::Class => "class",
                DiscoveredTestKind::Function => "function",
            };
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::lint("test-discovery"),
                Severity::Info,
                format!("{kind}: {}", self.item.id),
            );
            diagnostic.annotate(Annotation::primary(
                Span::from(self.file).with_range(self.item.range),
            ));
            diagnostic
        }
    }
}

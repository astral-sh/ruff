use ruff_db::parsed::parsed_module;
use ruff_python_ast::{Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::ProgramFile;
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

use crate::Db;

/// Whether a discovered test is a class or a function/method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveredTestKind {
    Class,
    Function,
}

/// A test discovered in context of a given file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredTest {
    /// Unique identifier for the test
    pub id: String,
    /// Whether this is a test class or a test function/method.
    pub kind: DiscoveredTestKind,
    /// The range of the test function or class name.
    pub range: TextRange,
    /// Human readable name for the test
    pub label: String,
    /// id of the parent of this test case. E.g. if the test is defined in a class.
    pub parent: Option<String>,
}

pub fn discover_tests<'db>(db: &'db dyn Db, program_file: ProgramFile<'db>) -> Vec<DiscoveredTest> {
    let parsed = parsed_module(db, program_file.python_file(db)).load(db);
    let model = SemanticModel::new(db, program_file);
    let mut tests = vec![];

    for stmt in &parsed.syntax().body {
        match stmt {
            Stmt::FunctionDef(func) => {
                if let Some(test) = test_function(func, None) {
                    tests.push(test);
                }
            }
            Stmt::ClassDef(class) => {
                if !is_pytest_test_class(class) && !is_unittest_test_class(db, &model, class) {
                    continue;
                }

                tests.push(DiscoveredTest {
                    label: class.name.to_string(),
                    id: class.name.to_string(),
                    kind: DiscoveredTestKind::Class,
                    range: class.name.range(),
                    parent: None,
                });

                for class_stmt in &class.body {
                    if let Stmt::FunctionDef(func) = class_stmt
                        && let Some(test) = test_function(func, Some(class))
                    {
                        tests.push(test);
                    }
                }
            }
            _ => {}
        }
    }

    tests
}

/// Returns whether pytest collects `class` by its naming conventions:
/// a name starting with `Test` and no `__init__` method.
///
/// <https://doc.pytest.org/en/latest/explanation/goodpractices.html#conventions-for-python-test-discovery>
// TODO: naming customization https://docs.pytest.org/en/stable/example/pythoncollection.html#changing-naming-conventions
fn is_pytest_test_class(class: &StmtClassDef) -> bool {
    class.name.as_str().starts_with("Test")
        && !class
            .body
            .iter()
            .any(|stmt| matches!(stmt, Stmt::FunctionDef(func) if func.name.as_str() == "__init__"))
}

/// Returns whether `class` is a `unittest.TestCase` subclass that defines at least one
/// test method in its own body.
///
/// pytest collects `TestCase` subclasses regardless of their name, but a subclass
/// without test methods of its own (such as a base class holding shared helpers)
/// would only show up as an empty entry, so it is skipped.
///
/// <https://docs.python.org/3/library/unittest.html#basic-example>
fn is_unittest_test_class<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    class: &StmtClassDef,
) -> bool {
    let defines_test_method = class.body.iter().any(
        |stmt| matches!(stmt, Stmt::FunctionDef(func) if func.name.as_str().starts_with("test")),
    );

    defines_test_method
        && class
            .inferred_type(model)
            .and_then(Type::as_class_literal)
            .is_some_and(|class| class.is_unittest_test_case(db))
}

fn test_function(func: &StmtFunctionDef, class: Option<&StmtClassDef>) -> Option<DiscoveredTest> {
    // TODO: naming customization https://docs.pytest.org/en/stable/example/pythoncollection.html#changing-naming-conventions
    if !func.name.as_str().starts_with("test") {
        return None;
    }
    let qualified_name = if let Some(class) = class {
        format!("{}::{}", class.name, func.name)
    } else {
        func.name.to_string()
    };
    let parent = class.map(|class| class.name.to_string());
    Some(DiscoveredTest {
        label: func.name.to_string(),
        id: qualified_name,
        kind: DiscoveredTestKind::Function,
        range: func.name.range(),
        parent,
    })
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::File;

    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic};

    fn test_discovery_test(path: &str, source: &str) -> CursorTest {
        CursorTest::builder()
            .source(path, format!("{source}\n<CURSOR>"))
            .build()
    }

    struct DiscoveredTestDiagnostic {
        file: File,
        test: DiscoveredTest,
    }

    impl IntoDiagnostic for DiscoveredTestDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let kind = match self.test.kind {
                DiscoveredTestKind::Class => "class",
                DiscoveredTestKind::Function => "function",
            };
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("test-discovery")),
                Severity::Info,
                format!("{kind}: {}", self.test.id),
            );
            diagnostic.annotate(Annotation::primary(
                Span::from(self.file).with_range(self.test.range),
            ));
            diagnostic
        }
    }

    impl CursorTest {
        fn discovered_tests(&self) -> String {
            let tests = discover_tests(&self.db, self.program_file(self.cursor.file));
            if tests.is_empty() {
                return "No tests found".to_string();
            }
            let diagnostics: Vec<DiscoveredTestDiagnostic> = tests
                .into_iter()
                .map(|test| DiscoveredTestDiagnostic {
                    file: self.cursor.file,
                    test,
                })
                .collect();
            self.render_diagnostics(diagnostics)
        }
    }

    #[test]
    fn discovers_function_tests() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
def test_foo():
    pass

def test_bar():
    pass

def helper():
    pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @r"
        info[test-discovery]: function: test_foo
         --> test_a.py:2:5
          |
        2 | def test_foo():
          |     ^^^^^^^^

        info[test-discovery]: function: test_bar
         --> test_a.py:5:5
          |
        5 | def test_bar():
          |     ^^^^^^^^
        ");
    }

    #[test]
    fn discovers_test_class() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
class TestFoo:
    def test_bar(self):
        pass

    def test_baz(self):
        pass

    def helper(self):
        pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @r"
        info[test-discovery]: class: TestFoo
         --> test_a.py:2:7
          |
        2 | class TestFoo:
          |       ^^^^^^^

        info[test-discovery]: function: TestFoo::test_bar
         --> test_a.py:3:9
          |
        3 |     def test_bar(self):
          |         ^^^^^^^^

        info[test-discovery]: function: TestFoo::test_baz
         --> test_a.py:6:9
          |
        6 |     def test_baz(self):
          |         ^^^^^^^^
        ");
    }

    #[test]
    fn discovers_unittest_testcase() {
        let test = test_discovery_test(
            "unittest_example.py",
            r#"
import unittest

class BaseTest(unittest.TestCase):
    def helper(self):
        pass

class TestMath(BaseTest):
    def test_add(self):
        self.assertEqual(1 + 1, 2)
"#,
        );

        assert_snapshot!(test.discovered_tests(), @r"
        info[test-discovery]: class: TestMath
         --> unittest_example.py:8:7
          |
        8 | class TestMath(BaseTest):
          |       ^^^^^^^^

        info[test-discovery]: function: TestMath::test_add
         --> unittest_example.py:9:9
          |
        9 |     def test_add(self):
          |         ^^^^^^^^
        ");
    }

    // A `unittest.TestCase` subclass is collected even when its name does not start
    // with `Test`.
    #[test]
    fn discovers_unittest_testcase_without_test_name_prefix() {
        let test = test_discovery_test(
            "test_math.py",
            r#"
import unittest

class MathCase(unittest.TestCase):
    def test_add(self):
        self.assertEqual(1 + 1, 2)
"#,
        );

        assert_snapshot!(test.discovered_tests(), @r"
        info[test-discovery]: class: MathCase
         --> test_math.py:4:7
          |
        4 | class MathCase(unittest.TestCase):
          |       ^^^^^^^^

        info[test-discovery]: function: MathCase::test_add
         --> test_math.py:5:9
          |
        5 |     def test_add(self):
          |         ^^^^^^^^
        ");
    }

    // An attribute whose name starts with `test` is not a test method.
    #[test]
    fn skips_unittest_testcase_without_test_methods() {
        let test = test_discovery_test(
            "test_data.py",
            r#"
import unittest

class DataCase(unittest.TestCase):
    test_data = [1]

    def helper(self):
        pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @"No tests found");
    }

    #[test]
    fn skips_non_test_functions() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
def helper():
    pass

def setup():
    pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @"No tests found");
    }

    #[test]
    fn skips_class_with_init() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
class TestFoo:
    def __init__(self):
        self.x = 1

    def test_bar(self):
        pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @"No tests found");
    }

    #[test]
    fn skips_non_test_class() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
class MyClass:
    def test_bar(self):
        pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @"No tests found");
    }

    #[test]
    fn discovers_async_test_functions() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
async def test_async_foo():
    pass

async def test_async_bar():
    pass

async def helper():
    pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @r"
        info[test-discovery]: function: test_async_foo
         --> test_a.py:2:11
          |
        2 | async def test_async_foo():
          |           ^^^^^^^^^^^^^^

        info[test-discovery]: function: test_async_bar
         --> test_a.py:5:11
          |
        5 | async def test_async_bar():
          |           ^^^^^^^^^^^^^^
        ");
    }

    // We intentionally do not support nested test classes because this pattern is uncommon.
    #[test]
    fn nested_test_class() {
        let test = test_discovery_test(
            "test_a.py",
            r#"
class TestOuter:
    def test_outer(self):
        pass

    class TestInner:
        def test_inner(self):
            pass
"#,
        );

        assert_snapshot!(test.discovered_tests(), @r"
        info[test-discovery]: class: TestOuter
         --> test_a.py:2:7
          |
        2 | class TestOuter:
          |       ^^^^^^^^^

        info[test-discovery]: function: TestOuter::test_outer
         --> test_a.py:3:9
          |
        3 |     def test_outer(self):
          |         ^^^^^^^^^^
        ");
    }
}

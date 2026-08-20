use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeLensCommand {
    /// fully qualified name of the test function
    RunTest { test: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLensItem {
    pub title: String,
    pub range: TextRange,
    pub command: CodeLensCommand,
}

pub fn code_lens(db: &dyn Db, file: File) -> Vec<CodeLensItem> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let mut items = vec![];

    for stmt in &parsed.syntax().body {
        match stmt {
            Stmt::FunctionDef(func) => {
                if let Some(lens) = test_func_codelens(func, None) {
                    items.push(lens);
                }
            }
            Stmt::ClassDef(class) => {
                // https://doc.pytest.org/en/latest/explanation/goodpractices.html#conventions-for-python-test-discovery
                let is_pytest_class = class.name.as_str().starts_with("Test")
                    && !class.body.iter().any(
                        |s| matches!(s, Stmt::FunctionDef(f) if f.name.as_str() == "__init__"),
                    );
                // https://docs.python.org/3/library/unittest.html#basic-example
                let is_unittest_test_case = class
                    .inferred_type(&model)
                    .and_then(Type::as_class_literal)
                    .is_some_and(|c| c.is_unittest_test_case(db));

                if !is_pytest_class && !is_unittest_test_case {
                    continue;
                }

                items.push(CodeLensItem {
                    range: class.name.range(),
                    title: String::from("Run tests"),
                    command: CodeLensCommand::RunTest {
                        test: class.name.to_string(),
                    },
                });

                for class_stmt in &class.body {
                    if let Stmt::FunctionDef(func) = class_stmt
                        && let Some(lens) = test_func_codelens(func, Some(class))
                    {
                        items.push(lens);
                    }
                }
            }
            _ => {}
        }
    }

    items
}

fn test_func_codelens(
    func: &StmtFunctionDef,
    class: Option<&StmtClassDef>,
) -> Option<CodeLensItem> {
    // TODO: naming customization https://docs.pytest.org/en/stable/example/pythoncollection.html#changing-naming-conventions
    if !func.name.as_str().starts_with("test") {
        return None;
    }
    let test = if let Some(class) = class {
        format!("{}::{}", class.name, func.name)
    } else {
        func.name.to_string()
    };
    Some(CodeLensItem {
        range: func.name.range(),
        title: String::from("Run test"),
        command: CodeLensCommand::RunTest { test },
    })
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::File;

    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic};

    fn code_lens_test(path: &str, source: &str) -> CursorTest {
        CursorTest::builder()
            .source(path, format!("{source}\n<CURSOR>"))
            .build()
    }

    struct CodeLensDiagnostic {
        file: File,
        item: CodeLensItem,
    }

    impl IntoDiagnostic for CodeLensDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let label = match &self.item.command {
                CodeLensCommand::RunTest { test } => test.clone(),
            };
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("code-lens")),
                Severity::Info,
                format!("{}: {label}", self.item.title),
            );
            diagnostic.annotate(Annotation::primary(
                Span::from(self.file).with_range(self.item.range),
            ));
            diagnostic
        }
    }

    impl CursorTest {
        fn code_lenses(&self) -> String {
            let items = code_lens(&self.db, self.cursor.file);
            if items.is_empty() {
                return "No code lenses found".to_string();
            }
            let diagnostics: Vec<CodeLensDiagnostic> = items
                .into_iter()
                .map(|item| CodeLensDiagnostic {
                    file: self.cursor.file,
                    item,
                })
                .collect();
            self.render_diagnostics(diagnostics)
        }
    }

    #[test]
    fn test_code_lens_function_tests() {
        let test = code_lens_test(
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

        assert_snapshot!(test.code_lenses(), @r"
        info[code-lens]: Run test: test_foo
         --> test_a.py:2:5
          |
        2 | def test_foo():
          |     ^^^^^^^^
        3 |     pass
          |

        info[code-lens]: Run test: test_bar
         --> test_a.py:5:5
          |
        3 |     pass
        4 |
        5 | def test_bar():
          |     ^^^^^^^^
        6 |     pass
          |
        ");
    }

    #[test]
    fn test_code_lens_test_class() {
        let test = code_lens_test(
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

        assert_snapshot!(test.code_lenses(), @r"
        info[code-lens]: Run tests: TestFoo
         --> test_a.py:2:7
          |
        2 | class TestFoo:
          |       ^^^^^^^
        3 |     def test_bar(self):
        4 |         pass
          |

        info[code-lens]: Run test: TestFoo::test_bar
         --> test_a.py:3:9
          |
        2 | class TestFoo:
        3 |     def test_bar(self):
          |         ^^^^^^^^
        4 |         pass
          |

        info[code-lens]: Run test: TestFoo::test_baz
         --> test_a.py:6:9
          |
        4 |         pass
        5 |
        6 |     def test_baz(self):
          |         ^^^^^^^^
        7 |         pass
          |
        ");
    }

    #[test]
    fn test_code_lens_unittest_testcase() {
        let test = code_lens_test(
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

        assert_snapshot!(test.code_lenses(), @r"
        info[code-lens]: Run tests: TestMath
          --> unittest_example.py:8:7
           |
         6 |         pass
         7 |
         8 | class TestMath(BaseTest):
           |       ^^^^^^^^
         9 |     def test_add(self):
        10 |         self.assertEqual(1 + 1, 2)
           |

        info[code-lens]: Run test: TestMath::test_add
          --> unittest_example.py:9:9
           |
         8 | class TestMath(BaseTest):
         9 |     def test_add(self):
           |         ^^^^^^^^
        10 |         self.assertEqual(1 + 1, 2)
           |
        ");
    }

    #[test]
    fn test_code_lens_skips_non_test_functions() {
        let test = code_lens_test(
            "test_a.py",
            r#"
def helper():
    pass

def setup():
    pass
"#,
        );

        assert_snapshot!(test.code_lenses(), @"No code lenses found");
    }

    #[test]
    fn test_code_lens_skips_class_with_init() {
        let test = code_lens_test(
            "test_a.py",
            r#"
class TestFoo:
    def __init__(self):
        self.x = 1

    def test_bar(self):
        pass
"#,
        );

        assert_snapshot!(test.code_lenses(), @"No code lenses found");
    }

    #[test]
    fn test_code_lens_skips_non_test_class() {
        let test = code_lens_test(
            "test_a.py",
            r#"
class MyClass:
    def test_bar(self):
        pass
"#,
        );

        assert_snapshot!(test.code_lenses(), @"No code lenses found");
    }

    #[test]
    fn test_code_lens_async_test_functions() {
        let test = code_lens_test(
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

        assert_snapshot!(test.code_lenses(), @r"
        info[code-lens]: Run test: test_async_foo
         --> test_a.py:2:11
          |
        2 | async def test_async_foo():
          |           ^^^^^^^^^^^^^^
        3 |     pass
          |

        info[code-lens]: Run test: test_async_bar
         --> test_a.py:5:11
          |
        3 |     pass
        4 |
        5 | async def test_async_bar():
          |           ^^^^^^^^^^^^^^
        6 |     pass
          |
        ");
    }

    // We intentionally do not support nested test classes because this pattern is uncommon.
    #[test]
    fn test_code_lens_nested_test_class() {
        let test = code_lens_test(
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

        assert_snapshot!(test.code_lenses(), @r"
        info[code-lens]: Run tests: TestOuter
         --> test_a.py:2:7
          |
        2 | class TestOuter:
          |       ^^^^^^^^^
        3 |     def test_outer(self):
        4 |         pass
          |

        info[code-lens]: Run test: TestOuter::test_outer
         --> test_a.py:3:9
          |
        2 | class TestOuter:
        3 |     def test_outer(self):
          |         ^^^^^^^^^^
        4 |         pass
          |
        ");
    }
}

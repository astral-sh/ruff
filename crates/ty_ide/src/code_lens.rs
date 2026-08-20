use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::Stmt;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeLensCommand {
    RunTest {
        class_names: Vec<Name>,
        function_name: Option<Name>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLensItem {
    pub title: String,
    pub range: TextRange,
    pub command: CodeLensCommand,
}

pub fn code_lens(db: &dyn Db, file: File) -> Vec<CodeLensItem> {
    let mut items = vec![];
    collect_run_test_items(db, file, &mut items);
    items
}

fn collect_run_test_items(db: &dyn Db, file: File, items: &mut Vec<CodeLensItem>) {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    collect_test_functions(db, &model, &parsed.syntax().body, &mut vec![], items);
}

fn collect_test_functions(
    db: &dyn Db,
    model: &SemanticModel,
    body: &[Stmt],
    class_names: &mut Vec<Name>,
    items: &mut Vec<CodeLensItem>,
) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(func) => {
                if func.name.as_str().starts_with("test_") {
                    items.push(CodeLensItem {
                        range: func.name.range(),
                        title: String::from("Run test"),
                        command: CodeLensCommand::RunTest {
                            class_names: class_names.clone(),
                            function_name: Some(func.name.id.clone()),
                        },
                    });
                }
            }
            Stmt::ClassDef(class) => {
                collect_class_tests(db, model, class, class_names, items);
            }
            _ => {}
        }
    }
}

fn collect_class_tests(
    db: &dyn Db,
    model: &SemanticModel,
    class: &ruff_python_ast::StmtClassDef,
    class_names: &mut Vec<Name>,
    items: &mut Vec<CodeLensItem>,
) {
    let is_pytest_class = class.name.as_str().starts_with("Test")
        && !class
            .body
            .iter()
            .any(|s| matches!(s, Stmt::FunctionDef(f) if f.name.as_str() == "__init__"));
    let is_unittest_test_case = class
        .inferred_type(model)
        .and_then(Type::as_class_literal)
        .is_some_and(|c| c.is_unittest_test_case(db));
    if !is_pytest_class && !is_unittest_test_case {
        return;
    }

    class_names.push(class.name.id.clone());

    items.push(CodeLensItem {
        range: class.name.range(),
        title: String::from("Run tests"),
        command: CodeLensCommand::RunTest {
            class_names: class_names.clone(),
            function_name: None,
        },
    });

    collect_test_functions(db, model, &class.body, class_names, items);

    class_names.pop();
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
                CodeLensCommand::RunTest {
                    class_names,
                    function_name,
                } => {
                    let mut parts: Vec<&str> = class_names.iter().map(Name::as_str).collect();
                    if let Some(func) = function_name {
                        parts.push(func.as_str());
                    }
                    parts.join("::")
                }
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

        info[code-lens]: Run tests: TestOuter::TestInner
         --> test_a.py:6:11
          |
        4 |         pass
        5 |
        6 |     class TestInner:
          |           ^^^^^^^^^
        7 |         def test_inner(self):
        8 |             pass
          |

        info[code-lens]: Run test: TestOuter::TestInner::test_inner
         --> test_a.py:7:13
          |
        6 |     class TestInner:
        7 |         def test_inner(self):
          |             ^^^^^^^^^^
        8 |             pass
          |
        ");
    }
}

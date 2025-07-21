use crate::goto::find_goto_target;
use crate::stub_mapping::StubMapper;
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};

/// Navigate to the definition of a symbol.
///
/// A "definition" is the actual implementation of a symbol, potentially in a source file
/// rather than a stub file. This differs from "declaration" which may navigate to stub files.
/// When possible, this function will map from stub file declarations to their corresponding
/// source file implementations using the `StubMapper`.
pub fn goto_definition(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let goto_target = find_goto_target(&module, offset)?;

    // Create a StubMapper to map from stub files to source files
    let stub_mapper = StubMapper::new(db);

    let definition_targets = goto_target.get_definition_targets(file, db, Some(&stub_mapper))?;

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: definition_targets,
    })
}

#[cfg(test)]
mod test {
    use crate::tests::{CursorTest, IntoDiagnostic};
    use crate::{NavigationTarget, goto_definition};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
    };
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    /// goto-definition on a module should go to the .py not the .pyi
    ///
    /// TODO: this currently doesn't work right! This is especially surprising
    /// because [`goto_definition_stub_map_module_ref`] works fine.
    #[test]
    fn goto_definition_stub_map_module_import() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymo<CURSOR>dule import my_function
",
            )
            .source(
                "mymodule.py",
                r#"
def my_function():
    return "hello"
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
def my_function(): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Definition
         --> mymodule.pyi:1:1
          |
        1 |
          | ^
        2 | def my_function(): ...
          |
        info: Source
         --> main.py:2:6
          |
        2 | from mymodule import my_function
          |      ^^^^^^^^
          |
        ");
    }

    /// goto-definition on a module ref should go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_module_ref() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
import mymodule
x = mymo<CURSOR>dule
",
            )
            .source(
                "mymodule.py",
                r#"
def my_function():
    return "hello"
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
def my_function(): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Definition
         --> mymodule.py:1:1
          |
        1 |
          | ^
        2 | def my_function():
        3 |     return "hello"
          |
        info: Source
         --> main.py:3:5
          |
        2 | import mymodule
        3 | x = mymodule
          |     ^^^^^^^^
          |
        "#);
    }

    /// goto-definition on a function call should go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_function() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import my_function
print(my_func<CURSOR>tion())
",
            )
            .source(
                "mymodule.py",
                r#"
def my_function():
    return "hello"

def other_function():
    return "other"
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
def my_function(): ...

def other_function(): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Definition
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     ^^^^^^^^^^^
        3 |     return "hello"
          |
        info: Source
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^
          |
        "#);
    }

    /// goto-definition on a function that's redefined many times in the impl .py
    ///
    /// Currently this yields all instances. There's an argument for only yielding
    /// the final one since that's the one "exported" but, this is consistent for
    /// how we do file-local goto-definition.
    #[test]
    fn goto_definition_stub_map_function_redefine() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import my_function
print(my_func<CURSOR>tion())
",
            )
            .source(
                "mymodule.py",
                r#"
def my_function():
    return "hello"

def my_function():
    return "hello again"

def my_function():
    return "we can't keep doing this"

def other_function():
    return "other"
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
def my_function(): ...

def other_function(): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Definition
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     ^^^^^^^^^^^
        3 |     return "hello"
          |
        info: Source
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^
          |

        info[goto-definition]: Definition
         --> mymodule.py:5:5
          |
        3 |     return "hello"
        4 |
        5 | def my_function():
          |     ^^^^^^^^^^^
        6 |     return "hello again"
          |
        info: Source
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^
          |

        info[goto-definition]: Definition
         --> mymodule.py:8:5
          |
        6 |     return "hello again"
        7 |
        8 | def my_function():
          |     ^^^^^^^^^^^
        9 |     return "we can't keep doing this"
          |
        info: Source
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^
          |
        "#);
    }

    /// goto-definition on a class ref go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_class_ref() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyC<CURSOR>lass
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    def __init__(self, val):
        self.val = val

class MyOtherClass:
    def __init__(self, val):
        self.val = val + 1
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass:
    def __init__(self, val: bool): ...

class MyOtherClass:
    def __init__(self, val: bool): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self, val):
        4 |         self.val = val
          |
        info: Source
         --> main.py:3:5
          |
        2 | from mymodule import MyClass
        3 | x = MyClass
          |     ^^^^^^^
          |
        ");
    }

    /// goto-definition on a class init should go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_class_init() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyCl<CURSOR>ass(0)
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    def __init__(self, val):
        self.val = val

class MyOtherClass:
    def __init__(self, val):
        self.val = val + 1
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass:
    def __init__(self, val: bool): ...

class MyOtherClass:
    def __init__(self, val: bool): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self, val):
        4 |         self.val = val
          |
        info: Source
         --> main.py:3:5
          |
        2 | from mymodule import MyClass
        3 | x = MyClass(0)
          |     ^^^^^^^
          |
        ");
    }

    /// goto-definition on a class method should go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_class_method() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyClass(0)
x.act<CURSOR>ion()
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    def __init__(self, val):
        self.val = val
    def action(self):
        print(self.val)

class MyOtherClass:
    def __init__(self, val):
        self.val = val + 1
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass:
    def __init__(self, val: bool): ...
    def action(self): ...

class MyOtherClass:
    def __init__(self, val: bool): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Definition
         --> mymodule.py:5:9
          |
        3 |     def __init__(self, val):
        4 |         self.val = val
        5 |     def action(self):
          |         ^^^^^^
        6 |         print(self.val)
          |
        info: Source
         --> main.py:4:1
          |
        2 | from mymodule import MyClass
        3 | x = MyClass(0)
        4 | x.action()
          | ^^^^^^^^
          |
        ");
    }

    /// goto-definition on a class function should go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_class_function() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyClass.act<CURSOR>ion()
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    def __init__(self, val):
        self.val = val
    def action():
        print("hi!")

class MyOtherClass:
    def __init__(self, val):
        self.val = val + 1
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass:
    def __init__(self, val: bool): ...
    def action(): ...

class MyOtherClass:
    def __init__(self, val: bool): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Definition
         --> mymodule.py:5:9
          |
        3 |     def __init__(self, val):
        4 |         self.val = val
        5 |     def action():
          |         ^^^^^^
        6 |         print("hi!")
          |
        info: Source
         --> main.py:3:5
          |
        2 | from mymodule import MyClass
        3 | x = MyClass.action()
          |     ^^^^^^^^^^^^^^
          |
        "#);
    }

    /// According to Alex Waygood this test should goto mymodule/MyClass.py
    /// (but it currently doesn't!)
    #[test]
    fn goto_definition_stub_map_many_empty_mods() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyC<CURSOR>lass
",
            )
            .source(
                "mymodule/__init__.py",
                r#"
from . import MyClass
"#,
            )
            .source(
                "mymodule/MyClass.py",
                r#"
# also empty file
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass: ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    /// If the .pyi and the .py both define the class with no body, still prefer the .py
    #[test]
    fn goto_definition_stub_map_both_stubbed() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyC<CURSOR>lass
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass: ...
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass: ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Definition
         --> mymodule.py:2:7
          |
        2 | class MyClass: ...
          |       ^^^^^^^
          |
        info: Source
         --> main.py:2:22
          |
        2 | from mymodule import MyClass
          |                      ^^^^^^^
          |
        ");
    }

    impl CursorTest {
        fn goto_definition(&self) -> String {
            let Some(targets) = goto_definition(&self.db, self.cursor.file, self.cursor.offset)
            else {
                return "No goto target found".to_string();
            };

            if targets.is_empty() {
                return "No definitions found".to_string();
            }

            let source = targets.range;
            self.render_diagnostics(
                targets
                    .into_iter()
                    .map(|target| GotoDefinitionDiagnostic::new(source, &target)),
            )
        }
    }

    struct GotoDefinitionDiagnostic {
        source: FileRange,
        target: FileRange,
    }

    impl GotoDefinitionDiagnostic {
        fn new(source: FileRange, target: &NavigationTarget) -> Self {
            Self {
                source,
                target: FileRange::new(target.file(), target.focus_range()),
            }
        }
    }

    impl IntoDiagnostic for GotoDefinitionDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut source = SubDiagnostic::new(Severity::Info, "Source");
            source.annotate(Annotation::primary(
                Span::from(self.source.file()).with_range(self.source.range()),
            ));

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("goto-definition")),
                Severity::Info,
                "Definition".to_string(),
            );
            main.annotate(Annotation::primary(
                Span::from(self.target.file()).with_range(self.target.range()),
            ));
            main.sub(source);

            main
        }
    }
}

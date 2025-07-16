use crate::goto::find_goto_target;
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};

/// Navigate to the declaration of a symbol.
///
/// A "declaration" includes both formal declarations (class statements, def statements,
/// and variable annotations) but also variable assignments. This expansive definition
/// is needed because Python doesn't require formal declarations of variables like most languages do.
pub fn goto_declaration(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let goto_target = find_goto_target(&module, offset)?;

    let declaration_targets = goto_target.get_definition_targets(file, db, None)?;

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: declaration_targets,
    })
}

#[cfg(test)]
mod tests {
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use crate::{NavigationTarget, goto_declaration};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
    };
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    #[test]
    fn goto_declaration_function_call_to_definition() {
        let test = cursor_test(
            "
            def my_function(x, y):
                return x + y

            result = my_func<CURSOR>tion(1, 2)
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:17
          |
        2 |             def my_function(x, y):
          |                 ^^^^^^^^^^^
        3 |                 return x + y
          |
        info: Source
         --> main.py:5:22
          |
        3 |                 return x + y
        4 |
        5 |             result = my_function(1, 2)
          |                      ^^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_variable_assignment() {
        let test = cursor_test(
            "
            x = 42
            y = x<CURSOR>
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:13
          |
        2 |             x = 42
          |             ^
        3 |             y = x
          |
        info: Source
         --> main.py:3:17
          |
        2 |             x = 42
        3 |             y = x
          |                 ^
          |
        ");
    }

    #[test]
    fn goto_declaration_class_instantiation() {
        let test = cursor_test(
            "
            class MyClass:
                def __init__(self):
                    pass

            instance = My<CURSOR>Class()
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:19
          |
        2 |             class MyClass:
          |                   ^^^^^^^
        3 |                 def __init__(self):
        4 |                     pass
          |
        info: Source
         --> main.py:6:24
          |
        4 |                     pass
        5 |
        6 |             instance = MyClass()
          |                        ^^^^^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_parameter_usage() {
        let test = cursor_test(
            "
            def foo(param):
                return pa<CURSOR>ram * 2
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:21
          |
        2 |             def foo(param):
          |                     ^^^^^
        3 |                 return param * 2
          |
        info: Source
         --> main.py:3:24
          |
        2 |             def foo(param):
        3 |                 return param * 2
          |                        ^^^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_type_parameter() {
        let test = cursor_test(
            "
            def generic_func[T](value: T) -> T:
                v: T<CURSOR> = value
                return v
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:30
          |
        2 |             def generic_func[T](value: T) -> T:
          |                              ^
        3 |                 v: T = value
        4 |                 return v
          |
        info: Source
         --> main.py:3:20
          |
        2 |             def generic_func[T](value: T) -> T:
        3 |                 v: T = value
          |                    ^
        4 |                 return v
          |
        ");
    }

    #[test]
    fn goto_declaration_type_parameter_class() {
        let test = cursor_test(
            "
            class GenericClass[T]:
                def __init__(self, value: T<CURSOR>):
                    self.value = value
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:32
          |
        2 |             class GenericClass[T]:
          |                                ^
        3 |                 def __init__(self, value: T):
        4 |                     self.value = value
          |
        info: Source
         --> main.py:3:43
          |
        2 |             class GenericClass[T]:
        3 |                 def __init__(self, value: T):
          |                                           ^
        4 |                     self.value = value
          |
        ");
    }

    #[test]
    fn goto_declaration_nested_scope_variable() {
        let test = cursor_test(
            "
            x = \"outer\"
            def outer_func():
                def inner_func():
                    return x<CURSOR>  # Should find outer x
                return inner_func
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> main.py:2:13
          |
        2 |             x = "outer"
          |             ^
        3 |             def outer_func():
        4 |                 def inner_func():
          |
        info: Source
         --> main.py:5:28
          |
        3 |             def outer_func():
        4 |                 def inner_func():
        5 |                     return x  # Should find outer x
          |                            ^
        6 |                 return inner_func
          |
        "#);
    }

    #[test]
    fn goto_declaration_class_scope_skipped() {
        let test = cursor_test(
            r#"
class A:
    x = 1
    
    def method(self):
        def inner():
            return <CURSOR>x  # Should NOT find class variable x
        return inner
"#,
        );

        // Should not find the class variable 'x' due to Python's scoping rules
        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_import_simple() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
import mymodule
print(mymod<CURSOR>ule.function())
",
            )
            .source(
                "mymodule.py",
                r#"
def function():
    return "hello from mymodule"

variable = 42
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> mymodule.py:1:1
          |
        1 |
          | ^
        2 | def function():
        3 |     return "hello from mymodule"
          |
        info: Source
         --> main.py:3:7
          |
        2 | import mymodule
        3 | print(mymodule.function())
          |       ^^^^^^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_import_from() {
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
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
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

    #[test]
    fn goto_declaration_import_as() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
import mymodule.submodule as sub
print(<CURSOR>sub.helper())
",
            )
            .source(
                "mymodule/__init__.py",
                "
# Main module init
",
            )
            .source(
                "mymodule/submodule.py",
                r#"
FOO = 0
"#,
            )
            .build();

        // Should find the submodule file itself
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> mymodule/submodule.py:1:1
          |
        1 |
          | ^
        2 | FOO = 0
          |
        info: Source
         --> main.py:3:7
          |
        2 | import mymodule.submodule as sub
        3 | print(sub.helper())
          |       ^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_from_import_as() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from utils import func as h
print(<CURSOR>h("test"))
"#,
            )
            .source(
                "utils.py",
                r#"
def func(arg):
    return f"Processed: {arg}"
"#,
            )
            .build();

        // Should resolve to the actual function definition, not the import statement
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> utils.py:2:5
          |
        2 | def func(arg):
          |     ^^^^
        3 |     return f"Processed: {arg}"
          |
        info: Source
         --> main.py:3:7
          |
        2 | from utils import func as h
        3 | print(h("test"))
          |       ^
          |
        "#);
    }

    #[test]
    fn goto_declaration_from_import_chain() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from intermediate import shared_function
print(shared_func<CURSOR>tion())
"#,
            )
            .source(
                "intermediate.py",
                r#"
# Re-export the function from the original module
from original import shared_function
"#,
            )
            .source(
                "original.py",
                r#"
def shared_function():
    return "from original"
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> original.py:2:5
          |
        2 | def shared_function():
          |     ^^^^^^^^^^^^^^^
        3 |     return "from original"
          |
        info: Source
         --> main.py:3:7
          |
        2 | from intermediate import shared_function
        3 | print(shared_function())
          |       ^^^^^^^^^^^^^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_from_star_import() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from math_utils import *
result = add_n<CURSOR>umbers(5, 3)
"#,
            )
            .source(
                "math_utils.py",
                r#"
def add_numbers(a, b):
    """Add two numbers together."""
    return a + b

def multiply_numbers(a, b):
    """Multiply two numbers together."""
    return a * b
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> math_utils.py:2:5
          |
        2 | def add_numbers(a, b):
          |     ^^^^^^^^^^^
        3 |     """Add two numbers together."""
        4 |     return a + b
          |
        info: Source
         --> main.py:3:10
          |
        2 | from math_utils import *
        3 | result = add_numbers(5, 3)
          |          ^^^^^^^^^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_relative_import() {
        let test = CursorTest::builder()
            .source(
                "package/main.py",
                r#"
from .utils import helper_function
result = helper_func<CURSOR>tion("test")
"#,
            )
            .source(
                "package/__init__.py",
                r#"
# Package init file
"#,
            )
            .source(
                "package/utils.py",
                r#"
def helper_function(arg):
    """A helper function in utils module."""
    return f"Processed: {arg}"

def another_helper():
    """Another helper function."""
    pass
"#,
            )
            .build();

        // Should resolve the relative import to find the actual function definition
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> package/utils.py:2:5
          |
        2 | def helper_function(arg):
          |     ^^^^^^^^^^^^^^^
        3 |     """A helper function in utils module."""
        4 |     return f"Processed: {arg}"
          |
        info: Source
         --> package/main.py:3:10
          |
        2 | from .utils import helper_function
        3 | result = helper_function("test")
          |          ^^^^^^^^^^^^^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_relative_star_import() {
        let test = CursorTest::builder()
            .source(
                "package/main.py",
                r#"
from .utils import *
result = helper_func<CURSOR>tion("test")
"#,
            )
            .source(
                "package/__init__.py",
                r#"
# Package init file
"#,
            )
            .source(
                "package/utils.py",
                r#"
def helper_function(arg):
    """A helper function in utils module."""
    return f"Processed: {arg}"

def another_helper():
    """Another helper function."""
    pass
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> package/utils.py:2:5
          |
        2 | def helper_function(arg):
          |     ^^^^^^^^^^^^^^^
        3 |     """A helper function in utils module."""
        4 |     return f"Processed: {arg}"
          |
        info: Source
         --> package/main.py:3:10
          |
        2 | from .utils import *
        3 | result = helper_function("test")
          |          ^^^^^^^^^^^^^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_builtin_type() {
        let test = cursor_test(
            r#"
x: i<CURSOR>nt = 42
"#,
        );

        // Test that we can navigate to builtin types, but don't snapshot the exact content
        // since typeshed stubs can change frequently
        let result = test.goto_declaration();

        // Should not be "No goto target found" - we should find the builtin int type
        assert!(
            !result.contains("No goto target found"),
            "Should find builtin int type"
        );
        assert!(
            !result.contains("No declarations found"),
            "Should find builtin int declarations"
        );

        // Should navigate to a stdlib file containing the int class
        assert!(
            result.contains("builtins.pyi"),
            "Should navigate to builtins.pyi"
        );
        assert!(
            result.contains("class int:"),
            "Should find the int class definition"
        );
        assert!(
            result.contains("info[goto-declaration]: Declaration"),
            "Should be a goto-declaration result"
        );
    }

    #[test]
    fn goto_declaration_nonlocal_binding() {
        let test = cursor_test(
            r#"
def outer():
    x = "outer_value"
    
    def inner():
        nonlocal x
        x = "modified"
        return x<CURSOR>  # Should find the nonlocal x declaration in outer scope
    
    return inner
"#,
        );

        // Should find the variable declaration in the outer scope, not the nonlocal statement
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> main.py:3:5
          |
        2 | def outer():
        3 |     x = "outer_value"
          |     ^
        4 |     
        5 |     def inner():
          |
        info: Source
          --> main.py:8:16
           |
         6 |         nonlocal x
         7 |         x = "modified"
         8 |         return x  # Should find the nonlocal x declaration in outer scope
           |                ^
         9 |     
        10 |     return inner
           |
        "#);
    }

    #[test]
    fn goto_declaration_global_binding() {
        let test = cursor_test(
            r#"
global_var = "global_value"

def function():
    global global_var
    global_var = "modified"
    return global_<CURSOR>var  # Should find the global variable declaration
"#,
        );

        // Should find the global variable declaration, not the global statement
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> main.py:2:1
          |
        2 | global_var = "global_value"
          | ^^^^^^^^^^
        3 |
        4 | def function():
          |
        info: Source
         --> main.py:7:12
          |
        5 |     global global_var
        6 |     global_var = "modified"
        7 |     return global_var  # Should find the global variable declaration
          |            ^^^^^^^^^^
          |
        "#);
    }

    #[test]
    fn goto_declaration_generic_method_class_type() {
        let test = cursor_test(
            r#"
class MyClass:
    ClassType = int
    
    def generic_method[T](self, value: Class<CURSOR>Type) -> T:
        return value
"#,
        );

        // Should find the ClassType defined in the class body, not fail to resolve
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:3:5
          |
        2 | class MyClass:
        3 |     ClassType = int
          |     ^^^^^^^^^
        4 |     
        5 |     def generic_method[T](self, value: ClassType) -> T:
          |
        info: Source
         --> main.py:5:40
          |
        3 |     ClassType = int
        4 |     
        5 |     def generic_method[T](self, value: ClassType) -> T:
          |                                        ^^^^^^^^^
        6 |         return value
          |
        ");
    }

    impl CursorTest {
        fn goto_declaration(&self) -> String {
            let Some(targets) = goto_declaration(&self.db, self.cursor.file, self.cursor.offset)
            else {
                return "No goto target found".to_string();
            };

            if targets.is_empty() {
                return "No declarations found".to_string();
            }

            let source = targets.range;
            self.render_diagnostics(
                targets
                    .into_iter()
                    .map(|target| GotoDeclarationDiagnostic::new(source, &target)),
            )
        }
    }

    struct GotoDeclarationDiagnostic {
        source: FileRange,
        target: FileRange,
    }

    impl GotoDeclarationDiagnostic {
        fn new(source: FileRange, target: &NavigationTarget) -> Self {
            Self {
                source,
                target: FileRange::new(target.file(), target.focus_range()),
            }
        }
    }

    impl IntoDiagnostic for GotoDeclarationDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut source = SubDiagnostic::new(Severity::Info, "Source");
            source.annotate(Annotation::primary(
                Span::from(self.source.file()).with_range(self.source.range()),
            ));

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("goto-declaration")),
                Severity::Info,
                "Declaration".to_string(),
            );
            main.annotate(Annotation::primary(
                Span::from(self.target.file()).with_range(self.target.range()),
            ));
            main.sub(source);

            main
        }
    }
}

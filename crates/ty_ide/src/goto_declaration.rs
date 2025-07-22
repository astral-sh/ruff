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
        SubDiagnosticSeverity,
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
    fn goto_declaration_import_as_alias_name() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
import mymodule.submodule as su<CURSOR>b
print(sub.helper())
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

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> mymodule/submodule.py:1:1
          |
        1 |
          | ^
        2 | FOO = 0
          |
        info: Source
         --> main.py:2:30
          |
        2 | import mymodule.submodule as sub
          |                              ^^^
        3 | print(sub.helper())
          |
        ");
    }

    #[test]
    fn goto_declaration_import_as_alias_name_on_module() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
import mymodule.submod<CURSOR>ule as sub
print(sub.helper())
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

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> mymodule/submodule.py:1:1
          |
        1 |
          | ^
        2 | FOO = 0
          |
        info: Source
         --> main.py:2:17
          |
        2 | import mymodule.submodule as sub
          |                 ^^^^^^^^^
        3 | print(sub.helper())
          |
        ");
    }

    #[test]
    fn goto_declaration_from_import_symbol_original() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from mypackage.utils import hel<CURSOR>per as h
result = h("/a", "/b")
"#,
            )
            .source(
                "mypackage/__init__.py",
                r#"
# Package init
"#,
            )
            .source(
                "mypackage/utils.py",
                r#"
def helper(a, b):
    return a + "/" + b

def another_helper(path):
    return "processed"
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> mypackage/utils.py:2:5
          |
        2 | def helper(a, b):
          |     ^^^^^^
        3 |     return a + "/" + b
          |
        info: Source
         --> main.py:2:29
          |
        2 | from mypackage.utils import helper as h
          |                             ^^^^^^
        3 | result = h("/a", "/b")
          |
        "#);
    }

    #[test]
    fn goto_declaration_from_import_symbol_alias() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from mypackage.utils import helper as h<CURSOR>
result = h("/a", "/b")
"#,
            )
            .source(
                "mypackage/__init__.py",
                r#"
# Package init
"#,
            )
            .source(
                "mypackage/utils.py",
                r#"
def helper(a, b):
    return a + "/" + b

def another_helper(path):
    return "processed"
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> mypackage/utils.py:2:5
          |
        2 | def helper(a, b):
          |     ^^^^^^
        3 |     return a + "/" + b
          |
        info: Source
         --> main.py:2:39
          |
        2 | from mypackage.utils import helper as h
          |                                       ^
        3 | result = h("/a", "/b")
          |
        "#);
    }

    #[test]
    fn goto_declaration_from_import_module() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from mypackage.ut<CURSOR>ils import helper as h
result = h("/a", "/b")
"#,
            )
            .source(
                "mypackage/__init__.py",
                r#"
# Package init
"#,
            )
            .source(
                "mypackage/utils.py",
                r#"
def helper(a, b):
    return a + "/" + b

def another_helper(path):
    return "processed"
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> mypackage/utils.py:1:1
          |
        1 |
          | ^
        2 | def helper(a, b):
        3 |     return a + "/" + b
          |
        info: Source
         --> main.py:2:16
          |
        2 | from mypackage.utils import helper as h
          |                ^^^^^
        3 | result = h("/a", "/b")
          |
        "#);
    }

    #[test]
    fn goto_declaration_instance_attribute() {
        let test = cursor_test(
            "
            class C:
                def __init__(self):
                    self.x: int = 1

            c = C()
            y = c.x<CURSOR>
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:4:21
          |
        2 |             class C:
        3 |                 def __init__(self):
        4 |                     self.x: int = 1
          |                     ^^^^^^
        5 |
        6 |             c = C()
          |
        info: Source
         --> main.py:7:17
          |
        6 |             c = C()
        7 |             y = c.x
          |                 ^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_instance_attribute_no_annotation() {
        let test = cursor_test(
            "
            class C:
                def __init__(self):
                    self.x = 1

            c = C()
            y = c.x<CURSOR>
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:4:21
          |
        2 |             class C:
        3 |                 def __init__(self):
        4 |                     self.x = 1
          |                     ^^^^^^
        5 |
        6 |             c = C()
          |
        info: Source
         --> main.py:7:17
          |
        6 |             c = C()
        7 |             y = c.x
          |                 ^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_method_call_to_definition() {
        let test = cursor_test(
            "
            class C:
                def foo(self):
                    return 42

            c = C()
            res = c.foo<CURSOR>()
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:3:21
          |
        2 |             class C:
        3 |                 def foo(self):
          |                     ^^^
        4 |                     return 42
          |
        info: Source
         --> main.py:7:19
          |
        6 |             c = C()
        7 |             res = c.foo()
          |                   ^^^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_module_attribute() {
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
    fn goto_declaration_inherited_attribute() {
        let test = cursor_test(
            "
            class A:
                x = 10

            class B(A):
                pass

            b = B()
            y = b.x<CURSOR>
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:3:17
          |
        2 |             class A:
        3 |                 x = 10
          |                 ^
        4 |
        5 |             class B(A):
          |
        info: Source
         --> main.py:9:17
          |
        8 |             b = B()
        9 |             y = b.x
          |                 ^^^
          |
        ");
    }

    #[test]
    fn goto_declaration_property_getter_setter() {
        let test = cursor_test(
            "
            class C:
                def __init__(self):
                    self._value = 0
                
                @property
                def value(self):
                    return self._value

            c = C()
            c.value<CURSOR> = 42
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:7:21
          |
        6 |                 @property
        7 |                 def value(self):
          |                     ^^^^^
        8 |                     return self._value
          |
        info: Source
          --> main.py:11:13
           |
        10 |             c = C()
        11 |             c.value = 42
           |             ^^^^^^^
           |
        ");
    }

    #[test]
    fn goto_declaration_function_doc_attribute() {
        let test = cursor_test(
            r#"
            def my_function():
                """This is a docstring."""
                return 42

            doc = my_function.__doc<CURSOR>__
            "#,
        );

        // Should navigate to the __doc__ property in the FunctionType class in typeshed
        let result = test.goto_declaration();

        assert!(
            !result.contains("No goto target found"),
            "Should find builtin __doc__ attribute"
        );
        assert!(
            !result.contains("No declarations found"),
            "Should find builtin __doc__ declarations"
        );

        // Should navigate to a typeshed file containing the __doc__ attribute
        assert!(
            result.contains("types.pyi") || result.contains("builtins.pyi"),
            "Should navigate to typeshed file with __doc__ definition"
        );
        assert!(
            result.contains("__doc__"),
            "Should find the __doc__ attribute definition"
        );
        assert!(
            result.contains("info[goto-declaration]: Declaration"),
            "Should be a goto-declaration result"
        );
    }

    #[test]
    fn goto_declaration_protocol_instance_attribute() {
        let test = cursor_test(
            "
            from typing import Protocol

            class Drawable(Protocol):
                def draw(self) -> None: ...
                name: str

            def use_drawable(obj: Drawable):
                obj.na<CURSOR>me
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:6:17
          |
        4 |             class Drawable(Protocol):
        5 |                 def draw(self) -> None: ...
        6 |                 name: str
          |                 ^^^^
        7 |
        8 |             def use_drawable(obj: Drawable):
          |
        info: Source
         --> main.py:9:17
          |
        8 |             def use_drawable(obj: Drawable):
        9 |                 obj.name
          |                 ^^^^^^^^
          |
        ");
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

    #[test]
    fn goto_declaration_keyword_argument_simple() {
        let test = cursor_test(
            "
            def my_function(x, y, z=10):
                return x + y + z

            result = my_function(1, y<CURSOR>=2, z=3)
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Declaration
         --> main.py:2:32
          |
        2 |             def my_function(x, y, z=10):
          |                                ^
        3 |                 return x + y + z
          |
        info: Source
         --> main.py:5:37
          |
        3 |                 return x + y + z
        4 |
        5 |             result = my_function(1, y=2, z=3)
          |                                     ^
          |
        ");
    }

    #[test]
    fn goto_declaration_keyword_argument_overloaded() {
        let test = cursor_test(
            r#"
            from typing import overload

            @overload
            def process(data: str, format: str) -> str: ...

            @overload
            def process(data: int, format: int) -> int: ...

            def process(data, format):
                return data

            # Call the overloaded function
            result = process("hello", format<CURSOR>="json")
            "#,
        );

        // Should navigate to the parameter in both matching overloads
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Declaration
         --> main.py:5:36
          |
        4 |             @overload
        5 |             def process(data: str, format: str) -> str: ...
          |                                    ^^^^^^
        6 |
        7 |             @overload
          |
        info: Source
          --> main.py:14:39
           |
        13 |             # Call the overloaded function
        14 |             result = process("hello", format="json")
           |                                       ^^^^^^
           |

        info[goto-declaration]: Declaration
          --> main.py:8:36
           |
         7 |             @overload
         8 |             def process(data: int, format: int) -> int: ...
           |                                    ^^^^^^
         9 |
        10 |             def process(data, format):
           |
        info: Source
          --> main.py:14:39
           |
        13 |             # Call the overloaded function
        14 |             result = process("hello", format="json")
           |                                       ^^^^^^
           |
        "#);
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
            let mut source = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Source");
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

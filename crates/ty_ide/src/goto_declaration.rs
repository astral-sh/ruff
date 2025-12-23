use crate::goto::find_goto_target;
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::{ImportAliasResolution, SemanticModel};

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
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;

    let declaration_targets = goto_target
        .get_definition_targets(&model, ImportAliasResolution::ResolveAliases)?
        .declaration_targets(db)?;

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: declaration_targets,
    })
}

#[cfg(test)]
mod tests {
    use crate::goto_declaration;
    use crate::tests::{CursorTest, cursor_test};
    use insta::assert_snapshot;

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
        info[goto-declaration]: Go to declaration
         --> main.py:5:10
          |
        3 |     return x + y
        4 |
        5 | result = my_function(1, 2)
          |          ^^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:5
          |
        2 | def my_function(x, y):
          |     -----------
        3 |     return x + y
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:5
          |
        2 | x = 42
        3 | y = x
          |     ^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:1
          |
        2 | x = 42
          | -
        3 | y = x
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
        info[goto-declaration]: Go to declaration
         --> main.py:6:12
          |
        4 |         pass
        5 |
        6 | instance = MyClass()
          |            ^^^^^^^ Clicking here
          |
        info: Found 2 declarations
         --> main.py:2:7
          |
        2 | class MyClass:
          |       -------
        3 |     def __init__(self):
          |         --------
        4 |         pass
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:12
          |
        2 | def foo(param):
        3 |     return param * 2
          |            ^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:9
          |
        2 | def foo(param):
          |         -----
        3 |     return param * 2
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:8
          |
        2 | def generic_func[T](value: T) -> T:
        3 |     v: T = value
          |        ^ Clicking here
        4 |     return v
          |
        info: Found 1 declaration
         --> main.py:2:18
          |
        2 | def generic_func[T](value: T) -> T:
          |                  -
        3 |     v: T = value
        4 |     return v
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:31
          |
        2 | class GenericClass[T]:
        3 |     def __init__(self, value: T):
          |                               ^ Clicking here
        4 |         self.value = value
          |
        info: Found 1 declaration
         --> main.py:2:20
          |
        2 | class GenericClass[T]:
          |                    -
        3 |     def __init__(self, value: T):
        4 |         self.value = value
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
        info[goto-declaration]: Go to declaration
         --> main.py:5:16
          |
        3 | def outer_func():
        4 |     def inner_func():
        5 |         return x  # Should find outer x
          |                ^ Clicking here
        6 |     return inner_func
          |
        info: Found 1 declaration
         --> main.py:2:1
          |
        2 | x = "outer"
          | -
        3 | def outer_func():
        4 |     def inner_func():
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:7
          |
        2 | import mymodule
        3 | print(mymodule.function())
          |       ^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> mymodule.py:1:1
          |
        1 |
          | -
        2 | def function():
        3 |     return "hello from mymodule"
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     -----------
        3 |     return "hello"
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
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:3:7
          |
        2 | import mymodule.submodule as sub
        3 | print(sub.helper())
          |       ^^^ Clicking here
          |
        info: Found 1 declaration
         --> mymodule/submodule.py:1:1
          |
        1 |
          | -
        2 | FOO = 0
          |
        ");
    }

    #[test]
    fn goto_declaration_from_import_rhs_is_module() {
        let test = CursorTest::builder()
            .source("lib/__init__.py", r#""#)
            .source("lib/module.py", r#""#)
            .source("main.py", r#"from lib import module<CURSOR>"#)
            .build();

        // Should resolve to the actual function definition, not the import statement
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:1:17
          |
        1 | from lib import module
          |                 ^^^^^^ Clicking here
          |
        info: Found 1 declaration
        --> lib/module.py:1:1
         |
         |
        ");
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:7
          |
        2 | from utils import func as h
        3 | print(h("test"))
          |       ^ Clicking here
          |
        info: Found 1 declaration
         --> utils.py:2:5
          |
        2 | def func(arg):
          |     ----
        3 |     return f"Processed: {arg}"
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:7
          |
        2 | from intermediate import shared_function
        3 | print(shared_function())
          |       ^^^^^^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> original.py:2:5
          |
        2 | def shared_function():
          |     ---------------
        3 |     return "from original"
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
        info[goto-declaration]: Go to declaration
         --> main.py:3:10
          |
        2 | from math_utils import *
        3 | result = add_numbers(5, 3)
          |          ^^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> math_utils.py:2:5
          |
        2 | def add_numbers(a, b):
          |     -----------
        3 |     """Add two numbers together."""
        4 |     return a + b
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
        info[goto-declaration]: Go to declaration
         --> package/main.py:3:10
          |
        2 | from .utils import helper_function
        3 | result = helper_function("test")
          |          ^^^^^^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> package/utils.py:2:5
          |
        2 | def helper_function(arg):
          |     ---------------
        3 |     """A helper function in utils module."""
        4 |     return f"Processed: {arg}"
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
        info[goto-declaration]: Go to declaration
         --> package/main.py:3:10
          |
        2 | from .utils import *
        3 | result = helper_function("test")
          |          ^^^^^^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> package/utils.py:2:5
          |
        2 | def helper_function(arg):
          |     ---------------
        3 |     """A helper function in utils module."""
        4 |     return f"Processed: {arg}"
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
        info[goto-declaration]: Go to declaration
         --> main.py:2:30
          |
        2 | import mymodule.submodule as sub
          |                              ^^^ Clicking here
        3 | print(sub.helper())
          |
        info: Found 1 declaration
         --> mymodule/submodule.py:1:1
          |
        1 |
          | -
        2 | FOO = 0
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
        info[goto-declaration]: Go to declaration
         --> main.py:2:17
          |
        2 | import mymodule.submodule as sub
          |                 ^^^^^^^^^ Clicking here
        3 | print(sub.helper())
          |
        info: Found 1 declaration
         --> mymodule/submodule.py:1:1
          |
        1 |
          | -
        2 | FOO = 0
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
        info[goto-declaration]: Go to declaration
         --> main.py:2:29
          |
        2 | from mypackage.utils import helper as h
          |                             ^^^^^^ Clicking here
        3 | result = h("/a", "/b")
          |
        info: Found 1 declaration
         --> mypackage/utils.py:2:5
          |
        2 | def helper(a, b):
          |     ------
        3 |     return a + "/" + b
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
        info[goto-declaration]: Go to declaration
         --> main.py:2:39
          |
        2 | from mypackage.utils import helper as h
          |                                       ^ Clicking here
        3 | result = h("/a", "/b")
          |
        info: Found 1 declaration
         --> mypackage/utils.py:2:5
          |
        2 | def helper(a, b):
          |     ------
        3 |     return a + "/" + b
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
        info[goto-declaration]: Go to declaration
         --> main.py:2:16
          |
        2 | from mypackage.utils import helper as h
          |                ^^^^^ Clicking here
        3 | result = h("/a", "/b")
          |
        info: Found 1 declaration
         --> mypackage/utils.py:1:1
          |
        1 |
          | -
        2 | def helper(a, b):
        3 |     return a + "/" + b
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
        info[goto-declaration]: Go to declaration
         --> main.py:7:7
          |
        6 | c = C()
        7 | y = c.x
          |       ^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:4:9
          |
        2 | class C:
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |         ------
        5 |
        6 | c = C()
          |
        ");
    }

    #[test]
    fn goto_declaration_string_annotation1() {
        let test = cursor_test(
            r#"
        a: "MyCla<CURSOR>ss" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:2:5
          |
        2 | a: "MyClass" = 1
          |     ^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 1 declaration
         --> main.py:4:7
          |
        2 | a: "MyClass" = 1
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn goto_declaration_string_annotation2() {
        let test = cursor_test(
            r#"
        a: "None | MyCl<CURSOR>ass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 1 declaration
         --> main.py:4:7
          |
        2 | a: "None | MyClass" = 1
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn goto_declaration_string_annotation3() {
        let test = cursor_test(
            r#"
        a: "None |<CURSOR> MyClass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_string_annotation4() {
        let test = cursor_test(
            r#"
        a: "None | MyClass<CURSOR>" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 1 declaration
         --> main.py:4:7
          |
        2 | a: "None | MyClass" = 1
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn goto_declaration_string_annotation5() {
        let test = cursor_test(
            r#"
        a: "None | MyClass"<CURSOR> = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_string_annotation_dangling1() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass |" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_string_annotation_dangling2() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass | No" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:2:5
          |
        2 | a: "MyClass | No" = 1
          |     ^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 1 declaration
         --> main.py:4:7
          |
        2 | a: "MyClass | No" = 1
        3 |
        4 | class MyClass:
          |       -------
        5 |     """some docs"""
          |
        "#);
    }

    #[test]
    fn goto_declaration_string_annotation_dangling3() {
        let test = cursor_test(
            r#"
        a: "MyClass | N<CURSOR>o" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_string_annotation_recursive() {
        let test = cursor_test(
            r#"
        ab: "a<CURSOR>b"
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:2:6
          |
        2 | ab: "ab"
          |      ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:1
          |
        2 | ab: "ab"
          | --
          |
        "#);
    }

    #[test]
    fn goto_declaration_string_annotation_unknown() {
        let test = cursor_test(
            r#"
        x: "foo<CURSOR>bar"
        "#,
        );

        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_nested_instance_attribute() {
        let test = cursor_test(
            "
            class C:
                def __init__(self):
                    self.x: int = 1

            class D:
                def __init__(self):
                    self.y: C = C()

            d = D()
            y = d.y.x<CURSOR>
            ",
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
          --> main.py:11:9
           |
        10 | d = D()
        11 | y = d.y.x
           |         ^ Clicking here
           |
        info: Found 1 declaration
         --> main.py:4:9
          |
        2 | class C:
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |         ------
        5 |
        6 | class D:
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
        info[goto-declaration]: Go to declaration
         --> main.py:7:7
          |
        6 | c = C()
        7 | y = c.x
          |       ^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:4:9
          |
        2 | class C:
        3 |     def __init__(self):
        4 |         self.x = 1
          |         ------
        5 |
        6 | c = C()
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
        info[goto-declaration]: Go to declaration
         --> main.py:7:9
          |
        6 | c = C()
        7 | res = c.foo()
          |         ^^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:3:9
          |
        2 | class C:
        3 |     def foo(self):
          |         ---
        4 |         return 42
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
            result.contains("info[goto-declaration]: Go to declaration"),
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
        info[goto-declaration]: Go to declaration
          --> main.py:8:16
           |
         6 |         nonlocal x
         7 |         x = "modified"
         8 |         return x  # Should find the nonlocal x declaration in outer scope
           |                ^ Clicking here
         9 |
        10 |     return inner
           |
        info: Found 1 declaration
         --> main.py:3:5
          |
        2 | def outer():
        3 |     x = "outer_value"
          |     -
        4 |
        5 |     def inner():
          |
        "#);
    }

    #[test]
    fn goto_declaration_nonlocal_stmt() {
        let test = cursor_test(
            r#"
def outer():
    xy = "outer_value"

    def inner():
        nonlocal x<CURSOR>y
        xy = "modified"
        return x  # Should find the nonlocal x declaration in outer scope

    return inner
"#,
        );

        // Should find the variable declaration in the outer scope, not the nonlocal statement
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:6:18
          |
        5 |     def inner():
        6 |         nonlocal xy
          |                  ^^ Clicking here
        7 |         xy = "modified"
        8 |         return x  # Should find the nonlocal x declaration in outer scope
          |
        info: Found 1 declaration
         --> main.py:3:5
          |
        2 | def outer():
        3 |     xy = "outer_value"
          |     --
        4 |
        5 |     def inner():
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
        info[goto-declaration]: Go to declaration
         --> main.py:7:12
          |
        5 |     global global_var
        6 |     global_var = "modified"
        7 |     return global_var  # Should find the global variable declaration
          |            ^^^^^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:1
          |
        2 | global_var = "global_value"
          | ----------
        3 |
        4 | def function():
          |
        "#);
    }

    #[test]
    fn goto_declaration_global_stmt() {
        let test = cursor_test(
            r#"
global_var = "global_value"

def function():
    global global_<CURSOR>var
    global_var = "modified"
    return global_var  # Should find the global variable declaration
"#,
        );

        // Should find the global variable declaration, not the global statement
        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:5:12
          |
        4 | def function():
        5 |     global global_var
          |            ^^^^^^^^^^ Clicking here
        6 |     global_var = "modified"
        7 |     return global_var  # Should find the global variable declaration
          |
        info: Found 1 declaration
         --> main.py:2:1
          |
        2 | global_var = "global_value"
          | ----------
        3 |
        4 | def function():
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
        info[goto-declaration]: Go to declaration
         --> main.py:9:7
          |
        8 | b = B()
        9 | y = b.x
          |       ^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:3:5
          |
        2 | class A:
        3 |     x = 10
          |     -
        4 |
        5 | class B(A):
          |
        ");
    }

    #[test]
    fn goto_declaration_match_name_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:4:22
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ab]:
          |                      ^^ Clicking here
        5 |             x = ab
          |
        info: Found 1 declaration
         --> main.py:4:22
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ab]:
          |                      --
        5 |             x = ab
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_name_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:5:17
          |
        3 |     match command.split():
        4 |         case ["get", ab]:
        5 |             x = ab
          |                 ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:4:22
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ab]:
          |                      --
        5 |             x = ab
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_rest_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:4:23
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", *ab]:
          |                       ^^ Clicking here
        5 |             x = ab
          |
        info: Found 1 declaration
         --> main.py:4:23
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", *ab]:
          |                       --
        5 |             x = ab
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_rest_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:5:17
          |
        3 |     match command.split():
        4 |         case ["get", *ab]:
        5 |             x = ab
          |                 ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:4:23
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", *ab]:
          |                       --
        5 |             x = ab
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_as_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:4:37
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     ^^ Clicking here
        5 |             x = ab
          |
        info: Found 1 declaration
         --> main.py:4:37
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     --
        5 |             x = ab
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_as_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:5:17
          |
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
        5 |             x = ab
          |                 ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:4:37
          |
        2 | def my_func(command: str):
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     --
        5 |             x = ab
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_keyword_stmt() {
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

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |                              ^^ Clicking here
        11 |             x = ab
           |
        info: Found 1 declaration
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |                              --
        11 |             x = ab
           |
        ");
    }

    #[test]
    fn goto_declaration_match_keyword_binding() {
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

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
          --> main.py:11:17
           |
         9 |     match event:
        10 |         case Click(x, button=ab):
        11 |             x = ab
           |                 ^^ Clicking here
           |
        info: Found 1 declaration
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |                              --
        11 |             x = ab
           |
        ");
    }

    #[test]
    fn goto_declaration_match_class_name() {
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

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
          --> main.py:10:14
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |              ^^^^^ Clicking here
        11 |             x = ab
           |
        info: Found 1 declaration
         --> main.py:2:7
          |
        2 | class Click:
          |       -----
        3 |     __match_args__ = ("position", "button")
        4 |     def __init__(self, pos, btn):
          |
        "#);
    }

    #[test]
    fn goto_declaration_match_class_field_name() {
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

        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_typevar_name_stmt() {
        let test = cursor_test(
            r#"
            type Alias1[A<CURSOR>B: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             --
          |
        ");
    }

    #[test]
    fn goto_declaration_typevar_name_binding() {
        let test = cursor_test(
            r#"
            type Alias1[AB: int = bool] = tuple[A<CURSOR>B, list[AB]]
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:2:37
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |                                     ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             --
          |
        ");
    }

    #[test]
    fn goto_declaration_typevar_spec_stmt() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**A<CURSOR>B = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:3:15
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:3:15
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               --
          |
        ");
    }

    #[test]
    fn goto_declaration_typevar_spec_binding() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[A<CURSOR>B, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:3:43
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |                                           ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:3:15
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               --
          |
        ");
    }

    #[test]
    fn goto_declaration_typevar_tuple_stmt() {
        let test = cursor_test(
            r#"
            type Alias3[*A<CURSOR>B = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              --
          |
        ");
    }

    #[test]
    fn goto_declaration_typevar_tuple_binding() {
        let test = cursor_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*A<CURSOR>B], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:2:38
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |                                      ^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              --
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
        info[goto-declaration]: Go to declaration
          --> main.py:11:3
           |
        10 | c = C()
        11 | c.value = 42
           |   ^^^^^ Clicking here
           |
        info: Found 1 declaration
         --> main.py:7:9
          |
        6 |     @property
        7 |     def value(self):
          |         -----
        8 |         return self._value
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
            result.contains("info[goto-declaration]: Go to declaration"),
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
        info[goto-declaration]: Go to declaration
         --> main.py:9:9
          |
        8 | def use_drawable(obj: Drawable):
        9 |     obj.name
          |         ^^^^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:6:5
          |
        4 | class Drawable(Protocol):
        5 |     def draw(self) -> None: ...
        6 |     name: str
          |     ----
        7 |
        8 | def use_drawable(obj: Drawable):
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
        info[goto-declaration]: Go to declaration
         --> main.py:5:40
          |
        3 |     ClassType = int
        4 |
        5 |     def generic_method[T](self, value: ClassType) -> T:
          |                                        ^^^^^^^^^ Clicking here
        6 |         return value
          |
        info: Found 1 declaration
         --> main.py:3:5
          |
        2 | class MyClass:
        3 |     ClassType = int
          |     ---------
        4 |
        5 |     def generic_method[T](self, value: ClassType) -> T:
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
        info[goto-declaration]: Go to declaration
         --> main.py:5:25
          |
        3 |     return x + y + z
        4 |
        5 | result = my_function(1, y=2, z=3)
          |                         ^ Clicking here
          |
        info: Found 1 declaration
         --> main.py:2:20
          |
        2 | def my_function(x, y, z=10):
          |                    -
        3 |     return x + y + z
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
        info[goto-declaration]: Go to declaration
          --> main.py:14:27
           |
        13 | # Call the overloaded function
        14 | result = process("hello", format="json")
           |                           ^^^^^^ Clicking here
           |
        info: Found 2 declarations
          --> main.py:5:24
           |
         4 | @overload
         5 | def process(data: str, format: str) -> str: ...
           |                        ------
         6 |
         7 | @overload
         8 | def process(data: int, format: int) -> int: ...
           |                        ------
         9 |
        10 | def process(data, format):
           |
        "#);
    }

    #[test]
    fn goto_declaration_overload_type_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int): ...

@overload
def ab(a: str): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1)
          | ^^ Clicking here
          |
        info: Found 2 declarations
         --> mymodule.pyi:5:5
          |
        4 | @overload
        5 | def ab(a: int): ...
          |     --
        6 |
        7 | @overload
        8 | def ab(a: str): ...
          |     --
          |
        ");
    }

    #[test]
    fn goto_declaration_overload_type_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from mymodule import ab

a<CURSOR>b("hello")
"#,
            )
            .source(
                "mymodule.py",
                r#"
def ab(a):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int): ...

@overload
def ab(a: str): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab("hello")
          | ^^ Clicking here
          |
        info: Found 2 declarations
         --> mymodule.pyi:5:5
          |
        4 | @overload
        5 | def ab(a: int): ...
          |     --
        6 |
        7 | @overload
        8 | def ab(a: str): ...
          |     --
          |
        "#);
    }

    #[test]
    fn goto_declaration_overload_arity_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1, 2)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, b = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int, b: int): ...

@overload
def ab(a: int): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, 2)
          | ^^ Clicking here
          |
        info: Found 2 declarations
         --> mymodule.pyi:5:5
          |
        4 | @overload
        5 | def ab(a: int, b: int): ...
          |     --
        6 |
        7 | @overload
        8 | def ab(a: int): ...
          |     --
          |
        ");
    }

    #[test]
    fn goto_declaration_overload_arity_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, b = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int, b: int): ...

@overload
def ab(a: int): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1)
          | ^^ Clicking here
          |
        info: Found 2 declarations
         --> mymodule.pyi:5:5
          |
        4 | @overload
        5 | def ab(a: int, b: int): ...
          |     --
        6 |
        7 | @overload
        8 | def ab(a: int): ...
          |     --
          |
        ");
    }

    #[test]
    fn goto_declaration_overload_keyword_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1, b=2)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, *, b = None, c = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int): ...

@overload
def ab(a: int, *, b: int): ...

@overload
def ab(a: int, *, c: int): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, b=2)
          | ^^ Clicking here
          |
        info: Found 3 declarations
          --> mymodule.pyi:5:5
           |
         4 | @overload
         5 | def ab(a: int): ...
           |     --
         6 |
         7 | @overload
         8 | def ab(a: int, *, b: int): ...
           |     --
         9 |
        10 | @overload
        11 | def ab(a: int, *, c: int): ...
           |     --
           |
        ");
    }

    #[test]
    fn goto_declaration_overload_keyword_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1, c=2)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, *, b = None, c = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int): ...

@overload
def ab(a: int, *, b: int): ...

@overload
def ab(a: int, *, c: int): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, c=2)
          | ^^ Clicking here
          |
        info: Found 3 declarations
          --> mymodule.pyi:5:5
           |
         4 | @overload
         5 | def ab(a: int): ...
           |     --
         6 |
         7 | @overload
         8 | def ab(a: int, *, b: int): ...
           |     --
         9 |
        10 | @overload
        11 | def ab(a: int, *, c: int): ...
           |     --
           |
        ");
    }

    #[test]
    fn goto_declaration_submodule_import_from_use() {
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

        // TODO(submodule-imports): this should only highlight `subpkg` in the import statement
        // This happens because DefinitionKind::ImportFromSubmodule claims the entire ImportFrom node,
        // which is correct but unhelpful. Unfortunately even if it only claimed the LHS identifier it
        // would highlight `subpkg.submod` which is strictly better but still isn't what we want.
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
        4 | x = subpkg
          |     ^^^^^^ Clicking here
          |
        info: Found 1 declaration
         --> mypackage/__init__.py:2:1
          |
        2 | from .subpkg.submod import val
          | ------------------------------
        3 |
        4 | x = subpkg
          |
        ");
    }

    #[test]
    fn goto_declaration_submodule_import_from_def() {
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

        // TODO(submodule-imports): I don't *think* this is what we want..?
        // It's a bit confusing because this symbol is essentially the LHS *and* RHS of
        // `subpkg = mypackage.subpkg`. As in, it's both defining a local `subpkg` and
        // loading the module `mypackage.subpkg`, so, it's understandable to get confused!
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg.submod import val
          |       ^^^^^^ Clicking here
        3 |
        4 | x = subpkg
          |
        info: Found 1 declaration
        --> mypackage/subpkg/__init__.py:1:1
         |
         |
        ");
    }

    #[test]
    fn goto_declaration_submodule_import_from_wrong_use() {
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

        // No result is correct!
        assert_snapshot!(test.goto_declaration(), @"No goto target found");
    }

    #[test]
    fn goto_declaration_submodule_import_from_wrong_def() {
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

        // Going to the submod module is correct!
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> mypackage/__init__.py:2:14
          |
        2 | from .subpkg.submod import val
          |              ^^^^^^ Clicking here
        3 |
        4 | x = submod
          |
        info: Found 1 declaration
         --> mypackage/subpkg/submod.py:1:1
          |
        1 |
          | -
        2 | val: int = 0
          |
        ");
    }

    #[test]
    fn goto_declaration_submodule_import_from_confusing_shadowed_def() {
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

        // Going to the subpkg module is correct!
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg import subpkg
          |       ^^^^^^ Clicking here
        3 |
        4 | x = subpkg
          |
        info: Found 1 declaration
         --> mypackage/subpkg/__init__.py:1:1
          |
        1 |
          | -
        2 | subpkg: int = 10
          |
        ");
    }

    #[test]
    fn goto_declaration_submodule_import_from_confusing_real_def() {
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

        // Going to the subpkg `int` is correct!
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> mypackage/__init__.py:2:21
          |
        2 | from .subpkg import subpkg
          |                     ^^^^^^ Clicking here
        3 |
        4 | x = subpkg
          |
        info: Found 1 declaration
         --> mypackage/subpkg/__init__.py:2:1
          |
        2 | subpkg: int = 10
          | ------
          |
        ");
    }

    #[test]
    fn goto_declaration_submodule_import_from_confusing_use() {
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

        // TODO(submodule-imports): Ok this one is FASCINATING and it's kinda right but confusing!
        //
        // So there's 3 relevant definitions here:
        //
        // * `subpkg: int = 10` in the other file is in fact the original definition
        //
        // *  the LHS `subpkg` in the import is an instance of `subpkg = ...`
        //    because it's a `DefinitionKind::ImportFromSubmodle`.
        //    This is the span that covers the entire import.
        //
        // * `the RHS `subpkg` in the import is a second instance of `subpkg = ...`
        //    that *immediately* overwrites the `ImportFromSubmodule`'s definition
        //    This span seemingly doesn't appear at all!? Is it getting hidden by the LHS span?
        assert_snapshot!(test.goto_declaration(), @r"
        info[goto-declaration]: Go to declaration
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg import subpkg
        3 |
        4 | x = subpkg
          |     ^^^^^^ Clicking here
          |
        info: Found 2 declarations
         --> mypackage/__init__.py:2:1
          |
        2 | from .subpkg import subpkg
          | --------------------------
        3 |
        4 | x = subpkg
          |
         ::: mypackage/subpkg/__init__.py:2:1
          |
        2 | subpkg: int = 10
          | ------
          |
        ");
    }

    // TODO: Should only return `a: int`
    #[test]
    fn redeclarations() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                a: str = "test"

                a: int = 10

                print(a<CURSOR>)

                a: bool = True
                "#,
            )
            .build();

        assert_snapshot!(test.goto_declaration(), @r#"
        info[goto-declaration]: Go to declaration
         --> main.py:6:7
          |
        4 | a: int = 10
        5 |
        6 | print(a)
          |       ^ Clicking here
        7 |
        8 | a: bool = True
          |
        info: Found 3 declarations
         --> main.py:2:1
          |
        2 | a: str = "test"
          | -
        3 |
        4 | a: int = 10
          | -
        5 |
        6 | print(a)
        7 |
        8 | a: bool = True
          | -
          |
        "#);
    }

    impl CursorTest {
        fn goto_declaration(&self) -> String {
            let Some(targets) = salsa::attach(&self.db, || {
                goto_declaration(&self.db, self.cursor.file, self.cursor.offset)
            }) else {
                return "No goto target found".to_string();
            };

            if targets.is_empty() {
                return "No declarations found".to_string();
            }

            self.render_diagnostics([crate::goto_definition::test::GotoDiagnostic::new(
                crate::goto_definition::test::GotoAction::Declaration,
                targets,
            )])
        }
    }
}

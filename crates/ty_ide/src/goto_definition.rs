use crate::goto::find_goto_target;
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::{ImportAliasResolution, SemanticModel};

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
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;
    let definition_targets = goto_target
        .get_definition_targets(&model, ImportAliasResolution::ResolveAliases)?
        .definition_targets(db)?;

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: definition_targets,
    })
}

#[cfg(test)]
pub(super) mod test {

    use crate::tests::{CursorTest, IntoDiagnostic};
    use crate::{NavigationTargets, RangedValue, goto_definition};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:2:6
          |
        2 | from mymodule import my_function
          |      ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:1:1
          |
        1 |
          | -
        2 | def my_function():
        3 |     return "hello"
          |
        "#);
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
        info[goto-definition]: Go to definition
         --> main.py:3:5
          |
        2 | import mymodule
        3 | x = mymodule
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:1:1
          |
        1 |
          | -
        2 | def my_function():
        3 |     return "hello"
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
        info[goto-definition]: Go to definition
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     -----------
        3 |     return "hello"
          |
        "#);
    }

    /// goto-definition on a function definition in a .pyi should go to the .py
    #[test]
    fn goto_definition_stub_map_function_def() {
        let test = CursorTest::builder()
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
def my_fun<CURSOR>ction(): ...

def other_function(): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> mymodule.pyi:2:5
          |
        2 | def my_function(): ...
          |     ^^^^^^^^^^^ Clicking here
        3 |
        4 | def other_function(): ...
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     -----------
        3 |     return "hello"
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
        info[goto-definition]: Go to definition
         --> main.py:3:7
          |
        2 | from mymodule import my_function
        3 | print(my_function())
          |       ^^^^^^^^^^^ Clicking here
          |
        info: Found 3 definitions
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     -----------
        3 |     return "hello"
        4 |
        5 | def my_function():
          |     -----------
        6 |     return "hello again"
        7 |
        8 | def my_function():
          |     -----------
        9 |     return "we can't keep doing this"
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
        info[goto-definition]: Go to definition
         --> main.py:3:5
          |
        2 | from mymodule import MyClass
        3 | x = MyClass
          |     ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       -------
        3 |     def __init__(self, val):
        4 |         self.val = val
          |
        ");
    }

    /// goto-definition on a class def in a .pyi should go to the .py
    #[test]
    fn goto_definition_stub_map_class_def() {
        let test = CursorTest::builder()
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
class MyCl<CURSOR>ass:
    def __init__(self, val: bool): ...

class MyOtherClass:
    def __init__(self, val: bool): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> mymodule.pyi:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ Clicking here
        3 |     def __init__(self, val: bool): ...
          |
        info: Found 1 definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       -------
        3 |     def __init__(self, val):
        4 |         self.val = val
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
        info[goto-definition]: Go to definition
         --> main.py:3:5
          |
        2 | from mymodule import MyClass
        3 | x = MyClass(0)
          |     ^^^^^^^ Clicking here
          |
        info: Found 2 definitions
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       -------
        3 |     def __init__(self, val):
          |         --------
        4 |         self.val = val
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
        info[goto-definition]: Go to definition
         --> main.py:4:3
          |
        2 | from mymodule import MyClass
        3 | x = MyClass(0)
        4 | x.action()
          |   ^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:5:9
          |
        3 |     def __init__(self, val):
        4 |         self.val = val
        5 |     def action(self):
          |         ------
        6 |         print(self.val)
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
        info[goto-definition]: Go to definition
         --> main.py:3:13
          |
        2 | from mymodule import MyClass
        3 | x = MyClass.action()
          |             ^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:5:9
          |
        3 |     def __init__(self, val):
        4 |         self.val = val
        5 |     def action():
          |         ------
        6 |         print("hi!")
          |
        "#);
    }

    /// goto-definition on a class import should go to the .py not the .pyi
    #[test]
    fn goto_definition_stub_map_class_import() {
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
        info[goto-definition]: Go to definition
         --> main.py:2:22
          |
        2 | from mymodule import MyClass
          |                      ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:7
          |
        2 | class MyClass: ...
          |       -------
          |
        ");
    }

    /// goto-definition on a nested call using a keyword arg where both funcs have that arg name
    ///
    /// In this case they ultimately have different signatures.
    #[test]
    fn goto_definition_nested_keyword_arg1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_func(ab, y, z = None): ...
def my_other_func(ab, y): ...

my_other_func(my_func(a<CURSOR>b=5, y=2), 0)
my_func(my_other_func(ab=5, y=2), 0)
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:5:23
          |
        3 | def my_other_func(ab, y): ...
        4 |
        5 | my_other_func(my_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
        6 | my_func(my_other_func(ab=5, y=2), 0)
          |
        info: Found 1 definition
         --> main.py:2:13
          |
        2 | def my_func(ab, y, z = None): ...
          |             --
        3 | def my_other_func(ab, y): ...
          |
        ");
    }

    /// goto-definition on a nested call using a keyword arg where both funcs have that arg name
    ///
    /// In this case they ultimately have different signatures.
    #[test]
    fn goto_definition_nested_keyword_arg2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_func(ab, y, z = None): ...
def my_other_func(ab, y): ...

my_other_func(my_func(ab=5, y=2), 0)
my_func(my_other_func(a<CURSOR>b=5, y=2), 0)
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:6:23
          |
        5 | my_other_func(my_func(ab=5, y=2), 0)
        6 | my_func(my_other_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:19
          |
        2 | def my_func(ab, y, z = None): ...
        3 | def my_other_func(ab, y): ...
          |                   --
        4 |
        5 | my_other_func(my_func(ab=5, y=2), 0)
          |
        ");
    }

    /// goto-definition on a nested call using a keyword arg where both funcs have that arg name
    ///
    /// In this case they have identical signatures.
    #[test]
    fn goto_definition_nested_keyword_arg3() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_func(ab, y): ...
def my_other_func(ab, y): ...

my_other_func(my_func(a<CURSOR>b=5, y=2), 0)
my_func(my_other_func(ab=5, y=2), 0)
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:5:23
          |
        3 | def my_other_func(ab, y): ...
        4 |
        5 | my_other_func(my_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
        6 | my_func(my_other_func(ab=5, y=2), 0)
          |
        info: Found 1 definition
         --> main.py:2:13
          |
        2 | def my_func(ab, y): ...
          |             --
        3 | def my_other_func(ab, y): ...
          |
        ");
    }

    /// goto-definition on a nested call using a keyword arg where both funcs have that arg name
    ///
    /// In this case they have identical signatures.
    #[test]
    fn goto_definition_nested_keyword_arg4() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_func(ab, y): ...
def my_other_func(ab, y): ...

my_other_func(my_func(ab=5, y=2), 0)
my_func(my_other_func(a<CURSOR>b=5, y=2), 0)
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:6:23
          |
        5 | my_other_func(my_func(ab=5, y=2), 0)
        6 | my_func(my_other_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:19
          |
        2 | def my_func(ab, y): ...
        3 | def my_other_func(ab, y): ...
          |                   --
        4 |
        5 | my_other_func(my_func(ab=5, y=2), 0)
          |
        ");
    }

    #[test]
    fn goto_definition_overload_type_disambiguated1() {
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a):
          |     --
        3 |     """the real implementation!"""
          |
        "#);
    }

    #[test]
    fn goto_definition_overload_type_disambiguated2() {
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab("hello")
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a):
          |     --
        3 |     """the real implementation!"""
          |
        "#);
    }

    #[test]
    fn goto_definition_overload_arity_disambiguated1() {
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, 2)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, b = None):
          |     --
        3 |     """the real implementation!"""
          |
        "#);
    }

    #[test]
    fn goto_definition_overload_arity_disambiguated2() {
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, b = None):
          |     --
        3 |     """the real implementation!"""
          |
        "#);
    }

    #[test]
    fn goto_definition_overload_keyword_disambiguated1() {
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, b=2)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, *, b = None, c = None):
          |     --
        3 |     """the real implementation!"""
          |
        "#);
    }

    #[test]
    fn goto_definition_overload_keyword_disambiguated2() {
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, c=2)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, *, b = None, c = None):
          |     --
        3 |     """the real implementation!"""
          |
        "#);
    }

    #[test]
    fn goto_definition_binary_operator() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __add__(self, other):
        return Test()


a = Test()
b = Test()

a <CURSOR>+ b
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
          --> main.py:10:3
           |
         8 | b = Test()
         9 |
        10 | a + b
           |   ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __add__(self, other):
          |         -------
        4 |         return Test()
          |
        ");
    }

    #[test]
    fn goto_definition_binary_operator_reflected_dunder() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class A:
    def __radd__(self, other) -> A:
        return self

class B: ...

B() <CURSOR>+ A()
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:8:5
          |
        6 | class B: ...
        7 |
        8 | B() + A()
          |     ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class A:
        3 |     def __radd__(self, other) -> A:
          |         --------
        4 |         return self
          |
        ");
    }

    #[test]
    fn goto_definition_binary_operator_no_spaces_before_operator() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __add__(self, other):
        return Test()


a = Test()
b = Test()

a<CURSOR>+b
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
          --> main.py:10:2
           |
         8 | b = Test()
         9 |
        10 | a+b
           |  ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __add__(self, other):
          |         -------
        4 |         return Test()
          |
        ");
    }

    #[test]
    fn goto_definition_binary_operator_no_spaces_after_operator() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __add__(self, other):
        return Test()


a = Test()
b = Test()

a+<CURSOR>b
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
          --> main.py:10:3
           |
         8 | b = Test()
         9 |
        10 | a+b
           |   ^ Clicking here
           |
        info: Found 1 definition
          --> main.py:8:1
           |
         7 | a = Test()
         8 | b = Test()
           | -
         9 |
        10 | a+b
           |
        ");
    }

    #[test]
    fn goto_definition_binary_operator_comment() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __add__(self, other):
        return Test()


(
    Test()  <CURSOR># comment
    + Test()
)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    #[test]
    fn goto_definition_unary_operator() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __invert__(self) -> 'Test': ...

a = Test()

<CURSOR>~a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        5 | a = Test()
        6 |
        7 | ~a
          | ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __invert__(self) -> 'Test': ...
          |         ----------
        4 |
        5 | a = Test()
          |
        ");
    }

    /// We jump to the `__invert__` definition here even though its signature is incorrect.
    #[test]
    fn goto_definition_unary_operator_with_bad_dunder_definition() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __invert__(self, extra_arg) -> 'Test': ...

a = Test()

<CURSOR>~a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        5 | a = Test()
        6 |
        7 | ~a
          | ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __invert__(self, extra_arg) -> 'Test': ...
          |         ----------
        4 |
        5 | a = Test()
          |
        ");
    }

    #[test]
    fn goto_definition_unary_after_operator() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __invert__(self) -> 'Test': ...

a = Test()

~<CURSOR> a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        5 | a = Test()
        6 |
        7 | ~ a
          | ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __invert__(self) -> 'Test': ...
          |         ----------
        4 |
        5 | a = Test()
          |
        ");
    }

    #[test]
    fn goto_definition_unary_between_operator_and_operand() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __invert__(self) -> 'Test': ...

a = Test()

-<CURSOR>a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:2
          |
        5 | a = Test()
        6 |
        7 | -a
          |  ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:5:1
          |
        3 |     def __invert__(self) -> 'Test': ...
        4 |
        5 | a = Test()
          | -
        6 |
        7 | -a
          |
        ");
    }

    #[test]
    fn goto_definition_unary_not_with_dunder_bool() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __bool__(self) -> bool: ...

a = Test()

<CURSOR>not a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        5 | a = Test()
        6 |
        7 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __bool__(self) -> bool: ...
          |         --------
        4 |
        5 | a = Test()
          |
        ");
    }

    #[test]
    fn goto_definition_unary_not_with_dunder_len() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __len__(self) -> 42: ...

a = Test()

<CURSOR>not a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        5 | a = Test()
        6 |
        7 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __len__(self) -> 42: ...
          |         -------
        4 |
        5 | a = Test()
          |
        ");
    }

    /// If `__bool__` is defined incorrectly, `not` does not fallback to `__len__`.
    /// Instead, we jump to the `__bool__` definition as usual.
    /// The fallback only occurs if `__bool__` is not defined at all.
    #[test]
    fn goto_definition_unary_not_with_bad_dunder_bool_and_dunder_len() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __bool__(self, extra_arg) -> bool: ...
    def __len__(self) -> 42: ...

a = Test()

<CURSOR>not a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:8:1
          |
        6 | a = Test()
        7 |
        8 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __bool__(self, extra_arg) -> bool: ...
          |         --------
        4 |     def __len__(self) -> 42: ...
          |
        ");
    }

    /// Same as for unary operators that only use a single dunder,
    /// we still jump to `__len__` for `not` goto-definition even if
    /// the `__len__` signature is incorrect (but only if there is no
    /// `__bool__` definition).
    #[test]
    fn goto_definition_unary_not_with_no_dunder_bool_and_bad_dunder_len() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Test:
    def __len__(self, extra_arg) -> 42: ...

a = Test()

<CURSOR>not a
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        5 | a = Test()
        6 |
        7 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        2 | class Test:
        3 |     def __len__(self, extra_arg) -> 42: ...
          |         -------
        4 |
        5 | a = Test()
          |
        ");
    }

    #[test]
    fn float_annotation() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
a: float<CURSOR> = 3.14
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:2:4
          |
        2 | a: float = 3.14
          |    ^^^^^ Clicking here
          |
        info: Found 2 definitions
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ---
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
           ::: stdlib/builtins.pyi:661:7
            |
        660 | @disjoint_base
        661 | class float:
            |       -----
        662 |     """Convert a string or number to a floating-point number, if possible."""
            |
        "#);
    }

    #[test]
    fn complex_annotation() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
a: complex<CURSOR> = 3.14
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:2:4
          |
        2 | a: complex = 3.14
          |    ^^^^^^^ Clicking here
          |
        info: Found 3 definitions
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ---
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
           ::: stdlib/builtins.pyi:661:7
            |
        660 | @disjoint_base
        661 | class float:
            |       -----
        662 |     """Convert a string or number to a floating-point number, if possible."""
            |
           ::: stdlib/builtins.pyi:822:7
            |
        821 | @disjoint_base
        822 | class complex:
            |       -------
        823 |     """Create a complex number from a string or numbers.
            |
        "#);
    }

    /// Regression test for <https://github.com/astral-sh/ty/issues/1451>.
    /// We must ensure we respect re-import convention for stub files for
    /// imports in builtins.pyi.
    #[test]
    fn goto_definition_unimported_symbol_imported_in_builtins() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
Traceb<CURSOR>ackType
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    // TODO: Should only list `a: int`
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

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:6:7
          |
        4 | a: int = 10
        5 |
        6 | print(a)
          |       ^ Clicking here
        7 |
        8 | a: bool = True
          |
        info: Found 3 definitions
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
        fn goto_definition(&self) -> String {
            let Some(targets) = salsa::attach(&self.db, || {
                goto_definition(&self.db, self.cursor.file, self.cursor.offset)
            }) else {
                return "No goto target found".to_string();
            };

            if targets.is_empty() {
                return "No definitions found".to_string();
            }

            self.render_diagnostics([GotoDiagnostic::new(GotoAction::Definition, targets)])
        }
    }

    pub(crate) struct GotoDiagnostic {
        action: GotoAction,
        targets: RangedValue<NavigationTargets>,
    }

    impl GotoDiagnostic {
        pub(crate) fn new(action: GotoAction, targets: RangedValue<NavigationTargets>) -> Self {
            Self { action, targets }
        }
    }

    impl IntoDiagnostic for GotoDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let source = self.targets.range;
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of(self.action.name())),
                Severity::Info,
                self.action.label().to_string(),
            );

            main.annotate(
                Annotation::primary(Span::from(source.file()).with_range(source.range()))
                    .message("Clicking here"),
            );

            let mut sub = SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format_args!(
                    "Found {} {}{}",
                    self.targets.len(),
                    self.action.item_label(),
                    if self.targets.len() == 1 { "" } else { "s" }
                ),
            );

            for target in self.targets {
                sub.annotate(Annotation::secondary(
                    Span::from(target.file()).with_range(target.focus_range()),
                ));
            }

            main.sub(sub);

            main
        }
    }

    pub(crate) enum GotoAction {
        Definition,
        Declaration,
        TypeDefinition,
    }

    impl GotoAction {
        fn name(&self) -> &'static str {
            match self {
                GotoAction::Definition => "goto-definition",
                GotoAction::Declaration => "goto-declaration",
                GotoAction::TypeDefinition => "goto-type definition",
            }
        }

        fn label(&self) -> &'static str {
            match self {
                GotoAction::Definition => "Go to definition",
                GotoAction::Declaration => "Go to declaration",
                GotoAction::TypeDefinition => "Go to type definition",
            }
        }

        fn item_label(&self) -> &'static str {
            match self {
                GotoAction::Definition => "definition",
                GotoAction::Declaration => "declaration",
                GotoAction::TypeDefinition => "type definition",
            }
        }
    }
}

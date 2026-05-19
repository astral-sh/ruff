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
        .definitions(&model, ImportAliasResolution::ResolveAliases)?
        .goto_definition(&model, &goto_target)?
        .into_navigation_targets(model.db());

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

    #[test]
    fn goto_definition_relative_import() {
        let test = CursorTest::builder()
            .source("mypackage/__init__.py", "from . import module_a<CURSOR>")
            .source("mypackage/module_a.py", "class Test: ...")
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> mypackage/__init__.py:1:15
          |
        1 | from . import module_a
          |               ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mypackage/module_a.py:1:1
          |
        1 | class Test: ...
          | -
          |
        ");
    }

    #[test]
    fn goto_definition_relative_import_reference() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                "from . import module_a\nx = module_a<CURSOR>",
            )
            .source("mypackage/module_a.py", "class Test: ...")
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> mypackage/__init__.py:2:5
          |
        2 | x = module_a
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mypackage/module_a.py:1:1
          |
        1 | class Test: ...
          | -
          |
        ");
    }

    #[test]
    fn goto_definition_relative_star_imported_submodule_reference() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                "from .exporter import *\nx = module_a<CURSOR>",
            )
            .source("mypackage/exporter.py", "from . import module_a")
            .source("mypackage/module_a.py", "class Test: ...")
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> mypackage/__init__.py:2:5
          |
        2 | x = module_a
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mypackage/module_a.py:1:1
          |
        1 | class Test: ...
          | -
          |
        ");
    }

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

        assert_snapshot!(test.goto_definition(), @"
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:3:5
          |
        3 | x = mymodule
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:1:1
          |
        1 |
          | -
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:3:7
          |
        3 | print(my_function())
          |       ^^^^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     -----------
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> mymodule.pyi:2:5
          |
        2 | def my_function(): ...
          |     ^^^^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def my_function():
          |     -----------
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:3:5
          |
        3 | x = MyClass
          |     ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> mymodule.pyi:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:3:5
          |
        3 | x = MyClass(0)
          |     ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:7
          |
        2 | class MyClass:
          |       -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:3
          |
        4 | x.action()
          |   ^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:5:9
          |
        5 |     def action(self):
          |         ------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:3:13
          |
        3 | x = MyClass.action()
          |             ^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:5:9
          |
        5 |     def action():
          |         ------
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:5:23
          |
        5 | my_other_func(my_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:2:13
          |
        2 | def my_func(ab, y, z = None): ...
          |             --
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:6:23
          |
        6 | my_func(my_other_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:19
          |
        3 | def my_other_func(ab, y): ...
          |                   --
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:5:23
          |
        5 | my_other_func(my_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:2:13
          |
        2 | def my_func(ab, y): ...
          |             --
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:6:23
          |
        6 | my_func(my_other_func(ab=5, y=2), 0)
          |                       ^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:19
          |
        3 | def my_other_func(ab, y): ...
          |                   --
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        4 | ab(1)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a):
          |     --
          |
        ");
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
        4 | ab("hello")
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a):
          |     --
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        4 | ab(1, 2)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, b = None):
          |     --
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        4 | ab(1)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, b = None):
          |     --
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        4 | ab(1, b=2)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, *, b = None, c = None):
          |     --
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:1
          |
        4 | ab(1, c=2)
          | ^^ Clicking here
          |
        info: Found 1 definition
         --> mymodule.py:2:5
          |
        2 | def ab(a, *, b = None, c = None):
          |     --
          |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:10:3
           |
        10 | a + b
           |   ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __add__(self, other):
          |         -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:5
          |
        8 | B() + A()
          |     ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __radd__(self, other) -> A:
          |         --------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:10:2
           |
        10 | a+b
           |  ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __add__(self, other):
          |         -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:10:3
           |
        10 | a+b
           |   ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:8:1
          |
        8 | b = Test()
          | -
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        7 | ~a
          | ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __invert__(self) -> 'Test': ...
          |         ----------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        7 | ~a
          | ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __invert__(self, extra_arg) -> 'Test': ...
          |         ----------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        7 | ~ a
          | ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __invert__(self) -> 'Test': ...
          |         ----------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:2
          |
        7 | -a
          |  ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:5:1
          |
        5 | a = Test()
          | -
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        7 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __bool__(self) -> bool: ...
          |         --------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        7 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __len__(self) -> 42: ...
          |         -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:1
          |
        8 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __bool__(self, extra_arg) -> bool: ...
          |         --------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:1
          |
        7 | not a
          | ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __len__(self, extra_arg) -> 42: ...
          |         -------
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:2:4
          |
        2 | a: float = 3.14
          |    ^^^^^ Clicking here
          |
        info: Found 2 definitions
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ---
            |
           ::: stdlib/builtins.pyi:661:7
            |
        661 | class float:
            |       -----
            |
        ");
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

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:2:4
          |
        2 | a: complex = 3.14
          |    ^^^^^^^ Clicking here
          |
        info: Found 3 definitions
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ---
            |
           ::: stdlib/builtins.pyi:661:7
            |
        661 | class float:
            |       -----
            |
           ::: stdlib/builtins.pyi:822:7
            |
        822 | class complex:
            |       -------
            |
        ");
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

    /// goto-definition on a class init opening parenthesis should go to constructor
    #[test]
    fn goto_definition_class_init_parenthesis_opening() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class MyClass:
    def __init__(self, val):
        self.val = val
x = MyClass<CURSOR>()
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:5:5
          |
        5 | x = MyClass()
          |     ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __init__(self, val):
          |         --------
          |
        ");
    }

    /// goto-definition on a class init closing parenthesis should go to constructor
    #[test]
    fn goto_definition_class_init_parenthesis_closing() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class MyClass:
    def __init__(self, val):
        self.val = val
x = MyClass(<CURSOR>)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:5:5
          |
        5 | x = MyClass()
          |     ^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __init__(self, val):
          |         --------
          |
        ");
    }

    /// goto-definition on a class init closing parenthesis
    /// when there is an argument is somewhat ambiguous, and
    /// so doesn't find any defs.
    #[test]
    fn goto_definition_class_init_parenthesis_ambiguous_closing() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class MyClass:
    def __init__(self, val):
        self.val = val
x = MyClass(0<CURSOR>)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    /// goto-definition on a class init closing parenthesis when there
    /// is an argument with its own definition is somewhat ambiguous,
    /// and but we currently go to the definition of the argument.
    #[test]
    fn goto_definition_class_init_parenthesis_ambiguous_argument_closing() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
foo = 1

class MyClass:
    def __init__(self, val):
        self.val = val
x = MyClass(foo<CURSOR>)
",
            )
            .build();

        assert_snapshot!(
            test.goto_definition(),
            @"
        info[goto-definition]: Go to definition
         --> main.py:7:13
          |
        7 | x = MyClass(foo)
          |             ^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:2:1
          |
        2 | foo = 1
          | ---
          |
        ",
        );
    }

    /// goto-definition on a class init parenthesis includes `__new__`
    #[test]
    fn goto_definition_class_init_parenthesis_includes_new() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class MyClass:
    def __init__(self, val):
        self.val = val
    def __new__(self, val):
        self.val = val
x = MyClass<CURSOR>()
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:7:5
          |
        7 | x = MyClass()
          |     ^^^^^^^ Clicking here
          |
        info: Found 2 definitions
         --> main.py:3:9
          |
        3 |     def __init__(self, val):
          |         --------
        4 |         self.val = val
        5 |     def __new__(self, val):
          |         -------
          |
        ");
    }

    /// goto-definition on a dynamic class literal (created via `type()`)
    #[test]
    fn goto_definition_dynamic_class_literal() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
DynClass = type("DynClass", (), {})

x = DynCla<CURSOR>ss()
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:4:5
          |
        4 | x = DynClass()
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:2:1
          |
        2 | DynClass = type("DynClass", (), {})
          | --------
          |
        "#);
    }

    /// goto-definition on a dynamic class literal (created via `type()`)
    /// when on the opening parenthesis.
    ///
    /// Unlike when the cursor is on the `DynClass` name itself, this
    /// will report the constructor method as the definition.
    #[test]
    fn goto_definition_dynamic_class_literal_parenthesis() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
DynClass = type("DynClass", (), {})

x = DynClass<CURSOR>()
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:5
          |
        4 | x = DynClass()
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
           --> stdlib/builtins.pyi:137:9
            |
        137 |     def __new__(cls) -> Self: ...
            |         -------
            |
        ");
    }

    /// goto-definition on a dangling dynamic class literal (not assigned to a variable)
    #[test]
    fn goto_definition_dangling_dynamic_class_literal() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
class Foo(type("Ba<CURSOR>r", (), {})):
    pass
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    /// goto-definition on a dynamic namedtuple class literal (created via `collections.namedtuple()`)
    #[test]
    fn goto_definition_dynamic_namedtuple_literal() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from collections import namedtuple

Point = namedtuple("Point", ["x", "y"])

p = Poi<CURSOR>nt(1, 2)
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:6:5
          |
        6 | p = Point(1, 2)
          |     ^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:4:1
          |
        4 | Point = namedtuple("Point", ["x", "y"])
          | -----
          |
        "#);
    }

    /// goto-definition on a dynamic namedtuple class literal via opening parenthesis.
    ///
    /// At time of writing (2026-02-04), goto-def doesn't report
    /// any possible constructor methods for this case. But normally,
    /// clicking on an opening parenthesis only goes to constructor
    /// methods. So this tests that even in that case, we still go
    /// to the actual definition.
    #[test]
    fn goto_definition_dynamic_namedtuple_literal_parenthesis() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from collections import namedtuple

Point = namedtuple("Point", ["x", "y"])

p = Point<CURSOR>(1, 2)
"#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @r#"
        info[goto-definition]: Go to definition
         --> main.py:6:5
          |
        6 | p = Point(1, 2)
          |     ^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:4:1
          |
        4 | Point = namedtuple("Point", ["x", "y"])
          | -----
          |
        "#);
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
        6 | print(a)
          |       ^ Clicking here
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

    #[test]
    fn goto_definition_attribute_redeclarations() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                class Test:
                    a: str
                    a: str

                test = Test()

                test.a<CURSOR>
                "#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:6
          |
        8 | test.a
          |      ^ Clicking here
          |
        info: Found 2 definitions
         --> main.py:3:5
          |
        3 |     a: str
          |     -
        4 |     a: str
          |     -
          |
        ");
    }

    #[test]
    fn goto_definition_property_getter_and_setter() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                class Test:
                    @property
                    def a(self) -> str:
                        return ""

                    @a.setter
                    def a(self, value: str) -> None:
                        pass

                test = Test()

                test.a<CURSOR>
                "#,
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:13:6
           |
        13 | test.a
           |      ^ Clicking here
           |
        info: Found 2 definitions
         --> main.py:4:9
          |
        4 |     def a(self) -> str:
          |         -
          |
         ::: main.py:8:9
          |
        8 |     def a(self, value: str) -> None:
          |         -
          |
        ");
    }

    /// Goto-definition works when accessing type attributes on class objects.
    #[test]
    fn goto_definition_for_type_attributes_on_class_objects() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
                class Foo: ...

                Foo.__dictoff<CURSOR>set__
                ",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:4:5
          |
        4 | Foo.__dictoffset__
          |     ^^^^^^^^^^^^^^ Clicking here
          |
        info: Found 1 definition
           --> stdlib/builtins.pyi:262:9
            |
        262 |     def __dictoffset__(self) -> int: ...
            |         --------------
            |
        ");
    }

    /// Goto-definition performs lookups on the metaclass when attributes are not found.
    #[test]
    fn goto_definition_performs_lookups_on_metaclass() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
                class Foo(type):
                    a: int

                class Bar(metaclass=Foo): ...
                Bar.<CURSOR>a
                ",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:6:5
          |
        6 | Bar.a
          |     ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:5
          |
        3 |     a: int
          |     -
          |
        ");
    }

    /// Goto-definition does not look up instance members on the metaclass.
    #[test]
    fn goto_definition_on_members_of_class_instances() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
                class Foo(type):
                    a: int

                class Bar(metaclass=Foo): ...
                Bar().<CURSOR>a
                ",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    /// Check that we don't fall into infinite recursion when e.g.
    /// looking up attributes on the metaclass of `type`
    /// (`type` is its own metaclass)
    #[test]
    fn goto_definition_on_builtins_dot_type_itself_unresolved() {
        let test = CursorTest::builder()
            .source("main.py", "type.<CURSOR>a")
            .build();

        assert_snapshot!(test.goto_definition(), @"No goto target found");
    }

    /// Check that we don't fall into infinite recursion when e.g.
    /// looking up attributes on the metaclass of `type`
    /// (`type` is its own metaclass)
    #[test]
    fn goto_definition_on_builtins_dot_type_itself_resolved() {
        let test = CursorTest::builder()
            .source("main.py", "type.__dict<CURSOR>offset__")
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:1:6
          |
        1 | type.__dictoffset__
          |      ^^^^^^^^^^^^^^ Clicking here
          |
        info: Found 1 definition
           --> stdlib/builtins.pyi:262:9
            |
        262 |     def __dictoffset__(self) -> int: ...
            |         --------------
            |
        ");
    }

    /// Go-to-definition should not point to while-loop header definitions.
    #[test]
    fn goto_definition_does_not_point_to_while_loop_header() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
while True:
    variable = 1

    vari<CURSOR>able
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:5:5
          |
        5 |     variable
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:5
          |
        3 |     variable = 1
          |     --------
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_typeddict() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from typing import TypedDict

class TD(TypedDict):
    f: int
    g: str

TD(f<CURSOR>=1)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:4
          |
        8 | TD(f=1)
          |    ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:5:5
          |
        5 |     f: int
          |     -
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_typeddict_update() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from typing import TypedDict

class TD(TypedDict):
    f: int
    g: str

td = TD(f=1, g=\"\")
td.update(f<CURSOR>=2)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:9:11
          |
        9 | td.update(f=2)
          |           ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:5:5
          |
        5 |     f: int
          |     -
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_unpack_typeddict() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from typing import TypedDict, Unpack

class TD(TypedDict):
    f: int
    g: str

def func(**kwargs: Unpack[TD]): ...

func(f<CURSOR>=1)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:10:6
           |
        10 | func(f=1)
           |      ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:5:5
          |
        5 |     f: int
          |     -
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_namedtuple() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from typing import NamedTuple

class NT(NamedTuple):
    f: int
    g: str

NT(f<CURSOR>=1)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:4
          |
        8 | NT(f=1)
          |    ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:5:5
          |
        5 |     f: int
          |     -
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_dataclass() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from dataclasses import dataclass

@dataclass
class DC:
    f: int
    g: str

DC(f<CURSOR>=1)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:9:4
          |
        9 | DC(f=1)
          |    ^ Clicking here
          |
        info: Found 1 definition
         --> main.py:6:5
          |
        6 |     f: int
          |     -
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_dataclass_custom_init() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from dataclasses import dataclass

@dataclass
class DC:
    f: int
    g: str

    def __init__(self, f: int) -> None: ...

DC(f<CURSOR>=1)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:11:4
           |
        11 | DC(f=1)
           |    ^ Clicking here
           |
        info: Found 1 definition
         --> main.py:9:24
          |
        9 |     def __init__(self, f: int) -> None: ...
          |                        -
          |
        ");
    }

    #[test]
    fn goto_definition_keyword_argument_dataclass_transform_alias() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from typing import dataclass_transform

def Field(alias: str = ...): ...

@dataclass_transform(field_specifiers=(Field,))
class MyDataclass: ...

class DC(MyDataclass):
    f: int = Field(alias='g')

DC(g<CURSOR>=1)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
          --> main.py:12:4
           |
        12 | DC(g=1)
           |    ^ Clicking here
           |
        info: Found 1 definition
          --> main.py:10:5
           |
        10 |     f: int = Field(alias='g')
           |     -
           |
        ");
    }

    /// Go-to-definition should not point to for-loop header definitions.
    #[test]
    fn goto_definition_does_not_point_to_for_loop_header() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
for x in range(10):
    variable = 1

    vari<CURSOR>able
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:5:5
          |
        5 |     variable
          |     ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:5
          |
        3 |     variable = 1
          |     --------
          |
        ");
    }

    /// Go-to-definition on `super()` should not lookup on the super class itself
    #[test]
    fn goto_definition_does_not_lookup_on_bound_super() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Foo:
    def __init__(self, x: int) -> None:
        self.x = x

class Bar(Foo):
    def __init__(self):
        super().__init<CURSOR>__(x)
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:17
          |
        8 |         super().__init__(x)
          |                 ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __init__(self, x: int) -> None:
          |         --------
          |
        ");
    }

    /// Go-to-definition should resolve to the parent class
    #[test]
    fn goto_definition_resolves_super_for_generic_class() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class Base:
    def __init__(self, x: int) -> None:
        self.x = x

class GenericFoo[T](Base):
    def __init__(self, x: int, y: T):
        super().__init<CURSOR>__(x)
        self.y = y
",
            )
            .build();

        assert_snapshot!(test.goto_definition(), @"
        info[goto-definition]: Go to definition
         --> main.py:8:17
          |
        8 |         super().__init__(x)
          |                 ^^^^^^^^ Clicking here
          |
        info: Found 1 definition
         --> main.py:3:9
          |
        3 |     def __init__(self, x: int) -> None:
          |         --------
          |
        ");
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

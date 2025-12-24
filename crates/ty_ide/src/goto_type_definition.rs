use crate::goto::find_goto_target;
use crate::{Db, HasNavigationTargets, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::SemanticModel;

pub fn goto_type_definition(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;

    let ty = goto_target.inferred_type(&model)?;

    tracing::debug!("Inferred type of covering node is {}", ty.display(db));

    let navigation_targets = ty.navigation_targets(db);

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: navigation_targets,
    })
}

#[cfg(test)]
mod tests {
    use crate::goto_type_definition;
    use crate::tests::{CursorTest, cursor_test};
    use insta::assert_snapshot;

    #[test]
    fn goto_type_of_expression_with_class_type() {
        let test = cursor_test(
            r#"
            class Test: ...

            a<CURSOR>b = Test()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | class Test: ...
        3 |
        4 | ab = Test()
          | ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:7
          |
        2 | class Test: ...
          |       ----
        3 |
        4 | ab = Test()
          |
        ");
    }

    #[test]
    fn goto_type_of_typing_dot_literal() {
        let test = cursor_test(
            r#"
            from typing import Literal

            a<CURSOR>b = Literal
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | from typing import Literal
        3 |
        4 | ab = Literal
          | ^^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | -------
        352 | TypedDict: _SpecialForm
            |
        ");
    }

    // this is a slightly different case to the one above,
    // since `Any` is a class in typeshed rather than a variable
    #[test]
    fn goto_type_of_typing_dot_any() {
        let test = cursor_test(
            r#"
            from typing import Any

            a<CURSOR>b = Any
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | from typing import Any
        3 |
        4 | ab = Any
          | ^^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/typing.pyi:166:7
            |
        164 | # from _typeshed import AnnotationForm
        165 |
        166 | class Any:
            |       ---
        167 |     """Special type indicating an unconstrained type.
            |
        "#);
    }

    // Similarly, `Generic` is a `type[]` type in typeshed
    #[test]
    fn goto_type_of_typing_dot_generic() {
        let test = cursor_test(
            r#"
            from typing import Generic

            a<CURSOR>b = Generic
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | from typing import Generic
        3 |
        4 | ab = Generic
          | ^^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/typing.pyi:781:1
            |
        779 |         def __class_getitem__(cls, args: TypeVar | tuple[TypeVar, ...]) -> _Final: ...
        780 |
        781 | Generic: type[_Generic]
            | -------
        782 |
        783 | class _ProtocolMeta(ABCMeta):
            |
        ");
    }

    #[test]
    fn goto_type_of_ty_extensions_special_form() {
        let test = cursor_test(
            r#"
            from ty_extensions import AlwaysTruthy

            a<CURSOR>b = AlwaysTruthy
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | from ty_extensions import AlwaysTruthy
        3 |
        4 | ab = AlwaysTruthy
          | ^^ Clicking here
          |
        info: Found 1 type definition
          --> stdlib/ty_extensions.pyi:21:1
           |
        19 | # Types
        20 | Unknown = object()
        21 | AlwaysTruthy = object()
           | ------------
        22 | AlwaysFalsy = object()
           |
        ");
    }

    #[test]
    fn goto_type_of_expression_with_function_type() {
        let test = cursor_test(
            r#"
            def foo(a, b): ...

            ab = foo

            a<CURSOR>b
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:6:1
          |
        4 | ab = foo
        5 |
        6 | ab
          | ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:5
          |
        2 | def foo(a, b): ...
          |     ---
        3 |
        4 | ab = foo
          |
        ");
    }

    #[test]
    fn goto_type_of_expression_with_union_type() {
        let test = cursor_test(
            r#"

            def foo(a, b): ...

            def bar(a, b): ...

            if random.choice():
                a = foo
            else:
                a = bar

            a<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
          --> main.py:12:1
           |
        10 |     a = bar
        11 |
        12 | a
           | ^ Clicking here
           |
        info: Found 2 type definitions
         --> main.py:3:5
          |
        3 | def foo(a, b): ...
          |     ---
        4 |
        5 | def bar(a, b): ...
          |     ---
        6 |
        7 | if random.choice():
          |
        ");
    }

    #[test]
    fn goto_type_of_import_module() {
        let mut test = cursor_test(
            r#"
            import l<CURSOR>ib
            "#,
        );

        test.write_file("lib.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:8
          |
        2 | import lib
          |        ^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib.py:1:1
          |
        1 | a = 10
          | ------
          |
        ");
    }

    #[test]
    fn goto_type_of_import_module_multi1() {
        let mut test = cursor_test(
            r#"
            import li<CURSOR>b.submod
            "#,
        );

        test.write_file("lib/__init__.py", "b = 7").unwrap();
        test.write_file("lib/submod.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:8
          |
        2 | import lib.submod
          |        ^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib/__init__.py:1:1
          |
        1 | b = 7
          | -----
          |
        ");
    }

    #[test]
    fn goto_type_of_import_module_multi2() {
        let mut test = cursor_test(
            r#"
            import lib.subm<CURSOR>od
            "#,
        );

        test.write_file("lib/__init__.py", "b = 7").unwrap();
        test.write_file("lib/submod.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:12
          |
        2 | import lib.submod
          |            ^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib/submod.py:1:1
          |
        1 | a = 10
          | ------
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_module() {
        let mut test = cursor_test(
            r#"
            from l<CURSOR>ib import a
            "#,
        );

        test.write_file("lib.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:6
          |
        2 | from lib import a
          |      ^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib.py:1:1
          |
        1 | a = 10
          | ------
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_module_multi1() {
        let mut test = cursor_test(
            r#"
            from li<CURSOR>b.submod import a
            "#,
        );

        test.write_file("lib/__init__.py", "b = 7").unwrap();
        test.write_file("lib/submod.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:6
          |
        2 | from lib.submod import a
          |      ^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib/__init__.py:1:1
          |
        1 | b = 7
          | -----
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_module_multi2() {
        let mut test = cursor_test(
            r#"
            from lib.subm<CURSOR>od import a
            "#,
        );

        test.write_file("lib/__init__.py", "b = 7").unwrap();
        test.write_file("lib/submod.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:10
          |
        2 | from lib.submod import a
          |          ^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib/submod.py:1:1
          |
        1 | a = 10
          | ------
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_rel1() {
        let mut test = CursorTest::builder()
            .source(
                "lib/sub/__init__.py",
                r#"
            from .bot.bot<CURSOR>mod import *
            sub = 2
            "#,
            )
            .build();

        test.write_file("lib/__init__.py", "lib = 1").unwrap();
        // test.write_file("lib/sub/__init__.py", "sub = 2").unwrap();
        test.write_file("lib/sub/bot/__init__.py", "bot = 3")
            .unwrap();
        test.write_file("lib/sub/submod.py", "submod = 21").unwrap();
        test.write_file("lib/sub/bot/botmod.py", "botmod = 31")
            .unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> lib/sub/__init__.py:2:11
          |
        2 | from .bot.botmod import *
          |           ^^^^^^ Clicking here
        3 | sub = 2
          |
        info: Found 1 type definition
         --> lib/sub/bot/botmod.py:1:1
          |
        1 | botmod = 31
          | -----------
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_rel2() {
        let mut test = CursorTest::builder()
            .source(
                "lib/sub/__init__.py",
                r#"
            from .bo<CURSOR>t.botmod import *
            sub = 2
            "#,
            )
            .build();

        test.write_file("lib/__init__.py", "lib = 1").unwrap();
        // test.write_file("lib/sub/__init__.py", "sub = 2").unwrap();
        test.write_file("lib/sub/bot/__init__.py", "bot = 3")
            .unwrap();
        test.write_file("lib/sub/submod.py", "submod = 21").unwrap();
        test.write_file("lib/sub/bot/botmod.py", "botmod = 31")
            .unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> lib/sub/__init__.py:2:7
          |
        2 | from .bot.botmod import *
          |       ^^^ Clicking here
        3 | sub = 2
          |
        info: Found 1 type definition
         --> lib/sub/bot/__init__.py:1:1
          |
        1 | bot = 3
          | -------
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_rel3() {
        let mut test = CursorTest::builder()
            .source(
                "lib/sub/__init__.py",
                r#"
            from .<CURSOR>bot.botmod import *
            sub = 2
            "#,
            )
            .build();

        test.write_file("lib/__init__.py", "lib = 1").unwrap();
        // test.write_file("lib/sub/__init__.py", "sub = 2").unwrap();
        test.write_file("lib/sub/bot/__init__.py", "bot = 3")
            .unwrap();
        test.write_file("lib/sub/submod.py", "submod = 21").unwrap();
        test.write_file("lib/sub/bot/botmod.py", "botmod = 31")
            .unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> lib/sub/__init__.py:2:7
          |
        2 | from .bot.botmod import *
          |       ^^^ Clicking here
        3 | sub = 2
          |
        info: Found 1 type definition
         --> lib/sub/bot/__init__.py:1:1
          |
        1 | bot = 3
          | -------
          |
        ");
    }

    #[test]
    fn goto_type_of_from_import_rel4() {
        let mut test = CursorTest::builder()
            .source(
                "lib/sub/__init__.py",
                r#"
            from .<CURSOR> import submod
            sub = 2
            "#,
            )
            .build();

        test.write_file("lib/__init__.py", "lib = 1").unwrap();
        // test.write_file("lib/sub/__init__.py", "sub = 2").unwrap();
        test.write_file("lib/sub/bot/__init__.py", "bot = 3")
            .unwrap();
        test.write_file("lib/sub/submod.py", "submod = 21").unwrap();
        test.write_file("lib/sub/bot/botmod.py", "botmod = 31")
            .unwrap();

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_of_expression_with_module() {
        let mut test = cursor_test(
            r#"
            import lib

            lib<CURSOR>
            "#,
        );

        test.write_file("lib.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | import lib
        3 |
        4 | lib
          | ^^^ Clicking here
          |
        info: Found 1 type definition
         --> lib.py:1:1
          |
        1 | a = 10
          | ------
          |
        ");
    }

    #[test]
    fn goto_type_of_expression_with_literal_type() {
        let test = cursor_test(
            r#"
            a: str = "test"

            a<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | a: str = "test"
        3 |
        4 | a
          | ^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }
    #[test]
    fn goto_type_of_expression_with_literal_node() {
        let test = cursor_test(
            r#"
            a: str = "te<CURSOR>st"
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:10
          |
        2 | a: str = "test"
          |          ^^^^^^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }

    #[test]
    fn goto_type_of_expression_with_type_var_type() {
        let test = cursor_test(
            r#"
            type Alias[T: int = bool] = list[T<CURSOR>]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:34
          |
        2 | type Alias[T: int = bool] = list[T]
          |                                  ^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:12
          |
        2 | type Alias[T: int = bool] = list[T]
          |            -
          |
        ");
    }

    #[test]
    fn goto_type_of_expression_with_type_param_spec() {
        let test = cursor_test(
            r#"
            type Alias[**P = [int, str]] = Callable[P<CURSOR>, int]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:41
          |
        2 | type Alias[**P = [int, str]] = Callable[P, int]
          |                                         ^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:14
          |
        2 | type Alias[**P = [int, str]] = Callable[P, int]
          |              -
          |
        ");
    }

    #[test]
    fn goto_type_of_expression_with_type_var_tuple() {
        let test = cursor_test(
            r#"
            type Alias[*Ts = ()] = tuple[*Ts<CURSOR>]
            "#,
        );

        // TODO: Goto type definition currently doesn't work for type var tuples
        // because the inference doesn't support them yet.
        // This snapshot should show a single target pointing to `T`
        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_of_bare_type_alias_type() {
        let test = cursor_test(
            r#"
            from typing_extensions import TypeAliasType

            Alias = TypeAliasType("Alias", tuple[int, int])

            Alias<CURSOR>
            "#,
        );

        // TODO: This should jump to the definition of `Alias` above.
        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_string_annotation1() {
        let test = cursor_test(
            r#"
        a: "MyCla<CURSOR>ss" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:5
          |
        2 | a: "MyClass" = 1
          |     ^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 1 type definition
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
    fn goto_type_string_annotation2() {
        let test = cursor_test(
            r#"
        a: "None | MyCl<CURSOR>ass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_string_annotation3() {
        let test = cursor_test(
            r#"
        a: "None |<CURSOR> MyClass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:4
          |
        2 | a: "None | MyClass" = 1
          |    ^^^^^^^^^^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 2 type definitions
           --> main.py:4:7
            |
          2 | a: "None | MyClass" = 1
          3 |
          4 | class MyClass:
            |       -------
          5 |     """some docs"""
            |
           ::: stdlib/types.pyi:950:11
            |
        948 | if sys.version_info >= (3, 10):
        949 |     @final
        950 |     class NoneType:
            |           --------
        951 |         """The type of the None singleton."""
            |
        "#);
    }

    #[test]
    fn goto_type_string_annotation4() {
        let test = cursor_test(
            r#"
        a: "None | MyClass<CURSOR>" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_string_annotation5() {
        let test = cursor_test(
            r#"
        a: "None | MyClass"<CURSOR> = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:4
          |
        2 | a: "None | MyClass" = 1
          |    ^^^^^^^^^^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 2 type definitions
           --> main.py:4:7
            |
          2 | a: "None | MyClass" = 1
          3 |
          4 | class MyClass:
            |       -------
          5 |     """some docs"""
            |
           ::: stdlib/types.pyi:950:11
            |
        948 | if sys.version_info >= (3, 10):
        949 |     @final
        950 |     class NoneType:
            |           --------
        951 |         """The type of the None singleton."""
            |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_dangling1() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass |" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:4
          |
        2 | a: "MyClass |" = 1
          |    ^^^^^^^^^^^ Clicking here
        3 |
        4 | class MyClass:
          |
        info: Found 1 type definition
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | -------
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_dangling2() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass | No" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_string_annotation_dangling3() {
        let test = cursor_test(
            r#"
        a: "MyClass | N<CURSOR>o" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_string_annotation_recursive() {
        let test = cursor_test(
            r#"
        ab: "a<CURSOR>b"
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:6
          |
        2 | ab: "ab"
          |      ^^ Clicking here
          |
        info: Found 1 type definition
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | -------
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_unknown() {
        let test = cursor_test(
            r#"
        x: "foo<CURSOR>bar"
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:5
          |
        2 | x: "foobar"
          |     ^^^^^^ Clicking here
          |
        info: Found 1 type definition
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | -------
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        "#);
    }

    #[test]
    fn goto_type_match_name_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_match_name_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_match_rest_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_match_rest_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_match_as_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_match_as_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_match_keyword_stmt() {
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

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_match_keyword_binding() {
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

        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_match_class_name() {
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:10:14
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |              ^^^^^ Clicking here
        11 |             x = ab
           |
        info: Found 1 type definition
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
    fn goto_type_match_class_field_name() {
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

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_typevar_name_stmt() {
        let test = cursor_test(
            r#"
            type Alias1[A<CURSOR>B: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             --
          |
        ");
    }

    #[test]
    fn goto_type_typevar_name_binding() {
        let test = cursor_test(
            r#"
            type Alias1[AB: int = bool] = tuple[A<CURSOR>B, list[AB]]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:37
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |                                     ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             --
          |
        ");
    }

    #[test]
    fn goto_type_typevar_spec_stmt() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**A<CURSOR>B = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_typevar_spec_binding() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[A<CURSOR>B, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_typevar_tuple_stmt() {
        let test = cursor_test(
            r#"
            type Alias3[*A<CURSOR>B = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_typevar_tuple_binding() {
        let test = cursor_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*A<CURSOR>B], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
    }

    #[test]
    fn goto_type_on_keyword_argument() {
        let test = cursor_test(
            r#"
            def test(a: str): ...

            test(a<CURSOR>= "123")
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:4:6
          |
        2 | def test(a: str): ...
        3 |
        4 | test(a= "123")
          |      ^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }

    #[test]
    fn goto_type_on_incorrectly_typed_keyword_argument() {
        let test = cursor_test(
            r#"
            def test(a: str): ...

            test(a<CURSOR>= 123)
            "#,
        );

        // TODO: This should jump to `str` and not `int` because
        //   the keyword is typed as a string. It's only the passed argument that
        //   is an int. Navigating to `str` would match pyright's behavior.
        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:4:6
          |
        2 | def test(a: str): ...
        3 |
        4 | test(a= 123)
          |      ^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ---
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        "#);
    }

    #[test]
    fn goto_type_on_kwargs() {
        let test = cursor_test(
            r#"
            def f(name: str): ...

kwargs = { "name": "test"}

f(**kwargs<CURSOR>)
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:6:5
          |
        4 | kwargs = { "name": "test"}
        5 |
        6 | f(**kwargs)
          |     ^^^^^^ Clicking here
          |
        info: Found 1 type definition
            --> stdlib/builtins.pyi:2947:7
             |
        2946 | @disjoint_base
        2947 | class dict(MutableMapping[_KT, _VT]):
             |       ----
        2948 |     """dict() -> new empty dictionary
        2949 |     dict(mapping) -> new dictionary initialized from a mapping object's
             |
        "#);
    }

    #[test]
    fn goto_type_nonlocal_binding() {
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
        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:8:16
           |
         6 |         nonlocal x
         7 |         x = "modified"
         8 |         return x  # Should find the nonlocal x declaration in outer scope
           |                ^ Clicking here
         9 |
        10 |     return inner
           |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }

    #[test]
    fn goto_type_nonlocal_stmt() {
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
        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_global_binding() {
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
        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:7:12
          |
        5 |     global global_var
        6 |     global_var = "modified"
        7 |     return global_var  # Should find the global variable declaration
          |            ^^^^^^^^^^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }

    #[test]
    fn goto_type_global_stmt() {
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
        assert_snapshot!(test.goto_type_definition(), @"No goto target found");
    }

    #[test]
    fn goto_type_of_expression_with_builtin() {
        let test = cursor_test(
            r#"
            def foo(a: str):
                a<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:3:5
          |
        2 | def foo(a: str):
        3 |     a
          |     ^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }

    #[test]
    fn goto_type_definition_cursor_between_object_and_attribute() {
        let test = cursor_test(
            r#"
            class X:
                def foo(a, b): ...

            x = X()

            x<CURSOR>.foo()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:7:1
          |
        5 | x = X()
        6 |
        7 | x.foo()
          | ^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:7
          |
        2 | class X:
          |       -
        3 |     def foo(a, b): ...
          |
        ");
    }

    #[test]
    fn goto_between_call_arguments() {
        let test = cursor_test(
            r#"
            def foo(a, b): ...

            foo<CURSOR>()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        2 | def foo(a, b): ...
        3 |
        4 | foo()
          | ^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:5
          |
        2 | def foo(a, b): ...
          |     ---
        3 |
        4 | foo()
          |
        ");
    }

    #[test]
    fn goto_type_narrowing() {
        let test = cursor_test(
            r#"
            def foo(a: str | None, b):
                if a is not None:
                    print(a<CURSOR>)
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:4:15
          |
        2 | def foo(a: str | None, b):
        3 |     if a is not None:
        4 |         print(a)
          |               ^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        "#);
    }

    #[test]
    fn goto_type_none() {
        let test = cursor_test(
            r#"
            def foo(a: str | None, b):
                a<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:3:5
          |
        2 | def foo(a: str | None, b):
        3 |     a
          |     ^ Clicking here
          |
        info: Found 2 type definitions
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ---
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
           ::: stdlib/types.pyi:950:11
            |
        948 | if sys.version_info >= (3, 10):
        949 |     @final
        950 |     class NoneType:
            |           --------
        951 |         """The type of the None singleton."""
            |
        "#);
    }

    #[test]
    fn goto_type_submodule_import_from_use() {
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

        // The module is the correct type definition
        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
        4 | x = subpkg
          |     ^^^^^^ Clicking here
          |
        info: Found 1 type definition
        --> mypackage/subpkg/__init__.py:1:1
         |
         |
        ");
    }

    #[test]
    fn goto_type_submodule_import_from_def() {
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

        // The module is the correct type definition
        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg.submod import val
          |       ^^^^^^ Clicking here
        3 |
        4 | x = subpkg
          |
        info: Found 1 type definition
        --> mypackage/subpkg/__init__.py:1:1
         |
         |
        ");
    }

    #[test]
    fn goto_type_submodule_import_from_wrong_use() {
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

        // Unknown is correct, `submod` is not in scope
        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
        4 | x = submod
          |     ^^^^^^ Clicking here
          |
        info: Found 1 type definition
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | -------
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        ");
    }

    #[test]
    fn goto_type_submodule_import_from_wrong_def() {
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

        // The module is correct
        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:14
          |
        2 | from .subpkg.submod import val
          |              ^^^^^^ Clicking here
        3 |
        4 | x = submod
          |
        info: Found 1 type definition
         --> mypackage/subpkg/submod.py:1:1
          |
        1 | /
        2 | | val: int = 0
          | |_____________-
          |
        ");
    }

    #[test]
    fn goto_type_submodule_import_from_confusing_shadowed_def() {
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

        // The module is correct
        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg import subpkg
          |       ^^^^^^ Clicking here
        3 |
        4 | x = subpkg
          |
        info: Found 1 type definition
         --> mypackage/subpkg/__init__.py:1:1
          |
        1 | /
        2 | | subpkg: int = 10
          | |_________________-
          |
        ");
    }

    #[test]
    fn goto_type_submodule_import_from_confusing_real_def() {
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

        // `int` is correct
        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:21
          |
        2 | from .subpkg import subpkg
          |                     ^^^^^^ Clicking here
        3 |
        4 | x = subpkg
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ---
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        "#);
    }

    #[test]
    fn goto_type_submodule_import_from_confusing_use() {
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

        // `int` is correct
        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg import subpkg
        3 |
        4 | x = subpkg
          |     ^^^^^^ Clicking here
          |
        info: Found 1 type definition
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ---
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        "#);
    }

    impl CursorTest {
        fn goto_type_definition(&self) -> String {
            let Some(targets) = salsa::attach(&self.db, || {
                goto_type_definition(&self.db, self.cursor.file, self.cursor.offset)
            }) else {
                return "No goto target found".to_string();
            };

            if targets.is_empty() {
                return "No type definitions found".to_string();
            }

            self.render_diagnostics([crate::goto_definition::test::GotoDiagnostic::new(
                crate::goto_definition::test::GotoAction::TypeDefinition,
                targets,
            )])
        }
    }
}

use crate::goto::find_goto_target;
use crate::{Db, HasNavigationTargets, NavigationTargets, RangedValue};
use ruff_db::PythonFile;
use ruff_db::files::FileRange;
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::SemanticModel;

pub fn goto_type_definition(
    db: &dyn Db,
    file: PythonFile<'_>,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;

    let ty = goto_target.inferred_type(&model)?;
    let ctx = model.semantic_context();

    tracing::debug!("Inferred type of covering node is {}", ty.display(&ctx));

    let navigation_targets = ty.navigation_targets(&ctx);

    Some(RangedValue {
        range: FileRange::new(file.file(db), goto_target.range()),
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        4 | ab = Test()
          | ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:7
          |
        2 | class Test: ...
          |       ----
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:1
           |
        LL | ab = Literal
           | ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/typing.pyi:LL:1
           |
        LL | Literal: _SpecialForm
           | -------
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:1
           |
        LL | ab = Any
           | ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/typing.pyi:LL:7
           |
        LL | class Any:
           |       ---
           |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:1
           |
        LL | ab = Generic
           | ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/typing.pyi:LL:1
           |
        LL | Generic: type[_Generic]
           | -------
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:1
           |
        LL | ab = AlwaysTruthy
           | ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | AlwaysTruthy: _SpecialForm
           | ------------
           |
        ");
    }

    #[test]
    fn goto_type_of_divergent() {
        let test = cursor_test(
            r#"
            class D:
                def copy(self, other: "D"):
                    self.x = other.x

            D().x<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:5
           |
        LL | D().x
           |     ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/_internal.pyi:LL:1
           |
        LL | Divergent: _SpecialForm
           | ---------
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> main.py:6:1
          |
        6 | ab
          | ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:5
          |
        2 | def foo(a, b): ...
          |     ---
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:12:1
           |
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
          |
        ");
    }

    #[test]
    fn goto_type_of_narrowed_singleton_enum_complement() {
        let test = cursor_test(
            r#"
            from enum import Enum

            class Color(Enum):
                RED = 1
                BLUE = 2

            def f(color: Color):
                if color is Color.RED:
                    return

                color<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:12:5
           |
        12 |     color
           |     ^^^^^ Clicking here
           |
        info: Found 1 type definition
         --> main.py:6:5
          |
        6 |     BLUE = 2
          |     ----
          |
        "#);
    }

    #[test]
    fn goto_type_of_narrowed_multi_member_enum_complement() {
        let test = cursor_test(
            r#"
            from enum import Enum

            class Color(Enum):
                RED = 1
                GREEN = 2
                BLUE = 3

            def f(color: Color):
                if color is Color.RED:
                    return

                color<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:13:5
           |
        13 |     color
           |     ^^^^^ Clicking here
           |
        info: Found 2 type definitions
         --> main.py:6:5
          |
        6 |     GREEN = 2
          |     -----
        7 |     BLUE = 3
          |     ----
          |
        "#);
    }

    #[test]
    fn goto_type_of_import_module() {
        let mut test = cursor_test(
            r#"
            import l<CURSOR>ib
            "#,
        );

        test.write_file("lib.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> lib/sub/__init__.py:2:11
          |
        2 | from .bot.botmod import *
          |           ^^^^^^ Clicking here
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> lib/sub/__init__.py:2:7
          |
        2 | from .bot.botmod import *
          |       ^^^ Clicking here
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> lib/sub/__init__.py:2:7
          |
        2 | from .bot.botmod import *
          |       ^^^ Clicking here
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:1
           |
        LL | a
           | ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        ");
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
          --> main.py:LL:10
           |
        LL | a: str = "test"
           |          ^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:31
          |
        2 | type Alias[*Ts = ()] = tuple[*Ts]
          |                               ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:13
          |
        2 | type Alias[*Ts = ()] = tuple[*Ts]
          |             --
          |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:6:1
          |
        6 | Alias
          | ^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:1
          |
        4 | Alias = TypeAliasType("Alias", tuple[int, int])
          | -----
          |
        "#);
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
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
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
          --> main.py:LL:4
           |
        LL | a: "None | MyClass" = 1
           |    ^^^^^^^^^^^^^^^^ Clicking here
           |
        info: Found 2 type definitions
          --> main.py:LL:7
           |
        LL | class MyClass:
           |       -------
           |
          ::: stdlib/types.pyi:LL:7
           |
        LL | class NoneType:
           |       --------
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
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
          --> main.py:LL:4
           |
        LL | a: "None | MyClass" = 1
           |    ^^^^^^^^^^^^^^^^ Clicking here
           |
        info: Found 2 type definitions
          --> main.py:LL:7
           |
        LL | class MyClass:
           |       -------
           |
          ::: stdlib/types.pyi:LL:7
           |
        LL | class NoneType:
           |       --------
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
          --> main.py:LL:4
           |
        LL | a: "MyClass |" = 1
           |    ^^^^^^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | Unknown: _SpecialForm
           | -------
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:5
          |
        2 | a: "MyClass | No" = 1
          |     ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:15
           |
        LL | a: "MyClass | No" = 1
           |               ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | Unknown: _SpecialForm
           | -------
           |
        "#);
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
          --> main.py:LL:6
           |
        LL | ab: "ab"
           |      ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | Unknown: _SpecialForm
           | -------
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
          --> main.py:LL:5
           |
        LL | x: "foobar"
           |     ^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | Unknown: _SpecialForm
           | -------
           |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_nested1() {
        let test = cursor_test(
            r#"
        x: "list['My<CURSOR>Class | int'] | None"

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:11
          |
        2 | x: "list['MyClass | int'] | None"
          |           ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_nested2() {
        let test = cursor_test(
            r#"
        x: "list['int | My<CURSOR>Class'] | None"

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:17
          |
        2 | x: "list['int | MyClass'] | None"
          |                 ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_nested3() {
        let test = cursor_test(
            r#"
        x: "list['int | None'] | My<CURSOR>Class"

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:26
          |
        2 | x: "list['int | None'] | MyClass"
          |                          ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_nested4() {
        let test = cursor_test(
            r#"
        x: "list['int' | 'My<CURSOR>Class'] | None"

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:19
          |
        2 | x: "list['int' | 'MyClass'] | None"
          |                   ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_nested5() {
        let test = cursor_test(
            r#"
        x: "list['My<CURSOR>Class' | 'str'] | None"

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:11
          |
        2 | x: "list['MyClass' | 'str'] | None"
          |           ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_too_nested1() {
        let test = cursor_test(
            r#"
        x: """'list["My<CURSOR>Class" | "str"]' | None"""

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:13
           |
        LL | x: """'list["MyClass" | "str"]' | None"""
           |             ^^^^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | Unknown: _SpecialForm
           | -------
           |
        "#);
    }

    #[test]
    fn goto_type_string_annotation_too_nested2() {
        let test = cursor_test(
            r#"
        x: """'list["int" | "str"]' | My<CURSOR>Class"""

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
         --> main.py:2:31
          |
        2 | x: """'list["int" | "str"]' | MyClass"""
          |                               ^^^^^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:4:7
          |
        4 | class MyClass:
          |       -------
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:17
           |
        LL |             x = ab
           |                 ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        "#);
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:17
           |
        LL |             x = ab
           |                 ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class list(MutableSequence[_T]):
           |       ----
           |
        "#);
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

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:17
           |
        LL |             x = ab
           |                 ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        "#);
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:17
           |
        LL |             x = ab
           |                 ^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:10:14
           |
        10 |         case Click(x, button=ab):
           |              ^^^^^ Clicking here
           |
        info: Found 1 type definition
         --> main.py:2:7
          |
        2 | class Click:
          |       -----
          |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @"
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

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type definition]: Go to type definition
         --> main.py:2:38
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |                                      ^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              --
          |
        ");
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
          --> main.py:LL:6
           |
        LL | test(a= "123")
           |      ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:6
           |
        LL | test(a= 123)
           |      ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class int:
           |       ---
           |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:5
           |
        LL | f(**kwargs)
           |     ^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class dict(MutableMapping[_KT, _VT]):
           |       ----
           |
        ");
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:16
           |
        LL |         return x  # Should find the nonlocal x declaration in outer scope
           |                ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        ");
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:12
           |
        LL |     return global_var  # Should find the global variable declaration
           |            ^^^^^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:5
           |
        LL |     a
           |     ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        ");
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> main.py:7:1
          |
        7 | x.foo()
          | ^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:7
          |
        2 | class X:
          |       -
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> main.py:4:1
          |
        4 | foo()
          | ^^^ Clicking here
          |
        info: Found 1 type definition
         --> main.py:2:5
          |
        2 | def foo(a, b): ...
          |     ---
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

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:15
           |
        LL |         print(a)
           |               ^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
        ");
    }

    #[test]
    fn goto_type_none() {
        let test = cursor_test(
            r#"
            def foo(a: str | None, b):
                a<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> main.py:LL:5
           |
        LL |     a
           |     ^ Clicking here
           |
        info: Found 2 type definitions
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class str(Sequence[str]):
           |       ---
           |
          ::: stdlib/types.pyi:LL:7
           |
        LL | class NoneType:
           |       --------
           |
        ");
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:4:5
          |
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg.submod import val
          |       ^^^^^^ Clicking here
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> mypackage/__init__.py:LL:5
           |
        LL | x = submod
           |     ^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/ty_extensions/__init__.pyi:LL:1
           |
        LL | Unknown: _SpecialForm
           | -------
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:14
          |
        2 | from .subpkg.submod import val
          |              ^^^^^^ Clicking here
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg import subpkg
          |       ^^^^^^ Clicking here
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> mypackage/__init__.py:LL:21
           |
        LL | from .subpkg import subpkg
           |                     ^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class int:
           |       ---
           |
        ");
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
        assert_snapshot!(test.goto_type_definition(), @"
        info[goto-type definition]: Go to type definition
          --> mypackage/__init__.py:LL:5
           |
        LL | x = subpkg
           |     ^^^^^^ Clicking here
           |
        info: Found 1 type definition
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class int:
           |       ---
           |
        ");
    }

    impl CursorTest {
        fn goto_type_definition(&self) -> String {
            let Some(targets) = salsa::attach(&self.db, || {
                goto_type_definition(
                    &self.db,
                    self.python_file(self.cursor.file),
                    self.cursor.offset,
                )
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

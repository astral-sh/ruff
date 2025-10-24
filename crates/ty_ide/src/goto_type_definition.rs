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
    let goto_target = find_goto_target(&module, offset)?;

    let model = SemanticModel::new(db, file);
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
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use crate::{NavigationTarget, goto_type_definition};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    #[test]
    fn goto_type_of_expression_with_class_type() {
        let test = cursor_test(
            r#"
            class Test: ...

            a<CURSOR>b = Test()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r"
        info[goto-type-definition]: Type definition
         --> main.py:2:7
          |
        2 | class Test: ...
          |       ^^^^
        3 |
        4 | ab = Test()
          |
        info: Source
         --> main.py:4:1
          |
        2 | class Test: ...
        3 |
        4 | ab = Test()
          | ^^
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
        info[goto-type-definition]: Type definition
         --> main.py:2:5
          |
        2 | def foo(a, b): ...
          |     ^^^
        3 |
        4 | ab = foo
          |
        info: Source
         --> main.py:6:1
          |
        4 | ab = foo
        5 |
        6 | ab
          | ^^
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
        info[goto-type-definition]: Type definition
         --> main.py:3:5
          |
        3 | def foo(a, b): ...
          |     ^^^
        4 |
        5 | def bar(a, b): ...
          |
        info: Source
          --> main.py:12:1
           |
        10 |     a = bar
        11 |
        12 | a
           | ^
           |

        info[goto-type-definition]: Type definition
         --> main.py:5:5
          |
        3 | def foo(a, b): ...
        4 |
        5 | def bar(a, b): ...
          |     ^^^
        6 |
        7 | if random.choice():
          |
        info: Source
          --> main.py:12:1
           |
        10 |     a = bar
        11 |
        12 | a
           | ^
           |
        ");
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
        info[goto-type-definition]: Type definition
         --> lib.py:1:1
          |
        1 | a = 10
          | ^^^^^^
          |
        info: Source
         --> main.py:4:1
          |
        2 | import lib
        3 |
        4 | lib
          | ^^^
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
        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:913:7
            |
        912 | @disjoint_base
        913 | class str(Sequence[str]):
            |       ^^^
        914 |     """str(object='') -> str
        915 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main.py:4:1
          |
        2 | a: str = "test"
        3 |
        4 | a
          | ^
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
        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:913:7
            |
        912 | @disjoint_base
        913 | class str(Sequence[str]):
            |       ^^^
        914 |     """str(object='') -> str
        915 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main.py:2:10
          |
        2 | a: str = "test"
          |          ^^^^^^
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
        info[goto-type-definition]: Type definition
         --> main.py:2:12
          |
        2 | type Alias[T: int = bool] = list[T]
          |            ^
          |
        info: Source
         --> main.py:2:34
          |
        2 | type Alias[T: int = bool] = list[T]
          |                                  ^
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

        // TODO: Goto type definition currently doesn't work for type param specs
        // because the inference doesn't support them yet.
        // This snapshot should show a single target pointing to `T`
        assert_snapshot!(test.goto_type_definition(), @"No type definitions found");
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
    fn goto_type_on_keyword_argument() {
        let test = cursor_test(
            r#"
            def test(a: str): ...

            test(a<CURSOR>= "123")
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r#"
        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:913:7
            |
        912 | @disjoint_base
        913 | class str(Sequence[str]):
            |       ^^^
        914 |     """str(object='') -> str
        915 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main.py:4:6
          |
        2 | def test(a: str): ...
        3 |
        4 | test(a= "123")
          |      ^
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
        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:346:7
            |
        345 | @disjoint_base
        346 | class int:
            |       ^^^
        347 |     """int([x]) -> integer
        348 |     int(x, base=10) -> integer
            |
        info: Source
         --> main.py:4:6
          |
        2 | def test(a: str): ...
        3 |
        4 | test(a= 123)
          |      ^
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
        info[goto-type-definition]: Type definition
            --> stdlib/builtins.pyi:2918:7
             |
        2917 | @disjoint_base
        2918 | class dict(MutableMapping[_KT, _VT]):
             |       ^^^^
        2919 |     """dict() -> new empty dictionary
        2920 |     dict(mapping) -> new dictionary initialized from a mapping object's
             |
        info: Source
         --> main.py:6:5
          |
        4 | kwargs = { "name": "test"}
        5 |
        6 | f(**kwargs)
          |     ^^^^^^
          |
        "#);
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
        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:913:7
            |
        912 | @disjoint_base
        913 | class str(Sequence[str]):
            |       ^^^
        914 |     """str(object='') -> str
        915 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(a: str):
        3 |     a
          |     ^
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
        info[goto-type-definition]: Type definition
         --> main.py:2:7
          |
        2 | class X:
          |       ^
        3 |     def foo(a, b): ...
          |
        info: Source
         --> main.py:7:1
          |
        5 | x = X()
        6 |
        7 | x.foo()
          | ^
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
        info[goto-type-definition]: Type definition
         --> main.py:2:5
          |
        2 | def foo(a, b): ...
          |     ^^^
        3 |
        4 | foo()
          |
        info: Source
         --> main.py:4:1
          |
        2 | def foo(a, b): ...
        3 |
        4 | foo()
          | ^^^
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
        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:913:7
            |
        912 | @disjoint_base
        913 | class str(Sequence[str]):
            |       ^^^
        914 |     """str(object='') -> str
        915 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main.py:4:15
          |
        2 | def foo(a: str | None, b):
        3 |     if a is not None:
        4 |         print(a)
          |               ^
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
        info[goto-type-definition]: Type definition
           --> stdlib/types.pyi:950:11
            |
        948 | if sys.version_info >= (3, 10):
        949 |     @final
        950 |     class NoneType:
            |           ^^^^^^^^
        951 |         """The type of the None singleton."""
            |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(a: str | None, b):
        3 |     a
          |     ^
          |

        info[goto-type-definition]: Type definition
           --> stdlib/builtins.pyi:913:7
            |
        912 | @disjoint_base
        913 | class str(Sequence[str]):
            |       ^^^
        914 |     """str(object='') -> str
        915 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(a: str | None, b):
        3 |     a
          |     ^
          |
        "#);
    }

    impl CursorTest {
        fn goto_type_definition(&self) -> String {
            let Some(targets) =
                goto_type_definition(&self.db, self.cursor.file, self.cursor.offset)
            else {
                return "No goto target found".to_string();
            };

            if targets.is_empty() {
                return "No type definitions found".to_string();
            }

            let source = targets.range;
            self.render_diagnostics(
                targets
                    .into_iter()
                    .map(|target| GotoTypeDefinitionDiagnostic::new(source, &target)),
            )
        }
    }

    struct GotoTypeDefinitionDiagnostic {
        source: FileRange,
        target: FileRange,
    }

    impl GotoTypeDefinitionDiagnostic {
        fn new(source: FileRange, target: &NavigationTarget) -> Self {
            Self {
                source,
                target: FileRange::new(target.file(), target.focus_range()),
            }
        }
    }

    impl IntoDiagnostic for GotoTypeDefinitionDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut source = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Source");
            source.annotate(Annotation::primary(
                Span::from(self.source.file()).with_range(self.source.range()),
            ));

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("goto-type-definition")),
                Severity::Info,
                "Type definition".to_string(),
            );
            main.annotate(Annotation::primary(
                Span::from(self.target.file()).with_range(self.target.range()),
            ));
            main.sub(source);

            main
        }
    }
}

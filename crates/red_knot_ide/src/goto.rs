use crate::find_node::covering_node;
use crate::{Db, HasNavigationTargets, NavigationTargets, RangeInfo};
use red_knot_python_semantic::{HasType, SemanticModel};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::{parsed_module, ParsedModule};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

pub fn go_to_type_definition(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangeInfo<NavigationTargets>> {
    let parsed = parsed_module(db.upcast(), file);
    let goto_target = find_goto_target(parsed, offset)?;

    let model = SemanticModel::new(db.upcast(), file);

    let ty = match goto_target {
        GotoTarget::Expression(expression) => expression.inferred_type(&model),
        GotoTarget::FunctionDef(function) => function.inferred_type(&model),
        GotoTarget::ClassDef(class) => class.inferred_type(&model),
        GotoTarget::Parameter(parameter) => parameter.inferred_type(&model),
        GotoTarget::Alias(alias) => alias.inferred_type(&model),
        GotoTarget::ExceptVariable(except) => except.inferred_type(&model),
        GotoTarget::KeywordArgument(argument) => {
            // TODO: Pyright resolves the declared type of the matching parameter. This seems more accurate
            // than using the inferred value.
            argument.value.inferred_type(&model)
        }

        // TODO: Better support for go to type definition in match pattern.
        // This may require improving type inference (e.g. it currently doesn't handle `...rest`)
        // but it also requires a new API to query the type because implementing `HasType` for `PatternMatchMapping`
        // is ambiguous.
        GotoTarget::PatternMatchRest(_)
        | GotoTarget::PatternKeywordArgument(_)
        | GotoTarget::PatternMatchStarName(_)
        | GotoTarget::PatternMatchAsName(_) => return None,

        // TODO: Resolve the module; The type inference already does all the work
        // but type isn't stored anywhere. We should either extract the logic
        // for resolving the module from a ImportFromStmt or store the type during semantic analysis
        GotoTarget::ImportedModule(_) => return None,

        // Targets without a type definition.
        GotoTarget::TypeParamTypeVarName(_)
        | GotoTarget::TypeParamParamSpecName(_)
        | GotoTarget::TypeParamTypeVarTupleName(_) => return None,
    };

    tracing::debug!(
        "Inferred type of covering node is {}",
        ty.display(db.upcast())
    );

    Some(RangeInfo {
        range: FileRange::new(file, goto_target.range()),
        info: ty.navigation_targets(db),
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GotoTarget<'a> {
    Expression(ast::ExprRef<'a>),
    FunctionDef(&'a ast::StmtFunctionDef),
    ClassDef(&'a ast::StmtClassDef),
    Parameter(&'a ast::Parameter),
    Alias(&'a ast::Alias),

    /// Go to on the module name of an import from
    /// ```py
    /// from foo import bar
    ///      ^^^
    /// ```
    ImportedModule(&'a ast::StmtImportFrom),

    /// Go to on the exception handler variable
    /// ```py
    /// try: ...
    /// except Exception as e: ...
    ///                     ^
    /// ```
    ExceptVariable(&'a ast::ExceptHandlerExceptHandler),

    /// Go to on a keyword argument
    /// ```py
    /// test(a = 1)
    ///      ^
    /// ```
    KeywordArgument(&'a ast::Keyword),

    /// Go to on the rest parameter of a pattern match
    ///
    /// ```py
    /// match x:
    ///     case {"a": a, "b": b, **rest}: ...
    ///                             ^^^^
    /// ```
    PatternMatchRest(&'a ast::PatternMatchMapping),

    /// Go to on a keyword argument of a class pattern
    ///
    /// ```py
    /// match Point3D(0, 0, 0):
    ///     case Point3D(x=0, y=0, z=0): ...
    ///                  ^    ^    ^
    /// ```
    PatternKeywordArgument(&'a ast::PatternKeyword),

    /// Go to on a pattern star argument
    ///
    /// ```py
    /// match array:
    ///     case [*args]: ...
    ///            ^^^^
    PatternMatchStarName(&'a ast::PatternMatchStar),

    /// Go to on the name of a pattern match as pattern
    ///
    /// ```py
    /// match x:
    ///     case [x] as y: ...
    ///                 ^
    PatternMatchAsName(&'a ast::PatternMatchAs),

    /// Go to on the name of a type variable
    ///
    /// ```py
    /// type Alias[T: int = bool] = list[T]
    ///            ^
    /// ```
    TypeParamTypeVarName(&'a ast::TypeParamTypeVar),

    /// Go to on the name of a type param spec
    ///
    /// ```py
    /// type Alias[**P = [int, str]] = Callable[P, int]
    ///              ^
    /// ```
    TypeParamParamSpecName(&'a ast::TypeParamParamSpec),

    /// Go to on the name of a type var tuple
    ///
    /// ```py
    /// type Alias[*Ts = ()] = tuple[*Ts]
    ///             ^^
    /// ```
    TypeParamTypeVarTupleName(&'a ast::TypeParamTypeVarTuple),
}

impl Ranged for GotoTarget<'_> {
    fn range(&self) -> TextRange {
        match self {
            GotoTarget::Expression(expression) => expression.range(),
            GotoTarget::FunctionDef(function) => function.name.range,
            GotoTarget::ClassDef(class) => class.name.range,
            GotoTarget::Parameter(parameter) => parameter.name.range,
            GotoTarget::Alias(alias) => alias.name.range,
            GotoTarget::ImportedModule(module) => module.module.as_ref().unwrap().range,
            GotoTarget::ExceptVariable(except) => except.name.as_ref().unwrap().range,
            GotoTarget::KeywordArgument(keyword) => keyword.arg.as_ref().unwrap().range,
            GotoTarget::PatternMatchRest(rest) => rest.rest.as_ref().unwrap().range,
            GotoTarget::PatternKeywordArgument(keyword) => keyword.attr.range,
            GotoTarget::PatternMatchStarName(star) => star.name.as_ref().unwrap().range,
            GotoTarget::PatternMatchAsName(as_name) => as_name.name.as_ref().unwrap().range,
            GotoTarget::TypeParamTypeVarName(type_var) => type_var.name.range,
            GotoTarget::TypeParamParamSpecName(spec) => spec.name.range,
            GotoTarget::TypeParamTypeVarTupleName(tuple) => tuple.name.range,
        }
    }
}

pub(crate) fn find_goto_target(parsed: &ParsedModule, offset: TextSize) -> Option<GotoTarget> {
    let token = parsed.tokens().at_offset(offset).find(|token| {
        matches!(
            token.kind(),
            TokenKind::Name
                | TokenKind::String
                | TokenKind::Complex
                | TokenKind::Float
                | TokenKind::Int
        )
    })?;
    let covering_node = covering_node(parsed.syntax().into(), token.range())
        .find(|node| node.is_identifier() || node.is_expression())
        .ok()?;

    tracing::trace!("Covering node is of kind {:?}", covering_node.node().kind());

    match covering_node.node() {
        AnyNodeRef::Identifier(_) => match covering_node.parent() {
            Some(AnyNodeRef::StmtFunctionDef(function)) => Some(GotoTarget::FunctionDef(function)),
            Some(AnyNodeRef::StmtClassDef(class)) => Some(GotoTarget::ClassDef(class)),
            Some(AnyNodeRef::Parameter(parameter)) => Some(GotoTarget::Parameter(parameter)),
            Some(AnyNodeRef::Alias(alias)) => Some(GotoTarget::Alias(alias)),
            Some(AnyNodeRef::StmtImportFrom(from)) => Some(GotoTarget::ImportedModule(from)),
            Some(AnyNodeRef::ExceptHandlerExceptHandler(handler)) => {
                Some(GotoTarget::ExceptVariable(handler))
            }
            Some(AnyNodeRef::Keyword(keyword)) => Some(GotoTarget::KeywordArgument(keyword)),
            Some(AnyNodeRef::PatternMatchMapping(mapping)) => {
                Some(GotoTarget::PatternMatchRest(mapping))
            }
            Some(AnyNodeRef::PatternKeyword(keyword)) => {
                Some(GotoTarget::PatternKeywordArgument(keyword))
            }
            Some(AnyNodeRef::PatternMatchStar(star)) => {
                Some(GotoTarget::PatternMatchStarName(star))
            }
            Some(AnyNodeRef::PatternMatchAs(as_pattern)) => {
                Some(GotoTarget::PatternMatchAsName(as_pattern))
            }
            Some(AnyNodeRef::TypeParamTypeVar(var)) => Some(GotoTarget::TypeParamTypeVarName(var)),
            Some(AnyNodeRef::TypeParamParamSpec(bound)) => {
                Some(GotoTarget::TypeParamParamSpecName(bound))
            }
            Some(AnyNodeRef::TypeParamTypeVarTuple(var_tuple)) => {
                Some(GotoTarget::TypeParamTypeVarTupleName(var_tuple))
            }
            None => None,
            Some(parent) => {
                tracing::debug!(
                    "Missing `GoToTarget` for identifier with parent {:?}",
                    parent.kind()
                );
                None
            }
        },

        // AnyNodeRef::Keyword(keyword) => Some(GotoTarget::KeywordArgument(keyword)),
        node => node.as_expr_ref().map(GotoTarget::Expression),
    }
}

#[cfg(test)]
mod tests {

    use crate::db::tests::TestDb;
    use crate::go_to_type_definition;
    use insta::assert_snapshot;
    use red_knot_python_semantic::{
        Program, ProgramSettings, PythonPath, PythonPlatform, SearchPathSettings,
    };
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig, LintName,
        Severity, Span, SubDiagnostic,
    };
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_python_ast::PythonVersion;
    use ruff_text_size::{Ranged, TextSize};

    #[test]
    fn goto_type_of_expression_with_class_type() {
        let test = goto_test(
            r#"
            class Test: ...

            a<CURSOR>b = Test()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /main.py:2:19
          |
        2 |             class Test: ...
          |                   ^^^^
        3 |
        4 |             ab = Test()
          |
        info: Source
         --> /main.py:4:13
          |
        2 |             class Test: ...
        3 |
        4 |             ab = Test()
          |             ^^
          |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_function_type() {
        let test = goto_test(
            r#"
            def foo(a, b): ...

            ab = foo

            a<CURSOR>b
        "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /main.py:2:17
          |
        2 |             def foo(a, b): ...
          |                 ^^^
        3 |
        4 |             ab = foo
          |
        info: Source
         --> /main.py:6:13
          |
        4 |             ab = foo
        5 |
        6 |             ab
          |             ^^
          |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_union_type() {
        let test = goto_test(
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

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /main.py:3:17
          |
        3 |             def foo(a, b): ...
          |                 ^^^
        4 |
        5 |             def bar(a, b): ...
          |
        info: Source
          --> /main.py:12:13
           |
        10 |                 a = bar
        11 |
        12 |             a
           |             ^
           |
        info: lint:goto-type-definition: Type definition
         --> /main.py:5:17
          |
        3 |             def foo(a, b): ...
        4 |
        5 |             def bar(a, b): ...
          |                 ^^^
        6 |
        7 |             if random.choice():
          |
        info: Source
          --> /main.py:12:13
           |
        10 |                 a = bar
        11 |
        12 |             a
           |             ^
           |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_module() {
        let mut test = goto_test(
            r#"
            import lib

            lib<CURSOR>
            "#,
        );

        test.write_file("lib.py", "a = 10").unwrap();

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /lib.py:1:1
          |
        1 | a = 10
          | ^
          |
        info: Source
         --> /main.py:4:13
          |
        2 |             import lib
        3 |
        4 |             lib
          |             ^^^
          |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_literal_type() {
        let test = goto_test(
            r#"
            a: str = "test"

            a<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
           --> stdlib/builtins.pyi:443:7
            |
        441 |     def __getitem__(self, key: int, /) -> str | int | None: ...
        442 |
        443 | class str(Sequence[str]):
            |       ^^^
        444 |     @overload
        445 |     def __new__(cls, object: object = ...) -> Self: ...
            |
        info: Source
         --> /main.py:4:13
          |
        2 |             a: str = "test"
        3 |
        4 |             a
          |             ^
          |
        "###);
    }
    #[test]
    fn goto_type_of_expression_with_literal_node() {
        let test = goto_test(
            r#"
            a: str = "te<CURSOR>st"
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
           --> stdlib/builtins.pyi:443:7
            |
        441 |     def __getitem__(self, key: int, /) -> str | int | None: ...
        442 |
        443 | class str(Sequence[str]):
            |       ^^^
        444 |     @overload
        445 |     def __new__(cls, object: object = ...) -> Self: ...
            |
        info: Source
         --> /main.py:2:22
          |
        2 |             a: str = "test"
          |                      ^^^^^^
          |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_type_var_type() {
        let test = goto_test(
            r#"
            type Alias[T: int = bool] = list[T<CURSOR>]
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /main.py:2:24
          |
        2 |             type Alias[T: int = bool] = list[T]
          |                        ^
          |
        info: Source
         --> /main.py:2:46
          |
        2 |             type Alias[T: int = bool] = list[T]
          |                                              ^
          |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_type_param_spec() {
        let test = goto_test(
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
        let test = goto_test(
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
    fn goto_type_on_keyword_argument() {
        let test = goto_test(
            r#"
            def test(a: str): ...
            
            test(a<CURSOR>= "123")
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
           --> stdlib/builtins.pyi:443:7
            |
        441 |     def __getitem__(self, key: int, /) -> str | int | None: ...
        442 |
        443 | class str(Sequence[str]):
            |       ^^^
        444 |     @overload
        445 |     def __new__(cls, object: object = ...) -> Self: ...
            |
        info: Source
         --> /main.py:4:18
          |
        2 |             def test(a: str): ...
        3 |             
        4 |             test(a= "123")
          |                  ^
          |
        "###);
    }

    #[test]
    fn goto_type_on_incorrectly_typed_keyword_argument() {
        let test = goto_test(
            r#"
            def test(a: str): ...

            test(a<CURSOR>= 123)
            "#,
        );

        // TODO: This should jump to `str` and not `int` because
        //   the keyword is typed as a string. It's only the passed argument that
        //   is an int. Navigating to `str` would match pyright's behavior.
        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
           --> stdlib/builtins.pyi:234:7
            |
        232 | _LiteralInteger = _PositiveInteger | _NegativeInteger | Literal[0]  # noqa: Y026  # TODO: Use TypeAlias once mypy bugs are fixed
        233 |
        234 | class int:
            |       ^^^
        235 |     @overload
        236 |     def __new__(cls, x: ConvertibleToInt = ..., /) -> Self: ...
            |
        info: Source
         --> /main.py:4:18
          |
        2 |             def test(a: str): ...
        3 |
        4 |             test(a= 123)
          |                  ^
          |
        "###);
    }

    #[test]
    fn goto_type_on_kwargs() {
        let test = goto_test(
            r#"
            def f(name: str): ...

kwargs = { "name": "test"}

f(**kwargs<CURSOR>)
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
            --> stdlib/builtins.pyi:1098:7
             |
        1096 |         def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...
        1097 |
        1098 | class dict(MutableMapping[_KT, _VT]):
             |       ^^^^
        1099 |     # __init__ should be kept roughly in line with `collections.UserDict.__init__`, which has similar semantics
        1100 |     # Also multiprocessing.managers.SyncManager.dict()
             |
        info: Source
         --> /main.py:6:5
          |
        4 | kwargs = { "name": "test"}
        5 |
        6 | f(**kwargs)
          |     ^^^^^^
          |
        "###);
    }

    #[test]
    fn goto_type_of_expression_with_builtin() {
        let test = goto_test(
            r#"
            def foo(a: str):
                a<CURSOR>
            "#,
        );

        // FIXME: This should go to `str`
        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
           --> stdlib/builtins.pyi:443:7
            |
        441 |     def __getitem__(self, key: int, /) -> str | int | None: ...
        442 |
        443 | class str(Sequence[str]):
            |       ^^^
        444 |     @overload
        445 |     def __new__(cls, object: object = ...) -> Self: ...
            |
        info: Source
         --> /main.py:3:17
          |
        2 |             def foo(a: str):
        3 |                 a
          |                 ^
          |
        "###);
    }

    #[test]
    fn goto_type_definition_cursor_between_object_and_attribute() {
        let test = goto_test(
            r#"
            class X:
                def foo(a, b): ...

            x = X()

            x<CURSOR>.foo()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /main.py:2:19
          |
        2 |             class X:
          |                   ^
        3 |                 def foo(a, b): ...
          |
        info: Source
         --> /main.py:7:13
          |
        5 |             x = X()
        6 |
        7 |             x.foo()
          |             ^
          |
        "###);
    }

    #[test]
    fn goto_between_call_arguments() {
        let test = goto_test(
            r#"
            def foo(a, b): ...

            foo<CURSOR>()
            "#,
        );

        assert_snapshot!(test.goto_type_definition(), @r###"
        info: lint:goto-type-definition: Type definition
         --> /main.py:2:17
          |
        2 |             def foo(a, b): ...
          |                 ^^^
        3 |
        4 |             foo()
          |
        info: Source
         --> /main.py:4:13
          |
        2 |             def foo(a, b): ...
        3 |
        4 |             foo()
          |             ^^^
          |
        "###);
    }

    fn goto_test(source: &str) -> GotoTest {
        let mut db = TestDb::new();
        let cursor_offset = source.find("<CURSOR>").expect(
            "`source`` should contain a `<CURSOR>` marker, indicating the position of the cursor.",
        );

        let mut content = source[..cursor_offset].to_string();
        content.push_str(&source[cursor_offset + "<CURSOR>".len()..]);

        db.write_file("main.py", &content)
            .expect("write to memory file system to be successful");

        let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersion::latest(),
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings {
                    extra_paths: vec![],
                    src_roots: vec![SystemPathBuf::from("/")],
                    custom_typeshed: None,
                    python_path: PythonPath::KnownSitePackages(vec![]),
                },
            },
        )
        .expect("Default settings to be valid");

        GotoTest {
            db,
            cursor_offset: TextSize::try_from(cursor_offset)
                .expect("source to be smaller than 4GB"),
            file,
        }
    }

    struct GotoTest {
        db: TestDb,
        cursor_offset: TextSize,
        file: File,
    }

    impl GotoTest {
        fn write_file(
            &mut self,
            path: impl AsRef<SystemPath>,
            content: &str,
        ) -> std::io::Result<()> {
            self.db.write_file(path, content)
        }

        fn goto_type_definition(&self) -> String {
            let Some(targets) = go_to_type_definition(&self.db, self.file, self.cursor_offset)
            else {
                return "No goto target found".to_string();
            };

            if targets.info.is_empty() {
                return "No type definitions found".to_string();
            }

            let mut buf = vec![];

            let mut source = SubDiagnostic::new(Severity::Info, "Source");
            source.annotate(Annotation::primary(
                Span::from(targets.range.file()).with_range(targets.range.range()),
            ));

            for target in targets.info {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("goto-type-definition")),
                    Severity::Info,
                    "Type definition".to_string(),
                );
                diagnostic.annotate(Annotation::primary(
                    Span::from(target.file).with_range(target.focus_range),
                ));
                diagnostic.sub(source.clone());

                diagnostic
                    .print(
                        &self.db,
                        &DisplayDiagnosticConfig::default()
                            .color(false)
                            .format(DiagnosticFormat::Full),
                        &mut buf,
                    )
                    .unwrap();
            }

            source.printed();

            String::from_utf8(buf).unwrap()
        }
    }
}

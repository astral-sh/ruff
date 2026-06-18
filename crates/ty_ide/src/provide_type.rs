use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::{AnyNodeRef, ExprRef};
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::types::{Type, print_type};
use ty_python_semantic::{HasType, SemanticModel};

/// Returns the endpoint-specific public type representation for the requested range.
///
/// This applies provide-type normalizations and is not a general-purpose type printing API.
pub fn provide_type(db: &dyn Db, file: File, range: TextRange) -> Option<String> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let covering_node = covering_node(parsed.syntax().into(), range);
    let ty = match covering_node.find_first(AnyNodeRef::is_expression) {
        Ok(found) => expression_type(&model, found.node())?,
        Err(covering_node) => {
            let handler = covering_node
                .find_first(|node| matches!(node, AnyNodeRef::ExceptHandlerExceptHandler(_)))
                .ok()?
                .node();
            let AnyNodeRef::ExceptHandlerExceptHandler(handler) = handler else {
                return None;
            };
            if !handler
                .name
                .as_ref()
                .is_some_and(|name| name.range().contains_range(range))
            {
                return None;
            }
            handler.inferred_type(&model)?
        }
    };

    print_type(db, ty).ok()
}

fn expression_type<'db>(model: &SemanticModel<'db>, node: AnyNodeRef<'_>) -> Option<Type<'db>> {
    let expression = node.as_expr_ref()?;
    let inferred = expression.inferred_type(model)?;

    let ExprRef::Name(name) = expression else {
        return Some(inferred);
    };
    let members = model.members_in_scope_at(node);
    let Some(value_ty) = members.get(&name.id).map(|member| member.ty) else {
        return Some(inferred);
    };

    // Names in annotations are inferred as their instance type, but provide-type reports the
    // runtime value type of the expression.
    if value_ty.is_class_literal() && !inferred.is_class_literal() {
        Some(value_ty)
    } else {
        Some(inferred)
    }
}

#[cfg(test)]
mod tests {
    use crate::provide_type::provide_type;

    use insta::assert_snapshot;
    use ruff_db::{
        files::{File, system_path_to_file},
        system::{DbWithTestSystem, DbWithWritableSystem, SystemPathBuf},
    };
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::{TextRange, TextSize};
    use ty_project::ProjectMetadata;

    #[test]
    fn provide_nested_class_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A:
                class B:
                    pass

            value = A.B()
            <START>value<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.A.B");
    }

    #[test]
    fn provide_generic_class_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A[T]:
                def __init__(self, value: T):
                    self.value = value

            class B: ...

            value = A(B())
            <START>value<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.A[foo.B]");
    }

    #[test]
    fn provide_class_local_to_function_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            def f():
                class A: ...
                value = A()
                <START>value<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.f.A");
    }

    #[test]
    fn provide_exception_variable_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            try:
                print("Test")
            except IOError as <START>e<END>:
                pass
            "#,
        );

        assert_snapshot!(test.provided_type(), @"builtins.OSError");
    }

    #[test]
    fn provide_exception_class_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            try:
                print("Test")
            except <START>IOError<END>:
                pass
            "#,
        );

        assert_snapshot!(
            test.provided_type(),
            @"ty_extensions.TypeOf[builtins.OSError]"
        );
    }

    #[test]
    fn provide_function_parameter_annotation_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A:
                pass
            def f(a: <START>A<END> | None):
                pass
            "#,
        );

        assert_snapshot!(test.provided_type(), @"ty_extensions.TypeOf[foo.A]");
    }

    #[test]
    fn provide_class_type_in_constructor_call() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A: ...
            <START>A<END>()
            "#,
        );

        assert_snapshot!(test.provided_type(), @"ty_extensions.TypeOf[foo.A]");
    }

    #[test]
    fn named_recursive_alias_is_preserved() {
        let test = ProvideTypeTest::with_source(
            r#"
            type Tree = int | list[Tree]
            <START>tree<END>: Tree
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.Tree");
    }

    #[test]
    fn generic_alias_application_is_preserved() {
        let test = ProvideTypeTest::with_source(
            r#"
            type Box[T] = list[T]
            <START>box<END>: Box[int]
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.Box[builtins.int]");
    }

    #[test]
    fn newtype_instances_use_their_declaration_name() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import NewType
            UserId = NewType("UserId", int)
            user = UserId(1)
            <START>user<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.UserId");
    }

    #[test]
    fn unspecialized_alias_object_uses_runtime_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            type Alias = int
            value = Alias
            <START>value<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"typing_extensions.TypeAliasType");
    }

    #[test]
    fn specialized_alias_object_uses_runtime_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            type Alias[T] = list[T]
            value = Alias[int]
            <START>value<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"types.GenericAlias");
    }

    #[test]
    fn alias_instance_is_not_resolved() {
        let test = ProvideTypeTest::with_source(
            r#"
            type Alias = int
            <START>value<END>: Alias
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.Alias");
    }

    #[test]
    fn generic_binders_use_local_type_variable_names() {
        let test = ProvideTypeTest::with_source(
            r#"
            def identity[T](value: T) -> T:
                return value
            <START>identity<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"def foo.identity[T](value: T) -> T: ...");
    }

    #[test]
    fn signatures_preserve_parameter_kinds_and_defaults() {
        let test = ProvideTypeTest::with_source(
            r#"
            async def f(
                x: int,
                /,
                value: str = "default",
                *args: bytes,
                flag: bool = False,
                **kwargs: float,
            ) -> None:
                pass
            <START>f<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @r#"async def foo.f(x: builtins.int, /, value: builtins.str = "default", *args: builtins.bytes, flag: builtins.bool = False, **kwargs: builtins.float) -> None: ..."#
        );
    }

    #[test]
    fn unannotated_parameters_are_unknown() {
        let test = ProvideTypeTest::with_source(
            r#"
            def defaults(value, count=1) -> int:
                return count
            <START>defaults<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"def foo.defaults(value: Unknown, count: Unknown = 1) -> builtins.int: ..."
        );
    }

    #[test]
    fn type_is_uses_public_spelling() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing_extensions import TypeIs

            def is_int(value: object) -> TypeIs[int]: ...
            <START>is_int<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"def foo.is_int(value: builtins.object) -> typing.TypeIs[builtins.int]: ..."
        );
    }

    #[test]
    fn type_guard_uses_public_spelling() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing_extensions import TypeGuard

            def is_str(value: object) -> TypeGuard[str]: ...
            <START>is_str<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"def foo.is_str(value: builtins.object) -> typing.TypeGuard[builtins.str]: ..."
        );
    }

    #[test]
    fn type_form_uses_public_spelling() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing_extensions import TypeForm

            <START>form<END>: TypeForm[int]
            "#,
        );
        assert_snapshot!(test.provided_type(), @"typing.TypeForm[builtins.int]");
    }

    #[test]
    fn anonymous_callable() {
        let test = ProvideTypeTest::with_source(
            r#"
            from collections.abc import Callable
            <START>callback<END>: Callable[[int, str], bool]
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"(builtins.int, builtins.str, /) -> builtins.bool"
        );
    }

    #[test]
    fn bound_method() {
        let test = ProvideTypeTest::with_source(
            r#"
            class C:
                def method(self, value: int) -> str:
                    return str(value)

            method = C().method
            <START>method<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"def foo.C.method(value: builtins.int) -> builtins.str: ..."
        );
    }

    #[test]
    fn concatenate_callable() {
        let test = ProvideTypeTest::with_source(
            r#"
            from collections.abc import Callable
            from typing import Concatenate

            <START>callback<END>: Callable[Concatenate[int, ...], str]
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"(builtins.int, /, *args: typing.Any, **kwargs: typing.Any) -> builtins.str"
        );
    }

    #[test]
    fn overloads_are_an_explicit_group() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import overload

            @overload
            def convert(value: int) -> str: ...
            @overload
            def convert(value: str) -> int: ...
            def convert(value: int | str) -> int | str:
                return value
            <START>convert<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"Overloads[def foo.convert(value: builtins.int) -> builtins.str: ..., def foo.convert(value: builtins.str) -> builtins.int: ...]"
        );
    }

    #[test]
    fn tuple_literals_are_complete() {
        let test = ProvideTypeTest::with_source(
            r#"
            value = (1, "two", b"three", True)
            <START>value<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @r#"builtins.tuple[typing.Literal[1], typing.Literal["two"], typing.Literal[b"three"], typing.Literal[True]]"#
        );
    }

    #[test]
    fn string_literals_are_escaped() {
        let test = ProvideTypeTest::with_source(
            r#"
            value = "\u0085"
            <START>value<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @r#"typing.Literal["\x85"]"#);
    }

    #[test]
    fn float_and_complex_literals_use_public_spellings() {
        let test = ProvideTypeTest::with_source(
            r#"
            value = (1.0, 1j)
            <START>value<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"builtins.tuple[builtins.float, builtins.complex]"
        );
    }

    #[test]
    fn float_annotation_uses_public_spelling() {
        let test = ProvideTypeTest::with_source("<START>value<END>: float");
        assert_snapshot!(test.provided_type(), @"builtins.float");
    }

    #[test]
    fn complex_annotation_uses_public_spelling() {
        let test = ProvideTypeTest::with_source("<START>value<END>: complex");
        assert_snapshot!(test.provided_type(), @"builtins.complex");
    }

    #[test]
    fn runtime_type_variable_uses_typeof() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import TypeVar

            T = TypeVar("T")
            typevar = T
            <START>typevar<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"ty_extensions.TypeOf[foo.T]");
    }

    #[test]
    fn runtime_newtype_uses_typeof() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import NewType

            UserId = NewType("UserId", int)
            newtype = UserId
            <START>newtype<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"ty_extensions.TypeOf[foo.UserId]");
    }

    #[test]
    fn runtime_literal_uses_typeof() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import Literal

            literal = Literal[1]
            <START>literal<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"ty_extensions.TypeOf[typing.Literal[1]]"
        );
    }

    #[test]
    fn runtime_union_uses_typeof() {
        let test = ProvideTypeTest::with_source(
            r#"
            union = int | str
            <START>union<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"ty_extensions.TypeOf[builtins.int | builtins.str]"
        );
    }

    #[test]
    fn runtime_generic_alias_uses_typeof() {
        let test = ProvideTypeTest::with_source(
            r#"
            generic_alias = type[int]
            <START>generic_alias<END>
            "#,
        );
        assert_snapshot!(
            test.provided_type(),
            @"ty_extensions.TypeOf[builtins.type[builtins.int]]"
        );
    }

    #[test]
    fn runtime_annotated_uses_bare_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import Annotated

            annotated = Annotated[int, "metadata"]
            <START>annotated<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"builtins.int");
    }

    #[test]
    fn first_ambiguous_declaration_uses_lexical_ordinal() {
        let test = ProvideTypeTest::with_source(
            r#"
            class C: ...
            first = C()
            class C: ...
            second = C()
            <START>first<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.C@1");
    }

    #[test]
    fn second_ambiguous_declaration_uses_lexical_ordinal() {
        let test = ProvideTypeTest::with_source(
            r#"
            class C: ...
            first = C()
            class C: ...
            second = C()
            <START>second<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.C@2");
    }

    #[test]
    fn first_ambiguous_ancestor_uses_lexical_ordinal() {
        let test = ProvideTypeTest::with_source(
            r#"
            class Outer:
                class C: ...

            FirstC = Outer.C
            first = FirstC()

            class Outer:
                class C: ...

            SecondC = Outer.C
            second = SecondC()
            <START>first<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.Outer@1.C");
    }

    #[test]
    fn second_ambiguous_ancestor_uses_lexical_ordinal() {
        let test = ProvideTypeTest::with_source(
            r#"
            class Outer:
                class C: ...

            FirstC = Outer.C
            first = FirstC()

            class Outer:
                class C: ...

            SecondC = Outer.C
            second = SecondC()
            <START>second<END>
            "#,
        );
        assert_snapshot!(test.provided_type(), @"foo.Outer@2.C");
    }

    #[test]
    fn synthesized_protocol_intersection_constraints_are_omitted() {
        let test = ProvideTypeTest::with_source(
            r#"
            class C: ...

            def f(value: C):
                if hasattr(value, "missing"):
                    <START>value<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.C");
    }

    #[test]
    fn synthesized_typed_dict_constraints_remain_unsupported() {
        let test = ProvideTypeTest::with_source(
            r#"
            from typing import TypedDict

            class Foo(TypedDict):
                foo: int

            class Bar(TypedDict):
                bar: int

            def f(value: Foo | Bar):
                if "foo" in value:
                    <START>value<END>
            "#,
        );

        assert_eq!(provide_type(&test.db, test.file, test.range), None);
    }

    #[test]
    fn free_type_variables_include_their_qualified_binding_scope() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A[T1]:
                def f[T2](self, value: T1 | T2):
                    <START>value<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"T1@foo.A | T2@foo.A.f");
    }

    #[test]
    fn provide_module_type() {
        let test = ProvideTypeTest::with_source("import sys\nvalue = <START>sys<END>");
        assert_snapshot!(test.provided_type(), @"Module[sys]");
    }

    #[test]
    fn anonymous_recursive_type_returns_none() {
        let test = ProvideTypeTest::with_source(
            r#"
            from ty_extensions import TypeOf

            def recursive(value: "TypeOf[recursive]"): ...
            <START>recursive<END>
            "#,
        );

        assert_eq!(provide_type(&test.db, test.file, test.range), None);
    }

    struct ProvideTypeTest {
        db: ty_project::TestDb,
        file: File,
        range: TextRange,
    }

    impl ProvideTypeTest {
        fn with_source(source: &str) -> Self {
            let project_root = SystemPathBuf::from("/src");
            let mut db =
                ty_project::TestDb::new(ProjectMetadata::new("test".into(), project_root.clone()));

            db.memory_file_system()
                .create_directory_all(&project_root)
                .expect("create /src directory");

            db.init_program().unwrap();

            let mut cleansed = dedent(source).to_string();

            let start = cleansed
                .find("<START>")
                .expect("source text should contain a `<START>` marker");
            cleansed.replace_range(start..start + "<START>".len(), "");

            let end = cleansed
                .find("<END>")
                .expect("source text should contain a `<END>` marker");
            cleansed.replace_range(end..end + "<END>".len(), "");

            assert!(start <= end, "<START> marker should be before <END> marker");

            let path = project_root.join("foo.py");

            db.write_file(&path, cleansed)
                .expect("write to memory file system to be successful");

            let file = system_path_to_file(&db, &path).expect("newly written file to existing");

            Self {
                db,
                file,
                range: TextRange::new(
                    TextSize::try_from(start).unwrap(),
                    TextSize::try_from(end).unwrap(),
                ),
            }
        }

        fn provided_type(&self) -> String {
            provide_type(&self.db, self.file, self.range)
                .expect("selected type should be printable")
        }
    }
}

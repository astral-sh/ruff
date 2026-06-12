use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::{AnyNodeRef, ExprRef};
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::types::{
    FullyQualifiedNameResolver, NameQualification, PrintTypeSettings, Type,
    print_type_for_provide_type,
};
use ty_python_semantic::{HasType, SemanticModel};

pub fn provide_types<I>(db: &dyn Db, file: File, ranges: I) -> Vec<Option<String>>
where
    I: IntoIterator<Item = Option<TextRange>>,
{
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let settings = PrintTypeSettings::default()
        .with_qualification(NameQualification::FullyQualified)
        .with_experimental_syntax(true);
    let mut resolver = FullyQualifiedNameResolver;

    ranges
        .into_iter()
        .map(|range| {
            let range = range?;
            let covering_node = covering_node(parsed.syntax().into(), range);
            let ty = match covering_node.find_first(AnyNodeRef::is_expression) {
                Ok(found) => expression_type(&model, found.node())?,
                Err(covering_node) => {
                    let handler = covering_node
                        .find_first(|node| {
                            matches!(node, AnyNodeRef::ExceptHandlerExceptHandler(_))
                        })
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

            print_type_for_provide_type(db, ty, settings, &mut resolver).ok()
        })
        .collect()
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
    use crate::provide_type::provide_types;

    use ruff_db::{
        files::{File, system_path_to_file},
        system::{DbWithTestSystem, DbWithWritableSystem, SystemPathBuf},
    };
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::{TextRange, TextSize};
    use ty_project::ProjectMetadata;

    #[test]
    fn provide_instance_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class C:
                pass
            def foo() -> C:
                return <START>C()<END>
            "#,
        );

        assert_eq!(test.provided_type(), "foo.C");
    }

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

        assert_eq!(test.provided_type(), "foo.A.B");
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

        assert_eq!(test.provided_type(), "foo.A[foo.B]");
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

        assert_eq!(test.provided_type(), "foo.f.A");
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

        assert_eq!(test.provided_type(), "builtins.OSError");
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

        assert_eq!(
            test.provided_type(),
            "ty_extensions.TypeOf[builtins.OSError]"
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

        assert_eq!(test.provided_type(), "ty_extensions.TypeOf[foo.A]");
    }

    #[test]
    fn provide_callable_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            def f() -> int:
                return 1
            <START>f<END>()
            "#,
        );

        assert_eq!(test.provided_type(), "def foo.f() -> builtins.int: ...");
    }

    #[test]
    fn provide_class_type_in_constructor_call() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A: ...
            <START>A<END>()
            "#,
        );

        assert_eq!(test.provided_type(), "ty_extensions.TypeOf[foo.A]");
    }

    #[test]
    fn provide_type_applies_documented_normalizations() {
        for (source, expected) in [
            ("value = <START>1.0<END>", "builtins.float"),
            (
                "type Alias = int\nvalue = <START>Alias<END>",
                "typing_extensions.TypeAliasType",
            ),
            (
                "type Alias[T] = list[T]\nvalue = <START>Alias[int]<END>",
                "types.GenericAlias",
            ),
            (
                "type Alias = int\ndef f(value: Alias):\n    <START>value<END>",
                "foo.Alias",
            ),
        ] {
            let test = ProvideTypeTest::with_source(source);
            assert_eq!(
                provide_types(&test.db, test.file, [Some(test.range)])[0].as_deref(),
                Some(expected),
                "source: {source}"
            );
        }
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

        assert_eq!(test.provided_type(), "foo.C");
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

        assert_eq!(
            provide_types(&test.db, test.file, [Some(test.range)]),
            [None]
        );
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

        assert_eq!(test.provided_type(), "T1@foo.A | T2@foo.A.f");
    }

    #[test]
    fn unsupported_type_returns_none() {
        let test = ProvideTypeTest::with_source(
            "from typing import Annotated\nvalue = <START>Annotated[int, 'metadata']<END>",
        );
        assert_eq!(
            provide_types(&test.db, test.file, [Some(test.range)]),
            [None]
        );
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
            let mut types = provide_types(&self.db, self.file, [Some(self.range)]);
            assert_eq!(types.len(), 1);
            types
                .pop()
                .flatten()
                .expect("selected type should be printable")
        }
    }
}

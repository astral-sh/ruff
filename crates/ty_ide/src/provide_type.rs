use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_text_size::TextRange;
use ty_python_semantic::{DisplaySettings, HasType, SemanticModel};

pub fn provide_types<I>(db: &dyn Db, file: File, ranges: I) -> Vec<Option<String>>
where
    I: IntoIterator<Item = Option<TextRange>>,
{
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);

    ranges
        .into_iter()
        .map(|range| {
            let range = range?;
            let covering_node = covering_node(parsed.syntax().into(), range);
            let node = match covering_node.find_first(|node| node.is_expression()) {
                Ok(found) => found.node(),
                Err(_) => return None,
            };
            let ty = node.as_expr_ref()?.inferred_type(&model)?;

            Some(
                ty.display_with(db, DisplaySettings::default().fully_qualified())
                    .to_string(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::provide_type::provide_types;

    use insta::{assert_snapshot, internals::SettingsBindDropGuard};
    use ruff_db::{
        files::{File, system_path_to_file},
        system::{DbWithTestSystem, DbWithWritableSystem, SystemPathBuf},
    };
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::{TextRange, TextSize};
    use ty_project::ProjectMetadata;

    #[test]
    fn provide_str_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class C:
                pass
            def foo() -> C:
                return <START>C()<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.C");
    }

    #[test]
    fn provide_int_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            a = int(10)
            <START>a<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"builtins.int");
    }

    #[test]
    fn provide_nested_class_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A:
                class B:
                    pass

            b = A.B()
            <START>b<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.A.B");
    }

    #[test]
    fn provide_generic_class_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A[T]:
                i: T
                def __init__(self, i: T):
                    self.i = i

            class B:
                pass

            a = A(B())
            <START>a<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.A[foo.B]");
    }

    #[test]
    fn provide_integer_literal_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            a = 1
            <START>a<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"typing.Literal[1]");
    }

    #[test]
    fn provide_callable_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            def a() -> int:
                return 1
            <START>a<END>()
            "#,
        );

        assert_snapshot!(test.provided_type(), @"def foo.a() -> builtins.int");
    }

    #[test]
    fn provide_function_with_default_parameter_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            def a(b, c=1) -> int:
                return 1
            <START>a<END>()
            "#,
        );

        assert_snapshot!(
            test.provided_type(),
            @"def foo.a(b: Unknown, c: Unknown = 1) -> builtins.int"
        );
    }

    #[test]
    fn provide_class_local_to_function_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            def a():
                class A:
                    pass
                a = A()
                <START>a<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"foo.a.A");
    }

    #[test]
    fn provide_type_variable_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A[T1]:
                def f[T2](self, t: T1 | T2):
                    <START>t<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"T1@foo.A | T2@foo.A.f");
    }

    #[test]
    fn provide_class_type() {
        let test = ProvideTypeTest::with_source(
            r#"
            class A:
                pass
            <START>A<END>
            "#,
        );

        assert_snapshot!(test.provided_type(), @"builtins.type[foo.A]");
    }

    struct ProvideTypeTest {
        db: ty_project::TestDb,
        file: File,
        range: TextRange,
    }

    impl ProvideTypeTest {
        fn with_source(source: &str) -> Self {
            let project_root = SystemPathBuf::from("/src");
            let mut db = ty_project::TestDb::new(ProjectMetadata::new(
                "test".into(),
                project_root.clone(),
            ));

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
            match provide_types(&self.db, self.file, [Some(self.range)]).as_slice() {
                [Some(ty)] => ty.clone(),
                [None] => "None".to_string(),
                other => format!("{other:#?}"),
            }
        }
    }
}

//! Rules from [flake8-future-annotations](https://pypi.org/project/flake8-future-annotations/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};
    use ruff_python_ast::PythonVersion;

    #[test_case(Path::new("edge_case.py"))]
    #[test_case(Path::new("from_typing_import.py"))]
    #[test_case(Path::new("from_typing_import_many.py"))]
    #[test_case(Path::new("import_typing.py"))]
    #[test_case(Path::new("import_typing_as.py"))]
    #[test_case(Path::new("no_future_import_uses_lowercase.py"))]
    #[test_case(Path::new("no_future_import_uses_union.py"))]
    #[test_case(Path::new("no_future_import_uses_union_inner.py"))]
    #[test_case(Path::new("ok_no_types.py"))]
    #[test_case(Path::new("ok_non_simplifiable_types.py"))]
    #[test_case(Path::new("ok_uses_future.py"))]
    #[test_case(Path::new("ok_variable_name.py"))]
    fn fa100(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_future_annotations").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY37,
                ..settings::LinterSettings::for_rule(Rule::FutureRewritableTypeAnnotation)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("no_future_import_uses_lowercase.py"))]
    #[test_case(Path::new("no_future_import_uses_union.py"))]
    #[test_case(Path::new("no_future_import_uses_union_inner.py"))]
    #[test_case(Path::new("ok_no_types.py"))]
    #[test_case(Path::new("ok_uses_future.py"))]
    #[test_case(Path::new("ok_quoted_type.py"))]
    fn fa102(path: &Path) -> Result<()> {
        let snapshot = format!("fa102_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_future_annotations").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY37,
                ..settings::LinterSettings::for_rule(Rule::FutureRequiredTypeAnnotation)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

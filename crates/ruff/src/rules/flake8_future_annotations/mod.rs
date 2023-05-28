//! Rules from [flake8-future-annotations](https://pypi.org/project/flake8-future-annotations/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::PythonVersion;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Path::new("edge_case.py"); "edge_case")]
    #[test_case(Path::new("from_typing_import.py"); "from_typing_import")]
    #[test_case(Path::new("from_typing_import_many.py"); "from_typing_import_many")]
    #[test_case(Path::new("import_typing.py"); "import_typing")]
    #[test_case(Path::new("import_typing_as.py"); "import_typing_as")]
    #[test_case(Path::new("no_future_import_uses_lowercase.py"); "no_future_import_uses_lowercase")]
    #[test_case(Path::new("no_future_import_uses_union.py"); "no_future_import_uses_union")]
    #[test_case(Path::new("no_future_import_uses_union_inner.py"); "no_future_import_uses_union_inner")]
    #[test_case(Path::new("ok_no_types.py"); "ok_no_types")]
    #[test_case(Path::new("ok_non_simplifiable_types.py"); "ok_non_simplifiable_types")]
    #[test_case(Path::new("ok_uses_future.py"); "ok_uses_future")]
    #[test_case(Path::new("ok_variable_name.py"); "ok_variable_name")]
    fn fa100(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_future_annotations").join(path).as_path(),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(Rule::MissingFutureAnnotationsImportOldStyle)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

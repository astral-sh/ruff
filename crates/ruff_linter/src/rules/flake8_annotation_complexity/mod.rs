pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Path::new("TAE002.py"))]
    #[test_case(Path::new("TAE002_quoted.py"))]
    fn tae002_yields_errors(path: &Path) -> Result<()> {
        let snapshot = format!("TAE002_yields_errors_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_annotation_complexity")
                .join(path)
                .as_path(),
            &LinterSettings {
                flake8_annotation_complexity: super::settings::Settings {
                    max_annotation_complexity: 3,
                },
                ..LinterSettings::for_rule(Rule::ComplexAnnotation)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}

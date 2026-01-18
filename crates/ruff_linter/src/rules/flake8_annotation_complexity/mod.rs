pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test]
    fn tae002_yields_errors() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotation_complexity/TAE002.py"),
            &LinterSettings {
                flake8_annotation_complexity: super::settings::Settings {
                    max_annotation_complexity: 3,
                },
                ..LinterSettings::for_rule(Rule::ComplexAnnotation)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_errors_when_high_max_annotation_complexity() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotation_complexity/TAE002.py"),
            &LinterSettings {
                flake8_annotation_complexity: super::settings::Settings {
                    max_annotation_complexity: 100,
                },
                ..LinterSettings::for_rule(Rule::ComplexAnnotation)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }
}

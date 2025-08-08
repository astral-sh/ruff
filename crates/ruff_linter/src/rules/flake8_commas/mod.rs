//! Rules from [flake8-commas](https://pypi.org/project/flake8-commas/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, assert_diagnostics_diff, settings};

    #[test_case(Path::new("COM81.py"))]
    #[test_case(Path::new("COM81_syntax_error.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_commas").join(path).as_path(),
            &settings::LinterSettings::for_rules(vec![
                Rule::MissingTrailingComma,
                Rule::TrailingCommaOnBareTuple,
                Rule::ProhibitedTrailingComma,
            ]),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("COM81.py"))]
    #[test_case(Path::new("COM81_syntax_error.py"))]
    fn preview_rules(path: &Path) -> Result<()> {
        let snapshot = format!("preview_diff__{}", path.to_string_lossy());
        let rules = vec![
            Rule::MissingTrailingComma,
            Rule::TrailingCommaOnBareTuple,
            Rule::ProhibitedTrailingComma,
        ];
        let settings_before = settings::LinterSettings::for_rules(rules.clone());
        let settings_after = settings::LinterSettings {
            preview: crate::settings::types::PreviewMode::Enabled,
            ..settings::LinterSettings::for_rules(rules)
        };

        assert_diagnostics_diff!(
            snapshot,
            Path::new("flake8_commas").join(path).as_path(),
            &settings_before,
            &settings_after
        );
        Ok(())
    }
}

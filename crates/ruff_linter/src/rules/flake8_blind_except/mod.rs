//! Rules from [flake8-blind-except](https://pypi.org/project/flake8-blind-except/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::PreviewMode;
    use crate::test::test_path;
    use crate::{assert_diagnostics, assert_diagnostics_diff, settings};

    #[test_case(Rule::BlindExcept, Path::new("BLE.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_blind_except").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::BlindExcept, Path::new("BLE.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview_{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        assert_diagnostics_diff!(
            snapshot,
            Path::new("flake8_blind_except").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Disabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        );
        Ok(())
    }
}

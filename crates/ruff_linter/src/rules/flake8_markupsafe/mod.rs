//! Rules from [flake8-markupsafe](https://github.com/vmagamedov/flake8-markupsafe).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::PreviewMode;
    use crate::settings::LinterSettings;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::UnsafeMarkupUse, Path::new("MS001.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_markupsafe").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_allowed_callable() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_markupsafe/MS001_extend_markup_names.py"),
            &LinterSettings {
                flake8_markupsafe: super::settings::Settings {
                    extend_markup_names: vec!["webhelpers.html.literal".to_string()],
                },
                preview: PreviewMode::Enabled,
                ..LinterSettings::for_rule(Rule::UnsafeMarkupUse)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}

//! Rules from [pygrep-hooks](https://github.com/pre-commit/pygrep-hooks).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;

    use crate::settings::types::PreviewMode;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(
        Rule::BlanketTypeIgnore,
        Path::new("PGH003_0.py"),
        PreviewMode::Disabled
    )]
    #[test_case(
        Rule::BlanketTypeIgnore,
        Path::new("PGH003_1.py"),
        PreviewMode::Disabled
    )]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_0.py"), PreviewMode::Disabled)]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_1.py"), PreviewMode::Disabled)]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_2.py"), PreviewMode::Disabled)]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_2.py"), PreviewMode::Enabled)]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_3.py"), PreviewMode::Disabled)]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_3.py"), PreviewMode::Enabled)]
    #[test_case(
        Rule::InvalidMockAccess,
        Path::new("PGH005_0.py"),
        PreviewMode::Disabled
    )]
    fn rules(rule_code: Rule, path: &Path, preview: PreviewMode) -> Result<()> {
        let snapshot = {
            let base = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
            // To keep snapshot filenames more succinct we're only adding the preview suffix when
            // enabled.
            match preview {
                PreviewMode::Disabled => base,
                PreviewMode::Enabled => {
                    format!("{base}_preview_{preview}")
                }
            }
        };
        let mut settings = settings::LinterSettings::for_rule(rule_code);
        settings.preview = preview;
        let diagnostics = test_path(Path::new("pygrep_hooks").join(path).as_path(), &settings)?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

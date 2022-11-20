pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::autofix::fixer;
    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{flake8_bugbear, Settings};

    #[test]
    fn extend_immutable_calls() -> Result<()> {
        let snapshot = "extend_immutable_calls".to_string();
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/B008_extended.py"),
            &Settings {
                flake8_bugbear: flake8_bugbear::settings::Settings {
                    extend_immutable_calls: vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                    ],
                },
                ..Settings::for_rules(vec![CheckCode::B008])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

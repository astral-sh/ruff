//! Module for Ruff-specific rules.

pub mod checks;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashSet;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test]
    fn confusables() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff/confusables.py"),
            &settings::Settings {
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::Settings::for_rules(vec![
                    CheckCode::RUF001,
                    CheckCode::RUF002,
                    CheckCode::RUF003,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ruf100() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff/RUF100.py"),
            &settings::Settings::for_rules(vec![
                CheckCode::RUF100,
                CheckCode::E501,
                CheckCode::F401,
                CheckCode::F841,
            ]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn flake8_noqa() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff/flake8_noqa.py"),
            &settings::Settings::for_rules(vec![CheckCode::F401, CheckCode::F841]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ruff_noqa() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff/ruff_noqa.py"),
            &settings::Settings::for_rules(vec![CheckCode::F401, CheckCode::F841]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn redirects() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff/redirects.py"),
            &settings::Settings::for_rules(vec![CheckCode::UP007]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

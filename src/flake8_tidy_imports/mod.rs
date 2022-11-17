pub mod checks;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::autofix::fixer;
    use crate::checks::CheckCode;
    use crate::flake8_tidy_imports::settings::Strictness;
    use crate::linter::test_path;
    use crate::{flake8_tidy_imports, Settings};

    #[test]
    fn ban_parent_imports() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/I252.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::Parents,
                },
                ..Settings::for_rules(vec![CheckCode::I252])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/I252.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::All,
                },
                ..Settings::for_rules(vec![CheckCode::I252])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

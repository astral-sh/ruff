pub mod checks;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{flake8_errmsg, settings};

    #[test]
    fn defaults() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_errmsg/EM.py"),
            &settings::Settings::for_rules(vec![
                CheckCode::EM101,
                CheckCode::EM102,
                CheckCode::EM103,
            ]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("defaults", checks);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_errmsg/EM.py"),
            &settings::Settings {
                flake8_errmsg: flake8_errmsg::settings::Settings {
                    max_string_length: 20,
                },
                ..settings::Settings::for_rules(vec![
                    CheckCode::EM101,
                    CheckCode::EM102,
                    CheckCode::EM103,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("custom", checks);
        Ok(())
    }
}

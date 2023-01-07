pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::linter::test_path;
    use crate::registry::CheckCode;
    use crate::{flake8_errmsg, settings};

    #[test]
    fn defaults() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_errmsg/EM.py"),
            &settings::Settings::for_rules(vec![
                CheckCode::EM101,
                CheckCode::EM102,
                CheckCode::EM103,
            ]),
        )?;
        insta::assert_yaml_snapshot!("defaults", checks);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let checks = test_path(
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
        insta::assert_yaml_snapshot!("custom", checks);
        Ok(())
    }
}

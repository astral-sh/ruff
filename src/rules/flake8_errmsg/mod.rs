//! Rules from [flake8-errmsg](https://pypi.org/project/flake8-errmsg/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::{assert_yaml_snapshot, settings};

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_errmsg/EM.py"),
            &settings::Settings::for_rules(vec![
                Rule::RawStringInException,
                Rule::FStringInException,
                Rule::DotFormatInException,
            ]),
        )?;
        assert_yaml_snapshot!("defaults", diagnostics);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_errmsg/EM.py"),
            &settings::Settings {
                flake8_errmsg: super::settings::Settings {
                    max_string_length: 20,
                },
                ..settings::Settings::for_rules(vec![
                    Rule::RawStringInException,
                    Rule::FStringInException,
                    Rule::DotFormatInException,
                ])
            },
        )?;
        assert_yaml_snapshot!("custom", diagnostics);
        Ok(())
    }
}

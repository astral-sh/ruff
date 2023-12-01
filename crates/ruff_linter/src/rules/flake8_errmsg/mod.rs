//! Rules from [flake8-errmsg](https://pypi.org/project/flake8-errmsg/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_errmsg/EM.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::RawStringInException,
                Rule::FStringInException,
                Rule::DotFormatInException,
            ]),
        )?;
        assert_messages!("defaults", diagnostics);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_errmsg/EM.py"),
            &settings::LinterSettings {
                flake8_errmsg: super::settings::Settings {
                    max_string_length: 20,
                },
                ..settings::LinterSettings::for_rules(vec![
                    Rule::RawStringInException,
                    Rule::FStringInException,
                    Rule::DotFormatInException,
                ])
            },
        )?;
        assert_messages!("custom", diagnostics);
        Ok(())
    }
}

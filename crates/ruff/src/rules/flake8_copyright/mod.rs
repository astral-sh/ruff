//! Rules from [flake8-copyright](https://github.com/savoirfairelinux/flake8-copyright).
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
    fn test_default_fail() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_copyright/CPY801_default_fail.py"),
            &settings::Settings::for_rules(vec![Rule::HeaderLacksCopyright]),
        )?;
        assert_messages!("test_default_fail", diagnostics);
        Ok(())
    }

    #[test]
    fn test_default_pass() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_copyright/CPY801.py"),
            &settings::Settings::for_rules(vec![Rule::HeaderLacksCopyright]),
        )?;
        assert!(diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn test_custom_regex_fail() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_copyright/CPY801_custom_author_fail.py"),
            &settings::Settings {
                flake8_copyright: super::settings::Settings {
                    copyright_author: "ruff".to_string(),
                    copyright_regexp: "(?i)Copyright \\d{4} \\(C\\".to_string(),
                    copyright_min_file_size: 0,
                },
                ..settings::Settings::for_rules(vec![Rule::HeaderLacksCopyright])
            },
        )?;
        assert_messages!("test_custom_regex_fail", diagnostics);
        Ok(())
    }

    #[test]
    fn test_custom_regex_pass() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_copyright/CPY801_custom_regexp_pass.py"),
            &settings::Settings {
                flake8_copyright: super::settings::Settings {
                    copyright_author: "ruff".to_string(),
                    copyright_regexp: "(?i)Copyright \\d{4} \\(C\\)".to_string(),
                    copyright_min_file_size: 300,
                },
                ..settings::Settings::for_rules(vec![Rule::HeaderLacksCopyright])
            },
        )?;
        assert!(diagnostics.is_empty());
        Ok(())
    }
}

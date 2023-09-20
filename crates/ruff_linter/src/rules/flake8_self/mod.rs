//! Rules from [flake8-self](https://pypi.org/project/flake8-self/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::flake8_self;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::PrivateMemberAccess, Path::new("SLF001.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_self").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn ignore_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_self/SLF001_extended.py"),
            &settings::Settings {
                flake8_self: flake8_self::settings::Settings {
                    ignore_names: vec!["_meta".to_string()],
                },
                ..settings::Settings::for_rule(Rule::PrivateMemberAccess)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}

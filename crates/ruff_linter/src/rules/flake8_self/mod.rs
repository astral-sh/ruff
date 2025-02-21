//! Rules from [flake8-self](https://pypi.org/project/flake8-self/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use crate::registry::Rule;
    use crate::rules::flake8_self;
    use crate::test::test_path;
    use crate::{assert_messages, settings};
    use anyhow::Result;
    use ruff_python_ast::name::Name;
    use test_case::test_case;

    #[test_case(Rule::PrivateMemberAccess, Path::new("SLF001.py"))]
    #[test_case(Rule::PrivateMemberAccess, Path::new("SLF001_1.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_self").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn ignore_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_self/SLF001_extended.py"),
            &settings::LinterSettings {
                flake8_self: flake8_self::settings::Settings {
                    ignore_names: vec![Name::new_static("_meta")],
                },
                ..settings::LinterSettings::for_rule(Rule::PrivateMemberAccess)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}

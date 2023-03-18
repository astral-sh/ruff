//! Rules from [flake8-variable-names](https://pypi.org/project/flake8-variables-names/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_yaml_snapshot;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Rule::SingleLetterVariableName, Path::new("VN001.py"); "VN001")]
    #[test_case(Rule::NonDescriptVariableName, Path::new("VN002.py"); "VN002")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_variable_names").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_strict_variable_single_letter() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variable_names")
                .join("VN001.py")
                .as_path(),
            &settings::Settings {
                flake8_variable_names: super::settings::Settings {
                    use_varnames_strict_mode: true
                },
                ..settings::Settings::for_rules(vec![Rule::SingleLetterVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_enforce_strict_variable_single_letter() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variable_names")
                .join("VN001.py")
                .as_path(),
            &settings::Settings {
                flake8_variable_names: super::settings::Settings {
                    use_varnames_strict_mode: false
                },
                ..settings::Settings::for_rules(vec![Rule::SingleLetterVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_strict_variable_non_descript() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variable_names")
                .join("VN002.py")
                .as_path(),
            &settings::Settings {
                flake8_variable_names: super::settings::Settings {
                    use_varnames_strict_mode: true
                },
                ..settings::Settings::for_rules(vec![Rule::NonDescriptVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_enforce_strict_variable_non_descript() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variable_names")
                .join("VN002.py")
                .as_path(),
            &settings::Settings {
                flake8_variable_names: super::settings::Settings {
                    use_varnames_strict_mode: false
                },
                ..settings::Settings::for_rules(vec![Rule::NonDescriptVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}

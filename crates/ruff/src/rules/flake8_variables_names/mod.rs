//! Rules from [flake8-variables-names](https://pypi.org/project/flake8-variables-names/).
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

    #[test_case(Rule::SingleLetterVariableName, Path::new("VNE001.py"); "VNE001")]
    #[test_case(Rule::NonDescriptVariableName, Path::new("VNE002.py"); "VNE002")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_variables_names").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_strict_variable_single_letter() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variables_names")
                .join("VNE001.py")
                .as_path(),
            &settings::Settings {
                flake8_variables_names: super::settings::Settings { strict: true },
                ..settings::Settings::for_rules(vec![Rule::SingleLetterVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_enforce_strict_variable_single_letter() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variables_names")
                .join("VNE001.py")
                .as_path(),
            &settings::Settings {
                flake8_variables_names: super::settings::Settings { strict: false },
                ..settings::Settings::for_rules(vec![Rule::SingleLetterVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_strict_variable_non_descript() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variables_names")
                .join("VNE002.py")
                .as_path(),
            &settings::Settings {
                flake8_variables_names: super::settings::Settings { strict: true },
                ..settings::Settings::for_rules(vec![Rule::NonDescriptVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_enforce_strict_variable_non_descript() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_variables_names")
                .join("VNE002.py")
                .as_path(),
            &settings::Settings {
                flake8_variables_names: super::settings::Settings { strict: false },
                ..settings::Settings::for_rules(vec![Rule::NonDescriptVariableName, Rule::AnyType])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}

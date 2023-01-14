mod helpers;
pub(crate) mod rules;
pub mod settings;
mod types;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;

    #[test_case(RuleCode::ARG001, Path::new("ARG.py"); "ARG001")]
    #[test_case(RuleCode::ARG002, Path::new("ARG.py"); "ARG002")]
    #[test_case(RuleCode::ARG003, Path::new("ARG.py"); "ARG003")]
    #[test_case(RuleCode::ARG004, Path::new("ARG.py"); "ARG004")]
    #[test_case(RuleCode::ARG005, Path::new("ARG.py"); "ARG005")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn ignore_variadic_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: super::settings::Settings {
                    ignore_variadic_names: true,
                },
                ..settings::Settings::for_rules(vec![
                    RuleCode::ARG001,
                    RuleCode::ARG002,
                    RuleCode::ARG003,
                    RuleCode::ARG004,
                    RuleCode::ARG005,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_variadic_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: super::settings::Settings {
                    ignore_variadic_names: false,
                },
                ..settings::Settings::for_rules(vec![
                    RuleCode::ARG001,
                    RuleCode::ARG002,
                    RuleCode::ARG003,
                    RuleCode::ARG004,
                    RuleCode::ARG005,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}

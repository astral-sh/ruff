//! Rules from [flake8-unused-arguments](https://pypi.org/project/flake8-unused-arguments/).
mod helpers;
pub(crate) mod rules;
pub mod settings;
mod types;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::UnusedFunctionArgument, Path::new("ARG.py"); "ARG001")]
    #[test_case(Rule::UnusedMethodArgument, Path::new("ARG.py"); "ARG002")]
    #[test_case(Rule::UnusedClassMethodArgument, Path::new("ARG.py"); "ARG003")]
    #[test_case(Rule::UnusedStaticMethodArgument, Path::new("ARG.py"); "ARG004")]
    #[test_case(Rule::UnusedLambdaArgument, Path::new("ARG.py"); "ARG005")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_unused_arguments").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn ignore_variadic_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: super::settings::Settings {
                    ignore_variadic_names: true,
                },
                ..settings::Settings::for_rules(vec![
                    Rule::UnusedFunctionArgument,
                    Rule::UnusedMethodArgument,
                    Rule::UnusedClassMethodArgument,
                    Rule::UnusedStaticMethodArgument,
                    Rule::UnusedLambdaArgument,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_variadic_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: super::settings::Settings {
                    ignore_variadic_names: false,
                },
                ..settings::Settings::for_rules(vec![
                    Rule::UnusedFunctionArgument,
                    Rule::UnusedMethodArgument,
                    Rule::UnusedClassMethodArgument,
                    Rule::UnusedStaticMethodArgument,
                    Rule::UnusedLambdaArgument,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}

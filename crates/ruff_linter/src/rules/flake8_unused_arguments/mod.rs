//! Rules from [flake8-unused-arguments](https://pypi.org/project/flake8-unused-arguments/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::UnusedFunctionArgument, Path::new("ARG.py"))]
    #[test_case(Rule::UnusedMethodArgument, Path::new("ARG.py"))]
    #[test_case(Rule::UnusedClassMethodArgument, Path::new("ARG.py"))]
    #[test_case(Rule::UnusedStaticMethodArgument, Path::new("ARG.py"))]
    #[test_case(Rule::UnusedLambdaArgument, Path::new("ARG.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_unused_arguments").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn ignore_variadic_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::LinterSettings {
                flake8_unused_arguments: super::settings::Settings {
                    ignore_variadic_names: true,
                },
                ..settings::LinterSettings::for_rules(vec![
                    Rule::UnusedFunctionArgument,
                    Rule::UnusedMethodArgument,
                    Rule::UnusedClassMethodArgument,
                    Rule::UnusedStaticMethodArgument,
                    Rule::UnusedLambdaArgument,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn enforce_variadic_names() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::LinterSettings {
                flake8_unused_arguments: super::settings::Settings {
                    ignore_variadic_names: false,
                },
                ..settings::LinterSettings::for_rules(vec![
                    Rule::UnusedFunctionArgument,
                    Rule::UnusedMethodArgument,
                    Rule::UnusedClassMethodArgument,
                    Rule::UnusedStaticMethodArgument,
                    Rule::UnusedLambdaArgument,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}

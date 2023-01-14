mod fixes;
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/annotation_presence.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    RuleCode::ANN001,
                    RuleCode::ANN002,
                    RuleCode::ANN003,
                    RuleCode::ANN101,
                    RuleCode::ANN102,
                    RuleCode::ANN201,
                    RuleCode::ANN202,
                    RuleCode::ANN204,
                    RuleCode::ANN205,
                    RuleCode::ANN206,
                    RuleCode::ANN401,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn suppress_dummy_args() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_dummy_args.py"),
            &Settings {
                flake8_annotations: super::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: true,
                    suppress_none_returning: false,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    RuleCode::ANN001,
                    RuleCode::ANN002,
                    RuleCode::ANN003,
                    RuleCode::ANN101,
                    RuleCode::ANN102,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn mypy_init_return() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/mypy_init_return.py"),
            &Settings {
                flake8_annotations: super::settings::Settings {
                    mypy_init_return: true,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    RuleCode::ANN201,
                    RuleCode::ANN202,
                    RuleCode::ANN204,
                    RuleCode::ANN205,
                    RuleCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn suppress_none_returning() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_none_returning.py"),
            &Settings {
                flake8_annotations: super::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: true,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    RuleCode::ANN201,
                    RuleCode::ANN202,
                    RuleCode::ANN204,
                    RuleCode::ANN205,
                    RuleCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_star_arg_any() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_star_arg_any.py"),
            &Settings {
                flake8_annotations: super::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
                    allow_star_arg_any: true,
                },
                ..Settings::for_rules(vec![RuleCode::ANN401])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_overload() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_overload.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    RuleCode::ANN201,
                    RuleCode::ANN202,
                    RuleCode::ANN204,
                    RuleCode::ANN205,
                    RuleCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_nested_overload() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_nested_overload.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    RuleCode::ANN201,
                    RuleCode::ANN202,
                    RuleCode::ANN204,
                    RuleCode::ANN205,
                    RuleCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}

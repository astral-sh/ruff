//! Rules from [flake8-annotations](https://pypi.org/project/flake8-annotations/).
mod fixes;
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::assert_yaml_snapshot;
    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings::Settings;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/annotation_presence.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingTypeArgs,
                    Rule::MissingTypeKwargs,
                    Rule::MissingTypeSelf,
                    Rule::MissingTypeCls,
                    Rule::MissingReturnTypePublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                    Rule::DynamicallyTypedExpression,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
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
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingTypeArgs,
                    Rule::MissingTypeKwargs,
                    Rule::MissingTypeSelf,
                    Rule::MissingTypeCls,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
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
                    Rule::MissingReturnTypePublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
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
                    Rule::MissingReturnTypePublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
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
                ..Settings::for_rules(vec![Rule::DynamicallyTypedExpression])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_overload() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_overload.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    Rule::MissingReturnTypePublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_nested_overload() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_nested_overload.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    Rule::MissingReturnTypePublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}

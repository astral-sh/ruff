//! Rules from [flake8-annotations](https://pypi.org/project/flake8-annotations/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::types::PythonVersion;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/annotation_presence.py"),
            &LinterSettings {
                ..LinterSettings::for_rules(vec![
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingTypeArgs,
                    Rule::MissingTypeKwargs,
                    Rule::MissingTypeSelf,
                    Rule::MissingTypeCls,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                    Rule::AnyType,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ignore_fully_untyped() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/ignore_fully_untyped.py"),
            &LinterSettings {
                flake8_annotations: super::settings::Settings {
                    ignore_fully_untyped: true,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingTypeArgs,
                    Rule::MissingTypeKwargs,
                    Rule::MissingTypeSelf,
                    Rule::MissingTypeCls,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                    Rule::AnyType,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn suppress_dummy_args() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/suppress_dummy_args.py"),
            &LinterSettings {
                flake8_annotations: super::settings::Settings {
                    suppress_dummy_args: true,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingTypeArgs,
                    Rule::MissingTypeKwargs,
                    Rule::MissingTypeSelf,
                    Rule::MissingTypeCls,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn mypy_init_return() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/mypy_init_return.py"),
            &LinterSettings {
                flake8_annotations: super::settings::Settings {
                    mypy_init_return: true,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn auto_return_type() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/auto_return_type.py"),
            &LinterSettings {
                ..LinterSettings::for_rules(vec![
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn auto_return_type_py38() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/auto_return_type.py"),
            &LinterSettings {
                target_version: PythonVersion::Py38,
                ..LinterSettings::for_rules(vec![
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn suppress_none_returning() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/suppress_none_returning.py"),
            &LinterSettings {
                flake8_annotations: super::settings::Settings {
                    suppress_none_returning: true,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingTypeArgs,
                    Rule::MissingTypeKwargs,
                    Rule::MissingTypeSelf,
                    Rule::MissingTypeCls,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                    Rule::AnyType,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_star_arg_any() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/allow_star_arg_any.py"),
            &LinterSettings {
                flake8_annotations: super::settings::Settings {
                    allow_star_arg_any: true,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![Rule::AnyType])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_overload() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/allow_overload.py"),
            &LinterSettings {
                ..LinterSettings::for_rules(vec![
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_nested_overload() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/allow_nested_overload.py"),
            &LinterSettings {
                ..LinterSettings::for_rules(vec![
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::MissingReturnTypePrivateFunction,
                    Rule::MissingReturnTypeSpecialMethod,
                    Rule::MissingReturnTypeStaticMethod,
                    Rule::MissingReturnTypeClassMethod,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn simple_magic_methods() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_annotations/simple_magic_methods.py"),
            &LinterSettings::for_rule(Rule::MissingReturnTypeSpecialMethod),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}

mod fixes;
pub mod helpers;
pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::{flake8_annotations, Settings};

    #[test]
    fn defaults() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/annotation_presence.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    DiagnosticCode::ANN001,
                    DiagnosticCode::ANN002,
                    DiagnosticCode::ANN003,
                    DiagnosticCode::ANN101,
                    DiagnosticCode::ANN102,
                    DiagnosticCode::ANN201,
                    DiagnosticCode::ANN202,
                    DiagnosticCode::ANN204,
                    DiagnosticCode::ANN205,
                    DiagnosticCode::ANN206,
                    DiagnosticCode::ANN401,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn suppress_dummy_args() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_dummy_args.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: true,
                    suppress_none_returning: false,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    DiagnosticCode::ANN001,
                    DiagnosticCode::ANN002,
                    DiagnosticCode::ANN003,
                    DiagnosticCode::ANN101,
                    DiagnosticCode::ANN102,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn mypy_init_return() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/mypy_init_return.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: true,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    DiagnosticCode::ANN201,
                    DiagnosticCode::ANN202,
                    DiagnosticCode::ANN204,
                    DiagnosticCode::ANN205,
                    DiagnosticCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn suppress_none_returning() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_none_returning.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: true,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    DiagnosticCode::ANN201,
                    DiagnosticCode::ANN202,
                    DiagnosticCode::ANN204,
                    DiagnosticCode::ANN205,
                    DiagnosticCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn allow_star_arg_any() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_star_arg_any.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
                    allow_star_arg_any: true,
                },
                ..Settings::for_rules(vec![DiagnosticCode::ANN401])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn allow_overload() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_overload.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    DiagnosticCode::ANN201,
                    DiagnosticCode::ANN202,
                    DiagnosticCode::ANN204,
                    DiagnosticCode::ANN205,
                    DiagnosticCode::ANN206,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

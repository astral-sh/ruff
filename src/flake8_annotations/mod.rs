pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::autofix::fixer;
    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{flake8_annotations, Settings};

    #[test]
    fn defaults() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/annotation_presence.py"),
            &Settings {
                ..Settings::for_rules(vec![
                    CheckCode::ANN001,
                    CheckCode::ANN002,
                    CheckCode::ANN003,
                    CheckCode::ANN101,
                    CheckCode::ANN102,
                    CheckCode::ANN201,
                    CheckCode::ANN202,
                    CheckCode::ANN204,
                    CheckCode::ANN205,
                    CheckCode::ANN206,
                    CheckCode::ANN401,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn suppress_dummy_args() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_dummy_args.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: true,
                    suppress_none_returning: false,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    CheckCode::ANN001,
                    CheckCode::ANN002,
                    CheckCode::ANN003,
                    CheckCode::ANN101,
                    CheckCode::ANN102,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn mypy_init_return() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/mypy_init_return.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: true,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    CheckCode::ANN201,
                    CheckCode::ANN202,
                    CheckCode::ANN204,
                    CheckCode::ANN205,
                    CheckCode::ANN206,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn suppress_none_returning() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_none_returning.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: true,
                    allow_star_arg_any: false,
                },
                ..Settings::for_rules(vec![
                    CheckCode::ANN201,
                    CheckCode::ANN202,
                    CheckCode::ANN204,
                    CheckCode::ANN205,
                    CheckCode::ANN206,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn allow_star_arg_any() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_annotations/allow_star_arg_any.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
                    allow_star_arg_any: true,
                },
                ..Settings::for_rules(vec![CheckCode::ANN401])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

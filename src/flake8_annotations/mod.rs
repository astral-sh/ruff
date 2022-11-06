pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::lexer::LexResult;

    use crate::autofix::fixer;
    use crate::checks::{Check, CheckCode};
    use crate::linter::tokenize;
    use crate::{flake8_annotations, fs, linter, noqa, Settings, SourceCodeLocator};

    fn check_path(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> Result<Vec<Check>> {
        let contents = fs::read_file(path)?;
        let tokens: Vec<LexResult> = tokenize(&contents);
        let locator = SourceCodeLocator::new(&contents);
        let noqa_line_for = noqa::extract_noqa_line_for(&tokens);
        linter::check_path(
            path,
            &contents,
            tokens,
            &locator,
            &noqa_line_for,
            settings,
            autofix,
        )
    }

    #[test]
    fn defaults() -> Result<()> {
        let mut checks = check_path(
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
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_dummy_args.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: true,
                    suppress_none_returning: false,
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
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_annotations/mypy_init_return.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: true,
                    suppress_dummy_args: false,
                    suppress_none_returning: false,
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
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_annotations/suppress_none_returning.py"),
            &Settings {
                flake8_annotations: flake8_annotations::settings::Settings {
                    mypy_init_return: false,
                    suppress_dummy_args: false,
                    suppress_none_returning: true,
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
}

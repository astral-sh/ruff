mod helpers;
pub mod plugins;
pub mod settings;
mod types;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{flake8_unused_arguments, settings};

    #[test_case(CheckCode::ARG001, Path::new("ARG.py"); "ARG001")]
    #[test_case(CheckCode::ARG002, Path::new("ARG.py"); "ARG002")]
    #[test_case(CheckCode::ARG003, Path::new("ARG.py"); "ARG003")]
    #[test_case(CheckCode::ARG004, Path::new("ARG.py"); "ARG004")]
    #[test_case(CheckCode::ARG005, Path::new("ARG.py"); "ARG005")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn ignore_variadic_names() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: flake8_unused_arguments::settings::Settings {
                    ignore_variadic_names: true,
                },
                ..settings::Settings::for_rules(vec![
                    CheckCode::ARG001,
                    CheckCode::ARG002,
                    CheckCode::ARG003,
                    CheckCode::ARG004,
                    CheckCode::ARG005,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn enforce_variadic_names() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: flake8_unused_arguments::settings::Settings {
                    ignore_variadic_names: false,
                },
                ..settings::Settings::for_rules(vec![
                    CheckCode::ARG001,
                    CheckCode::ARG002,
                    CheckCode::ARG003,
                    CheckCode::ARG004,
                    CheckCode::ARG005,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

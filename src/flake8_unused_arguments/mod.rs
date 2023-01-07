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

    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::{flake8_unused_arguments, settings};

    #[test_case(DiagnosticCode::ARG001, Path::new("ARG.py"); "ARG001")]
    #[test_case(DiagnosticCode::ARG002, Path::new("ARG.py"); "ARG002")]
    #[test_case(DiagnosticCode::ARG003, Path::new("ARG.py"); "ARG003")]
    #[test_case(DiagnosticCode::ARG004, Path::new("ARG.py"); "ARG004")]
    #[test_case(DiagnosticCode::ARG005, Path::new("ARG.py"); "ARG005")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn ignore_variadic_names() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: flake8_unused_arguments::settings::Settings {
                    ignore_variadic_names: true,
                },
                ..settings::Settings::for_rules(vec![
                    DiagnosticCode::ARG001,
                    DiagnosticCode::ARG002,
                    DiagnosticCode::ARG003,
                    DiagnosticCode::ARG004,
                    DiagnosticCode::ARG005,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn enforce_variadic_names() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_unused_arguments/ignore_variadic_names.py"),
            &settings::Settings {
                flake8_unused_arguments: flake8_unused_arguments::settings::Settings {
                    ignore_variadic_names: false,
                },
                ..settings::Settings::for_rules(vec![
                    DiagnosticCode::ARG001,
                    DiagnosticCode::ARG002,
                    DiagnosticCode::ARG003,
                    DiagnosticCode::ARG004,
                    DiagnosticCode::ARG005,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

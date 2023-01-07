pub mod checks;
mod helpers;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::{flake8_bandit, Settings};

    #[test_case(DiagnosticCode::S101, Path::new("S101.py"); "S101")]
    #[test_case(DiagnosticCode::S102, Path::new("S102.py"); "S102")]
    #[test_case(DiagnosticCode::S103, Path::new("S103.py"); "S103")]
    #[test_case(DiagnosticCode::S104, Path::new("S104.py"); "S104")]
    #[test_case(DiagnosticCode::S105, Path::new("S105.py"); "S105")]
    #[test_case(DiagnosticCode::S106, Path::new("S106.py"); "S106")]
    #[test_case(DiagnosticCode::S107, Path::new("S107.py"); "S107")]
    #[test_case(DiagnosticCode::S108, Path::new("S108.py"); "S108")]
    #[test_case(DiagnosticCode::S113, Path::new("S113.py"); "S113")]
    #[test_case(DiagnosticCode::S324, Path::new("S324.py"); "S324")]
    #[test_case(DiagnosticCode::S501, Path::new("S501.py"); "S501")]
    #[test_case(DiagnosticCode::S506, Path::new("S506.py"); "S506")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_bandit")
                .join(path)
                .as_path(),
            &Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn check_hardcoded_tmp_additional_dirs() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_bandit/S108.py"),
            &Settings {
                flake8_bandit: flake8_bandit::settings::Settings {
                    hardcoded_tmp_directory: vec![
                        "/tmp".to_string(),
                        "/var/tmp".to_string(),
                        "/dev/shm".to_string(),
                        "/foo".to_string(),
                    ],
                },
                ..Settings::for_rule(DiagnosticCode::S108)
            },
        )?;
        insta::assert_yaml_snapshot!("S108_extend", checks);
        Ok(())
    }
}

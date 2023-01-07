//! Module for Ruff-specific rules.

pub mod checks;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::settings;
    #[test_case(DiagnosticCode::RUF004, Path::new("RUF004.py"); "RUF004")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn confusables() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff/confusables.py"),
            &settings::Settings {
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::Settings::for_rules(vec![
                    DiagnosticCode::RUF001,
                    DiagnosticCode::RUF002,
                    DiagnosticCode::RUF003,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ruf100_0() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff/RUF100_0.py"),
            &settings::Settings::for_rules(vec![
                DiagnosticCode::RUF100,
                DiagnosticCode::E501,
                DiagnosticCode::F401,
                DiagnosticCode::F841,
            ]),
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ruf100_1() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff/RUF100_1.py"),
            &settings::Settings::for_rules(vec![DiagnosticCode::RUF100, DiagnosticCode::F401]),
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn flake8_noqa() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff/flake8_noqa.py"),
            &settings::Settings::for_rules(vec![DiagnosticCode::F401, DiagnosticCode::F841]),
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ruff_noqa() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff/ruff_noqa.py"),
            &settings::Settings::for_rules(vec![DiagnosticCode::F401, DiagnosticCode::F841]),
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn redirects() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/ruff/redirects.py"),
            &settings::Settings::for_rules(vec![DiagnosticCode::UP007]),
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

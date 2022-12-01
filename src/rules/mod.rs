//! Module for Ruff-specific rules.

pub mod checks;
pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test_case(CheckCode::RUF001, Path::new("RUF001.py"); "RUF001")]
    #[test_case(CheckCode::RUF002, Path::new("RUF002.py"); "RUF002")]
    #[test_case(CheckCode::RUF003, Path::new("RUF003.py"); "RUF003")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_0.py"); "RUF101_0")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_1.py"); "RUF101_1")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_2.py"); "RUF101_2")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_3.py"); "RUF101_3")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_4.py"); "RUF101_4")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_5.py"); "RUF101_5")]
    #[test_case(CheckCode::RUF101, Path::new("RUF101_6.py"); "RUF101_6")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn m001() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/ruff/M001.py"),
            &settings::Settings::for_rules(vec![CheckCode::M001, CheckCode::E501, CheckCode::F841]),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}

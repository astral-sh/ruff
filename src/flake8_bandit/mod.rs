mod helpers;
pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::CheckCode;
    use crate::settings;

    #[test_case(CheckCode::S101, Path::new("S101.py"); "S101")]
    #[test_case(CheckCode::S102, Path::new("S102.py"); "S102")]
    #[test_case(CheckCode::S104, Path::new("S104.py"); "S104")]
    #[test_case(CheckCode::S105, Path::new("S105.py"); "S105")]
    #[test_case(CheckCode::S106, Path::new("S106.py"); "S106")]
    #[test_case(CheckCode::S107, Path::new("S107.py"); "S107")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_bandit")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

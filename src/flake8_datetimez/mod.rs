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

    #[test_case(CheckCode::DTZ001, Path::new("DTZ001.py"); "DTZ001")]
    #[test_case(CheckCode::DTZ002, Path::new("DTZ002.py"); "DTZ002")]
    #[test_case(CheckCode::DTZ003, Path::new("DTZ003.py"); "DTZ003")]
    #[test_case(CheckCode::DTZ004, Path::new("DTZ004.py"); "DTZ004")]
    #[test_case(CheckCode::DTZ005, Path::new("DTZ005.py"); "DTZ005")]
    #[test_case(CheckCode::DTZ006, Path::new("DTZ006.py"); "DTZ006")]
    #[test_case(CheckCode::DTZ007, Path::new("DTZ007.py"); "DTZ007")]
    #[test_case(CheckCode::DTZ011, Path::new("DTZ011.py"); "DTZ011")]
    #[test_case(CheckCode::DTZ012, Path::new("DTZ012.py"); "DTZ012")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_datetimez")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

pub mod checks;
pub mod types;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test_case(CheckCode::A001, Path::new("A001.py"); "A001")]
    #[test_case(CheckCode::A002, Path::new("A002.py"); "A002")]
    #[test_case(CheckCode::A003, Path::new("A003.py"); "A003")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_builtins")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

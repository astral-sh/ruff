mod helpers;
pub mod plugins;
mod types;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

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
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

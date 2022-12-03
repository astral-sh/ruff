pub mod plugins;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::Settings;

    #[test_case(CheckCode::PLE0206, Path::new("property_with_parameters.py"); "PLE0206")]
    #[test_case(CheckCode::PLE1142, Path::new("await_outside_async.py"); "PLE1142")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pylint")
                .join(path)
                .as_path(),
            &Settings::for_rules(vec![check_code]),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

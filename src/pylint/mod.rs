pub mod plugins;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::Settings;

    #[test_case(CheckCode::PLC2201, Path::new("misplaced_comparison_constant.py"); "PLC2201")]
    #[test_case(CheckCode::PLC3002, Path::new("unnecessary_direct_lambda_call.py"); "PLC3002")]
    #[test_case(CheckCode::PLE1142, Path::new("await_outside_async.py"); "PLE1142")]
    #[test_case(CheckCode::PLR0206, Path::new("property_with_parameters.py"); "PLR0206")]
    #[test_case(CheckCode::PLR0402, Path::new("import_aliasing.py"); "PLR0402")]
    #[test_case(CheckCode::PLR1701, Path::new("consider_merging_isinstance.py"); "PLR1701")]
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

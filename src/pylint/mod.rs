pub mod plugins;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::Settings;

    #[test_case(CheckCode::PLC0414, Path::new("import_aliasing.py"); "PLC0414")]
    #[test_case(CheckCode::PLC2201, Path::new("misplaced_comparison_constant.py"); "PLC2201")]
    #[test_case(CheckCode::PLC3002, Path::new("unnecessary_direct_lambda_call.py"); "PLC3002")]
    #[test_case(CheckCode::PLE0117, Path::new("nonlocal_without_binding.py"); "PLE0117")]
    #[test_case(CheckCode::PLE0118, Path::new("used_prior_global_declaration.py"); "PLE0118")]
    #[test_case(CheckCode::PLE1142, Path::new("await_outside_async.py"); "PLE1142")]
    #[test_case(CheckCode::PLR0206, Path::new("property_with_parameters.py"); "PLR0206")]
    #[test_case(CheckCode::PLR0402, Path::new("import_aliasing.py"); "PLR0402")]
    #[test_case(CheckCode::PLR1701, Path::new("consider_merging_isinstance.py"); "PLR1701")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_0.py"); "PLR1722_0")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_1.py"); "PLR1722_1")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_2.py"); "PLR1722_2")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_3.py"); "PLR1722_3")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_4.py"); "PLR1722_4")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_5.py"); "PLR1722_5")]
    #[test_case(CheckCode::PLR1722, Path::new("consider_using_sys_exit_6.py"); "PLR1722_6")]
    #[test_case(CheckCode::PLW0120, Path::new("useless_else_on_loop.py"); "PLW0120")]
    #[test_case(CheckCode::PLW0602, Path::new("global_variable_not_assigned.py"); "PLW0602")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pylint")
                .join(path)
                .as_path(),
            &Settings::for_rules(vec![check_code]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}

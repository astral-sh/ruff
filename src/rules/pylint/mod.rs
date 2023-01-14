pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test_case(RuleCode::PLC0414, Path::new("import_aliasing.py"); "PLC0414")]
    #[test_case(RuleCode::PLC2201, Path::new("misplaced_comparison_constant.py"); "PLC2201")]
    #[test_case(RuleCode::PLC3002, Path::new("unnecessary_direct_lambda_call.py"); "PLC3002")]
    #[test_case(RuleCode::PLE0117, Path::new("nonlocal_without_binding.py"); "PLE0117")]
    #[test_case(RuleCode::PLE0118, Path::new("used_prior_global_declaration.py"); "PLE0118")]
    #[test_case(RuleCode::PLE1142, Path::new("await_outside_async.py"); "PLE1142")]
    #[test_case(RuleCode::PLR0133, Path::new("constant_comparison.py"); "PLR0133")]
    #[test_case(RuleCode::PLR0206, Path::new("property_with_parameters.py"); "PLR0206")]
    #[test_case(RuleCode::PLR0402, Path::new("import_aliasing.py"); "PLR0402")]
    #[test_case(RuleCode::PLR1701, Path::new("consider_merging_isinstance.py"); "PLR1701")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_0.py"); "PLR1722_0")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_1.py"); "PLR1722_1")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_2.py"); "PLR1722_2")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_3.py"); "PLR1722_3")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_4.py"); "PLR1722_4")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_5.py"); "PLR1722_5")]
    #[test_case(RuleCode::PLR1722, Path::new("consider_using_sys_exit_6.py"); "PLR1722_6")]
    #[test_case(RuleCode::PLR2004, Path::new("magic_value_comparison.py"); "PLR2004")]
    #[test_case(RuleCode::PLW0120, Path::new("useless_else_on_loop.py"); "PLW0120")]
    #[test_case(RuleCode::PLW0602, Path::new("global_variable_not_assigned.py"); "PLW0602")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pylint")
                .join(path)
                .as_path(),
            &Settings::for_rules(vec![rule_code]),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

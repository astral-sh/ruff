pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;

    #[test_case(RuleCode::PIE790, Path::new("PIE790.py"); "PIE790")]
    #[test_case(RuleCode::PIE794, Path::new("PIE794.py"); "PIE794")]
    #[test_case(RuleCode::PIE796, Path::new("PIE796.py"); "PIE796")]
    #[test_case(RuleCode::PIE807, Path::new("PIE807.py"); "PIE807")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_pie")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

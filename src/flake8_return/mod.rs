mod helpers;
pub(crate) mod rules;
mod visitor;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test_case(RuleCode::RET501, Path::new("RET501.py"); "RET501")]
    #[test_case(RuleCode::RET502, Path::new("RET502.py"); "RET502")]
    #[test_case(RuleCode::RET503, Path::new("RET503.py"); "RET503")]
    #[test_case(RuleCode::RET504, Path::new("RET504.py"); "RET504")]
    #[test_case(RuleCode::RET505, Path::new("RET505.py"); "RET505")]
    #[test_case(RuleCode::RET506, Path::new("RET506.py"); "RET506")]
    #[test_case(RuleCode::RET507, Path::new("RET507.py"); "RET507")]
    #[test_case(RuleCode::RET508, Path::new("RET508.py"); "RET508")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_return")
                .join(path)
                .as_path(),
            &Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

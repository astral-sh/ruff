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

    #[test_case(RuleCode::DTZ001, Path::new("DTZ001.py"); "DTZ001")]
    #[test_case(RuleCode::DTZ002, Path::new("DTZ002.py"); "DTZ002")]
    #[test_case(RuleCode::DTZ003, Path::new("DTZ003.py"); "DTZ003")]
    #[test_case(RuleCode::DTZ004, Path::new("DTZ004.py"); "DTZ004")]
    #[test_case(RuleCode::DTZ005, Path::new("DTZ005.py"); "DTZ005")]
    #[test_case(RuleCode::DTZ006, Path::new("DTZ006.py"); "DTZ006")]
    #[test_case(RuleCode::DTZ007, Path::new("DTZ007.py"); "DTZ007")]
    #[test_case(RuleCode::DTZ011, Path::new("DTZ011.py"); "DTZ011")]
    #[test_case(RuleCode::DTZ012, Path::new("DTZ012.py"); "DTZ012")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_datetimez")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

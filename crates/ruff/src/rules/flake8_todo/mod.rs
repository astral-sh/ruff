pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_yaml_snapshot;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Rule::InvalidTODOTag, Path::new("T001.py"); "T001")]
    #[test_case(Rule::MissingAuthorInTODO, Path::new("T002.py"); "T002")]
    #[test_case(Rule::MissingColonInTODO, Path::new("T004.py"); "T004")]
    #[test_case(Rule::MissingTextInTODO, Path::new("T005.py"); "T005")]
    #[test_case(Rule::MissingSpaceAfterColonInTODO, Path::new("T007.py"); "T007")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_todo").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::InvalidTodoTag, Path::new("TDO001.py"); "TDO001")]
    #[test_case(Rule::MissingAuthorInTodo, Path::new("TDO002.py"); "TDO002")]
    #[test_case(Rule::MissingLinkInTodo, Path::new("TDO003.py"); "TDO003")]
    #[test_case(Rule::MissingColonInTodo, Path::new("TDO004.py"); "TDO004")]
    #[test_case(Rule::MissingTextInTodo, Path::new("TDO005.py"); "TDO005")]
    #[test_case(Rule::InvalidCapitalizationInTodo, Path::new("TDO006.py"); "TDO006")]
    #[test_case(Rule::MissingSpaceAfterColonInTodo, Path::new("TDO007.py"); "TDO007")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_todo").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

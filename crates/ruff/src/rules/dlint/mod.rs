//! Rules from [dlint](https://pypi.org/project/dlint/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::BadEvalUse, Path::new("DUO104.py"); "DUO104")]
    #[test_case(Rule::BadExecUse, Path::new("DUO105.py"); "DUO105")]
    #[test_case(Rule::BadCompileUse, Path::new("DUO110.py"); "DUO110")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("dlint").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

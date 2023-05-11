//! Rules from [flake8-async](https://pypi.org/project/flake8-async/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::assert_messages;
    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::BlockingHttpCallInsideAsyncDef, Path::new("ASY100.py"); "ASY100")]
    #[test_case(Rule::OpenSleepOrSubprocessInsideAsyncDef, Path::new("ASY101.py"); "ASY101")]
    #[test_case(Rule::UnsafeOsMethodInsideAsyncDef, Path::new("ASY102.py"); "ASY102")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_async").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

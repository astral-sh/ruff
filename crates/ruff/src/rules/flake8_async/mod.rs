//! Rules from [flake8-async](https://pypi.org/project/flake8-async/).
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

    #[test_case(Rule::BlockingHttpCallInAsyncFunction, Path::new("ASYNC100.py"))]
    #[test_case(Rule::OpenSleepOrSubprocessInAsyncFunction, Path::new("ASYNC101.py"))]
    #[test_case(Rule::BlockingOsCallInAsyncFunction, Path::new("ASYNC102.py"))]
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

//! Rules from [flake8-trio](https://pypi.org/project/flake8-trio/).
pub(super) mod method_name;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::TrioTimeoutWithoutAwait, Path::new("TRIO100.py"))]
    #[test_case(Rule::TrioSyncCall, Path::new("TRIO105.py"))]
    #[test_case(Rule::TrioAsyncFunctionWithTimeout, Path::new("TRIO109.py"))]
    #[test_case(Rule::TrioUnneededSleep, Path::new("TRIO110.py"))]
    #[test_case(Rule::TrioZeroSleepCall, Path::new("TRIO115.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_trio").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

//! Rules from [flake8-async](https://pypi.org/project/flake8-async/).
mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::PreviewMode;
    use crate::settings::LinterSettings;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::TrioTimeoutWithoutAwait, Path::new("ASYNC100.py"))]
    #[test_case(Rule::TrioSyncCall, Path::new("ASYNC105.py"))]
    #[test_case(Rule::AsyncFunctionWithTimeout, Path::new("ASYNC109_0.py"))]
    #[test_case(Rule::AsyncFunctionWithTimeout, Path::new("ASYNC109_1.py"))]
    #[test_case(Rule::TrioUnneededSleep, Path::new("ASYNC110.py"))]
    #[test_case(Rule::TrioZeroSleepCall, Path::new("ASYNC115.py"))]
    #[test_case(Rule::SleepForeverCall, Path::new("ASYNC116.py"))]
    #[test_case(Rule::BlockingHttpCallInAsyncFunction, Path::new("ASYNC210.py"))]
    #[test_case(Rule::CreateSubprocessInAsyncFunction, Path::new("ASYNC22x.py"))]
    #[test_case(Rule::RunProcessInAsyncFunction, Path::new("ASYNC22x.py"))]
    #[test_case(Rule::WaitForProcessInAsyncFunction, Path::new("ASYNC22x.py"))]
    #[test_case(Rule::BlockingOpenCallInAsyncFunction, Path::new("ASYNC230.py"))]
    #[test_case(Rule::BlockingSleepInAsyncFunction, Path::new("ASYNC251.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_async").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::AsyncFunctionWithTimeout, Path::new("ASYNC109_0.py"))]
    #[test_case(Rule::AsyncFunctionWithTimeout, Path::new("ASYNC109_1.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_async").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

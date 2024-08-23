//! Rules from [flake8-async](https://pypi.org/project/flake8-async/).
mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::types::PythonVersion;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::CancelScopeNoCheckpoint, Path::new("ASYNC100.py"))]
    #[test_case(Rule::TrioSyncCall, Path::new("ASYNC105.py"))]
    #[test_case(Rule::AsyncFunctionWithTimeout, Path::new("ASYNC109_0.py"))]
    #[test_case(Rule::AsyncFunctionWithTimeout, Path::new("ASYNC109_1.py"))]
    #[test_case(Rule::AsyncBusyWait, Path::new("ASYNC110.py"))]
    #[test_case(Rule::AsyncZeroSleep, Path::new("ASYNC115.py"))]
    #[test_case(Rule::LongSleepNotForever, Path::new("ASYNC116.py"))]
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

    #[test_case(Path::new("ASYNC109_0.py"); "asyncio")]
    #[test_case(Path::new("ASYNC109_1.py"); "trio")]
    fn async109_python_310_or_older(path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_async").join(path),
            &LinterSettings {
                target_version: PythonVersion::Py310,
                ..LinterSettings::for_rule(Rule::AsyncFunctionWithTimeout)
            },
        )?;
        assert_messages!(path.file_name().unwrap().to_str().unwrap(), diagnostics);
        Ok(())
    }
}

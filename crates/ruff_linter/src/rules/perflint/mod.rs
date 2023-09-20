//! Rules from [perflint](https://pypi.org/project/perflint/).
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

    #[test_case(Rule::UnnecessaryListCast, Path::new("PERF101.py"))]
    #[test_case(Rule::IncorrectDictIterator, Path::new("PERF102.py"))]
    #[test_case(Rule::TryExceptInLoop, Path::new("PERF203.py"))]
    #[test_case(Rule::ManualListComprehension, Path::new("PERF401.py"))]
    #[test_case(Rule::ManualListCopy, Path::new("PERF402.py"))]
    #[test_case(Rule::ManualDictComprehension, Path::new("PERF403.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("perflint").join(path).as_path(),
            &LinterSettings::for_rule(rule_code).with_target_version(PythonVersion::Py310),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

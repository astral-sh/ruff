//! Rules from [flake8-raise](https://pypi.org/project/flake8-raise/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};

    #[test_case(Rule::UnnecessaryParenOnRaiseException, Path::new("RSE102.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_raise").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}

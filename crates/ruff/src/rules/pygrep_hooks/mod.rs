//! Rules from [pygrep-hooks](https://github.com/pre-commit/pygrep-hooks).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::Eval, Path::new("PGH001_0.py"))]
    #[test_case(Rule::Eval, Path::new("PGH001_1.py"))]
    #[test_case(Rule::DeprecatedLogWarn, Path::new("PGH002_0.py"))]
    #[test_case(Rule::DeprecatedLogWarn, Path::new("PGH002_1.py"))]
    #[test_case(Rule::BlanketTypeIgnore, Path::new("PGH003_0.py"))]
    #[test_case(Rule::BlanketTypeIgnore, Path::new("PGH003_1.py"))]
    #[test_case(Rule::BlanketNOQA, Path::new("PGH004_0.py"))]
    #[test_case(Rule::InvalidMockAccess, Path::new("PGH005_0.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pygrep_hooks").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

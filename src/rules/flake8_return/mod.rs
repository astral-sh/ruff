//! Rules from [flake8-return](https://pypi.org/project/flake8-return/1.2.0/).
mod helpers;
pub(crate) mod rules;
mod visitor;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings::Settings;

    #[test_case(Rule::UnnecessaryReturnNone, Path::new("RET501.py"); "RET501")]
    #[test_case(Rule::ImplicitReturnValue, Path::new("RET502.py"); "RET502")]
    #[test_case(Rule::ImplicitReturn, Path::new("RET503.py"); "RET503")]
    #[test_case(Rule::UnnecessaryAssign, Path::new("RET504.py"); "RET504")]
    #[test_case(Rule::SuperfluousElseReturn, Path::new("RET505.py"); "RET505")]
    #[test_case(Rule::SuperfluousElseRaise, Path::new("RET506.py"); "RET506")]
    #[test_case(Rule::SuperfluousElseContinue, Path::new("RET507.py"); "RET507")]
    #[test_case(Rule::SuperfluousElseBreak, Path::new("RET508.py"); "RET508")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_return")
                .join(path)
                .as_path(),
            &Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

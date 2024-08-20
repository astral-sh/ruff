//! Rules from [flake8-return](https://pypi.org/project/flake8-return/).
mod branch;
mod helpers;
pub(crate) mod rules;
mod visitor;

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

    #[test_case(Rule::UnnecessaryReturnNone, Path::new("RET501.py"))]
    #[test_case(Rule::ImplicitReturnValue, Path::new("RET502.py"))]
    #[test_case(Rule::ImplicitReturn, Path::new("RET503.py"))]
    #[test_case(Rule::UnnecessaryAssign, Path::new("RET504.py"))]
    #[test_case(Rule::SuperfluousElseReturn, Path::new("RET505.py"))]
    #[test_case(Rule::SuperfluousElseRaise, Path::new("RET506.py"))]
    #[test_case(Rule::SuperfluousElseContinue, Path::new("RET507.py"))]
    #[test_case(Rule::SuperfluousElseBreak, Path::new("RET508.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_return").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::ImplicitReturn, Path::new("RET503.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_return").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

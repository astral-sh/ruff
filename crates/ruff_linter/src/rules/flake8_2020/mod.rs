//! Rules from [flake8-2020](https://pypi.org/project/flake8-2020/).
mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::SysVersionSlice3, Path::new("YTT101.py"))]
    #[test_case(Rule::SysVersion2, Path::new("YTT102.py"))]
    #[test_case(Rule::SysVersionCmpStr3, Path::new("YTT103.py"))]
    #[test_case(Rule::SysVersionInfo0Eq3, Path::new("YTT201.py"))]
    #[test_case(Rule::SixPY3, Path::new("YTT202.py"))]
    #[test_case(Rule::SysVersionInfo1CmpInt, Path::new("YTT203.py"))]
    #[test_case(Rule::SysVersionInfoMinorCmpInt, Path::new("YTT204.py"))]
    #[test_case(Rule::SysVersion0, Path::new("YTT301.py"))]
    #[test_case(Rule::SysVersionCmpStr10, Path::new("YTT302.py"))]
    #[test_case(Rule::SysVersionSlice1, Path::new("YTT303.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_2020").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

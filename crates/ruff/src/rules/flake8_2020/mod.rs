//! Rules from [flake8-2020](https://pypi.org/project/flake8-2020/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_yaml_snapshot;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Rule::SysVersionSlice3, Path::new("YTT101.py"); "YTT101")]
    #[test_case(Rule::SysVersion2, Path::new("YTT102.py"); "YTT102")]
    #[test_case(Rule::SysVersionCmpStr3, Path::new("YTT103.py"); "YTT103")]
    #[test_case(Rule::SysVersionInfo0Eq3, Path::new("YTT201.py"); "YTT201")]
    #[test_case(Rule::SixPY3, Path::new("YTT202.py"); "YTT202")]
    #[test_case(Rule::SysVersionInfo1CmpInt, Path::new("YTT203.py"); "YTT203")]
    #[test_case(Rule::SysVersionInfoMinorCmpInt, Path::new("YTT204.py"); "YTT204")]
    #[test_case(Rule::SysVersion0, Path::new("YTT301.py"); "YTT301")]
    #[test_case(Rule::SysVersionCmpStr10, Path::new("YTT302.py"); "YTT302")]
    #[test_case(Rule::SysVersionSlice1, Path::new("YTT303.py"); "YTT303")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_2020").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

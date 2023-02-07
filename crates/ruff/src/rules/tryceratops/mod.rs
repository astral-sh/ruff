//! Rules from [tryceratops](https://pypi.org/project/tryceratops/1.1.0/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::RaiseVanillaClass, Path::new("TRY002.py"); "TRY002")]
    #[test_case(Rule::RaiseVanillaArgs, Path::new("TRY003.py"); "TRY003")]
    #[test_case(Rule::PreferTypeError, Path::new("TRY004.py"); "TRY004")]
    #[test_case(Rule::ReraiseNoCause, Path::new("TRY200.py"); "TRY200")]
    #[test_case(Rule::VerboseRaise, Path::new("TRY201.py"); "TRY201")]
    #[test_case(Rule::TryConsiderElse, Path::new("TRY300.py"); "TRY300")]
    #[test_case(Rule::RaiseWithinTry , Path::new("TRY301.py"); "TRY301")]
    #[test_case(Rule::ErrorInsteadOfException, Path::new("TRY400.py"); "TRY400")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("tryceratops").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

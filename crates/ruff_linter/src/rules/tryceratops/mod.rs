//! Rules from [tryceratops](https://pypi.org/project/tryceratops/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;

    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::RaiseVanillaClass, Path::new("TRY002.py"))]
    #[test_case(Rule::RaiseVanillaArgs, Path::new("TRY003.py"))]
    #[test_case(Rule::TypeCheckWithoutTypeError, Path::new("TRY004.py"))]
    #[test_case(Rule::VerboseRaise, Path::new("TRY201.py"))]
    #[test_case(Rule::UselessTryExcept, Path::new("TRY203.py"))]
    #[test_case(Rule::TryConsiderElse, Path::new("TRY300.py"))]
    #[test_case(Rule::RaiseWithinTry, Path::new("TRY301.py"))]
    #[test_case(Rule::ErrorInsteadOfException, Path::new("TRY400.py"))]
    #[test_case(Rule::VerboseLogMessage, Path::new("TRY401.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("tryceratops").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

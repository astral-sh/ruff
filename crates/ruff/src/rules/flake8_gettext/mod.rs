//! Rules from [flake8-gettext](https://pypi.org/project/flake8-gettext/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::FStringInGetTextFuncCall,Path::new("INT001.py"); "INT001")]
    #[test_case(Rule::FormatInGetTextFuncCall, Path::new("INT002.py"); "INT002")]
    #[test_case(Rule::PrintfInGetTextFuncCall, Path::new("INT003.py"); "INT003")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_gettext").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

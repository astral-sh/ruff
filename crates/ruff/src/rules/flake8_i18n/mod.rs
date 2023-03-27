//! Rules from [flake8-i18n](https://pypi.org/project/flake8-i18n/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_yaml_snapshot;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Rule::FStringInI18NFuncCall,Path::new("INT001.py"); "INT001")]
    #[test_case(Rule::FormatInI18NFuncCall, Path::new("INT002.py"); "INT002")]
    #[test_case(Rule::PrintfInI18NFuncCall, Path::new("INT003.py"); "INT003")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_i18n").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

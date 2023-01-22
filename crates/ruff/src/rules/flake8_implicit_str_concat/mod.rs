//! Rules from [flake8-implicit-str-concat](https://pypi.org/project/flake8-implicit-str-concat/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::SingleLineImplicitStringConcatenation, Path::new("ISC.py"); "ISC001")]
    #[test_case(Rule::MultiLineImplicitStringConcatenation, Path::new("ISC.py"); "ISC002")]
    #[test_case(Rule::ExplicitStringConcatenation, Path::new("ISC.py"); "ISC003")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_implicit_str_concat").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::SingleLineImplicitStringConcatenation, Path::new("ISC.py"); "ISC001")]
    #[test_case(Rule::MultiLineImplicitStringConcatenation, Path::new("ISC.py"); "ISC002")]
    #[test_case(Rule::ExplicitStringConcatenation, Path::new("ISC.py"); "ISC003")]
    fn multiline(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("multiline_{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_implicit_str_concat").join(path).as_path(),
            &settings::Settings {
                flake8_implicit_str_concat: super::settings::Settings {
                    allow_multiline: false,
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

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
    use crate::{assert_messages, settings};

    #[test_case(Rule::SingleLineImplicitStringConcatenation, Path::new("ISC.py"))]
    #[test_case(Rule::MultiLineImplicitStringConcatenation, Path::new("ISC.py"))]
    #[test_case(
        Rule::SingleLineImplicitStringConcatenation,
        Path::new("ISC_syntax_error.py")
    )]
    #[test_case(
        Rule::MultiLineImplicitStringConcatenation,
        Path::new("ISC_syntax_error.py")
    )]
    #[test_case(Rule::ExplicitStringConcatenation, Path::new("ISC.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_implicit_str_concat").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::SingleLineImplicitStringConcatenation, Path::new("ISC.py"))]
    #[test_case(Rule::MultiLineImplicitStringConcatenation, Path::new("ISC.py"))]
    #[test_case(Rule::ExplicitStringConcatenation, Path::new("ISC.py"))]
    fn multiline(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "multiline_{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_implicit_str_concat").join(path).as_path(),
            &settings::LinterSettings {
                flake8_implicit_str_concat: super::settings::Settings {
                    allow_multiline: false,
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}

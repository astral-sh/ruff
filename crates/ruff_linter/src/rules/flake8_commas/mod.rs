//! Rules from [flake8-commas](https://pypi.org/project/flake8-commas/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Path::new("COM81.py"))]
    #[test_case(Path::new("COM81_syntax_error.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_commas").join(path).as_path(),
            &LinterSettings::for_rules(vec![
                Rule::MissingTrailingComma,
                Rule::TrailingCommaOnBareTuple,
                Rule::ProhibitedTrailingComma,
            ]),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::MissingTrailingComma, Path::new("COM81.py"))]
    fn allow_single_arg_function_calls(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_{}_allow_single_arg_function_calls",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_commas").join(path).as_path(),
            &LinterSettings {
                flake8_commas: super::settings::Settings {
                    allow_single_arg_function_calls: true,
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}

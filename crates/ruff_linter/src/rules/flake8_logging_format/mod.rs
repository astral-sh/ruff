//! Rules from [flake8-logging-format](https://pypi.org/project/flake8-logging-format/).
pub(crate) mod rules;
pub(crate) mod violations;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};

    #[test_case(Path::new("G_argparse_parser_error_ok.py"))]
    #[test_case(Path::new("G_extra_ok.py"))]
    #[test_case(Path::new("G_extra_str_format_ok.py"))]
    #[test_case(Path::new("G_simple_ok.py"))]
    #[test_case(Path::new("G_warnings_ok.py"))]
    #[test_case(Path::new("G001.py"))]
    #[test_case(Path::new("G002.py"))]
    #[test_case(Path::new("G003.py"))]
    #[test_case(Path::new("G004.py"))]
    #[test_case(Path::new("G004_arg_order.py"))]
    #[test_case(Path::new("G004_implicit_concat.py"))]
    #[test_case(Path::new("G010.py"))]
    #[test_case(Path::new("G101_1.py"))]
    #[test_case(Path::new("G101_2.py"))]
    #[test_case(Path::new("G201.py"))]
    #[test_case(Path::new("G202.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_logging_format").join(path).as_path(),
            &settings::LinterSettings {
                logger_objects: vec!["logging_setup.logger".to_string()],
                ..settings::LinterSettings::for_rules(vec![
                    Rule::LoggingStringFormat,
                    Rule::LoggingPercentFormat,
                    Rule::LoggingStringConcat,
                    Rule::LoggingFString,
                    Rule::LoggingWarn,
                    Rule::LoggingExtraAttrClash,
                    Rule::LoggingExcInfo,
                    Rule::LoggingRedundantExcInfo,
                ])
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::LoggingFString, Path::new("G004.py"))]
    #[test_case(Rule::LoggingFString, Path::new("G004_arg_order.py"))]
    #[test_case(Rule::LoggingFString, Path::new("G004_implicit_concat.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_logging_format").join(path).as_path(),
            &settings::LinterSettings {
                logger_objects: vec!["logging_setup.logger".to_string()],
                preview: settings::types::PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}

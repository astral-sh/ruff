//! Rules from [flake8-logging-format](https://pypi.org/project/flake8-logging-format/0.9.0/).
pub(crate) mod rules;
pub(crate) mod violations;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Path::new("G_argparse_parser_error_ok.py"); "G_argparse_parser_error_ok")]
    #[test_case(Path::new("G_extra_ok.py"); "G_extra_ok")]
    #[test_case(Path::new("G_extra_str_format_ok.py"); "G_extra_str_format_ok")]
    #[test_case(Path::new("G_simple_ok.py"); "G_simple_ok")]
    #[test_case(Path::new("G_warnings_ok.py"); "G_warnings_ok")]
    #[test_case(Path::new("G001.py"); "G001")]
    #[test_case(Path::new("G002.py"); "G002")]
    #[test_case(Path::new("G003.py"); "G003")]
    #[test_case(Path::new("G004.py"); "G004")]
    #[test_case(Path::new("G010.py"); "G010")]
    #[test_case(Path::new("G101_1.py"); "G101_1")]
    #[test_case(Path::new("G101_2.py"); "G101_2")]
    #[test_case(Path::new("G201.py"); "G201")]
    #[test_case(Path::new("G202.py"); "G202")]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_logging_format").join(path).as_path(),
            &settings::Settings::for_rules(vec![
                Rule::LoggingStringFormat,
                Rule::LoggingPercentFormat,
                Rule::LoggingStringConcat,
                Rule::LoggingFString,
                Rule::LoggingWarn,
                Rule::LoggingExtraAttrClash,
                Rule::LoggingExcInfo,
                Rule::LoggingRedundantExcInfo,
            ]),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}

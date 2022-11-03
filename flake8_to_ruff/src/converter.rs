use std::collections::HashMap;

use anyhow::Result;
use ruff::flake8_quotes::settings::Quote;
use ruff::settings::options::Options;
use ruff::settings::pyproject::Pyproject;
use ruff::{flake8_quotes, pep8_naming};

use crate::parser;

pub fn convert(config: HashMap<String, HashMap<String, Option<String>>>) -> Result<Pyproject> {
    // Extract the Flake8 section.
    let flake8 = config
        .get("flake8")
        .expect("Unable to find flake8 section in INI file.");

    // Parse each supported option.
    let mut options: Options = Default::default();
    let mut flake8_quotes: flake8_quotes::settings::Options = Default::default();
    let mut pep8_naming: pep8_naming::settings::Options = Default::default();
    for (key, value) in flake8 {
        if let Some(value) = value {
            match key.as_str() {
                // flake8
                "max-line-length" | "max_line_length" => match value.clone().parse::<usize>() {
                    Ok(line_length) => options.line_length = Some(line_length),
                    Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                },
                "select" => {
                    options.select = Some(parser::parse_prefix_codes(value.as_ref()));
                }
                "extend-select" | "extend_select" => {
                    options.extend_select = Some(parser::parse_prefix_codes(value.as_ref()));
                }
                "ignore" => {
                    options.ignore = Some(parser::parse_prefix_codes(value.as_ref()));
                }
                "extend-ignore" | "extend_ignore" => {
                    options.extend_ignore = Some(parser::parse_prefix_codes(value.as_ref()));
                }
                "exclude" => {
                    options.exclude = Some(parser::parse_strings(value.as_ref()));
                }
                "extend-exclude" | "extend_exclude" => {
                    options.extend_exclude = Some(parser::parse_strings(value.as_ref()));
                }
                "per-file-ignores" | "per_file_ignores" => {
                    match parser::parse_files_to_codes_mapping(value.as_ref()) {
                        Ok(per_file_ignores) => {
                            options.per_file_ignores =
                                Some(parser::collect_per_file_ignores(per_file_ignores))
                        }
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                // flake8-quotes
                "quotes" | "inline-quotes" | "inline_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.inline_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.inline_quotes = Some(Quote::Single),
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                "multiline-quotes" | "multiline_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.multiline_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.multiline_quotes = Some(Quote::Single),
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                "docstring-quotes" | "docstring_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.docstring_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.docstring_quotes = Some(Quote::Single),
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                "avoid-escape" | "avoid_escape" => match value.trim() {
                    "true" => flake8_quotes.avoid_escape = Some(true),
                    "false" => flake8_quotes.avoid_escape = Some(false),
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                // pep8-naming
                "ignore-names" | "ignore_names" => {
                    pep8_naming.ignore_names = Some(parser::parse_strings(value.as_ref()));
                }
                "classmethod-decorators" | "classmethod_decorators" => {
                    pep8_naming.classmethod_decorators =
                        Some(parser::parse_strings(value.as_ref()));
                }
                "staticmethod-decorators" | "staticmethod_decorators" => {
                    pep8_naming.staticmethod_decorators =
                        Some(parser::parse_strings(value.as_ref()));
                }
                // Unknown
                _ => eprintln!("Skipping unsupported property: {key}"),
            }
        }
    }

    if flake8_quotes != Default::default() {
        options.flake8_quotes = Some(flake8_quotes);
    }
    if pep8_naming != Default::default() {
        options.pep8_naming = Some(pep8_naming);
    }

    // Create the pyproject.toml.
    Ok(Pyproject::new(options))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use anyhow::Result;
    use ruff::flake8_quotes;
    use ruff::settings::options::Options;
    use ruff::settings::pyproject::Pyproject;

    use crate::converter::convert;

    #[test]
    fn it_converts_empty() -> Result<()> {
        let actual = convert(HashMap::from([("flake8".to_string(), HashMap::from([]))]))?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: None,
            extend_select: None,
            ignore: None,
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_dashes() -> Result<()> {
        let actual = convert(HashMap::from([(
            "flake8".to_string(),
            HashMap::from([("max-line-length".to_string(), Some("100".to_string()))]),
        )]))?;
        let expected = Pyproject::new(Options {
            line_length: Some(100),
            exclude: None,
            extend_exclude: None,
            select: None,
            extend_select: None,
            ignore: None,
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_underscores() -> Result<()> {
        let actual = convert(HashMap::from([(
            "flake8".to_string(),
            HashMap::from([("max_line_length".to_string(), Some("100".to_string()))]),
        )]))?;
        let expected = Pyproject::new(Options {
            line_length: Some(100),
            exclude: None,
            extend_exclude: None,
            select: None,
            extend_select: None,
            ignore: None,
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_ignores_parse_errors() -> Result<()> {
        let actual = convert(HashMap::from([(
            "flake8".to_string(),
            HashMap::from([("max_line_length".to_string(), Some("abc".to_string()))]),
        )]))?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: None,
            extend_select: None,
            ignore: None,
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_extensions() -> Result<()> {
        let actual = convert(HashMap::from([(
            "flake8".to_string(),
            HashMap::from([("inline-quotes".to_string(), Some("single".to_string()))]),
        )]))?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: None,
            extend_select: None,
            ignore: None,
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_quotes: Some(flake8_quotes::settings::Options {
                inline_quotes: Some(flake8_quotes::settings::Quote::Single),
                multiline_quotes: None,
                docstring_quotes: None,
                avoid_escape: None,
            }),
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }
}

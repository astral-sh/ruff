use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use ruff::checks_gen::CheckCodePrefix;
use ruff::flake8_quotes::settings::Quote;
use ruff::settings::options::Options;
use ruff::settings::pyproject::Pyproject;
use ruff::{flake8_annotations, flake8_quotes, pep8_naming};

use crate::plugin::Plugin;
use crate::{parser, plugin};

pub fn convert(
    flake8: &HashMap<String, Option<String>>,
    plugins: Option<Vec<Plugin>>,
) -> Result<Pyproject> {
    // Extract all referenced check code prefixes, to power plugin inference.
    let mut referenced_codes: BTreeSet<CheckCodePrefix> = Default::default();
    for (key, value) in flake8 {
        if let Some(value) = value {
            match key.as_str() {
                "select" | "ignore" | "extend-select" | "extend_select" | "extend-ignore"
                | "extend_ignore" => {
                    referenced_codes.extend(parser::parse_prefix_codes(value.as_ref()));
                }
                "per-file-ignores" | "per_file_ignores" => {
                    if let Ok(per_file_ignores) =
                        parser::parse_files_to_codes_mapping(value.as_ref())
                    {
                        for (_, codes) in parser::collect_per_file_ignores(per_file_ignores) {
                            referenced_codes.extend(codes);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Check if the user has specified a `select`. If not, we'll add our own
    // default `select`, and populate it based on user plugins.
    let mut select = flake8
        .get("select")
        .and_then(|value| {
            value
                .as_ref()
                .map(|value| BTreeSet::from_iter(parser::parse_prefix_codes(value)))
        })
        .unwrap_or_else(|| {
            plugin::resolve_select(
                flake8,
                &plugins.unwrap_or_else(|| {
                    plugin::infer_plugins_from_options(flake8)
                        .into_iter()
                        .chain(plugin::infer_plugins_from_codes(&referenced_codes))
                        .collect()
                }),
            )
        });
    let mut ignore = flake8
        .get("ignore")
        .and_then(|value| {
            value
                .as_ref()
                .map(|value| BTreeSet::from_iter(parser::parse_prefix_codes(value)))
        })
        .unwrap_or_default();

    // Parse each supported option.
    let mut options: Options = Default::default();
    let mut flake8_annotations: flake8_annotations::settings::Options = Default::default();
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
                    // No-op (handled above).
                    select.extend(parser::parse_prefix_codes(value.as_ref()));
                }
                "ignore" => {
                    // No-op (handled above).
                }
                "extend-select" | "extend_select" => {
                    // Unlike Flake8, use a single explicit `select`.
                    select.extend(parser::parse_prefix_codes(value.as_ref()));
                }
                "extend-ignore" | "extend_ignore" => {
                    // Unlike Flake8, use a single explicit `ignore`.
                    ignore.extend(parser::parse_prefix_codes(value.as_ref()));
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
                // flake8-annotations
                "suppress-none-returning" | "suppress_none_returning" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.suppress_none_returning = Some(bool),
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                "suppress-dummy-args" | "suppress_dummy_args" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.suppress_dummy_args = Some(bool),
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                "mypy-init-return" | "mypy_init_return" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.mypy_init_return = Some(bool),
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
                "avoid-escape" | "avoid_escape" => match parser::parse_bool(value.as_ref()) {
                    Ok(bool) => flake8_quotes.avoid_escape = Some(bool),
                    Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
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
                // flake8-docstrings
                "docstring-convention" => {
                    // No-op (handled above).
                }
                // Unknown
                _ => eprintln!("Skipping unsupported property: {key}"),
            }
        }
    }

    // Deduplicate and sort.
    options.select = Some(Vec::from_iter(select));
    options.ignore = Some(Vec::from_iter(ignore));
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
    use ruff::checks_gen::CheckCodePrefix;
    use ruff::flake8_quotes;
    use ruff::settings::options::Options;
    use ruff::settings::pyproject::Pyproject;

    use crate::converter::convert;
    use crate::plugin::Plugin;

    #[test]
    fn it_converts_empty() -> Result<()> {
        let actual = convert(&HashMap::from([]), None)?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_dashes() -> Result<()> {
        let actual = convert(
            &HashMap::from([("max-line-length".to_string(), Some("100".to_string()))]),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: Some(100),
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_underscores() -> Result<()> {
        let actual = convert(
            &HashMap::from([("max_line_length".to_string(), Some("100".to_string()))]),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: Some(100),
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_ignores_parse_errors() -> Result<()> {
        let actual = convert(
            &HashMap::from([("max_line_length".to_string(), Some("abc".to_string()))]),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_plugin_options() -> Result<()> {
        let actual = convert(
            &HashMap::from([("inline-quotes".to_string(), Some("single".to_string()))]),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
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

    #[test]
    fn it_converts_docstring_conventions() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "docstring-convention".to_string(),
                Some("numpy".to_string()),
            )]),
            Some(vec![Plugin::Flake8Docstrings]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::D100,
                CheckCodePrefix::D101,
                CheckCodePrefix::D102,
                CheckCodePrefix::D103,
                CheckCodePrefix::D104,
                CheckCodePrefix::D105,
                CheckCodePrefix::D106,
                CheckCodePrefix::D200,
                CheckCodePrefix::D201,
                CheckCodePrefix::D202,
                CheckCodePrefix::D204,
                CheckCodePrefix::D205,
                CheckCodePrefix::D206,
                CheckCodePrefix::D207,
                CheckCodePrefix::D208,
                CheckCodePrefix::D209,
                CheckCodePrefix::D210,
                CheckCodePrefix::D211,
                CheckCodePrefix::D214,
                CheckCodePrefix::D215,
                CheckCodePrefix::D300,
                CheckCodePrefix::D400,
                CheckCodePrefix::D403,
                CheckCodePrefix::D404,
                CheckCodePrefix::D405,
                CheckCodePrefix::D406,
                CheckCodePrefix::D407,
                CheckCodePrefix::D408,
                CheckCodePrefix::D409,
                CheckCodePrefix::D410,
                CheckCodePrefix::D411,
                CheckCodePrefix::D412,
                CheckCodePrefix::D414,
                CheckCodePrefix::D418,
                CheckCodePrefix::D419,
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
            flake8_quotes: None,
            pep8_naming: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_infers_plugins_if_omitted() -> Result<()> {
        let actual = convert(
            &HashMap::from([("inline-quotes".to_string(), Some("single".to_string()))]),
            None,
        )?;
        let expected = Pyproject::new(Options {
            line_length: None,
            exclude: None,
            extend_exclude: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::Q,
                CheckCodePrefix::W,
            ]),
            extend_select: None,
            ignore: Some(vec![]),
            extend_ignore: None,
            per_file_ignores: None,
            dummy_variable_rgx: None,
            target_version: None,
            flake8_annotations: None,
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

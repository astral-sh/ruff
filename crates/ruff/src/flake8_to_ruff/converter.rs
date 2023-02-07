use std::collections::{HashMap, HashSet};

use anyhow::Result;
use itertools::Itertools;

use super::external_config::ExternalConfig;
use super::plugin::Plugin;
use super::{parser, plugin};
use crate::registry::RuleCodePrefix;
use crate::rule_selector::{prefix_to_selector, RuleSelector};
use crate::rules::flake8_pytest_style::types::{
    ParametrizeNameType, ParametrizeValuesRowType, ParametrizeValuesType,
};
use crate::rules::flake8_quotes::settings::Quote;
use crate::rules::flake8_tidy_imports::relative_imports::Strictness;
use crate::rules::pydocstyle::settings::Convention;
use crate::rules::{
    flake8_annotations, flake8_bugbear, flake8_builtins, flake8_errmsg, flake8_pytest_style,
    flake8_quotes, flake8_tidy_imports, mccabe, pep8_naming, pydocstyle,
};
use crate::settings::options::Options;
use crate::settings::pyproject::Pyproject;
use crate::warn_user;

const DEFAULT_SELECTORS: &[RuleSelector] = &[
    prefix_to_selector(RuleCodePrefix::F),
    prefix_to_selector(RuleCodePrefix::E),
    prefix_to_selector(RuleCodePrefix::W),
];

pub fn convert(
    config: &HashMap<String, HashMap<String, Option<String>>>,
    external_config: &ExternalConfig,
    plugins: Option<Vec<Plugin>>,
) -> Result<Pyproject> {
    // Extract the Flake8 section.
    let flake8 = config
        .get("flake8")
        .expect("Unable to find flake8 section in INI file");

    // Extract all referenced rule code prefixes, to power plugin inference.
    let mut referenced_codes: HashSet<RuleSelector> = HashSet::default();
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

    // Infer plugins, if not provided.
    let plugins = plugins.unwrap_or_else(|| {
        let from_options = plugin::infer_plugins_from_options(flake8);
        if !from_options.is_empty() {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Inferred plugins from settings: {from_options:#?}");
            }
        }
        let from_codes = plugin::infer_plugins_from_codes(&referenced_codes);
        if !from_codes.is_empty() {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Inferred plugins from referenced codes: {from_codes:#?}");
            }
        }
        from_options.into_iter().chain(from_codes).collect()
    });

    // Check if the user has specified a `select`. If not, we'll add our own
    // default `select`, and populate it based on user plugins.
    let mut select = flake8
        .get("select")
        .and_then(|value| {
            value
                .as_ref()
                .map(|value| HashSet::from_iter(parser::parse_prefix_codes(value)))
        })
        .unwrap_or_else(|| resolve_select(&plugins));
    let mut ignore: HashSet<RuleSelector> = flake8
        .get("ignore")
        .and_then(|value| {
            value
                .as_ref()
                .map(|value| HashSet::from_iter(parser::parse_prefix_codes(value)))
        })
        .unwrap_or_default();

    // Parse each supported option.
    let mut options = Options::default();
    let mut flake8_annotations = flake8_annotations::settings::Options::default();
    let mut flake8_bugbear = flake8_bugbear::settings::Options::default();
    let mut flake8_builtins = flake8_builtins::settings::Options::default();
    let mut flake8_errmsg = flake8_errmsg::settings::Options::default();
    let mut flake8_pytest_style = flake8_pytest_style::settings::Options::default();
    let mut flake8_quotes = flake8_quotes::settings::Options::default();
    let mut flake8_tidy_imports = flake8_tidy_imports::options::Options::default();
    let mut mccabe = mccabe::settings::Options::default();
    let mut pep8_naming = pep8_naming::settings::Options::default();
    let mut pydocstyle = pydocstyle::settings::Options::default();
    for (key, value) in flake8 {
        if let Some(value) = value {
            match key.as_str() {
                // flake8
                "builtins" => {
                    options.builtins = Some(parser::parse_strings(value.as_ref()));
                }
                "max-line-length" | "max_line_length" => match value.parse::<usize>() {
                    Ok(line_length) => options.line_length = Some(line_length),
                    Err(e) => {
                        warn_user!("Unable to parse '{key}' property: {e}");
                    }
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
                                Some(parser::collect_per_file_ignores(per_file_ignores));
                        }
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                // flake8-bugbear
                "extend-immutable-calls" | "extend_immutable_calls" => {
                    flake8_bugbear.extend_immutable_calls =
                        Some(parser::parse_strings(value.as_ref()));
                }
                // flake8-builtins
                "builtins-ignorelist" | "builtins_ignorelist" => {
                    flake8_builtins.builtins_ignorelist =
                        Some(parser::parse_strings(value.as_ref()));
                }
                // flake8-annotations
                "suppress-none-returning" | "suppress_none_returning" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.suppress_none_returning = Some(bool),
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                "suppress-dummy-args" | "suppress_dummy_args" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.suppress_dummy_args = Some(bool),
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                "mypy-init-return" | "mypy_init_return" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.mypy_init_return = Some(bool),
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                "allow-star-arg-any" | "allow_star_arg_any" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.allow_star_arg_any = Some(bool),
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                // flake8-quotes
                "quotes" | "inline-quotes" | "inline_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.inline_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.inline_quotes = Some(Quote::Double),
                    _ => {
                        warn_user!("Unexpected '{key}' value: {value}");
                    }
                },
                "multiline-quotes" | "multiline_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.multiline_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.multiline_quotes = Some(Quote::Double),
                    _ => {
                        warn_user!("Unexpected '{key}' value: {value}");
                    }
                },
                "docstring-quotes" | "docstring_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.docstring_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.docstring_quotes = Some(Quote::Double),
                    _ => {
                        warn_user!("Unexpected '{key}' value: {value}");
                    }
                },
                "avoid-escape" | "avoid_escape" => match parser::parse_bool(value.as_ref()) {
                    Ok(bool) => flake8_quotes.avoid_escape = Some(bool),
                    Err(e) => {
                        warn_user!("Unable to parse '{key}' property: {e}");
                    }
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
                // flake8-tidy-imports
                "ban-relative-imports" | "ban_relative_imports" => match value.trim() {
                    "true" => flake8_tidy_imports.ban_relative_imports = Some(Strictness::All),
                    "parents" => {
                        flake8_tidy_imports.ban_relative_imports = Some(Strictness::Parents);
                    }
                    _ => {
                        warn_user!("Unexpected '{key}' value: {value}");
                    }
                },
                // flake8-docstrings
                "docstring-convention" => match value.trim() {
                    "google" => pydocstyle.convention = Some(Convention::Google),
                    "numpy" => pydocstyle.convention = Some(Convention::Numpy),
                    "pep257" => pydocstyle.convention = Some(Convention::Pep257),
                    "all" => pydocstyle.convention = None,
                    _ => {
                        warn_user!("Unexpected '{key}' value: {value}");
                    }
                },
                // mccabe
                "max-complexity" | "max_complexity" => match value.parse::<usize>() {
                    Ok(max_complexity) => mccabe.max_complexity = Some(max_complexity),
                    Err(e) => {
                        warn_user!("Unable to parse '{key}' property: {e}");
                    }
                },
                // flake8-errmsg
                "errmsg-max-string-length" | "errmsg_max_string_length" => {
                    match value.parse::<usize>() {
                        Ok(max_string_length) => {
                            flake8_errmsg.max_string_length = Some(max_string_length);
                        }
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                // flake8-pytest-style
                "pytest-fixture-no-parentheses" | "pytest_fixture_no_parentheses " => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_pytest_style.fixture_parentheses = Some(!bool),
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                "pytest-parametrize-names-type" | "pytest_parametrize_names_type" => {
                    match value.trim() {
                        "csv" => {
                            flake8_pytest_style.parametrize_names_type =
                                Some(ParametrizeNameType::Csv);
                        }
                        "tuple" => {
                            flake8_pytest_style.parametrize_names_type =
                                Some(ParametrizeNameType::Tuple);
                        }
                        "list" => {
                            flake8_pytest_style.parametrize_names_type =
                                Some(ParametrizeNameType::List);
                        }
                        _ => {
                            warn_user!("Unexpected '{key}' value: {value}");
                        }
                    }
                }
                "pytest-parametrize-values-type" | "pytest_parametrize_values_type" => {
                    match value.trim() {
                        "tuple" => {
                            flake8_pytest_style.parametrize_values_type =
                                Some(ParametrizeValuesType::Tuple);
                        }
                        "list" => {
                            flake8_pytest_style.parametrize_values_type =
                                Some(ParametrizeValuesType::List);
                        }
                        _ => {
                            warn_user!("Unexpected '{key}' value: {value}");
                        }
                    }
                }
                "pytest-parametrize-values-row-type" | "pytest_parametrize_values_row_type" => {
                    match value.trim() {
                        "tuple" => {
                            flake8_pytest_style.parametrize_values_row_type =
                                Some(ParametrizeValuesRowType::Tuple);
                        }
                        "list" => {
                            flake8_pytest_style.parametrize_values_row_type =
                                Some(ParametrizeValuesRowType::List);
                        }
                        _ => {
                            warn_user!("Unexpected '{key}' value: {value}");
                        }
                    }
                }
                "pytest-raises-require-match-for" | "pytest_raises_require_match_for" => {
                    flake8_pytest_style.raises_require_match_for =
                        Some(parser::parse_strings(value.as_ref()));
                }
                "pytest-mark-no-parentheses" | "pytest_mark_no_parentheses" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_pytest_style.mark_parentheses = Some(!bool),
                        Err(e) => {
                            warn_user!("Unable to parse '{key}' property: {e}");
                        }
                    }
                }
                // Unknown
                _ => {
                    warn_user!("Skipping unsupported property: {}", key);
                }
            }
        }
    }

    // Deduplicate and sort.
    options.select = Some(
        select
            .into_iter()
            .sorted_by_key(RuleSelector::short_code)
            .collect(),
    );
    options.ignore = Some(
        ignore
            .into_iter()
            .sorted_by_key(RuleSelector::short_code)
            .collect(),
    );
    if flake8_annotations != flake8_annotations::settings::Options::default() {
        options.flake8_annotations = Some(flake8_annotations);
    }
    if flake8_bugbear != flake8_bugbear::settings::Options::default() {
        options.flake8_bugbear = Some(flake8_bugbear);
    }
    if flake8_builtins != flake8_builtins::settings::Options::default() {
        options.flake8_builtins = Some(flake8_builtins);
    }
    if flake8_errmsg != flake8_errmsg::settings::Options::default() {
        options.flake8_errmsg = Some(flake8_errmsg);
    }
    if flake8_pytest_style != flake8_pytest_style::settings::Options::default() {
        options.flake8_pytest_style = Some(flake8_pytest_style);
    }
    if flake8_quotes != flake8_quotes::settings::Options::default() {
        options.flake8_quotes = Some(flake8_quotes);
    }
    if flake8_tidy_imports != flake8_tidy_imports::options::Options::default() {
        options.flake8_tidy_imports = Some(flake8_tidy_imports);
    }
    if mccabe != mccabe::settings::Options::default() {
        options.mccabe = Some(mccabe);
    }
    if pep8_naming != pep8_naming::settings::Options::default() {
        options.pep8_naming = Some(pep8_naming);
    }
    if pydocstyle != pydocstyle::settings::Options::default() {
        options.pydocstyle = Some(pydocstyle);
    }

    // Extract any settings from the existing `pyproject.toml`.
    if let Some(black) = &external_config.black {
        if let Some(line_length) = &black.line_length {
            options.line_length = Some(*line_length);
        }

        if let Some(target_version) = &black.target_version {
            if let Some(target_version) = target_version.iter().min() {
                options.target_version = Some(*target_version);
            }
        }
    }

    if let Some(isort) = &external_config.isort {
        if let Some(src_paths) = &isort.src_paths {
            match options.src.as_mut() {
                Some(src) => {
                    src.extend(src_paths.clone());
                }
                None => {
                    options.src = Some(src_paths.clone());
                }
            }
        }
    }

    // Create the pyproject.toml.
    Ok(Pyproject::new(options))
}

/// Resolve the set of enabled `RuleSelector` values for the given
/// plugins.
fn resolve_select(plugins: &[Plugin]) -> HashSet<RuleSelector> {
    let mut select: HashSet<_> = DEFAULT_SELECTORS.iter().cloned().collect();
    select.extend(plugins.iter().map(Plugin::selector));
    select
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use anyhow::Result;
    use itertools::Itertools;

    use super::super::plugin::Plugin;
    use super::convert;
    use crate::flake8_to_ruff::converter::DEFAULT_SELECTORS;
    use crate::flake8_to_ruff::ExternalConfig;
    use crate::registry::RuleCodePrefix;
    use crate::rule_selector::RuleSelector;
    use crate::rules::pydocstyle::settings::Convention;
    use crate::rules::{flake8_quotes, pydocstyle};
    use crate::settings::options::Options;
    use crate::settings::pyproject::Pyproject;

    fn default_options(plugins: impl IntoIterator<Item = RuleSelector>) -> Options {
        Options {
            ignore: Some(vec![]),
            select: Some(
                DEFAULT_SELECTORS
                    .iter()
                    .cloned()
                    .chain(plugins)
                    .sorted_by_key(RuleSelector::short_code)
                    .collect(),
            ),
            ..Options::default()
        }
    }

    #[test]
    fn it_converts_empty() -> Result<()> {
        let actual = convert(
            &HashMap::from([("flake8".to_string(), HashMap::default())]),
            &ExternalConfig::default(),
            None,
        )?;
        let expected = Pyproject::new(default_options([]));
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_dashes() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "flake8".to_string(),
                HashMap::from([("max-line-length".to_string(), Some("100".to_string()))]),
            )]),
            &ExternalConfig::default(),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: Some(100),
            ..default_options([])
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_underscores() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "flake8".to_string(),
                HashMap::from([("max_line_length".to_string(), Some("100".to_string()))]),
            )]),
            &ExternalConfig::default(),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            line_length: Some(100),
            ..default_options([])
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_ignores_parse_errors() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "flake8".to_string(),
                HashMap::from([("max_line_length".to_string(), Some("abc".to_string()))]),
            )]),
            &ExternalConfig::default(),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(default_options([]));
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_plugin_options() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "flake8".to_string(),
                HashMap::from([("inline-quotes".to_string(), Some("single".to_string()))]),
            )]),
            &ExternalConfig::default(),
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            flake8_quotes: Some(flake8_quotes::settings::Options {
                inline_quotes: Some(flake8_quotes::settings::Quote::Single),
                multiline_quotes: None,
                docstring_quotes: None,
                avoid_escape: None,
            }),
            ..default_options([])
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_converts_docstring_conventions() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "flake8".to_string(),
                HashMap::from([(
                    "docstring-convention".to_string(),
                    Some("numpy".to_string()),
                )]),
            )]),
            &ExternalConfig::default(),
            Some(vec![Plugin::Flake8Docstrings]),
        )?;
        let expected = Pyproject::new(Options {
            pydocstyle: Some(pydocstyle::settings::Options {
                convention: Some(Convention::Numpy),
            }),
            ..default_options([RuleCodePrefix::D.into()])
        });
        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn it_infers_plugins_if_omitted() -> Result<()> {
        let actual = convert(
            &HashMap::from([(
                "flake8".to_string(),
                HashMap::from([("inline-quotes".to_string(), Some("single".to_string()))]),
            )]),
            &ExternalConfig::default(),
            None,
        )?;
        let expected = Pyproject::new(Options {
            flake8_quotes: Some(flake8_quotes::settings::Options {
                inline_quotes: Some(flake8_quotes::settings::Quote::Single),
                multiline_quotes: None,
                docstring_quotes: None,
                avoid_escape: None,
            }),
            ..default_options([RuleCodePrefix::Q.into()])
        });
        assert_eq!(actual, expected);

        Ok(())
    }
}

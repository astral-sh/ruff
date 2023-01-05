use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use ruff::flake8_pytest_style::types::{
    ParametrizeNameType, ParametrizeValuesRowType, ParametrizeValuesType,
};
use ruff::flake8_quotes::settings::Quote;
use ruff::flake8_tidy_imports::settings::Strictness;
use ruff::pydocstyle::settings::Convention;
use ruff::registry_gen::CheckCodePrefix;
use ruff::settings::options::Options;
use ruff::settings::pyproject::Pyproject;
use ruff::{
    flake8_annotations, flake8_bugbear, flake8_errmsg, flake8_pytest_style, flake8_quotes,
    flake8_tidy_imports, mccabe, pep8_naming, pydocstyle,
};

use crate::black::Black;
use crate::plugin::Plugin;
use crate::{parser, plugin};

pub fn convert(
    config: &HashMap<String, HashMap<String, Option<String>>>,
    black: Option<&Black>,
    plugins: Option<Vec<Plugin>>,
) -> Result<Pyproject> {
    // Extract the Flake8 section.
    let flake8 = config
        .get("flake8")
        .expect("Unable to find flake8 section in INI file");

    // Extract all referenced check code prefixes, to power plugin inference.
    let mut referenced_codes: BTreeSet<CheckCodePrefix> = BTreeSet::default();
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
            eprintln!("Inferred plugins from settings: {from_options:#?}");
        }
        let from_codes = plugin::infer_plugins_from_codes(&referenced_codes);
        if !from_codes.is_empty() {
            eprintln!("Inferred plugins from referenced check codes: {from_codes:#?}");
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
                .map(|value| BTreeSet::from_iter(parser::parse_prefix_codes(value)))
        })
        .unwrap_or_else(|| plugin::resolve_select(&plugins));
    let mut ignore = flake8
        .get("ignore")
        .and_then(|value| {
            value
                .as_ref()
                .map(|value| BTreeSet::from_iter(parser::parse_prefix_codes(value)))
        })
        .unwrap_or_default();

    // Parse each supported option.
    let mut options = Options::default();
    let mut flake8_annotations = flake8_annotations::settings::Options::default();
    let mut flake8_bugbear = flake8_bugbear::settings::Options::default();
    let mut flake8_errmsg = flake8_errmsg::settings::Options::default();
    let mut flake8_pytest_style = flake8_pytest_style::settings::Options::default();
    let mut flake8_quotes = flake8_quotes::settings::Options::default();
    let mut flake8_tidy_imports = flake8_tidy_imports::settings::Options::default();
    let mut mccabe = mccabe::settings::Options::default();
    let mut pep8_naming = pep8_naming::settings::Options::default();
    let mut pydocstyle = pydocstyle::settings::Options::default();
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
                                Some(parser::collect_per_file_ignores(per_file_ignores));
                        }
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                // flake8-bugbear
                "extend-immutable-calls" | "extend_immutable_calls" => {
                    flake8_bugbear.extend_immutable_calls =
                        Some(parser::parse_strings(value.as_ref()));
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
                "allow-star-arg-any" | "allow_star_arg_any" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_annotations.allow_star_arg_any = Some(bool),
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                // flake8-quotes
                "quotes" | "inline-quotes" | "inline_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.inline_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.inline_quotes = Some(Quote::Double),
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                "multiline-quotes" | "multiline_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.multiline_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.multiline_quotes = Some(Quote::Double),
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                "docstring-quotes" | "docstring_quotes" => match value.trim() {
                    "'" | "single" => flake8_quotes.docstring_quotes = Some(Quote::Single),
                    "\"" | "double" => flake8_quotes.docstring_quotes = Some(Quote::Double),
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
                // flake8-tidy-imports
                "ban-relative-imports" | "ban_relative_imports" => match value.trim() {
                    "true" => flake8_tidy_imports.ban_relative_imports = Some(Strictness::All),
                    "parents" => {
                        flake8_tidy_imports.ban_relative_imports = Some(Strictness::Parents);
                    }
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                // flake8-docstrings
                "docstring-convention" => match value.trim() {
                    "google" => pydocstyle.convention = Some(Convention::Google),
                    "numpy" => pydocstyle.convention = Some(Convention::Numpy),
                    "pep257" => pydocstyle.convention = Some(Convention::Pep257),
                    "all" => pydocstyle.convention = None,
                    _ => eprintln!("Unexpected '{key}' value: {value}"),
                },
                // mccabe
                "max-complexity" | "max_complexity" => match value.clone().parse::<usize>() {
                    Ok(max_complexity) => mccabe.max_complexity = Some(max_complexity),
                    Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                },
                // flake8-errmsg
                "errmsg-max-string-length" | "errmsg_max_string_length" => {
                    match value.clone().parse::<usize>() {
                        Ok(max_string_length) => {
                            flake8_errmsg.max_string_length = Some(max_string_length);
                        }
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                // flake8-pytest-style
                "pytest-fixture-no-parentheses" | "pytest_fixture_no_parentheses " => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_pytest_style.fixture_parentheses = Some(!bool),
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                "pytest-parametrize-names-type" | "pytest_parametrize_names_type" => {
                    match value.trim() {
                        "csv" => {
                            flake8_pytest_style.parametrize_names_type =
                                Some(ParametrizeNameType::CSV);
                        }
                        "tuple" => {
                            flake8_pytest_style.parametrize_names_type =
                                Some(ParametrizeNameType::Tuple);
                        }
                        "list" => {
                            flake8_pytest_style.parametrize_names_type =
                                Some(ParametrizeNameType::List);
                        }
                        _ => eprintln!("Unexpected '{key}' value: {value}"),
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
                        _ => eprintln!("Unexpected '{key}' value: {value}"),
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
                        _ => eprintln!("Unexpected '{key}' value: {value}"),
                    }
                }
                "pytest-raises-require-match-for" | "pytest_raises_require_match_for" => {
                    flake8_pytest_style.raises_require_match_for =
                        Some(parser::parse_strings(value.as_ref()));
                }
                "pytest-mark-no-parentheses" | "pytest_mark_no_parentheses" => {
                    match parser::parse_bool(value.as_ref()) {
                        Ok(bool) => flake8_pytest_style.mark_parentheses = Some(!bool),
                        Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                    }
                }
                // Unknown
                _ => eprintln!("Skipping unsupported property: {key}"),
            }
        }
    }

    // Deduplicate and sort.
    options.select = Some(Vec::from_iter(select));
    options.ignore = Some(Vec::from_iter(ignore));
    if flake8_annotations != flake8_annotations::settings::Options::default() {
        options.flake8_annotations = Some(flake8_annotations);
    }
    if flake8_bugbear != flake8_bugbear::settings::Options::default() {
        options.flake8_bugbear = Some(flake8_bugbear);
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
    if flake8_tidy_imports != flake8_tidy_imports::settings::Options::default() {
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
    if let Some(black) = black {
        if let Some(line_length) = &black.line_length {
            options.line_length = Some(*line_length);
        }

        if let Some(target_version) = &black.target_version {
            if let Some(target_version) = target_version.iter().min() {
                options.target_version = Some(*target_version);
            }
        }
    }

    // Create the pyproject.toml.
    Ok(Pyproject::new(options))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use anyhow::Result;
    use ruff::pydocstyle::settings::Convention;
    use ruff::registry_gen::CheckCodePrefix;
    use ruff::settings::options::Options;
    use ruff::settings::pyproject::Pyproject;
    use ruff::{flake8_quotes, pydocstyle};

    use crate::converter::convert;
    use crate::plugin::Plugin;

    #[test]
    fn it_converts_empty() -> Result<()> {
        let actual = convert(
            &HashMap::from([("flake8".to_string(), HashMap::default())]),
            None,
            None,
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: None,
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: None,
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: None,
            pyupgrade: None,
        });
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
            None,
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: Some(100),
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: None,
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: None,
            pyupgrade: None,
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
            None,
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: Some(100),
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: None,
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: None,
            pyupgrade: None,
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
            None,
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: None,
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: None,
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: None,
            pyupgrade: None,
        });
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
            None,
            Some(vec![]),
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: None,
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: Some(flake8_quotes::settings::Options {
                inline_quotes: Some(flake8_quotes::settings::Quote::Single),
                multiline_quotes: None,
                docstring_quotes: None,
                avoid_escape: None,
            }),
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: None,
            pyupgrade: None,
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
            None,
            Some(vec![Plugin::Flake8Docstrings]),
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: None,
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::D,
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: None,
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: Some(pydocstyle::settings::Options {
                convention: Some(Convention::Numpy),
            }),
            pyupgrade: None,
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
            None,
            None,
        )?;
        let expected = Pyproject::new(Options {
            allowed_confusables: None,
            cache_dir: None,
            dummy_variable_rgx: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_ignore: None,
            extend_select: None,
            external: None,
            fix: None,
            fix_only: None,
            fixable: None,
            format: None,
            force_exclude: None,
            ignore: Some(vec![]),
            ignore_init_module_imports: None,
            line_length: None,
            per_file_ignores: None,
            required_version: None,
            respect_gitignore: None,
            select: Some(vec![
                CheckCodePrefix::E,
                CheckCodePrefix::F,
                CheckCodePrefix::Q,
                CheckCodePrefix::W,
            ]),
            show_source: None,
            src: None,
            target_version: None,
            unfixable: None,
            update_check: None,
            flake8_annotations: None,
            flake8_bandit: None,
            flake8_bugbear: None,
            flake8_errmsg: None,
            flake8_pytest_style: None,
            flake8_quotes: Some(flake8_quotes::settings::Options {
                inline_quotes: Some(flake8_quotes::settings::Quote::Single),
                multiline_quotes: None,
                docstring_quotes: None,
                avoid_escape: None,
            }),
            flake8_tidy_imports: None,
            flake8_import_conventions: None,
            flake8_unused_arguments: None,
            isort: None,
            mccabe: None,
            pep8_naming: None,
            pydocstyle: None,
            pyupgrade: None,
        });
        assert_eq!(actual, expected);

        Ok(())
    }
}

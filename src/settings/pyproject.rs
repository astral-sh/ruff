//! Utilities for locating (and extracting configuration from) a pyproject.toml.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::fs;
use crate::settings::options::Options;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Tools {
    ruff: Option<Options>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pyproject {
    tool: Option<Tools>,
}

impl Pyproject {
    pub fn new(options: Options) -> Self {
        Self {
            tool: Some(Tools {
                ruff: Some(options),
            }),
        }
    }
}

/// Parse a `ruff.toml` file.
fn parse_ruff_toml<P: AsRef<Path>>(path: P) -> Result<Options> {
    let contents = fs::read_file(path)?;
    toml::from_str(&contents).map_err(std::convert::Into::into)
}

/// Parse a `pyproject.toml` file.
fn parse_pyproject_toml<P: AsRef<Path>>(path: P) -> Result<Pyproject> {
    let contents = fs::read_file(path)?;
    toml::from_str(&contents).map_err(std::convert::Into::into)
}

/// Return `true` if a `pyproject.toml` contains a `[tool.ruff]` section.
pub fn ruff_enabled<P: AsRef<Path>>(path: P) -> Result<bool> {
    let pyproject = parse_pyproject_toml(path)?;
    Ok(pyproject.tool.and_then(|tool| tool.ruff).is_some())
}

/// Return the path to the `pyproject.toml` or `ruff.toml` file in a given
/// directory.
pub fn settings_toml<P: AsRef<Path>>(path: P) -> Result<Option<PathBuf>> {
    // Check for `ruff.toml`.
    let ruff_toml = path.as_ref().join("ruff.toml");
    if ruff_toml.is_file() {
        return Ok(Some(ruff_toml));
    }

    // Check for `pyproject.toml`.
    let pyproject_toml = path.as_ref().join("pyproject.toml");
    if pyproject_toml.is_file() && ruff_enabled(&pyproject_toml)? {
        return Ok(Some(pyproject_toml));
    }

    Ok(None)
}

/// Find the path to the `pyproject.toml` or `ruff.toml` file, if such a file
/// exists.
pub fn find_settings_toml<P: AsRef<Path>>(path: P) -> Result<Option<PathBuf>> {
    for directory in path.as_ref().ancestors() {
        if let Some(pyproject) = settings_toml(directory)? {
            return Ok(Some(pyproject));
        }
    }
    Ok(None)
}

/// Find the path to the user-specific `pyproject.toml` or `ruff.toml`, if it
/// exists.
pub fn find_user_settings_toml() -> Option<PathBuf> {
    // Search for a user-specific `ruff.toml`.
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push("ruff.toml");
    if path.is_file() {
        return Some(path);
    }

    // Search for a user-specific `pyproject.toml`.
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push("pyproject.toml");
    if path.is_file() {
        return Some(path);
    }

    None
}

/// Load `Options` from a `pyproject.toml` or `ruff.toml` file.
pub fn load_options<P: AsRef<Path>>(path: P) -> Result<Options> {
    if path.as_ref().ends_with("ruff.toml") {
        parse_ruff_toml(path)
    } else if path.as_ref().ends_with("pyproject.toml") {
        let pyproject = parse_pyproject_toml(&path).map_err(|err| {
            anyhow!(
                "Failed to parse `{}`: {}",
                path.as_ref().to_string_lossy(),
                err
            )
        })?;
        Ok(pyproject
            .tool
            .and_then(|tool| tool.ruff)
            .unwrap_or_default())
    } else {
        Err(anyhow!(
            "Unrecognized settings file: `{}`",
            path.as_ref().to_string_lossy()
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::str::FromStr;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::flake8_quotes::settings::Quote;
    use crate::flake8_tidy_imports::settings::{BannedApi, Strictness};
    use crate::registry_gen::CheckCodePrefix;
    use crate::settings::pyproject::{
        find_settings_toml, parse_pyproject_toml, Options, Pyproject, Tools,
    };
    use crate::settings::types::PatternPrefixPair;
    use crate::{
        flake8_bugbear, flake8_errmsg, flake8_import_conventions, flake8_pytest_style,
        flake8_quotes, flake8_tidy_imports, mccabe, pep8_naming,
    };

    #[test]
    fn deserialize() -> Result<()> {
        let pyproject: Pyproject = toml::from_str(r#""#)?;
        assert_eq!(pyproject.tool, None);

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
"#,
        )?;
        assert_eq!(pyproject.tool, Some(Tools { ruff: None }));

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
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
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    required_version: None,
                    respect_gitignore: None,
                    select: None,
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
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
line-length = 79
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    allowed_confusables: None,
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
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: Some(79),
                    per_file_ignores: None,
                    respect_gitignore: None,
                    required_version: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    update_check: None,
                    cache_dir: None,
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
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
exclude = ["foo.py"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    allowed_confusables: None,
                    cache_dir: None,
                    dummy_variable_rgx: None,
                    exclude: Some(vec!["foo.py".to_string()]),
                    extend: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    external: None,
                    fix: None,
                    fix_only: None,
                    fixable: None,
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    required_version: None,
                    respect_gitignore: None,
                    select: None,
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
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
select = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
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
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    required_version: None,
                    respect_gitignore: None,
                    select: Some(vec![CheckCodePrefix::E501]),
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
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
extend-select = ["RUF100"]
ignore = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    allowed_confusables: None,
                    cache_dir: None,
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: Some(vec![CheckCodePrefix::RUF100]),
                    external: None,
                    fix: None,
                    fix_only: None,
                    fixable: None,
                    force_exclude: None,
                    format: None,
                    ignore: Some(vec![CheckCodePrefix::E501]),
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    required_version: None,
                    respect_gitignore: None,
                    select: None,
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
                })
            })
        );

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff]
line_length = 79
"#,
        )
        .is_err());

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff]
select = ["E123"]
"#,
        )
        .is_err());

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff]
line-length = 79
other-attribute = 1
"#,
        )
        .is_err());

        Ok(())
    }

    #[test]
    fn find_and_parse_pyproject_toml() -> Result<()> {
        let cwd = current_dir()?;
        let pyproject =
            find_settings_toml(cwd.join("resources/test/fixtures/__init__.py"))?.unwrap();
        assert_eq!(
            pyproject,
            cwd.join("resources/test/fixtures/pyproject.toml")
        );

        let pyproject = parse_pyproject_toml(&pyproject)?;
        let config = pyproject.tool.unwrap().ruff.unwrap();
        assert_eq!(
            config,
            Options {
                allowed_confusables: Some(vec!['−', 'ρ', '∗']),
                line_length: Some(88),
                fix: None,
                fix_only: None,
                exclude: None,
                extend: None,
                extend_exclude: Some(vec![
                    "excluded_file.py".to_string(),
                    "migrations".to_string(),
                    "with_excluded_file/other_excluded_file.py".to_string(),
                ]),
                select: None,
                extend_select: None,
                external: Some(vec!["V101".to_string()]),
                ignore: None,
                ignore_init_module_imports: None,
                extend_ignore: None,
                fixable: None,
                format: None,
                force_exclude: None,
                unfixable: None,
                update_check: None,
                cache_dir: None,
                per_file_ignores: Some(FxHashMap::from_iter([(
                    "__init__.py".to_string(),
                    vec![CheckCodePrefix::F401]
                )])),
                dummy_variable_rgx: None,
                respect_gitignore: None,
                required_version: None,
                src: None,
                target_version: None,
                show_source: None,
                flake8_annotations: None,
                flake8_bandit: None,
                flake8_bugbear: Some(flake8_bugbear::settings::Options {
                    extend_immutable_calls: Some(vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                    ]),
                }),
                flake8_errmsg: Some(flake8_errmsg::settings::Options {
                    max_string_length: Some(20),
                }),
                flake8_pytest_style: Some(flake8_pytest_style::settings::Options {
                    fixture_parentheses: Some(false),
                    parametrize_names_type: Some(
                        flake8_pytest_style::types::ParametrizeNameType::CSV
                    ),
                    parametrize_values_type: Some(
                        flake8_pytest_style::types::ParametrizeValuesType::Tuple,
                    ),
                    parametrize_values_row_type: Some(
                        flake8_pytest_style::types::ParametrizeValuesRowType::List,
                    ),
                    raises_require_match_for: Some(vec![
                        "Exception".to_string(),
                        "TypeError".to_string(),
                        "KeyError".to_string(),
                    ]),
                    raises_extend_require_match_for: Some(vec![
                        "requests.RequestException".to_string(),
                    ]),
                    mark_parentheses: Some(false),
                }),
                flake8_quotes: Some(flake8_quotes::settings::Options {
                    inline_quotes: Some(Quote::Single),
                    multiline_quotes: Some(Quote::Double),
                    docstring_quotes: Some(Quote::Double),
                    avoid_escape: Some(true),
                }),
                flake8_tidy_imports: Some(flake8_tidy_imports::settings::Options {
                    ban_relative_imports: Some(Strictness::Parents),
                    banned_api: Some(FxHashMap::from_iter([
                        (
                            "cgi".to_string(),
                            BannedApi {
                                msg: "The cgi module is deprecated.".to_string()
                            }
                        ),
                        (
                            "typing.TypedDict".to_string(),
                            BannedApi {
                                msg: "Use typing_extensions.TypedDict instead.".to_string()
                            }
                        )
                    ]))
                }),
                flake8_import_conventions: Some(flake8_import_conventions::settings::Options {
                    aliases: Some(FxHashMap::from_iter([(
                        "pandas".to_string(),
                        "pd".to_string(),
                    )])),
                    extend_aliases: Some(FxHashMap::from_iter([(
                        "dask.dataframe".to_string(),
                        "dd".to_string(),
                    )])),
                }),
                flake8_unused_arguments: None,
                isort: None,
                mccabe: Some(mccabe::settings::Options {
                    max_complexity: Some(10),
                }),
                pep8_naming: Some(pep8_naming::settings::Options {
                    ignore_names: Some(vec![
                        "setUp".to_string(),
                        "tearDown".to_string(),
                        "setUpClass".to_string(),
                        "tearDownClass".to_string(),
                        "setUpModule".to_string(),
                        "tearDownModule".to_string(),
                        "asyncSetUp".to_string(),
                        "asyncTearDown".to_string(),
                        "setUpTestData".to_string(),
                        "failureException".to_string(),
                        "longMessage".to_string(),
                        "maxDiff".to_string(),
                    ]),
                    classmethod_decorators: Some(vec![
                        "classmethod".to_string(),
                        "pydantic.validator".to_string()
                    ]),
                    staticmethod_decorators: Some(vec!["staticmethod".to_string()]),
                }),
                pydocstyle: None,
                pyupgrade: None,
            }
        );

        Ok(())
    }

    #[test]
    fn str_check_code_pair_strings() {
        let result = PatternPrefixPair::from_str("foo:E501");
        assert!(result.is_ok());
        let result = PatternPrefixPair::from_str("foo: E501");
        assert!(result.is_ok());
        let result = PatternPrefixPair::from_str("E501:foo");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("E501");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("foo");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("foo:E501:E402");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("**/bar:E501");
        assert!(result.is_ok());
        let result = PatternPrefixPair::from_str("bar:E502");
        assert!(result.is_err());
    }
}

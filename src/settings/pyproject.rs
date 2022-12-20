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

fn parse_pyproject_toml<P: AsRef<Path>>(path: P) -> Result<Pyproject> {
    let contents = fs::read_file(path)?;
    toml::from_str(&contents).map_err(std::convert::Into::into)
}

/// Return `true` if a `pyproject.toml` contains a `[tool.ruff]` section.
pub fn has_ruff_section<P: AsRef<Path>>(path: P) -> Result<bool> {
    let pyproject = parse_pyproject_toml(path)?;
    Ok(pyproject.tool.and_then(|tool| tool.ruff).is_some())
}

/// Find the path to the `pyproject.toml` file, if such a file exists.
pub fn find_pyproject_toml<P: AsRef<Path>>(path: P) -> Result<Option<PathBuf>> {
    for directory in path.as_ref().ancestors() {
        let pyproject = directory.join("pyproject.toml");
        if pyproject.is_file() && has_ruff_section(&pyproject)? {
            return Ok(Some(pyproject));
        }
    }
    Ok(None)
}

/// Find the path to the user-specific `pyproject.toml`, if it exists.
pub fn find_user_pyproject_toml() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push("pyproject.toml");
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

/// Load `Options` from a `pyproject.toml`.
pub fn load_options<P: AsRef<Path>>(pyproject: P) -> Result<Options> {
    Ok(parse_pyproject_toml(&pyproject)
        .map_err(|err| {
            anyhow!(
                "Failed to parse `{}`: {}",
                pyproject.as_ref().to_string_lossy(),
                err
            )
        })?
        .tool
        .and_then(|tool| tool.ruff)
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::str::FromStr;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::checks_gen::CheckCodePrefix;
    use crate::flake8_quotes::settings::Quote;
    use crate::flake8_tidy_imports::settings::Strictness;
    use crate::settings::pyproject::{
        find_pyproject_toml, parse_pyproject_toml, Options, Pyproject, Tools,
    };
    use crate::settings::types::PatternPrefixPair;
    use crate::{
        flake8_bugbear, flake8_errmsg, flake8_import_conventions, flake8_quotes,
        flake8_tidy_imports, mccabe, pep8_naming,
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
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    external: None,
                    fix: None,
                    fixable: None,
                    format: None,
                    force_exclude: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    respect_gitignore: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_errmsg: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    flake8_import_conventions: None,
                    flake8_unused_arguments: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    fixable: None,
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: Some(79),
                    per_file_ignores: None,
                    respect_gitignore: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_errmsg: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    flake8_import_conventions: None,
                    flake8_unused_arguments: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    dummy_variable_rgx: None,
                    exclude: Some(vec!["foo.py".to_string()]),
                    extend: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    external: None,
                    fix: None,
                    fixable: None,
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    respect_gitignore: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_errmsg: None,
                    flake8_bugbear: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    flake8_import_conventions: None,
                    flake8_unused_arguments: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    external: None,
                    fix: None,
                    fixable: None,
                    force_exclude: None,
                    format: None,
                    ignore: None,
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    respect_gitignore: None,
                    select: Some(vec![CheckCodePrefix::E501]),
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_errmsg: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    flake8_import_conventions: None,
                    flake8_unused_arguments: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: Some(vec![CheckCodePrefix::RUF100]),
                    external: None,
                    fix: None,
                    fixable: None,
                    force_exclude: None,
                    format: None,
                    ignore: Some(vec![CheckCodePrefix::E501]),
                    ignore_init_module_imports: None,
                    line_length: None,
                    per_file_ignores: None,
                    respect_gitignore: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_errmsg: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    flake8_import_conventions: None,
                    flake8_unused_arguments: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
            find_pyproject_toml(cwd.join("resources/test/fixtures/__init__.py"))?.unwrap();
        assert_eq!(
            pyproject,
            cwd.join("resources/test/fixtures/pyproject.toml")
        );

        let pyproject = parse_pyproject_toml(&pyproject)?;
        let config = pyproject.tool.and_then(|tool| tool.ruff).unwrap();
        assert_eq!(
            config,
            Options {
                allowed_confusables: Some(vec!['−', 'ρ', '∗']),
                line_length: Some(88),
                fix: None,
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
                per_file_ignores: Some(FxHashMap::from_iter([(
                    "__init__.py".to_string(),
                    vec![CheckCodePrefix::F401]
                )])),
                dummy_variable_rgx: None,
                respect_gitignore: None,
                src: None,
                target_version: None,
                show_source: None,
                flake8_annotations: None,
                flake8_bugbear: Some(flake8_bugbear::settings::Options {
                    extend_immutable_calls: Some(vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                    ]),
                }),
                flake8_errmsg: Some(flake8_errmsg::settings::Options {
                    max_string_length: Some(20),
                }),
                flake8_quotes: Some(flake8_quotes::settings::Options {
                    inline_quotes: Some(Quote::Single),
                    multiline_quotes: Some(Quote::Double),
                    docstring_quotes: Some(Quote::Double),
                    avoid_escape: Some(true),
                }),
                flake8_tidy_imports: Some(flake8_tidy_imports::settings::Options {
                    ban_relative_imports: Some(Strictness::Parents)
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

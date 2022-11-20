//! Utilities for locating (and extracting configuration from) a pyproject.toml.

use std::path::{Path, PathBuf};

use anyhow::Result;
use common_path::common_path_all;
use log::debug;
use path_absolutize::Absolutize;
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

fn parse_pyproject_toml(path: &Path) -> Result<Pyproject> {
    let contents = fs::read_file(path)?;
    toml::from_str(&contents).map_err(|e| e.into())
}

pub fn find_pyproject_toml(path: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(path) = path {
        let path_pyproject_toml = path.join("pyproject.toml");
        if path_pyproject_toml.is_file() {
            return Some(path_pyproject_toml);
        }
    }

    find_user_pyproject_toml()
}

fn find_user_pyproject_toml() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push("pyproject.toml");
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

pub fn find_project_root(sources: &[PathBuf]) -> Option<PathBuf> {
    let absolute_sources: Vec<PathBuf> = sources
        .iter()
        .flat_map(|source| source.absolutize().map(|path| path.to_path_buf()))
        .collect();
    if let Some(prefix) = common_path_all(absolute_sources.iter().map(PathBuf::as_path)) {
        for directory in prefix.ancestors() {
            if directory.join(".git").is_dir() {
                return Some(directory.to_path_buf());
            }
            if directory.join(".hg").is_dir() {
                return Some(directory.to_path_buf());
            }
            if directory.join("pyproject.toml").is_file() {
                return Some(directory.to_path_buf());
            }
        }
    }

    None
}

pub fn load_options(pyproject: Option<&PathBuf>) -> Result<Options> {
    match pyproject {
        Some(pyproject) => Ok(parse_pyproject_toml(pyproject)?
            .tool
            .and_then(|tool| tool.ruff)
            .unwrap_or_default()),
        None => {
            debug!("No pyproject.toml found.");
            debug!("Falling back to default configuration...");
            Ok(Default::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::path::PathBuf;
    use std::str::FromStr;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::checks_gen::CheckCodePrefix;
    use crate::flake8_quotes::settings::Quote;
    use crate::flake8_tidy_imports::settings::Strictness;
    use crate::settings::pyproject::{
        find_project_root, find_pyproject_toml, parse_pyproject_toml, Options, Pyproject, Tools,
    };
    use crate::settings::types::PatternPrefixPair;
    use crate::{flake8_bugbear, flake8_quotes, flake8_tidy_imports, mccabe, pep8_naming};

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
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    fix: None,
                    fixable: None,
                    ignore: None,
                    line_length: None,
                    per_file_ignores: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    fix: None,
                    fixable: None,
                    ignore: None,
                    line_length: Some(79),
                    per_file_ignores: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    line_length: None,
                    fix: None,
                    exclude: Some(vec!["foo.py".to_string()]),
                    extend_exclude: None,
                    select: None,
                    extend_select: None,
                    ignore: None,
                    extend_ignore: None,
                    fixable: None,
                    unfixable: None,
                    per_file_ignores: None,
                    dummy_variable_rgx: None,
                    src: None,
                    target_version: None,
                    show_source: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: None,
                    fix: None,
                    fixable: None,
                    ignore: None,
                    line_length: None,
                    per_file_ignores: None,
                    select: Some(vec![CheckCodePrefix::E501]),
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
extend-select = ["M001"]
ignore = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    dummy_variable_rgx: None,
                    exclude: None,
                    extend_exclude: None,
                    extend_ignore: None,
                    extend_select: Some(vec![CheckCodePrefix::M001]),
                    fix: None,
                    fixable: None,
                    ignore: Some(vec![CheckCodePrefix::E501]),
                    line_length: None,
                    per_file_ignores: None,
                    select: None,
                    show_source: None,
                    src: None,
                    target_version: None,
                    unfixable: None,
                    flake8_annotations: None,
                    flake8_bugbear: None,
                    flake8_quotes: None,
                    flake8_tidy_imports: None,
                    isort: None,
                    mccabe: None,
                    pep8_naming: None,
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
        let project_root =
            find_project_root(&[PathBuf::from("resources/test/fixtures/__init__.py")])
                .expect("Unable to find project root.");
        assert_eq!(project_root, cwd.join("resources/test/fixtures"));

        let path =
            find_pyproject_toml(Some(&project_root)).expect("Unable to find pyproject.toml.");
        assert_eq!(path, cwd.join("resources/test/fixtures/pyproject.toml"));

        let pyproject = parse_pyproject_toml(&path)?;
        let config = pyproject
            .tool
            .and_then(|tool| tool.ruff)
            .expect("Unable to find tool.ruff.");
        assert_eq!(
            config,
            Options {
                line_length: Some(88),
                fix: None,
                exclude: None,
                extend_exclude: Some(vec![
                    "excluded_file.py".to_string(),
                    "migrations".to_string(),
                    "with_excluded_file/other_excluded_file.py".to_string(),
                ]),
                select: None,
                extend_select: None,
                ignore: None,
                extend_ignore: None,
                fixable: None,
                unfixable: None,
                per_file_ignores: Some(FxHashMap::from_iter([(
                    "__init__.py".to_string(),
                    vec![CheckCodePrefix::F401]
                ),])),
                dummy_variable_rgx: None,
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
                flake8_quotes: Some(flake8_quotes::settings::Options {
                    inline_quotes: Some(Quote::Single),
                    multiline_quotes: Some(Quote::Double),
                    docstring_quotes: Some(Quote::Double),
                    avoid_escape: Some(true),
                }),
                flake8_tidy_imports: Some(flake8_tidy_imports::settings::Options {
                    ban_relative_imports: Some(Strictness::Parents)
                }),
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

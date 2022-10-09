use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use common_path::common_path_all;
use path_absolutize::Absolutize;
use serde::de;
use serde::{Deserialize, Deserializer};

use crate::checks::CheckCode;
use crate::fs;
use crate::settings::PythonVersion;

pub fn load_config(pyproject: &Option<PathBuf>) -> Result<Config> {
    match pyproject {
        Some(pyproject) => Ok(parse_pyproject_toml(pyproject)?
            .tool
            .and_then(|tool| tool.ruff)
            .unwrap_or_default()),
        None => {
            eprintln!("No pyproject.toml found.");
            eprintln!("Falling back to default configuration...");
            Ok(Default::default())
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub line_length: Option<usize>,
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub extend_exclude: Vec<String>,
    pub select: Option<Vec<CheckCode>>,
    #[serde(default)]
    pub extend_select: Vec<CheckCode>,
    #[serde(default)]
    pub ignore: Vec<CheckCode>,
    #[serde(default)]
    pub extend_ignore: Vec<CheckCode>,
    #[serde(default)]
    pub per_file_ignores: Vec<StrCheckCodePair>,
    pub dummy_variable_rgx: Option<String>,
    pub target_version: Option<PythonVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrCheckCodePair {
    pub pattern: String,
    pub code: CheckCode,
}

impl StrCheckCodePair {
    const EXPECTED_PATTERN: &'static str = "<FilePattern>:<CheckCode> pattern";
}

impl<'de> Deserialize<'de> for StrCheckCodePair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str_result = String::deserialize(deserializer)?;
        Self::from_str(str_result.as_str()).map_err(|_| {
            de::Error::invalid_value(
                de::Unexpected::Str(str_result.as_str()),
                &Self::EXPECTED_PATTERN,
            )
        })
    }
}

impl FromStr for StrCheckCodePair {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let (pattern_str, code_string) = {
            let tokens = string.split(':').collect::<Vec<_>>();
            if tokens.len() != 2 {
                return Err(anyhow!("Expected {}", Self::EXPECTED_PATTERN));
            }
            (tokens[0], tokens[1])
        };
        let code = CheckCode::from_str(code_string)?;
        let pattern = pattern_str.into();
        Ok(Self { pattern, code })
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Tools {
    ruff: Option<Config>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct PyProject {
    tool: Option<Tools>,
}

fn parse_pyproject_toml(path: &Path) -> Result<PyProject> {
    let contents = fs::read_file(path)?;
    toml::from_str(&contents).map_err(|e| e.into())
}

pub fn find_pyproject_toml(path: &Option<PathBuf>) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::path::PathBuf;
    use std::str::FromStr;

    use anyhow::Result;

    use crate::checks::CheckCode;
    use crate::pyproject::{
        find_project_root, find_pyproject_toml, parse_pyproject_toml, Config, PyProject, Tools,
    };

    use super::StrCheckCodePair;

    #[test]
    fn deserialize() -> Result<()> {
        let pyproject: PyProject = toml::from_str(r#""#)?;
        assert_eq!(pyproject.tool, None);

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
"#,
        )?;
        assert_eq!(pyproject.tool, Some(Tools { ruff: None }));

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Config {
                    line_length: None,
                    exclude: None,
                    extend_exclude: vec![],
                    select: None,
                    extend_select: vec![],
                    ignore: vec![],
                    extend_ignore: vec![],
                    per_file_ignores: vec![],
                    dummy_variable_rgx: None,
                    target_version: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
line-length = 79
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Config {
                    line_length: Some(79),
                    exclude: None,
                    extend_exclude: vec![],
                    select: None,
                    extend_select: vec![],
                    ignore: vec![],
                    extend_ignore: vec![],
                    per_file_ignores: vec![],
                    dummy_variable_rgx: None,
                    target_version: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
exclude = ["foo.py"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Config {
                    line_length: None,
                    exclude: Some(vec!["foo.py".to_string()]),
                    extend_exclude: vec![],
                    select: None,
                    extend_select: vec![],
                    ignore: vec![],
                    extend_ignore: vec![],
                    per_file_ignores: vec![],
                    dummy_variable_rgx: None,
                    target_version: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
select = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Config {
                    line_length: None,
                    exclude: None,
                    extend_exclude: vec![],
                    select: Some(vec![CheckCode::E501]),
                    extend_select: vec![],
                    ignore: vec![],
                    extend_ignore: vec![],
                    per_file_ignores: vec![],
                    dummy_variable_rgx: None,
                    target_version: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
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
                ruff: Some(Config {
                    line_length: None,
                    exclude: None,
                    extend_exclude: vec![],
                    select: None,
                    extend_select: vec![CheckCode::M001],
                    ignore: vec![CheckCode::E501],
                    extend_ignore: vec![],
                    per_file_ignores: vec![],
                    dummy_variable_rgx: None,
                    target_version: None,
                })
            })
        );

        assert!(toml::from_str::<PyProject>(
            r#"
[tool.black]
[tool.ruff]
line_length = 79
"#,
        )
        .is_err());

        assert!(toml::from_str::<PyProject>(
            r#"
[tool.black]
[tool.ruff]
select = ["E123"]
"#,
        )
        .is_err());

        assert!(toml::from_str::<PyProject>(
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
            find_pyproject_toml(&Some(project_root)).expect("Unable to find pyproject.toml.");
        assert_eq!(path, cwd.join("resources/test/fixtures/pyproject.toml"));

        let pyproject = parse_pyproject_toml(&path)?;
        let config = pyproject
            .tool
            .and_then(|tool| tool.ruff)
            .expect("Unable to find tool.ruff.");
        assert_eq!(
            config,
            Config {
                line_length: Some(88),
                exclude: None,
                extend_exclude: vec![
                    "excluded.py".to_string(),
                    "migrations".to_string(),
                    "directory/also_excluded.py".to_string(),
                ],
                select: None,
                extend_select: vec![],
                ignore: vec![],
                extend_ignore: vec![],
                per_file_ignores: vec![],
                dummy_variable_rgx: None,
                target_version: None,
            }
        );

        Ok(())
    }

    #[test]
    fn str_check_code_pair_strings() {
        let result = StrCheckCodePair::from_str("foo:E501");
        assert!(result.is_ok());
        let result = StrCheckCodePair::from_str("E501:foo");
        assert!(result.is_err());
        let result = StrCheckCodePair::from_str("E501");
        assert!(result.is_err());
        let result = StrCheckCodePair::from_str("foo");
        assert!(result.is_err());
        let result = StrCheckCodePair::from_str("foo:E501:E402");
        assert!(result.is_err());
        let result = StrCheckCodePair::from_str("**/bar:E501");
        assert!(result.is_ok());
        let result = StrCheckCodePair::from_str("bar:E502");
        assert!(result.is_err());
    }
}

use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Result;
use common_path::common_path_all;
use path_absolutize::path_dedot;
use serde::Deserialize;

use crate::checks::CheckCode;
use crate::fs;

pub fn load_config(pyproject: &Option<PathBuf>) -> Config {
    match pyproject {
        Some(pyproject) => match parse_pyproject_toml(pyproject) {
            Ok(pyproject) => pyproject
                .tool
                .and_then(|tool| tool.ruff)
                .unwrap_or_default(),
            Err(e) => {
                println!("Failed to load pyproject.toml: {:?}", e);
                println!("Falling back to default configuration...");
                Default::default()
            }
        },
        None => {
            println!("No pyproject.toml found.");
            println!("Falling back to default configuration...");
            Default::default()
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub line_length: Option<usize>,
    pub exclude: Option<Vec<String>>,
    pub extend_exclude: Option<Vec<String>>,
    pub select: Option<Vec<CheckCode>>,
    pub ignore: Option<Vec<CheckCode>>,
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
    let cwd = path_dedot::CWD.deref();
    let absolute_sources: Vec<PathBuf> = sources.iter().map(|source| cwd.join(source)).collect();
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

    use anyhow::Result;

    use crate::checks::CheckCode;
    use crate::pyproject::{
        find_project_root, find_pyproject_toml, parse_pyproject_toml, Config, PyProject, Tools,
    };

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
                    extend_exclude: None,
                    select: None,
                    ignore: None,
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
                    extend_exclude: None,
                    select: None,
                    ignore: None,
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
                    extend_exclude: None,
                    select: None,
                    ignore: None,
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
                    extend_exclude: None,
                    select: Some(vec![CheckCode::E501]),
                    ignore: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
ignore = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Config {
                    line_length: None,
                    exclude: None,
                    extend_exclude: None,
                    select: None,
                    ignore: Some(vec![CheckCode::E501]),
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
                extend_exclude: Some(vec![
                    "excluded.py".to_string(),
                    "migrations".to_string(),
                    "directory/also_excluded.py".to_string(),
                ]),
                select: None,
                ignore: None,
            }
        );

        Ok(())
    }
}

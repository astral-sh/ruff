use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use common_path::common_path_all;
use log::debug;
use serde::Deserialize;

use crate::checks::CheckCode;
use crate::fs;

pub fn load_config<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<(PathBuf, Config)> {
    match find_project_root(paths) {
        Some(project_root) => match find_pyproject_toml(&project_root) {
            Some(path) => {
                debug!("Found pyproject.toml at: {}", path.to_string_lossy());
                let pyproject = parse_pyproject_toml(&path)?;
                let config = pyproject
                    .tool
                    .and_then(|tool| tool.linter)
                    .unwrap_or_default();
                Ok((project_root, config))
            }
            None => Ok(Default::default()),
        },
        None => Ok(Default::default()),
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub line_length: Option<usize>,
    pub exclude: Option<Vec<PathBuf>>,
    pub select: Option<BTreeSet<CheckCode>>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Tools {
    linter: Option<Config>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct PyProject {
    tool: Option<Tools>,
}

fn parse_pyproject_toml(path: &Path) -> Result<PyProject> {
    let contents = fs::read_file(path)?;
    toml::from_str(&contents).map_err(|e| e.into())
}

// https://github.com/psf/black/blob/44d5da00b520a05cd56e58b3998660f64ea59ebd/src/black/files.py#L84
fn find_pyproject_toml(path: &Path) -> Option<PathBuf> {
    let path_pyproject_toml = path.join("pyproject.toml");
    if path_pyproject_toml.is_file() {
        return Some(path_pyproject_toml);
    }
    find_user_pyproject_toml()
}

// https://github.com/psf/black/blob/44d5da00b520a05cd56e58b3998660f64ea59ebd/src/black/files.py#L117
fn find_user_pyproject_toml() -> Option<PathBuf> {
    dirs::home_dir().map(|path| path.join(".linter"))
}

// https://github.com/psf/black/blob/44d5da00b520a05cd56e58b3998660f64ea59ebd/src/black/files.py#L42
fn find_project_root<'a>(sources: impl IntoIterator<Item = &'a Path>) -> Option<PathBuf> {
    if let Some(prefix) = common_path_all(sources) {
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
    use std::collections::BTreeSet;
    use std::path::Path;

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
        assert_eq!(pyproject.tool, Some(Tools { linter: None }));

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.linter]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                linter: Some(Config {
                    line_length: None,
                    exclude: None,
                    select: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.linter]
line-length = 79
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                linter: Some(Config {
                    line_length: Some(79),
                    exclude: None,
                    select: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.linter]
exclude = ["foo.py"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                linter: Some(Config {
                    line_length: None,
                    exclude: Some(vec![Path::new("foo.py").to_path_buf()]),
                    select: None,
                })
            })
        );

        let pyproject: PyProject = toml::from_str(
            r#"
[tool.black]
[tool.linter]
select = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                linter: Some(Config {
                    line_length: None,
                    exclude: None,
                    select: Some(BTreeSet::from([CheckCode::E501])),
                })
            })
        );

        assert!(toml::from_str::<PyProject>(
            r#"
[tool.black]
[tool.linter]
line_length = 79
"#,
        )
        .is_err());

        assert!(toml::from_str::<PyProject>(
            r#"
[tool.black]
[tool.linter]
select = ["E123"]
"#,
        )
        .is_err());

        assert!(toml::from_str::<PyProject>(
            r#"
[tool.black]
[tool.linter]
line-length = 79
other-attribute = 1
"#,
        )
        .is_err());

        Ok(())
    }

    #[test]
    fn find_and_parse_pyproject_toml() -> Result<()> {
        let project_root = find_project_root([Path::new("resources/test/src/__init__.py")])
            .expect("Unable to find project root.");
        assert_eq!(project_root, Path::new("resources/test/src"));

        let path = find_pyproject_toml(&project_root).expect("Unable to find pyproject.toml.");
        assert_eq!(path, Path::new("resources/test/src/pyproject.toml"));

        let pyproject = parse_pyproject_toml(&path)?;
        let config = pyproject
            .tool
            .map(|tool| tool.linter)
            .flatten()
            .expect("Unable to find tool.linter.");
        assert_eq!(
            config,
            Config {
                line_length: Some(88),
                exclude: Some(vec![Path::new("excluded.py").to_path_buf()]),
                select: None,
            }
        );

        Ok(())
    }
}

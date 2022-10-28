use std::collections::BTreeSet;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use glob::Pattern;
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::checks::CheckCode;
use crate::checks_gen::CheckCodePrefix;
use crate::fs;

#[derive(Clone, Debug, PartialOrd, PartialEq, Eq, Serialize, Deserialize)]
pub enum PythonVersion {
    Py33,
    Py34,
    Py35,
    Py36,
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
}

impl FromStr for PythonVersion {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "py33" => Ok(PythonVersion::Py33),
            "py34" => Ok(PythonVersion::Py34),
            "py35" => Ok(PythonVersion::Py35),
            "py36" => Ok(PythonVersion::Py36),
            "py37" => Ok(PythonVersion::Py37),
            "py38" => Ok(PythonVersion::Py38),
            "py39" => Ok(PythonVersion::Py39),
            "py310" => Ok(PythonVersion::Py310),
            "py311" => Ok(PythonVersion::Py311),
            _ => Err(anyhow!("Unknown version: {}", string)),
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub enum FilePattern {
    Simple(&'static str),
    Complex(Pattern, Option<Pattern>),
}

impl FilePattern {
    pub fn from_user(pattern: &str, project_root: &Option<PathBuf>) -> Self {
        let path = Path::new(pattern);
        let absolute_path = match project_root {
            Some(project_root) => fs::normalize_path_to(path, project_root),
            None => fs::normalize_path(path),
        };

        let absolute = Pattern::new(&absolute_path.to_string_lossy()).expect("Invalid pattern.");
        let basename = if !pattern.contains(std::path::MAIN_SEPARATOR) {
            Some(Pattern::new(pattern).expect("Invalid pattern."))
        } else {
            None
        };

        FilePattern::Complex(absolute, basename)
    }
}

#[derive(Debug, Clone, Hash)]
pub struct PerFileIgnore {
    pub pattern: FilePattern,
    pub codes: BTreeSet<CheckCode>,
}

impl PerFileIgnore {
    pub fn new(user_in: StrCheckCodePair, project_root: &Option<PathBuf>) -> Self {
        let pattern = FilePattern::from_user(user_in.pattern.as_str(), project_root);
        let codes = BTreeSet::from_iter(user_in.code.codes());
        Self { pattern, codes }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrCheckCodePair {
    pub pattern: String,
    pub code: CheckCodePrefix,
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
            (tokens[0].trim(), tokens[1].trim())
        };
        let code = CheckCodePrefix::from_str(code_string)?;
        let pattern = pattern_str.into();
        Ok(Self { pattern, code })
    }
}

use anyhow::Result;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum CheckCode {
    E501,
    F401,
    F403,
    F541,
    F634,
    F706,
    F831,
    F901,
}

impl FromStr for CheckCode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "E501" => Ok(CheckCode::E501),
            "F401" => Ok(CheckCode::F401),
            "F403" => Ok(CheckCode::F403),
            "F541" => Ok(CheckCode::F541),
            "F634" => Ok(CheckCode::F634),
            "F706" => Ok(CheckCode::F706),
            "F831" => Ok(CheckCode::F831),
            "F901" => Ok(CheckCode::F901),
            _ => Err(anyhow::anyhow!("Unknown check code: {s}")),
        }
    }
}

impl CheckCode {
    pub fn as_str(&self) -> &str {
        match self {
            CheckCode::E501 => "E501",
            CheckCode::F401 => "F401",
            CheckCode::F403 => "F403",
            CheckCode::F541 => "F541",
            CheckCode::F634 => "F634",
            CheckCode::F706 => "F706",
            CheckCode::F831 => "F831",
            CheckCode::F901 => "F901",
        }
    }

    /// The source for the check (either the AST, or the physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::E501 => &LintSource::Lines,
            CheckCode::F401 => &LintSource::AST,
            CheckCode::F403 => &LintSource::AST,
            CheckCode::F541 => &LintSource::AST,
            CheckCode::F634 => &LintSource::AST,
            CheckCode::F706 => &LintSource::AST,
            CheckCode::F831 => &LintSource::AST,
            CheckCode::F901 => &LintSource::AST,
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub enum LintSource {
    AST,
    Lines,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    DuplicateArgumentName,
    FStringMissingPlaceholders,
    IfTuple,
    ImportStarUsage,
    LineTooLong,
    RaiseNotImplemented,
    ReturnOutsideFunction,
    UnusedImport(String),
}

impl CheckKind {
    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static CheckCode {
        match self {
            CheckKind::DuplicateArgumentName => &CheckCode::F831,
            CheckKind::FStringMissingPlaceholders => &CheckCode::F541,
            CheckKind::IfTuple => &CheckCode::F634,
            CheckKind::ImportStarUsage => &CheckCode::F403,
            CheckKind::LineTooLong => &CheckCode::E501,
            CheckKind::RaiseNotImplemented => &CheckCode::F901,
            CheckKind::ReturnOutsideFunction => &CheckCode::F706,
            CheckKind::UnusedImport(_) => &CheckCode::F401,
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> String {
        match self {
            CheckKind::DuplicateArgumentName => {
                "Duplicate argument name in function definition".to_string()
            }
            CheckKind::FStringMissingPlaceholders => {
                "f-string without any placeholders".to_string()
            }
            CheckKind::IfTuple => {
                "If test is a tuple.to_string(), which is always `True`".to_string()
            }
            CheckKind::ImportStarUsage => "Unable to detect undefined names".to_string(),
            CheckKind::LineTooLong => "Line too long".to_string(),
            CheckKind::RaiseNotImplemented => {
                "'raise NotImplemented' should be 'raise NotImplementedError".to_string()
            }
            CheckKind::ReturnOutsideFunction => {
                "a `return` statement outside of a function/method".to_string()
            }
            CheckKind::UnusedImport(name) => format!("`{name}` imported but unused"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
}

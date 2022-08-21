use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum CheckCode {
    F831,
    F541,
    F634,
    F403,
    F706,
    F901,
    E501,
}

impl CheckCode {
    pub fn as_str(&self) -> &str {
        match self {
            CheckCode::F831 => "F831",
            CheckCode::F541 => "F541",
            CheckCode::F634 => "F634",
            CheckCode::F403 => "F403",
            CheckCode::F706 => "F706",
            CheckCode::F901 => "F901",
            CheckCode::E501 => "E501",
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
    ReturnOutsideFunction,
    RaiseNotImplemented,
    LineTooLong,
}

impl CheckKind {
    /// Get the CheckKind for a corresponding code.
    pub fn new(code: &CheckCode) -> Self {
        match code {
            CheckCode::F831 => CheckKind::DuplicateArgumentName,
            CheckCode::F541 => CheckKind::FStringMissingPlaceholders,
            CheckCode::F634 => CheckKind::IfTuple,
            CheckCode::F403 => CheckKind::ImportStarUsage,
            CheckCode::F706 => CheckKind::ReturnOutsideFunction,
            CheckCode::F901 => CheckKind::RaiseNotImplemented,
            CheckCode::E501 => CheckKind::LineTooLong,
        }
    }

    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static CheckCode {
        match self {
            CheckKind::DuplicateArgumentName => &CheckCode::F831,
            CheckKind::FStringMissingPlaceholders => &CheckCode::F541,
            CheckKind::IfTuple => &CheckCode::F634,
            CheckKind::ImportStarUsage => &CheckCode::F403,
            CheckKind::ReturnOutsideFunction => &CheckCode::F706,
            CheckKind::RaiseNotImplemented => &CheckCode::F901,
            CheckKind::LineTooLong => &CheckCode::E501,
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> &'static str {
        match self {
            CheckKind::DuplicateArgumentName => "Duplicate argument name in function definition",
            CheckKind::FStringMissingPlaceholders => "f-string without any placeholders",
            CheckKind::IfTuple => "If test is a tuple, which is always `True`",
            CheckKind::ImportStarUsage => "Unable to detect undefined names",
            CheckKind::ReturnOutsideFunction => "a `return` statement outside of a function/method",
            CheckKind::RaiseNotImplemented => {
                "'raise NotImplemented' should be 'raise NotImplementedError"
            }
            CheckKind::LineTooLong => "Line too long",
        }
    }

    /// The source for the checks (either the AST, or the physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckKind::DuplicateArgumentName => &LintSource::AST,
            CheckKind::FStringMissingPlaceholders => &LintSource::AST,
            CheckKind::IfTuple => &LintSource::AST,
            CheckKind::ImportStarUsage => &LintSource::AST,
            CheckKind::ReturnOutsideFunction => &LintSource::AST,
            CheckKind::RaiseNotImplemented => &LintSource::AST,
            CheckKind::LineTooLong => &LintSource::Lines,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
}

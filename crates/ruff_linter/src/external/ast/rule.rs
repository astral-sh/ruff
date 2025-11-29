use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use ruff_db::diagnostic::SecondaryCode;
use serde::Deserialize;
use thiserror::Error;

use crate::external::ast::target::{AstTarget, AstTargetSpec};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExternalRuleCode(Box<str>);

impl ExternalRuleCode {
    pub fn new<S: AsRef<str>>(code: S) -> Result<Self, ExternalRuleCodeError> {
        let code_ref = code.as_ref();
        if code_ref.is_empty() {
            return Err(ExternalRuleCodeError::Empty);
        }
        if !Self::matches_format(code_ref) {
            return Err(ExternalRuleCodeError::InvalidCharacters(
                code_ref.to_string(),
            ));
        }
        Ok(Self(code_ref.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_secondary_code(&self) -> SecondaryCode {
        SecondaryCode::new(self.as_str().to_string())
    }

    fn pattern() -> &'static Regex {
        static PATTERN: OnceLock<Regex> = OnceLock::new();
        PATTERN.get_or_init(|| Regex::new(r"^[A-Z]+[0-9]+$").expect("valid external rule regex"))
    }

    pub(crate) fn matches_format(code: &str) -> bool {
        Self::pattern().is_match(code)
    }
}

impl std::fmt::Display for ExternalRuleCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
pub enum ExternalRuleCodeError {
    #[error("external rule codes must not be empty")]
    Empty,
    #[error("external rule codes must contain only uppercase ASCII letters and digits: `{0}`")]
    InvalidCharacters(String),
}

/// Fully resolved script content that can be handed to the runtime for compilation.
#[derive(Debug, Clone)]
pub struct ExternalRuleScript {
    path: PathBuf,
    contents: String,
}

impl ExternalRuleScript {
    pub fn file(path: PathBuf, contents: impl Into<String>) -> Self {
        Self {
            path,
            contents: contents.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn body(&self) -> &str {
        &self.contents
    }
}

/// User-facing metadata describing an external AST rule before targets are resolved.
#[derive(Debug, Clone, Deserialize)]
pub struct ExternalAstRuleSpec {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub targets: Vec<AstTargetSpec>,
    #[serde(default, rename = "call-callee-regex")]
    pub call_callee_regex: Option<String>,
    pub script: PathBuf,
}

/// A validated, ready-to-run external AST rule definition.
#[derive(Debug, Clone)]
pub struct ExternalAstRule {
    pub code: ExternalRuleCode,
    pub name: String,
    pub summary: Option<String>,
    pub targets: Box<[AstTarget]>,
    pub script: ExternalRuleScript,
    pub call_callee: Option<CallCalleeMatcher>,
}

impl ExternalAstRule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code: ExternalRuleCode,
        name: impl Into<String>,
        summary: Option<impl Into<String>>,
        targets: Vec<AstTarget>,
        script: ExternalRuleScript,
        call_callee: Option<CallCalleeMatcher>,
    ) -> Self {
        let targets = targets.into_boxed_slice();
        Self {
            code,
            name: name.into(),
            summary: summary.map(Into::into),
            targets,
            script,
            call_callee,
        }
    }

    pub fn call_callee(&self) -> Option<&CallCalleeMatcher> {
        self.call_callee.as_ref()
    }
}

/// Metadata about a collection of external AST rules loaded from a user-defined linter file.
#[derive(Debug, Clone)]
pub struct ExternalAstLinter {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub rules: Vec<ExternalAstRule>,
}

impl ExternalAstLinter {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: Option<impl Into<String>>,
        enabled: bool,
        rules: Vec<ExternalAstRule>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.map(Into::into),
            enabled,
            rules,
        }
    }
}

impl std::fmt::Display for ExternalAstLinter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}{}",
            self.id,
            if self.enabled { "" } else { " (disabled)" }
        )?;
        writeln!(f, "    name: {}", self.name)?;
        if let Some(description) = &self.description {
            writeln!(f, "    description: {description}")?;
        }
        writeln!(f, "    rules:")?;
        for rule in &self.rules {
            writeln!(f, "      - {} ({})", rule.code.as_str(), rule.name)?;
        }
        writeln!(f)
    }
}

#[derive(Debug, Clone)]
pub struct CallCalleeMatcher {
    pattern: String,
    regex: Regex,
}

impl CallCalleeMatcher {
    pub fn new(pattern: impl Into<String>) -> Result<Self, regex::Error> {
        let pattern = pattern.into();
        let regex = Regex::new(pattern.as_ref())?;
        Ok(Self { pattern, regex })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn regex(&self) -> &Regex {
        &self.regex
    }
}

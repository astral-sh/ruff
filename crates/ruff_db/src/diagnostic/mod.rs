use std::borrow::Cow;
use std::fmt::Formatter;

use thiserror::Error;

use ruff_annotate_snippets::Level as AnnotateLevel;
use ruff_python_parser::ParseError;
use ruff_text_size::TextRange;

pub use crate::diagnostic::old::{
    OldDiagnosticTrait, OldDisplayDiagnostic, OldSecondaryDiagnosticMessage,
};
use crate::files::File;

// This module should not be exported. We are planning to migrate off
// the APIs in this module.
mod old;

/// A string identifier for a lint rule.
///
/// This string is used in command line and configuration interfaces. The name should always
/// be in kebab case, e.g. `no-foo` (all lower case).
///
/// Rules use kebab case, e.g. `no-foo`.
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct LintName(&'static str);

impl LintName {
    pub const fn of(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl std::ops::Deref for LintName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl std::fmt::Display for LintName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl PartialEq<str> for LintName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for LintName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Uniquely identifies the kind of a diagnostic.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub enum DiagnosticId {
    /// Some I/O operation failed
    Io,

    /// Some code contains a syntax error
    InvalidSyntax,

    /// A lint violation.
    ///
    /// Lints can be suppressed and some lints can be enabled or disabled in the configuration.
    Lint(LintName),

    /// A revealed type: Created by `reveal_type(expression)`.
    RevealedType,

    /// No rule with the given name exists.
    UnknownRule,
}

impl DiagnosticId {
    /// Creates a new `DiagnosticId` for a lint with the given name.
    pub const fn lint(name: &'static str) -> Self {
        Self::Lint(LintName::of(name))
    }

    /// Returns `true` if this `DiagnosticId` represents a lint.
    pub fn is_lint(&self) -> bool {
        matches!(self, DiagnosticId::Lint(_))
    }

    /// Returns `true` if this `DiagnosticId` represents a lint with the given name.
    pub fn is_lint_named(&self, name: &str) -> bool {
        matches!(self, DiagnosticId::Lint(self_name) if self_name == name)
    }

    pub fn strip_category(code: &str) -> Option<&str> {
        code.split_once(':').map(|(_, rest)| rest)
    }

    /// Returns `true` if this `DiagnosticId` matches the given name.
    ///
    /// ## Examples
    /// ```
    /// use ruff_db::diagnostic::DiagnosticId;
    ///
    /// assert!(DiagnosticId::Io.matches("io"));
    /// assert!(DiagnosticId::lint("test").matches("lint:test"));
    /// assert!(!DiagnosticId::lint("test").matches("test"));
    /// ```
    pub fn matches(&self, expected_name: &str) -> bool {
        match self.as_str() {
            Ok(id) => id == expected_name,
            Err(DiagnosticAsStrError::Category { category, name }) => expected_name
                .strip_prefix(category)
                .and_then(|prefix| prefix.strip_prefix(":"))
                .is_some_and(|rest| rest == name),
        }
    }

    pub fn as_str(&self) -> Result<&str, DiagnosticAsStrError> {
        Ok(match self {
            DiagnosticId::Io => "io",
            DiagnosticId::InvalidSyntax => "invalid-syntax",
            DiagnosticId::Lint(name) => {
                return Err(DiagnosticAsStrError::Category {
                    category: "lint",
                    name: name.as_str(),
                })
            }
            DiagnosticId::RevealedType => "revealed-type",
            DiagnosticId::UnknownRule => "unknown-rule",
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Error)]
pub enum DiagnosticAsStrError {
    /// The id can't be converted to a string because it belongs to a sub-category.
    #[error("id from a sub-category: {category}:{name}")]
    Category {
        /// The id's category.
        category: &'static str,
        /// The diagnostic id in this category.
        name: &'static str,
    },
}

impl std::fmt::Display for DiagnosticId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.as_str() {
            Ok(name) => f.write_str(name),
            Err(DiagnosticAsStrError::Category { category, name }) => {
                write!(f, "{category}:{name}")
            }
        }
    }
}

/// A span represents the source of a diagnostic.
///
/// It consists of a `File` and an optional range into that file. When the
/// range isn't present, it semantically implies that the diagnostic refers to
/// the entire file. For example, when the file should be executable but isn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    file: File,
    range: Option<TextRange>,
}

impl Span {
    /// Returns the `File` attached to this `Span`.
    pub fn file(&self) -> File {
        self.file
    }

    /// Returns the range, if available, attached to this `Span`.
    ///
    /// When there is no range, it is convention to assume that this `Span`
    /// refers to the corresponding `File` as a whole. In some cases, consumers
    /// of this API may use the range `0..0` to represent this case.
    pub fn range(&self) -> Option<TextRange> {
        self.range
    }

    /// Returns a new `Span` with the given `range` attached to it.
    pub fn with_range(self, range: TextRange) -> Span {
        self.with_optional_range(Some(range))
    }

    /// Returns a new `Span` with the given optional `range` attached to it.
    pub fn with_optional_range(self, range: Option<TextRange>) -> Span {
        Span { range, ..self }
    }
}

impl From<File> for Span {
    fn from(file: File) -> Span {
        Span { file, range: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

impl Severity {
    fn to_annotate(self) -> AnnotateLevel {
        match self {
            Severity::Info => AnnotateLevel::Info,
            Severity::Warning => AnnotateLevel::Warning,
            Severity::Error => AnnotateLevel::Error,
            // NOTE: Should we really collapse this to "error"?
            //
            // After collapsing this, the snapshot tests seem to reveal that we
            // don't currently have any *tests* with a `fatal` severity level.
            // And maybe *rendering* this as just an `error` is fine. If we
            // really do need different rendering, then I think we can add a
            // `Level::Fatal`. ---AG
            Severity::Fatal => AnnotateLevel::Error,
        }
    }
}

/// Configuration for rendering diagnostics.
#[derive(Clone, Debug, Default)]
pub struct DisplayDiagnosticConfig {
    /// Whether to enable colors or not.
    ///
    /// Disabled by default.
    color: bool,
}

impl DisplayDiagnosticConfig {
    /// Whether to enable colors or not.
    pub fn color(self, yes: bool) -> DisplayDiagnosticConfig {
        DisplayDiagnosticConfig { color: yes }
    }
}

#[derive(Debug)]
pub struct ParseDiagnostic {
    file: File,
    error: ParseError,
}

impl ParseDiagnostic {
    pub fn new(file: File, error: ParseError) -> Self {
        Self { file, error }
    }
}

impl OldDiagnosticTrait for ParseDiagnostic {
    fn id(&self) -> DiagnosticId {
        DiagnosticId::InvalidSyntax
    }

    fn message(&self) -> Cow<str> {
        self.error.error.to_string().into()
    }

    fn span(&self) -> Option<Span> {
        Some(Span::from(self.file).with_range(self.error.location))
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }
}

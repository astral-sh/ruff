use std::borrow::Cow;
use std::fmt::Formatter;

use thiserror::Error;

use ruff_python_parser::ParseError;
use ruff_text_size::TextRange;

use crate::{
    files::File,
    source::{line_index, source_text},
    Db,
};

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

pub trait Diagnostic: Send + Sync + std::fmt::Debug {
    fn id(&self) -> DiagnosticId;

    fn message(&self) -> Cow<str>;

    /// The file this diagnostic is associated with.
    ///
    /// File can be `None` for diagnostics that don't originate from a file.
    /// For example:
    /// * A diagnostic indicating that a directory couldn't be read.
    /// * A diagnostic related to a CLI argument
    fn file(&self) -> Option<File>;

    /// The primary range of the diagnostic in `file`.
    ///
    /// The range can be `None` if the diagnostic doesn't have a file
    /// or it applies to the entire file (e.g. the file should be executable but isn't).
    fn range(&self) -> Option<TextRange>;

    fn severity(&self) -> Severity;

    fn display<'a>(&'a self, db: &'a dyn Db) -> DisplayDiagnostic<'a>
    where
        Self: Sized,
    {
        DisplayDiagnostic {
            db,
            diagnostic: self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

pub struct DisplayDiagnostic<'db> {
    db: &'db dyn Db,
    diagnostic: &'db dyn Diagnostic,
}

impl<'db> DisplayDiagnostic<'db> {
    pub fn new(db: &'db dyn Db, diagnostic: &'db dyn Diagnostic) -> Self {
        Self { db, diagnostic }
    }
}

impl std::fmt::Display for DisplayDiagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.diagnostic.severity() {
            Severity::Info => f.write_str("info")?,
            Severity::Warning => f.write_str("warning")?,
            Severity::Error => f.write_str("error")?,
            Severity::Fatal => f.write_str("fatal")?,
        }

        write!(f, "[{rule}]", rule = self.diagnostic.id())?;

        if let Some(file) = self.diagnostic.file() {
            write!(f, " {path}", path = file.path(self.db))?;
        }

        if let (Some(file), Some(range)) = (self.diagnostic.file(), self.diagnostic.range()) {
            let index = line_index(self.db, file);
            let source = source_text(self.db, file);

            let start = index.source_location(range.start(), &source);

            write!(f, ":{line}:{col}", line = start.row, col = start.column)?;
        }

        write!(f, " {message}", message = self.diagnostic.message())
    }
}

impl<T> Diagnostic for Box<T>
where
    T: Diagnostic,
{
    fn id(&self) -> DiagnosticId {
        (**self).id()
    }

    fn message(&self) -> Cow<str> {
        (**self).message()
    }

    fn file(&self) -> Option<File> {
        (**self).file()
    }

    fn range(&self) -> Option<TextRange> {
        (**self).range()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl<T> Diagnostic for std::sync::Arc<T>
where
    T: Diagnostic,
{
    fn id(&self) -> DiagnosticId {
        (**self).id()
    }

    fn message(&self) -> std::borrow::Cow<str> {
        (**self).message()
    }

    fn file(&self) -> Option<File> {
        (**self).file()
    }

    fn range(&self) -> Option<TextRange> {
        (**self).range()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl Diagnostic for Box<dyn Diagnostic> {
    fn id(&self) -> DiagnosticId {
        (**self).id()
    }

    fn message(&self) -> Cow<str> {
        (**self).message()
    }

    fn file(&self) -> Option<File> {
        (**self).file()
    }

    fn range(&self) -> Option<TextRange> {
        (**self).range()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
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

impl Diagnostic for ParseDiagnostic {
    fn id(&self) -> DiagnosticId {
        DiagnosticId::InvalidSyntax
    }

    fn message(&self) -> Cow<str> {
        self.error.error.to_string().into()
    }

    fn file(&self) -> Option<File> {
        Some(self.file)
    }

    fn range(&self) -> Option<TextRange> {
        Some(self.error.location)
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }
}

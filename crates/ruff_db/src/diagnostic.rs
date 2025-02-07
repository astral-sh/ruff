use std::borrow::Cow;
use std::fmt::Formatter;

use thiserror::Error;

use ruff_annotate_snippets::{Level, Renderer, Snippet};
use ruff_python_parser::ParseError;
use ruff_source_file::{OneIndexed, SourceCode};
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

    /// Some code contains a syntax error.
    ///
    /// Contains `Some` for syntax errors that are individually documented (as opposed to those
    /// emitted by the parser). An example of an individually documented syntax error might be use of
    /// the `match` statement on a Python version before 3.10.
    InvalidSyntax(Option<LintName>),

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

    /// Creates a new `DiagnosticId` for a syntax error with an optional name.
    pub const fn invalid_syntax(name: Option<&'static str>) -> Self {
        Self::InvalidSyntax(match name {
            Some(name) => Some(LintName::of(name)),
            None => None,
        })
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
            DiagnosticId::InvalidSyntax(None) => "invalid-syntax",
            DiagnosticId::InvalidSyntax(Some(name)) | DiagnosticId::Lint(name) => {
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
        let level = match self.diagnostic.severity() {
            Severity::Info => Level::Info,
            Severity::Warning => Level::Warning,
            Severity::Error => Level::Error,
            // NOTE: Should we really collapse this to "error"?
            //
            // After collapsing this, the snapshot tests seem to reveal that we
            // don't currently have any *tests* with a `fatal` severity level.
            // And maybe *rendering* this as just an `error` is fine. If we
            // really do need different rendering, then I think we can add a
            // `Level::Fatal`. ---AG
            Severity::Fatal => Level::Error,
        };

        let render = |f: &mut std::fmt::Formatter, message| {
            let renderer = if !cfg!(test) && colored::control::SHOULD_COLORIZE.should_colorize() {
                Renderer::styled()
            } else {
                Renderer::plain()
            }
            .cut_indicator("…");
            let rendered = renderer.render(message);
            writeln!(f, "{rendered}")
        };
        match (self.diagnostic.file(), self.diagnostic.range()) {
            (None, _) => {
                // NOTE: This is pretty sub-optimal. It doesn't render well. We
                // really want a snippet, but without a `File`, we can't really
                // render anything. It looks like this case currently happens
                // for configuration errors. It looks like we can probably
                // produce a snippet for this if it comes from a file, but if
                // it comes from the CLI, I'm not quite sure exactly what to
                // do. ---AG
                let msg = format!("{}: {}", self.diagnostic.id(), self.diagnostic.message());
                render(f, level.title(&msg))
            }
            (Some(file), range) => {
                let path = file.path(self.db).to_string();
                let source = source_text(self.db, file);
                let title = self.diagnostic.id().to_string();
                let message = self.diagnostic.message();

                let Some(range) = range else {
                    let snippet = Snippet::source(source.as_str()).origin(&path).line_start(1);
                    return render(f, level.title(&title).snippet(snippet));
                };

                // The bits below are a simplified copy from
                // `crates/ruff_linter/src/message/text.rs`.
                let index = line_index(self.db, file);
                let source_code = SourceCode::new(source.as_str(), &index);

                let content_start_index = source_code.line_index(range.start());
                let mut start_index = content_start_index.saturating_sub(2);
                // Trim leading empty lines.
                while start_index < content_start_index {
                    if !source_code.line_text(start_index).trim().is_empty() {
                        break;
                    }
                    start_index = start_index.saturating_add(1);
                }

                let content_end_index = source_code.line_index(range.end());
                let mut end_index = content_end_index
                    .saturating_add(2)
                    .min(OneIndexed::from_zero_indexed(index.line_count()));
                // Trim trailing empty lines.
                while end_index > content_end_index {
                    if !source_code.line_text(end_index).trim().is_empty() {
                        break;
                    }
                    end_index = end_index.saturating_sub(1);
                }

                // Slice up the code frame and adjust our range.
                let start_offset = source_code.line_start(start_index);
                let end_offset = source_code.line_end(end_index);
                let frame = source_code.slice(TextRange::new(start_offset, end_offset));
                let span = range - start_offset;

                let annotation = level.span(span.into()).label(&message);
                let snippet = Snippet::source(frame)
                    .origin(&path)
                    .line_start(start_index.get())
                    .annotation(annotation);
                render(f, level.title(&title).snippet(snippet))
            }
        }
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

impl Diagnostic for &'_ dyn Diagnostic {
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

impl Diagnostic for std::sync::Arc<dyn Diagnostic> {
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
        DiagnosticId::InvalidSyntax(None)
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

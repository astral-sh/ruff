use std::borrow::Cow;
use std::fmt::Formatter;

use thiserror::Error;

use ruff_annotate_snippets::{
    Annotation as AnnotateAnnotation, Level as AnnotateLevel, Message as AnnotateMessage,
    Renderer as AnnotateRenderer, Snippet as AnnotateSnippet,
};
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

    /// The primary span of the diagnostic.
    ///
    /// The range can be `None` if the diagnostic doesn't have a file
    /// or it applies to the entire file (e.g. the file should be executable but isn't).
    fn span(&self) -> Option<Span>;

    /// Returns an optional sequence of "secondary" messages (with spans) to
    /// include in the rendering of this diagnostic.
    fn secondary_messages(&self) -> &[SecondaryDiagnosticMessage] {
        &[]
    }

    fn severity(&self) -> Severity;

    fn display<'db, 'diag, 'config>(
        &'diag self,
        db: &'db dyn Db,
        config: &'config DisplayDiagnosticConfig,
    ) -> DisplayDiagnostic<'db, 'diag, 'config>
    where
        Self: Sized,
    {
        DisplayDiagnostic {
            db,
            diagnostic: self,
            config,
        }
    }
}

/// A single secondary message assigned to a `Diagnostic`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecondaryDiagnosticMessage {
    span: Span,
    message: String,
}

impl SecondaryDiagnosticMessage {
    pub fn new(span: Span, message: impl Into<String>) -> SecondaryDiagnosticMessage {
        SecondaryDiagnosticMessage {
            span,
            message: message.into(),
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

pub struct DisplayDiagnostic<'db, 'diag, 'config> {
    db: &'db dyn Db,
    diagnostic: &'diag dyn Diagnostic,
    config: &'config DisplayDiagnosticConfig,
}

impl std::fmt::Display for DisplayDiagnostic<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let render = |f: &mut std::fmt::Formatter, message| {
            let renderer = if self.config.color {
                AnnotateRenderer::styled()
            } else {
                AnnotateRenderer::plain()
            }
            .cut_indicator("…");
            let rendered = renderer.render(message);
            writeln!(f, "{rendered}")
        };
        let Some(span) = self.diagnostic.span() else {
            // NOTE: This is pretty sub-optimal. It doesn't render well. We
            // really want a snippet, but without a `File`, we can't really
            // render anything. It looks like this case currently happens
            // for configuration errors. It looks like we can probably
            // produce a snippet for this if it comes from a file, but if
            // it comes from the CLI, I'm not quite sure exactly what to
            // do. ---AG
            let msg = format!("{}: {}", self.diagnostic.id(), self.diagnostic.message());
            return render(f, self.diagnostic.severity().to_annotate().title(&msg));
        };

        let mut message = Message::new(self.diagnostic.severity(), self.diagnostic.id());
        message.add_snippet(Snippet::new(
            self.db,
            self.diagnostic.severity(),
            &span,
            &self.diagnostic.message(),
        ));
        for secondary_msg in self.diagnostic.secondary_messages() {
            message.add_snippet(Snippet::new(
                self.db,
                Severity::Info,
                &secondary_msg.span,
                &secondary_msg.message,
            ));
        }
        render(f, message.to_annotate())
    }
}

#[derive(Debug)]
struct Message {
    level: AnnotateLevel,
    title: String,
    snippets: Vec<Snippet>,
}

#[derive(Debug)]
struct Snippet {
    source: String,
    origin: String,
    line_start: usize,
    annotation: Option<Annotation>,
}

#[derive(Debug)]
struct Annotation {
    level: AnnotateLevel,
    span: TextRange,
    label: String,
}

impl Message {
    fn new(severity: Severity, id: DiagnosticId) -> Message {
        Message {
            level: severity.to_annotate(),
            title: id.to_string(),
            snippets: vec![],
        }
    }

    fn add_snippet(&mut self, snippet: Snippet) {
        self.snippets.push(snippet);
    }

    fn to_annotate(&self) -> AnnotateMessage<'_> {
        self.level
            .title(&self.title)
            .snippets(self.snippets.iter().map(|snippet| snippet.to_annotate()))
    }
}

impl Snippet {
    fn new(db: &'_ dyn Db, severity: Severity, span: &Span, message: &str) -> Snippet {
        let origin = span.file.path(db).to_string();
        let source_text = source_text(db, span.file);
        let Some(range) = span.range else {
            return Snippet {
                source: source_text.to_string(),
                origin,
                line_start: 1,
                annotation: None,
            };
        };

        // The bits below are a simplified copy from
        // `crates/ruff_linter/src/message/text.rs`.
        let index = line_index(db, span.file);
        let source_code = SourceCode::new(source_text.as_str(), &index);

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
        let range = range - start_offset;

        Snippet {
            source: frame.to_string(),
            origin,
            line_start: start_index.get(),
            annotation: Some(Annotation {
                level: severity.to_annotate(),
                span: range,
                label: message.to_string(),
            }),
        }
    }

    fn to_annotate(&self) -> AnnotateSnippet<'_> {
        AnnotateSnippet::source(&self.source)
            .origin(&self.origin)
            .line_start(self.line_start)
            .annotations(self.annotation.as_ref().map(|a| a.to_annotate()))
    }
}

impl Annotation {
    fn to_annotate(&self) -> AnnotateAnnotation<'_> {
        self.level.span(self.span.into()).label(&self.label)
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

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[SecondaryDiagnosticMessage] {
        (**self).secondary_messages()
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

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[SecondaryDiagnosticMessage] {
        (**self).secondary_messages()
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

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[SecondaryDiagnosticMessage] {
        (**self).secondary_messages()
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

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[SecondaryDiagnosticMessage] {
        (**self).secondary_messages()
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

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[SecondaryDiagnosticMessage] {
        (**self).secondary_messages()
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

    fn span(&self) -> Option<Span> {
        Some(Span::from(self.file).with_range(self.error.location))
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }
}

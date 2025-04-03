use std::{fmt::Formatter, sync::Arc};

use thiserror::Error;

use ruff_annotate_snippets::Level as AnnotateLevel;
use ruff_text_size::TextRange;

pub use self::render::DisplayDiagnostic;
pub use crate::diagnostic::old::OldSecondaryDiagnosticMessage;
use crate::files::File;
use crate::Db;

use self::render::FileResolver;

// This module should not be exported. We are planning to migrate off
// the APIs in this module.
mod old;
mod render;

/// A collection of information that can be rendered into a diagnostic.
///
/// A diagnostic is a collection of information gathered by a tool intended
/// for presentation to an end user, and which describes a group of related
/// characteristics in the inputs given to the tool. Typically, but not always,
/// a characteristic is a deficiency. An example of a characteristic that is
/// _not_ a deficiency is the `reveal_type` diagnostic for our type checker.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
    /// The actual diagnostic.
    ///
    /// We box the diagnostic since it is somewhat big.
    inner: Arc<DiagnosticInner>,
}

impl Diagnostic {
    /// Create a new diagnostic with the given identifier, severity and
    /// message.
    ///
    /// The identifier should be something that uniquely identifies the _type_
    /// of diagnostic being reported. It should be usable as a reference point
    /// for humans communicating about diagnostic categories. It will also
    /// appear in the output when this diagnostic is rendered.
    ///
    /// The severity should describe the assumed level of importance to an end
    /// user.
    ///
    /// The message is meant to be read by end users. The primary message
    /// is meant to be a single terse description (usually a short phrase)
    /// describing the group of related characteristics that the diagnostic
    /// describes. Stated differently, if only one thing from a diagnostic can
    /// be shown to an end user in a particular context, it is the primary
    /// message.
    pub fn new<'a>(
        id: DiagnosticId,
        severity: Severity,
        message: impl std::fmt::Display + 'a,
    ) -> Diagnostic {
        let message = message.to_string().into_boxed_str();
        let inner = Arc::new(DiagnosticInner {
            id,
            severity,
            message,
            annotations: vec![],
            subs: vec![],
        });
        Diagnostic { inner }
    }

    /// Add an annotation to this diagnostic.
    ///
    /// Annotations for a diagnostic are optional, but if any are added,
    /// callers should strive to make at least one of them primary. That is, it
    /// should be constructed via [`Annotation::primary`]. A diagnostic with no
    /// primary annotations is allowed, but its rendering may be sub-optimal.
    pub fn annotate(&mut self, ann: Annotation) {
        Arc::make_mut(&mut self.inner).annotations.push(ann);
    }

    /// Adds an "info" sub-diagnostic with the given message.
    ///
    /// If callers want to add an "info" sub-diagnostic with annotations, then
    /// create a [`SubDiagnostic`] manually and use [`Diagnostic::sub`] to
    /// attach it to a parent diagnostic.
    ///
    /// An "info" diagnostic is useful when contextualizing or otherwise
    /// helpful information can be added to help end users understand the
    /// main diagnostic message better. For example, if a the main diagnostic
    /// message is about a function call being invalid, a useful "info"
    /// sub-diagnostic could show the function definition (or only the relevant
    /// parts of it).
    pub fn info<'a>(&mut self, message: impl std::fmt::Display + 'a) {
        self.sub(SubDiagnostic::new(Severity::Info, message));
    }

    /// Adds a "sub" diagnostic to this diagnostic.
    ///
    /// This is useful when a sub diagnostic has its own annotations attached
    /// to it. For the simpler case of a sub-diagnostic with only a message,
    /// using a method like [`Diagnostic::info`] may be more convenient.
    pub fn sub(&mut self, sub: SubDiagnostic) {
        Arc::make_mut(&mut self.inner).subs.push(sub);
    }

    /// Return a `std::fmt::Display` implementation that renders this
    /// diagnostic into a human readable format.
    ///
    /// Note that this `Display` impl includes a trailing line terminator, so
    /// callers should prefer using this with `write!` instead of `writeln!`.
    pub fn display<'a>(
        &'a self,
        db: &'a dyn Db,
        config: &'a DisplayDiagnosticConfig,
    ) -> DisplayDiagnostic<'a> {
        let resolver = FileResolver::new(db);
        DisplayDiagnostic::new(resolver, config, self)
    }

    /// Returns the identifier for this diagnostic.
    pub fn id(&self) -> DiagnosticId {
        self.inner.id
    }

    /// Returns the primary message for this diagnostic.
    ///
    /// A diagnostic always has a message, but it may be empty.
    pub fn primary_message(&self) -> &str {
        if !self.inner.message.is_empty() {
            return &self.inner.message;
        }
        // FIXME: As a special case, while we're migrating Red Knot
        // to the new diagnostic data model, we'll look for a primary
        // message from the primary annotation. This is because most
        // Red Knot diagnostics are created with an empty diagnostic
        // message and instead attach the message to the annotation.
        // Fixing this will require touching basically every diagnostic
        // in Red Knot, so we do it this way for now to match the old
        // semantics. ---AG
        self.primary_annotation()
            .and_then(|ann| ann.message.as_deref())
            .unwrap_or_default()
    }

    /// Returns the severity of this diagnostic.
    ///
    /// Note that this may be different than the severity of sub-diagnostics.
    pub fn severity(&self) -> Severity {
        self.inner.severity
    }

    /// Returns the "primary" annotation of this diagnostic if one exists.
    ///
    /// When there are multiple primary annotation, then the first one that was
    /// added to this diagnostic is returned.
    pub fn primary_annotation(&self) -> Option<&Annotation> {
        self.inner.annotations.iter().find(|ann| ann.is_primary)
    }

    /// Returns the "primary" span of this diagnostic if one exists.
    ///
    /// When there are multiple primary spans, then the first one that was
    /// added to this diagnostic is returned.
    pub fn primary_span(&self) -> Option<Span> {
        self.primary_annotation().map(|ann| ann.span.clone())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DiagnosticInner {
    id: DiagnosticId,
    severity: Severity,
    message: Box<str>,
    annotations: Vec<Annotation>,
    subs: Vec<SubDiagnostic>,
}

/// A collection of information subservient to a diagnostic.
///
/// A sub-diagnostic is always rendered after the parent diagnostic it is
/// attached to. A parent diagnostic may have many sub-diagnostics, and it is
/// guaranteed that they will not interleave with one another in rendering.
///
/// Currently, the order in which sub-diagnostics are rendered relative to one
/// another (for a single parent diagnostic) is the order in which they were
/// attached to the diagnostic.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SubDiagnostic {
    /// Like with `Diagnostic`, we box the `SubDiagnostic` to make it
    /// pointer-sized.
    inner: Box<SubDiagnosticInner>,
}

impl SubDiagnostic {
    /// Create a new sub-diagnostic with the given severity and message.
    ///
    /// The severity should describe the assumed level of importance to an end
    /// user.
    ///
    /// The message is meant to be read by end users. The primary message
    /// is meant to be a single terse description (usually a short phrase)
    /// describing the group of related characteristics that the sub-diagnostic
    /// describes. Stated differently, if only one thing from a diagnostic can
    /// be shown to an end user in a particular context, it is the primary
    /// message.
    pub fn new<'a>(severity: Severity, message: impl std::fmt::Display + 'a) -> SubDiagnostic {
        let message = message.to_string().into_boxed_str();
        let inner = Box::new(SubDiagnosticInner {
            severity,
            message,
            annotations: vec![],
        });
        SubDiagnostic { inner }
    }

    /// Add an annotation to this sub-diagnostic.
    ///
    /// Annotations for a sub-diagnostic, like for a diagnostic, are optional.
    /// If any are added, callers should strive to make at least one of them
    /// primary. That is, it should be constructed via [`Annotation::primary`].
    /// A diagnostic with no primary annotations is allowed, but its rendering
    /// may be sub-optimal.
    ///
    /// Note that it is expected to be somewhat more common for sub-diagnostics
    /// to have no annotations (e.g., a simple note) than for a diagnostic to
    /// have no annotations.
    pub fn annotate(&mut self, ann: Annotation) {
        self.inner.annotations.push(ann);
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SubDiagnosticInner {
    severity: Severity,
    message: Box<str>,
    annotations: Vec<Annotation>,
}

/// A pointer to a subsequence in the end user's input.
///
/// Also known as an annotation, the pointer can optionally contain a short
/// message, typically describing in general terms what is being pointed to.
///
/// An annotation is either primary or secondary, depending on whether it was
/// constructed via [`Annotation::primary`] or [`Annotation::secondary`].
/// Semantically, a primary annotation is meant to point to the "locus" of a
/// diagnostic. Visually, the difference between a primary and a secondary
/// annotation is usually just a different form of highlighting on the
/// corresponding span.
///
/// # Advice
///
/// The span on an annotation should be as _specific_ as possible. For example,
/// if there is a problem with a function call because one of its arguments has
/// an invalid type, then the span should point to the specific argument and
/// not to the entire function call.
///
/// Messages attached to annotations should also be as brief and specific as
/// possible. Long messages could negative impact the quality of rendering.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Annotation {
    /// The span of this annotation, corresponding to some subsequence of the
    /// user's input that we want to highlight.
    span: Span,
    /// An optional message associated with this annotation's span.
    ///
    /// When present, rendering will include this message in the output and
    /// draw a line between the highlighted span and the message.
    message: Option<Box<str>>,
    /// Whether this annotation is "primary" or not. When it isn't primary, an
    /// annotation is said to be "secondary."
    is_primary: bool,
}

impl Annotation {
    /// Create a "primary" annotation.
    ///
    /// A primary annotation is meant to highlight the "locus" of a diagnostic.
    /// That is, it should point to something in the end user's input that is
    /// the subject or "point" of a diagnostic.
    ///
    /// A diagnostic may have many primary annotations. A diagnostic may not
    /// have any annotations, but if it does, at least one _ought_ to be
    /// primary.
    pub fn primary(span: Span) -> Annotation {
        Annotation {
            span,
            message: None,
            is_primary: true,
        }
    }

    /// Create a "secondary" annotation.
    ///
    /// A secondary annotation is meant to highlight relevant context for a
    /// diagnostic, but not to point to the "locus" of the diagnostic.
    ///
    /// A diagnostic with only secondary annotations is usually not sensible,
    /// but it is allowed and will produce a reasonable rendering.
    pub fn secondary(span: Span) -> Annotation {
        Annotation {
            span,
            message: None,
            is_primary: false,
        }
    }

    /// Attach a message to this annotation.
    ///
    /// An annotation without a message will still have a presence in
    /// rendering. In particular, it will highlight the span association with
    /// this annotation in some way.
    ///
    /// When a message is attached to an annotation, then it will be associated
    /// with the highlighted span in some way during rendering.
    pub fn message<'a>(self, message: impl std::fmt::Display + 'a) -> Annotation {
        let message = Some(message.to_string().into_boxed_str());
        Annotation { message, ..self }
    }
}

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
#[derive(Clone, Debug)]
pub struct DisplayDiagnosticConfig {
    /// The format to use for diagnostic rendering.
    ///
    /// This uses the "full" format by default.
    format: DiagnosticFormat,
    /// Whether to enable colors or not.
    ///
    /// Disabled by default.
    color: bool,
    /// The number of non-empty lines to show around each snippet.
    ///
    /// NOTE: It seems like making this a property of rendering *could*
    /// be wrong. In particular, I have a suspicion that we may want
    /// more granular control over this, perhaps based on the kind of
    /// diagnostic or even the snippet itself. But I chose to put this
    /// here for now as the most "sensible" place for it to live until
    /// we had more concrete use cases. ---AG
    context: usize,
}

impl DisplayDiagnosticConfig {
    /// Whether to enable concise diagnostic output or not.
    pub fn format(self, format: DiagnosticFormat) -> DisplayDiagnosticConfig {
        DisplayDiagnosticConfig { format, ..self }
    }

    /// Whether to enable colors or not.
    pub fn color(self, yes: bool) -> DisplayDiagnosticConfig {
        DisplayDiagnosticConfig { color: yes, ..self }
    }

    /// Set the number of contextual lines to show around each snippet.
    pub fn context(self, lines: usize) -> DisplayDiagnosticConfig {
        DisplayDiagnosticConfig {
            context: lines,
            ..self
        }
    }
}

impl Default for DisplayDiagnosticConfig {
    fn default() -> DisplayDiagnosticConfig {
        DisplayDiagnosticConfig {
            format: DiagnosticFormat::default(),
            color: false,
            context: 2,
        }
    }
}

/// The diagnostic output format.
#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DiagnosticFormat {
    /// The default full mode will print "pretty" diagnostics.
    ///
    /// That is, color will be used when printing to a `tty`.
    /// Moreover, diagnostic messages may include additional
    /// context and annotations on the input to help understand
    /// the message.
    #[default]
    Full,
    /// Print diagnostics in a concise mode.
    ///
    /// This will guarantee that each diagnostic is printed on
    /// a single line. Only the most important or primary aspects
    /// of the diagnostic are included. Contextual information is
    /// dropped.
    ///
    /// This may use color when printing to a `tty`.
    Concise,
}

/// Creates a `Diagnostic` from a parse error.
///
/// This should _probably_ be a method on `ruff_python_parser::ParseError`, but
/// at time of writing, `ruff_db` depends on `ruff_python_parser` instead of
/// the other way around. And since we want to do this conversion in a couple
/// places, it makes sense to centralize it _somewhere_. So it's here for now.
pub fn create_parse_diagnostic(file: File, err: &ruff_python_parser::ParseError) -> Diagnostic {
    let mut diag = Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Error, "");
    let span = Span::from(file).with_range(err.location);
    diag.annotate(Annotation::primary(span).message(&err.error));
    diag
}

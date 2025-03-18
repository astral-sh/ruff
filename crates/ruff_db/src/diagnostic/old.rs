use std::borrow::Cow;

use ruff_python_parser::ParseError;

use crate::{
    diagnostic::{
        Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig, Severity,
        Span, SubDiagnostic,
    },
    files::File,
    source::{line_index, source_text},
    Db,
};

pub trait OldDiagnosticTrait: Send + Sync + std::fmt::Debug {
    fn id(&self) -> DiagnosticId;

    fn message(&self) -> Cow<str>;

    /// The primary span of the diagnostic.
    ///
    /// The range can be `None` if the diagnostic doesn't have a file
    /// or it applies to the entire file (e.g. the file should be executable but isn't).
    fn span(&self) -> Option<Span>;

    /// Returns an optional sequence of "secondary" messages (with spans) to
    /// include in the rendering of this diagnostic.
    fn secondary_messages(&self) -> &[OldSecondaryDiagnosticMessage] {
        &[]
    }

    fn severity(&self) -> Severity;

    fn display<'db, 'diag, 'config>(
        &'diag self,
        db: &'db dyn Db,
        config: &'config DisplayDiagnosticConfig,
    ) -> OldDisplayDiagnostic<'db, 'diag, 'config>
    where
        Self: Sized,
    {
        OldDisplayDiagnostic {
            db,
            diagnostic: self,
            config,
        }
    }
}

/// A single secondary message assigned to a `Diagnostic`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OldSecondaryDiagnosticMessage {
    span: Span,
    message: String,
}

impl OldSecondaryDiagnosticMessage {
    pub fn new(span: Span, message: impl Into<String>) -> OldSecondaryDiagnosticMessage {
        OldSecondaryDiagnosticMessage {
            span,
            message: message.into(),
        }
    }
}

pub struct OldDisplayDiagnostic<'db, 'diag, 'config> {
    db: &'db dyn Db,
    diagnostic: &'diag dyn OldDiagnosticTrait,
    config: &'config DisplayDiagnosticConfig,
}

impl std::fmt::Display for OldDisplayDiagnostic<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if matches!(self.config.format, DiagnosticFormat::Concise) {
            match self.diagnostic.severity() {
                Severity::Info => f.write_str("info")?,
                Severity::Warning => f.write_str("warning")?,
                Severity::Error => f.write_str("error")?,
                Severity::Fatal => f.write_str("fatal")?,
            }

            write!(f, "[{rule}]", rule = self.diagnostic.id())?;
            if let Some(span) = self.diagnostic.span() {
                write!(f, " {path}", path = span.file().path(self.db))?;
                if let Some(range) = span.range() {
                    let index = line_index(self.db, span.file());
                    let source = source_text(self.db, span.file());
                    let start = index.source_location(range.start(), &source);
                    write!(f, ":{line}:{col}", line = start.row, col = start.column)?;
                }
                write!(f, ":")?;
            }
            return write!(f, " {message}", message = self.diagnostic.message());
        }

        let mut diag = match self.diagnostic.span() {
            None => {
                // NOTE: This is pretty sub-optimal. It doesn't render well. We
                // really want a snippet, but without a `File`, we can't really
                // render anything. It looks like this case currently happens
                // for configuration errors. It looks like we can probably
                // produce a snippet for this if it comes from a file, but if
                // it comes from the CLI, I'm not quite sure exactly what to
                // do. ---AG
                Diagnostic::new(
                    self.diagnostic.id(),
                    self.diagnostic.severity(),
                    self.diagnostic.message(),
                )
            }
            Some(span) => {
                let mut diag =
                    Diagnostic::new(self.diagnostic.id(), self.diagnostic.severity(), "");
                diag.annotate(Annotation::primary(span).message(self.diagnostic.message()));
                diag
            }
        };
        for secondary_msg in self.diagnostic.secondary_messages() {
            // Secondary messages carry one span and a message
            // attached to that span. Since we also want them to
            // appear after the primary diagnostic, we encode them as
            // sub-diagnostics.
            //
            // Moreover, since we only have one message, we attach it
            // to the annotation and leave the sub-diagnostic message
            // empty. This leads to somewhat awkward rendering, but
            // the way to fix that is to migrate Red Knot to the more
            // expressive `Diagnostic` API.
            let mut sub = SubDiagnostic::new(Severity::Info, "");
            sub.annotate(
                Annotation::secondary(secondary_msg.span.clone()).message(&secondary_msg.message),
            );
            diag.sub(sub);
        }

        // The main way to print a `Diagnostic` is via its `print`
        // method, which specifically goes to a `std::io::Write` in
        // order to strongly suggest that it is actually emitted
        // somewhere. We should probably make callers use *that* API
        // instead of this `Display` impl, but for now, we keep the API
        // the same and write to a `Vec<u8>` under the covers.
        let mut buf = vec![];
        // OK because printing to a `Vec<u8>` will never return an error.
        diag.print(self.db, self.config, &mut buf)
            .map_err(|_| std::fmt::Error)?;
        // OK because diagnostic rendering will always emit valid UTF-8.
        let string = String::from_utf8(buf).map_err(|_| std::fmt::Error)?;
        write!(f, "{string}")
    }
}

impl<T> OldDiagnosticTrait for Box<T>
where
    T: OldDiagnosticTrait,
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

    fn secondary_messages(&self) -> &[OldSecondaryDiagnosticMessage] {
        (**self).secondary_messages()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl<T> OldDiagnosticTrait for std::sync::Arc<T>
where
    T: OldDiagnosticTrait,
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

    fn secondary_messages(&self) -> &[OldSecondaryDiagnosticMessage] {
        (**self).secondary_messages()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl OldDiagnosticTrait for Box<dyn OldDiagnosticTrait> {
    fn id(&self) -> DiagnosticId {
        (**self).id()
    }

    fn message(&self) -> Cow<str> {
        (**self).message()
    }

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[OldSecondaryDiagnosticMessage] {
        (**self).secondary_messages()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl OldDiagnosticTrait for &'_ dyn OldDiagnosticTrait {
    fn id(&self) -> DiagnosticId {
        (**self).id()
    }

    fn message(&self) -> Cow<str> {
        (**self).message()
    }

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[OldSecondaryDiagnosticMessage] {
        (**self).secondary_messages()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl OldDiagnosticTrait for std::sync::Arc<dyn OldDiagnosticTrait> {
    fn id(&self) -> DiagnosticId {
        (**self).id()
    }

    fn message(&self) -> Cow<str> {
        (**self).message()
    }

    fn span(&self) -> Option<Span> {
        (**self).span()
    }

    fn secondary_messages(&self) -> &[OldSecondaryDiagnosticMessage] {
        (**self).secondary_messages()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

#[derive(Debug)]
pub struct OldParseDiagnostic {
    file: File,
    error: ParseError,
}

impl OldParseDiagnostic {
    pub fn new(file: File, error: ParseError) -> Self {
        Self { file, error }
    }
}

impl OldDiagnosticTrait for OldParseDiagnostic {
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

use std::borrow::Cow;

use ruff_annotate_snippets::{
    Annotation as AnnotateAnnotation, Level as AnnotateLevel, Message as AnnotateMessage,
    Renderer as AnnotateRenderer, Snippet as AnnotateSnippet,
};
use ruff_source_file::{OneIndexed, SourceCode};
use ruff_text_size::TextRange;

use crate::{
    diagnostic::{DiagnosticId, DisplayDiagnosticConfig, Severity, Span},
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

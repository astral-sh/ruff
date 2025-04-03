use std::collections::BTreeMap;

use ruff_annotate_snippets::{
    Annotation as AnnotateAnnotation, Level as AnnotateLevel, Message as AnnotateMessage,
    Renderer as AnnotateRenderer, Snippet as AnnotateSnippet,
};
use ruff_source_file::{LineIndex, OneIndexed, SourceCode};
use ruff_text_size::{TextRange, TextSize};

use crate::{
    files::File,
    source::{line_index, source_text, SourceText},
    Db,
};

use super::{
    Annotation, Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig, Severity, SubDiagnostic,
};

/// A type that implements `std::fmt::Display` for diagnostic rendering.
///
/// It is created via [`Diagnostic::display`].
///
/// The lifetime parameter, `'a`, refers to the shorter of:
///
/// * The lifetime of the rendering configuration.
/// * The lifetime of the resolver used to load the contents of `Span`
///   values. When using Salsa, this most commonly corresponds to the lifetime
///   of a Salsa `Db`.
/// * The lifetime of the diagnostic being rendered.
#[derive(Debug)]
pub struct DisplayDiagnostic<'a> {
    config: &'a DisplayDiagnosticConfig,
    resolver: FileResolver<'a>,
    annotate_renderer: AnnotateRenderer,
    diag: &'a Diagnostic,
}

impl<'a> DisplayDiagnostic<'a> {
    pub(crate) fn new(
        resolver: FileResolver<'a>,
        config: &'a DisplayDiagnosticConfig,
        diag: &'a Diagnostic,
    ) -> DisplayDiagnostic<'a> {
        let annotate_renderer = if config.color {
            AnnotateRenderer::styled()
        } else {
            AnnotateRenderer::plain()
        };
        DisplayDiagnostic {
            config,
            resolver,
            annotate_renderer,
            diag,
        }
    }
}

impl std::fmt::Display for DisplayDiagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if matches!(self.config.format, DiagnosticFormat::Concise) {
            match self.diag.severity() {
                Severity::Info => f.write_str("info")?,
                Severity::Warning => f.write_str("warning")?,
                Severity::Error => f.write_str("error")?,
                Severity::Fatal => f.write_str("fatal")?,
            }

            write!(f, "[{rule}]", rule = self.diag.id())?;
            if let Some(span) = self.diag.primary_span() {
                write!(f, " {path}", path = self.resolver.path(span.file()))?;
                if let Some(range) = span.range() {
                    let input = self.resolver.input(span.file());
                    let start = input.as_source_code().source_location(range.start());
                    write!(f, ":{line}:{col}", line = start.row, col = start.column)?;
                }
                write!(f, ":")?;
            }
            return writeln!(f, " {message}", message = self.diag.primary_message());
        }

        let resolved = Resolved::new(&self.resolver, self.diag);
        let renderable = resolved.to_renderable(self.config.context);
        for diag in renderable.diagnostics.iter() {
            writeln!(f, "{}", self.annotate_renderer.render(diag.to_annotate()))?;
        }
        writeln!(f)
    }
}

/// A sequence of resolved diagnostics.
///
/// Resolving a diagnostic refers to the process of restructuring its internal
/// data in a way that enables rendering decisions. For example, a `Span`
/// on an `Annotation` in a `Diagnostic` is intentionally very minimal, and
/// thus doesn't have information like line numbers or even the actual file
/// path. Resolution retrieves this information and puts it into a structured
/// representation specifically intended for diagnostic rendering.
///
/// The lifetime `'a` refers to the shorter of the lifetimes between the file
/// resolver and the diagnostic itself. (The resolved types borrow data from
/// both.)
#[derive(Debug)]
struct Resolved<'a> {
    id: String,
    diagnostics: Vec<ResolvedDiagnostic<'a>>,
}

impl<'a> Resolved<'a> {
    /// Creates a new resolved set of diagnostics.
    fn new(resolver: &FileResolver<'a>, diag: &'a Diagnostic) -> Resolved<'a> {
        let mut diagnostics = vec![];
        diagnostics.push(ResolvedDiagnostic::from_diagnostic(resolver, diag));
        for sub in &diag.inner.subs {
            diagnostics.push(ResolvedDiagnostic::from_sub_diagnostic(resolver, sub));
        }
        let id = diag.inner.id.to_string();
        Resolved { id, diagnostics }
    }

    /// Creates a value that is amenable to rendering directly.
    fn to_renderable(&self, context: usize) -> Renderable<'_> {
        Renderable {
            id: &self.id,
            diagnostics: self
                .diagnostics
                .iter()
                .map(|diag| diag.to_renderable(context))
                .collect(),
        }
    }
}

/// A single resolved diagnostic.
///
/// The lifetime `'a` refers to the shorter of the lifetimes between the file
/// resolver and the diagnostic itself. (The resolved types borrow data from
/// both.)
#[derive(Debug)]
struct ResolvedDiagnostic<'a> {
    severity: Severity,
    message: String,
    annotations: Vec<ResolvedAnnotation<'a>>,
}

impl<'a> ResolvedDiagnostic<'a> {
    /// Resolve a single diagnostic.
    fn from_diagnostic(
        resolver: &FileResolver<'a>,
        diag: &'a Diagnostic,
    ) -> ResolvedDiagnostic<'a> {
        let annotations: Vec<_> = diag
            .inner
            .annotations
            .iter()
            .filter_map(|ann| {
                let path = resolver.path(ann.span.file);
                let input = resolver.input(ann.span.file);
                ResolvedAnnotation::new(path, &input, ann)
            })
            .collect();
        let message = if diag.inner.message.is_empty() {
            diag.inner.id.to_string()
        } else {
            // TODO: See the comment on `Renderable::id` for
            // a plausible better idea than smushing the ID
            // into the diagnostic message.
            format!(
                "{id}: {message}",
                id = diag.inner.id,
                message = diag.inner.message
            )
        };
        ResolvedDiagnostic {
            severity: diag.inner.severity,
            message,
            annotations,
        }
    }

    /// Resolve a single sub-diagnostic.
    fn from_sub_diagnostic(
        resolver: &FileResolver<'a>,
        diag: &'a SubDiagnostic,
    ) -> ResolvedDiagnostic<'a> {
        let annotations: Vec<_> = diag
            .inner
            .annotations
            .iter()
            .filter_map(|ann| {
                let path = resolver.path(ann.span.file);
                let input = resolver.input(ann.span.file);
                ResolvedAnnotation::new(path, &input, ann)
            })
            .collect();
        ResolvedDiagnostic {
            severity: diag.inner.severity,
            message: diag.inner.message.to_string(),
            annotations,
        }
    }

    /// Create a diagnostic amenable for rendering.
    ///
    /// `context` refers to the number of lines both before and after to show
    /// for each snippet.
    fn to_renderable<'r>(&'r self, context: usize) -> RenderableDiagnostic<'r> {
        let mut ann_by_path: BTreeMap<&'a str, Vec<&ResolvedAnnotation<'a>>> = BTreeMap::new();
        for ann in &self.annotations {
            ann_by_path.entry(ann.path).or_default().push(ann);
        }
        for anns in ann_by_path.values_mut() {
            anns.sort_by_key(|ann1| ann1.range.start());
        }

        let mut snippet_by_path: BTreeMap<&'a str, Vec<Vec<&ResolvedAnnotation<'a>>>> =
            BTreeMap::new();
        for (path, anns) in ann_by_path {
            let mut snippet = vec![];
            for ann in anns {
                let Some(prev) = snippet.last() else {
                    snippet.push(ann);
                    continue;
                };

                let prev_context_ends =
                    context_after(&prev.input.as_source_code(), context, prev.line_end).get();
                let this_context_begins =
                    context_before(&ann.input.as_source_code(), context, ann.line_start).get();
                // The boundary case here is when `prev_context_ends`
                // is exactly one less than `this_context_begins`. In
                // that case, the context windows are adajcent and we
                // should fall through below to add this annotation to
                // the existing snippet.
                if this_context_begins.saturating_sub(prev_context_ends) > 1 {
                    snippet_by_path
                        .entry(path)
                        .or_default()
                        .push(std::mem::take(&mut snippet));
                }
                snippet.push(ann);
            }
            if !snippet.is_empty() {
                snippet_by_path.entry(path).or_default().push(snippet);
            }
        }

        let mut snippets_by_input = vec![];
        for (path, snippets) in snippet_by_path {
            snippets_by_input.push(RenderableSnippets::new(context, path, &snippets));
        }
        snippets_by_input
            .sort_by(|snips1, snips2| snips1.has_primary.cmp(&snips2.has_primary).reverse());
        RenderableDiagnostic {
            severity: self.severity,
            message: &self.message,
            snippets_by_input,
        }
    }
}

/// A resolved annotation with information needed for rendering.
///
/// For example, this annotation has the corresponding file path, entire
/// source code and the line numbers corresponding to its range in the source
/// code. This information can be used to create renderable data and also
/// sort/organize the annotations into snippets.
#[derive(Debug)]
struct ResolvedAnnotation<'a> {
    path: &'a str,
    input: Input,
    range: TextRange,
    line_start: OneIndexed,
    line_end: OneIndexed,
    message: Option<&'a str>,
    is_primary: bool,
}

impl<'a> ResolvedAnnotation<'a> {
    /// Resolve an annotation.
    ///
    /// `path` is the path of the file that this annotation points to.
    ///
    /// `input` is the contents of the file that this annotation points to.
    fn new(path: &'a str, input: &Input, ann: &'a Annotation) -> Option<ResolvedAnnotation<'a>> {
        let source = input.as_source_code();
        let (range, line_start, line_end) = match (ann.span.range(), ann.message.is_some()) {
            // An annotation with no range AND no message is probably(?)
            // meaningless, so just ignore it.
            (None, false) => return None,
            (None, _) => (
                TextRange::empty(TextSize::new(0)),
                OneIndexed::MIN,
                OneIndexed::MIN,
            ),
            (Some(range), _) => {
                let line_start = source.line_index(range.start());
                let mut line_end = source.line_index(range.end());
                // As a special case, if the *end* of our range comes
                // right after a line terminator, we say that the last
                // line number for this annotation is the previous
                // line and not the next line. In other words, in this
                // case, we treat our line number as an inclusive
                // upper bound.
                if source.slice(range).ends_with(['\r', '\n']) {
                    line_end = line_end.saturating_sub(1).max(line_start);
                }
                (range, line_start, line_end)
            }
        };
        Some(ResolvedAnnotation {
            path,
            input: input.clone(),
            range,
            line_start,
            line_end,
            message: ann.message.as_deref(),
            is_primary: ann.is_primary,
        })
    }
}

/// A single unit of rendering consisting of one or more diagnostics.
///
/// There is always exactly one "main" diagnostic that comes first, followed by
/// zero or more sub-diagnostics.
///
/// The lifetime parameter `'r` refers to the lifetime of whatever created this
/// renderable value. This is usually the lifetime of `Resolved`.
#[derive(Debug)]
struct Renderable<'r> {
    // TODO: This is currently unused in the rendering logic below. I'm not
    // 100% sure yet where I want to put it, but I like what `rustc` does:
    //
    //     error[E0599]: no method named `sub_builder` <..snip..>
    //
    // I believe in order to do this, we'll need to patch it in to
    // `ruff_annotate_snippets` though. We leave it here for now with that plan
    // in mind.
    //
    // (At time of writing, 2025-03-13, we currently render the diagnostic
    // ID into the main message of the parent diagnostic. We don't use this
    // specific field to do that though.)
    #[allow(dead_code)]
    id: &'r str,
    diagnostics: Vec<RenderableDiagnostic<'r>>,
}

/// A single diagnostic amenable to rendering.
#[derive(Debug)]
struct RenderableDiagnostic<'r> {
    /// The severity of the diagnostic.
    severity: Severity,
    /// The message emitted with the diagnostic, before any snippets are
    /// rendered.
    message: &'r str,
    /// A collection of collections of snippets. Each collection of snippets
    /// should be from the same file, and none of the snippets inside of a
    /// collection should overlap with one another or be directly adjacent.
    snippets_by_input: Vec<RenderableSnippets<'r>>,
}

impl RenderableDiagnostic<'_> {
    /// Convert this to an "annotate" snippet.
    fn to_annotate(&self) -> AnnotateMessage<'_> {
        let level = self.severity.to_annotate();
        let snippets = self.snippets_by_input.iter().flat_map(|snippets| {
            let path = snippets.path;
            snippets
                .snippets
                .iter()
                .map(|snippet| snippet.to_annotate(path))
        });
        level.title(self.message).snippets(snippets)
    }
}

/// A collection of renderable snippets for a single file.
#[derive(Debug)]
struct RenderableSnippets<'r> {
    /// The path to the file from which all snippets originate from.
    path: &'r str,
    /// The snippets, the in order of desired rendering.
    snippets: Vec<RenderableSnippet<'r>>,
    /// Whether this contains any snippets with any annotations marked
    /// as primary. This is useful for re-sorting snippets such that
    /// the ones with primary annotations are rendered first.
    has_primary: bool,
}

impl<'r> RenderableSnippets<'r> {
    /// Creates a new collection of renderable snippets.
    ///
    /// `context` is the number of lines to include before and after each
    /// snippet.
    ///
    /// `path` is the file path containing the given snippets. (They should all
    /// come from the same file path.)
    ///
    /// The lifetime parameter `'r` refers to the lifetime of the resolved
    /// annotation given (since the renderable snippet returned borrows from
    /// the resolved annotation's `Input`). This is no longer than the lifetime
    /// of the resolver that produced the resolved annotation.
    ///
    /// # Panics
    ///
    /// When `resolved_snippets.is_empty()`.
    fn new<'a>(
        context: usize,
        path: &'r str,
        resolved_snippets: &'a [Vec<&'r ResolvedAnnotation<'r>>],
    ) -> RenderableSnippets<'r> {
        assert!(!resolved_snippets.is_empty());

        let mut has_primary = false;
        let mut snippets = vec![];
        for anns in resolved_snippets {
            let snippet = RenderableSnippet::new(context, anns);
            has_primary = has_primary || snippet.has_primary;
            snippets.push(snippet);
        }
        snippets.sort_by(|s1, s2| s1.has_primary.cmp(&s2.has_primary).reverse());
        RenderableSnippets {
            path,
            snippets,
            has_primary,
        }
    }
}

/// A single snippet of code that is rendered as part of a diagnostic message.
///
/// The intent is that a snippet for one diagnostic does not overlap (or is
/// even directly adjacent to) any other snippets for that same diagnostic.
/// Callers creating a `RenderableSnippet` should enforce this guarantee by
/// grouping annotations according to the lines on which they start and stop.
///
/// Snippets from different diagnostics (including sub-diagnostics) may
/// overlap.
#[derive(Debug)]
struct RenderableSnippet<'r> {
    /// The actual snippet text.
    snippet: &'r str,
    /// The absolute line number corresponding to where this
    /// snippet begins.
    line_start: OneIndexed,
    /// A non-zero number of annotations on this snippet.
    annotations: Vec<RenderableAnnotation<'r>>,
    /// Whether this snippet contains at least one primary
    /// annotation.
    has_primary: bool,
}

impl<'r> RenderableSnippet<'r> {
    /// Creates a new snippet with one or more annotations that is ready to be
    /// renderer.
    ///
    /// The first line of the snippet is the smallest line number on which one
    /// of the annotations begins, minus the context window size. The last line
    /// is the largest line number on which one of the annotations ends, plus
    /// the context window size.
    ///
    /// Callers should guarantee that the `input` on every `ResolvedAnnotation`
    /// given is identical.
    ///
    /// The lifetime of the snippet returned is only tied to the lifetime of
    /// the borrowed resolved annotation given (which is no longer than the
    /// lifetime of the resolver that produced the resolved annotation).
    ///
    /// # Panics
    ///
    /// When `anns.is_empty()`.
    fn new<'a>(context: usize, anns: &'a [&'r ResolvedAnnotation<'r>]) -> RenderableSnippet<'r> {
        assert!(
            !anns.is_empty(),
            "creating a renderable snippet requires a non-zero number of annotations",
        );
        let input = &anns[0].input;
        let source = input.as_source_code();
        let has_primary = anns.iter().any(|ann| ann.is_primary);

        let line_start = context_before(
            &source,
            context,
            anns.iter().map(|ann| ann.line_start).min().unwrap(),
        );
        let line_end = context_after(
            &source,
            context,
            anns.iter().map(|ann| ann.line_end).max().unwrap(),
        );

        let snippet_start = source.line_start(line_start);
        let snippet_end = source.line_end(line_end);
        let snippet = input
            .as_source_code()
            .slice(TextRange::new(snippet_start, snippet_end));

        let annotations = anns
            .iter()
            .map(|ann| RenderableAnnotation::new(snippet_start, ann))
            .collect();
        RenderableSnippet {
            snippet,
            line_start,
            annotations,
            has_primary,
        }
    }

    /// Convert this to an "annotate" snippet.
    fn to_annotate<'a>(&'a self, path: &'a str) -> AnnotateSnippet<'a> {
        AnnotateSnippet::source(self.snippet)
            .origin(path)
            .line_start(self.line_start.get())
            .annotations(
                self.annotations
                    .iter()
                    .map(RenderableAnnotation::to_annotate),
            )
    }
}

/// A single annotation represented in a way that is amenable to rendering.
#[derive(Debug)]
struct RenderableAnnotation<'r> {
    /// The range of the annotation relative to the snippet
    /// it points to. This is *not* the absolute range in the
    /// corresponding file.
    range: TextRange,
    /// An optional message or label associated with this annotation.
    message: Option<&'r str>,
    /// Whether this annotation is considered "primary" or not.
    is_primary: bool,
}

impl<'r> RenderableAnnotation<'r> {
    /// Create a new renderable annotation.
    ///
    /// `snippet_start` should be the absolute offset at which the snippet
    /// pointing to by the given annotation begins.
    ///
    /// The lifetime of the resolved annotation does not matter. The `'r`
    /// lifetime parameter here refers to the lifetime of the resolver that
    /// created the given `ResolvedAnnotation`.
    fn new(snippet_start: TextSize, ann: &'_ ResolvedAnnotation<'r>) -> RenderableAnnotation<'r> {
        let range = ann.range - snippet_start;
        RenderableAnnotation {
            range,
            message: ann.message,
            is_primary: ann.is_primary,
        }
    }

    /// Convert this to an "annotate" annotation.
    fn to_annotate(&self) -> AnnotateAnnotation<'_> {
        // This is not really semantically meaningful, but
        // it does currently result in roughly the message
        // we want to convey.
        //
        // TODO: While this means primary annotations use `^` and
        // secondary annotations use `-` (which is fine), this does
        // result in coloring for primary annotations that looks like
        // an error (red) and coloring for secondary annotations that
        // looks like a warning (yellow). This is perhaps not quite in
        // line with what we want, but fixing this probably requires
        // changes to `ruff_annotate_snippets`, so we punt for now.
        let level = if self.is_primary {
            AnnotateLevel::Error
        } else {
            AnnotateLevel::Warning
        };
        let mut ann = level.span(self.range.into());
        if let Some(message) = self.message {
            ann = ann.label(message);
        }
        ann
    }
}

/// A type that facilitates the retrieval of source code from a `Span`.
///
/// At present, this is tightly coupled with a Salsa database. In the future,
/// it is intended for this resolver to become an abstraction providing a
/// similar API. We define things this way for now to keep the Salsa coupling
/// at "arm's" length, and to make it easier to do the actual de-coupling in
/// the future.
///
/// For example, at time of writing (2025-03-07), the plan is (roughly) for
/// Ruff to grow its own interner of file paths so that a `Span` can store an
/// interned ID instead of a (roughly) `Arc<Path>`. This interner is planned
/// to be entirely separate from the Salsa interner used by Red Knot, and so,
/// callers will need to pass in a different "resolver" for turning `Span`s
/// into actual file paths/contents. The infrastructure for this isn't fully in
/// place, but this type serves to demarcate the intended abstraction boundary.
pub(crate) struct FileResolver<'a> {
    db: &'a dyn Db,
}

impl<'a> FileResolver<'a> {
    /// Creates a new resolver from a Salsa database.
    pub(crate) fn new(db: &'a dyn Db) -> FileResolver<'a> {
        FileResolver { db }
    }

    /// Returns the path associated with the file given.
    fn path(&self, file: File) -> &'a str {
        file.path(self.db).as_str()
    }

    /// Returns the input contents associated with the file given.
    fn input(&self, file: File) -> Input {
        Input {
            text: source_text(self.db, file),
            line_index: line_index(self.db, file),
        }
    }
}

impl std::fmt::Debug for FileResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "<salsa based file resolver>")
    }
}

/// An abstraction over a unit of user input.
///
/// A single unit of user input usually corresponds to a `File`.
/// This contains the actual content of that input as well as a
/// line index for efficiently querying its contents.
#[derive(Clone, Debug)]
struct Input {
    text: SourceText,
    line_index: LineIndex,
}

impl Input {
    /// Returns this input as a `SourceCode` for convenient querying.
    fn as_source_code(&self) -> SourceCode<'_, '_> {
        SourceCode::new(self.text.as_str(), &self.line_index)
    }
}

/// Returns the line number accounting for the given `len`
/// number of preceding context lines.
///
/// The line number returned is guaranteed to be less than
/// or equal to `start`.
fn context_before(source: &SourceCode<'_, '_>, len: usize, start: OneIndexed) -> OneIndexed {
    let mut line = start.saturating_sub(len);
    // Trim leading empty lines.
    while line < start {
        if !source.line_text(line).trim().is_empty() {
            break;
        }
        line = line.saturating_add(1);
    }
    line
}

/// Returns the line number accounting for the given `len`
/// number of following context lines.
///
/// The line number returned is guaranteed to be greater
/// than or equal to `start` and no greater than the
/// number of lines in `source`.
fn context_after(source: &SourceCode<'_, '_>, len: usize, start: OneIndexed) -> OneIndexed {
    let max_lines = OneIndexed::from_zero_indexed(source.line_count());
    let mut line = start.saturating_add(len).min(max_lines);
    // Trim trailing empty lines.
    while line > start {
        if !source.line_text(line).trim().is_empty() {
            break;
        }
        line = line.saturating_sub(1);
    }
    line
}

#[cfg(test)]
mod tests {

    use crate::diagnostic::{Annotation, DiagnosticId, Severity, Span};
    use crate::files::system_path_to_file;
    use crate::system::{DbWithWritableSystem, SystemPath};
    use crate::tests::TestDb;

    use super::*;

    static ANIMALS: &str = "\
aardvark
beetle
canary
dog
elephant
finch
gorilla
hippopotamus
inchworm
jackrabbit
kangaroo
";

    // Useful for testing context windows that trim leading/trailing
    // lines that are pure whitespace or empty.
    static SPACEY_ANIMALS: &str = "\
aardvark

beetle

canary

dog
elephant
finch

gorilla
hippopotamus
inchworm
jackrabbit

kangaroo
";

    static FRUITS: &str = "\
apple
banana
cantelope
lime
orange
pear
raspberry
strawberry
tomato
watermelon
";

    static NON_ASCII: &str = "\
☃☃☃☃☃☃☃☃☃☃☃☃
💩💩💩💩💩💩💩💩💩💩💩💩
ΔΔΔΔΔΔΔΔΔΔΔΔ
ββββββββββββ
ΣΣΣΣΣΣΣΣΣΣΣΣ
ξξξξξξξξξξξξ
ππππππππππππ
θθθθθθθθθθθθ
ΦΦΦΦΦΦΦΦΦΦΦΦ
λλλλλλλλλλλλ
";

    #[test]
    fn basic() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        let diag = env.err().primary("animals", "5", "5", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 | canary
        4 | dog
        5 | elephant
          | ^^^^^^^^
        6 | finch
        7 | gorilla
          |
        ",
        );

        let diag = env
            .builder(
                "test-diagnostic",
                Severity::Warning,
                "main diagnostic message",
            )
            .primary("animals", "5", "5", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        warning: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 | canary
        4 | dog
        5 | elephant
          | ^^^^^^^^
        6 | finch
        7 | gorilla
          |
        ",
        );

        let diag = env
            .builder("test-diagnostic", Severity::Info, "main diagnostic message")
            .primary("animals", "5", "5", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        info: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 | canary
        4 | dog
        5 | elephant
          | ^^^^^^^^
        6 | finch
        7 | gorilla
          |
        ",
        );
    }

    #[test]
    fn non_ascii() {
        let mut env = TestEnvironment::new();
        env.add("non-ascii", NON_ASCII);

        let diag = env.err().primary("non-ascii", "5", "5", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /non-ascii:5:1
          |
        3 | ΔΔΔΔΔΔΔΔΔΔΔΔ
        4 | ββββββββββββ
        5 | ΣΣΣΣΣΣΣΣΣΣΣΣ
          | ^^^^^^^^^^^^
        6 | ξξξξξξξξξξξξ
        7 | ππππππππππππ
          |
        ",
        );

        // Just highlight one multi-byte codepoint
        // that has a >1 Unicode width.
        let diag = env.err().primary("non-ascii", "2:4", "2:8", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /non-ascii:2:2
          |
        1 | ☃☃☃☃☃☃☃☃☃☃☃☃
        2 | 💩💩💩💩💩💩💩💩💩💩💩💩
          |   ^^
        3 | ΔΔΔΔΔΔΔΔΔΔΔΔ
        4 | ββββββββββββ
          |
        ",
        );
    }

    #[test]
    fn config_context() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        // Smaller context
        let diag = env.err().primary("animals", "5", "5", "").build();
        env.context(1);
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        4 | dog
        5 | elephant
          | ^^^^^^^^
        6 | finch
          |
        ",
        );

        // No context
        let diag = env.err().primary("animals", "5", "5", "").build();
        env.context(0);
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        5 | elephant
          | ^^^^^^^^
          |
        ",
        );

        // No context before snippet
        let diag = env.err().primary("animals", "1", "1", "").build();
        env.context(2);
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:1:1
          |
        1 | aardvark
          | ^^^^^^^^
        2 | beetle
        3 | canary
          |
        ",
        );

        // No context after snippet
        let diag = env.err().primary("animals", "11", "11", "").build();
        env.context(2);
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:11:1
           |
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           | ^^^^^^^^
           |
        ",
        );

        // Context that exceeds source
        let diag = env.err().primary("animals", "5", "5", "").build();
        env.context(200);
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:5:1
           |
         1 | aardvark
         2 | beetle
         3 | canary
         4 | dog
         5 | elephant
           | ^^^^^^^^
         6 | finch
         7 | gorilla
         8 | hippopotamus
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           |
        ",
        );
    }

    #[test]
    fn multiple_annotations_non_overlapping() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            .primary("animals", "11", "11", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:1:1
           |
         1 | aardvark
           | ^^^^^^^^
         2 | beetle
         3 | canary
           |
          ::: /animals:11:1
           |
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           | ^^^^^^^^
           |
        ",
        );
    }

    #[test]
    fn multiple_annotations_adjacent_context() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        // Set the context explicitly to 1 to make
        // it easier to reason about, and to avoid
        // making this test tricky to update if the
        // default context changes.
        env.context(1);

        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            // This is the line that immediately follows
            // the context from the first annotation,
            // so there is no overlap. But since it's
            // adjacent, the snippet "expands" out to
            // include this line. (And the line after,
            // for one additional line of context.)
            .primary("animals", "3", "3", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:1:1
          |
        1 | aardvark
          | ^^^^^^^^
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
          |
        ",
        );

        // If the annotation were on the next line,
        // then the context windows for each annotation
        // are adjacent, and thus we still end up with
        // one snippet.
        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            .primary("animals", "4", "4", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:1:1
          |
        1 | aardvark
          | ^^^^^^^^
        2 | beetle
        3 | canary
        4 | dog
          | ^^^
        5 | elephant
          |
        ",
        );

        // But the line after that one, the context
        // windows are no longer adjacent. You can
        // tell this is correct because line 3 is
        // omitted from the snippet below, since it
        // is not in either annotation's context
        // window.
        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            .primary("animals", "5", "5", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:1:1
          |
        1 | aardvark
          | ^^^^^^^^
        2 | beetle
          |
         ::: /animals:5:1
          |
        4 | dog
        5 | elephant
          | ^^^^^^^^
        6 | finch
          |
        ",
        );

        // Do the same round of tests as above,
        // but with a bigger context window.
        env.context(3);
        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            .primary("animals", "5", "5", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:1:1
          |
        1 | aardvark
          | ^^^^^^^^
        2 | beetle
        3 | canary
        4 | dog
        5 | elephant
          | ^^^^^^^^
        6 | finch
        7 | gorilla
        8 | hippopotamus
          |
        ",
        );

        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            .primary("animals", "8", "8", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:1:1
           |
         1 | aardvark
           | ^^^^^^^^
         2 | beetle
         3 | canary
         4 | dog
         5 | elephant
         6 | finch
         7 | gorilla
         8 | hippopotamus
           | ^^^^^^^^^^^^
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           |
        ",
        );

        let diag = env
            .err()
            .primary("animals", "1", "1", "")
            .primary("animals", "9", "9", "")
            .build();
        // Line 5 is missing, as expected, since
        // it is not in either annotation's context
        // window.
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:1:1
           |
         1 | aardvark
           | ^^^^^^^^
         2 | beetle
         3 | canary
         4 | dog
           |
          ::: /animals:9:1
           |
         6 | finch
         7 | gorilla
         8 | hippopotamus
         9 | inchworm
           | ^^^^^^^^
        10 | jackrabbit
        11 | kangaroo
           |
        ",
        );
    }

    #[test]
    fn trimmed_context() {
        let mut env = TestEnvironment::new();
        env.add("spacey-animals", SPACEY_ANIMALS);

        // Set the context to `2` and pick `elephant`
        // from the input. It has two adjacent non-whitespace
        // lines on both sides, but then two whitespace
        // lines after that. As a result, the context window
        // effectively shrinks to `1`.
        env.context(2);
        let diag = env.err().primary("spacey-animals", "8", "8", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /spacey-animals:8:1
          |
        7 | dog
        8 | elephant
          | ^^^^^^^^
        9 | finch
          |
        ",
        );

        // Same thing, but where trimming only happens
        // in the preceding context.
        let diag = env.err().primary("spacey-animals", "12", "12", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /spacey-animals:12:1
           |
        11 | gorilla
        12 | hippopotamus
           | ^^^^^^^^^^^^
        13 | inchworm
        14 | jackrabbit
           |
        ",
        );

        // Again, with trimming only happening in the
        // following context.
        let diag = env.err().primary("spacey-animals", "13", "13", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /spacey-animals:13:1
           |
        11 | gorilla
        12 | hippopotamus
        13 | inchworm
           | ^^^^^^^^
        14 | jackrabbit
           |
        ",
        );
    }

    #[test]
    fn multiple_annotations_trimmed_context() {
        let mut env = TestEnvironment::new();
        env.add("spacey-animals", SPACEY_ANIMALS);

        env.context(1);
        let diag = env
            .err()
            .primary("spacey-animals", "3", "3", "")
            .primary("spacey-animals", "5", "5", "")
            .build();
        // Normally this would be one snippet, since
        // a context of `1` on line `3` will be adjacent
        // to the same sized context on line `5`. But since
        // the context calculation trims leading/trailing
        // whitespace lines, the context is not actually
        // adjacent.
        //
        // Arguably, this is perhaps not what we want. In
        // this case, the whitespace trimming is probably
        // getting in the way of a more succinct and less
        // jarring snippet. I wasn't 100% sure which
        // behavior we wanted, so I left it as-is for now
        // instead of special casing the snippet assembly.
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /spacey-animals:3:1
          |
        3 | beetle
          | ^^^^^^
          |
         ::: /spacey-animals:5:1
          |
        5 | canary
          | ^^^^^^
          |
        ",
        );
    }

    #[test]
    fn multiple_files_basic() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);
        env.add("fruits", FRUITS);

        let diag = env
            .err()
            .primary("animals", "3", "3", "")
            .primary("fruits", "3", "3", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
         ::: /fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantelope
          | ^^^^^^^^^
        4 | lime
        5 | orange
          |
        ",
        );
    }

    #[test]
    fn sub_diag_note_only_message() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);
        env.add("fruits", FRUITS);

        let mut diag = env.err().primary("animals", "3", "3", "").build();
        diag.sub(
            env.sub_builder(Severity::Info, "this is a helpful note")
                .build(),
        );
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        info: this is a helpful note
        ",
        );
    }

    #[test]
    fn sub_diag_many_notes() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);
        env.add("fruits", FRUITS);

        let mut diag = env.err().primary("animals", "3", "3", "").build();
        diag.sub(
            env.sub_builder(Severity::Info, "this is a helpful note")
                .build(),
        );
        diag.sub(
            env.sub_builder(Severity::Info, "another helpful note")
                .build(),
        );
        diag.sub(
            env.sub_builder(Severity::Info, "and another helpful note")
                .build(),
        );
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        info: this is a helpful note
        info: another helpful note
        info: and another helpful note
        ",
        );
    }

    #[test]
    fn sub_diag_warning_with_annotation() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);
        env.add("fruits", FRUITS);

        let mut diag = env.err().primary("animals", "3", "3", "").build();
        diag.sub(env.sub_warn().primary("fruits", "3", "3", "").build());
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
         --> /fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantelope
          | ^^^^^^^^^
        4 | lime
        5 | orange
          |
        ",
        );
    }

    #[test]
    fn sub_diag_many_warning_with_annotation_order() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);
        env.add("fruits", FRUITS);

        let mut diag = env.err().primary("animals", "3", "3", "").build();
        diag.sub(env.sub_warn().primary("fruits", "3", "3", "").build());
        diag.sub(env.sub_warn().primary("animals", "11", "11", "").build());
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
         --> /fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantelope
          | ^^^^^^^^^
        4 | lime
        5 | orange
          |
        warning: sub-diagnostic message
          --> /animals:11:1
           |
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           | ^^^^^^^^
           |
        ",
        );

        // Flip the order of the subs and ensure
        // this is reflected in the output.
        let mut diag = env.err().primary("animals", "3", "3", "").build();
        diag.sub(env.sub_warn().primary("animals", "11", "11", "").build());
        diag.sub(env.sub_warn().primary("fruits", "3", "3", "").build());
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
          --> /animals:11:1
           |
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           | ^^^^^^^^
           |
        warning: sub-diagnostic message
         --> /fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantelope
          | ^^^^^^^^^
        4 | lime
        5 | orange
          |
        ",
        );
    }

    #[test]
    fn sub_diag_repeats_snippet() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        let mut diag = env.err().primary("animals", "3", "3", "").build();
        // There's nothing preventing a sub-diagnostic from referencing
        // the same snippet rendered in another sub-diagnostic or the
        // parent diagnostic. While annotations *within* a diagnostic
        // (sub or otherwise) are coalesced into a minimal number of
        // snippets, no such minimizing is done for sub-diagnostics.
        // Namely, they are generally treated as completely separate.
        diag.sub(env.sub_warn().secondary("animals", "3", "3", "").build());
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
         --> /animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ------
        4 | dog
        5 | elephant
          |
        ",
        );
    }

    #[test]
    fn annotation_multi_line() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        // We just try out various offsets here.

        // Two entire lines.
        let diag = env.err().primary("animals", "5", "6", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 |   canary
        4 |   dog
        5 | / elephant
        6 | | finch
          | |_____^
        7 |   gorilla
        8 |   hippopotamus
          |
        ",
        );

        // Two lines plus the start of a third. Since we treat the end
        // position as inclusive AND because `ruff_annotate_snippets`
        // will render the position of the start of the line as just
        // past the end of the previous line, our annotation still only
        // extends across two lines.
        let diag = env.err().primary("animals", "5", "7:0", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 |   canary
        4 |   dog
        5 | / elephant
        6 | | finch
          | |______^
        7 |   gorilla
        8 |   hippopotamus
          |
        ",
        );

        // Add one more to our end position though, and the third
        // line gets included (as you might expect).
        let diag = env.err().primary("animals", "5", "7:1", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 |   canary
        4 |   dog
        5 | / elephant
        6 | | finch
        7 | | gorilla
          | |_^
        8 |   hippopotamus
        9 |   inchworm
          |
        ",
        );

        // Starting and stopping in the middle of two different lines.
        let diag = env.err().primary("animals", "5:3", "8:8", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:5:4
           |
         3 |   canary
         4 |   dog
         5 |   elephant
           |  ____^
         6 | | finch
         7 | | gorilla
         8 | | hippopotamus
           | |________^
         9 |   inchworm
        10 |   jackrabbit
           |
        ",
        );

        // Same as above, but with a secondary annotation.
        let diag = env.err().secondary("animals", "5:3", "8:8", "").build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:5:4
           |
         3 |   canary
         4 |   dog
         5 |   elephant
           |  ____-
         6 | | finch
         7 | | gorilla
         8 | | hippopotamus
           | |________-
         9 |   inchworm
        10 |   jackrabbit
           |
        ",
        );
    }

    #[test]
    fn annotation_overlapping_multi_line() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        // One annotation fully contained within another.
        let diag = env
            .err()
            .primary("animals", "5", "6", "")
            .primary("animals", "4", "7", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:4:1
          |
        2 |    beetle
        3 |    canary
        4 |    dog
          |  __^
        5 | |  elephant
          | | _^
        6 | || finch
          | ||_____^
        7 | |  gorilla
          | |________^
        8 |    hippopotamus
        9 |    inchworm
          |
        ",
        );

        // Same as above, but with order swapped.
        // Shouldn't impact rendering.
        let diag = env
            .err()
            .primary("animals", "4", "7", "")
            .primary("animals", "5", "6", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:4:1
          |
        2 |    beetle
        3 |    canary
        4 |    dog
          |  __^
        5 | |  elephant
          | | _^
        6 | || finch
          | ||_____^
        7 | |  gorilla
          | |________^
        8 |    hippopotamus
        9 |    inchworm
          |
        ",
        );

        // One annotation is completely contained
        // by the other, but the other has one
        // non-overlapping line preceding the
        // overlapping portion.
        let diag = env
            .err()
            .primary("animals", "5", "7", "")
            .primary("animals", "6", "7", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 |    canary
        4 |    dog
        5 |    elephant
          |  __^
        6 | |  finch
          | | _^
        7 | || gorilla
          | ||_______^
          |  |_______|
          |
        8 |    hippopotamus
        9 |    inchworm
          |
        ",
        );

        // One annotation is completely contained
        // by the other, but the other has one
        // non-overlapping line following the
        // overlapping portion.
        let diag = env
            .err()
            .primary("animals", "5", "6", "")
            .primary("animals", "5", "7", "")
            .build();
        // NOTE: I find the rendering here pretty
        // confusing, but I believe it is correct.
        // I'm not sure if it's possible to do much
        // better using only ASCII art.
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 |    canary
        4 |    dog
        5 |    elephant
          |   _^
          |  |_|
        6 | || finch
          | ||_____^
        7 |  | gorilla
          |  |_______^
        8 |    hippopotamus
        9 |    inchworm
          |
        ",
        );

        // Annotations partially overlap, but both
        // contain lines that aren't in the other.
        let diag = env
            .err()
            .primary("animals", "5", "6", "")
            .primary("animals", "6", "7", "")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        3 |    canary
        4 |    dog
        5 |    elephant
          |  __^
        6 | |  finch
          | |__^___^
          |   _|
          |  |
        7 |  | gorilla
          |  |_______^
        8 |    hippopotamus
        9 |    inchworm
          |
        ",
        );
    }

    #[test]
    fn annotation_message() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        let diag = env
            .err()
            .primary("animals", "5:2", "5:6", "giant land mammal")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:3
          |
        3 | canary
        4 | dog
        5 | elephant
          |   ^^^^ giant land mammal
        6 | finch
        7 | gorilla
          |
        ",
        );

        // Same as above, but add two annotations for the same range.
        let diag = env
            .err()
            .primary("animals", "5:2", "5:6", "giant land mammal")
            .secondary("animals", "5:2", "5:6", "but afraid of mice")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:3
          |
        3 | canary
        4 | dog
        5 | elephant
          |   ----
          |   |
          |   giant land mammal
          |   but afraid of mice
        6 | finch
        7 | gorilla
          |
        ",
        );
    }

    #[test]
    fn annotation_one_file_primary_always_comes_first() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        // The secondary annotation is not only added first,
        // but it appears first in the source. But it still
        // comes second.
        let diag = env
            .err()
            .secondary("animals", "1", "1", "secondary")
            .primary("animals", "8", "8", "primary")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:8:1
           |
         6 | finch
         7 | gorilla
         8 | hippopotamus
           | ^^^^^^^^^^^^ primary
         9 | inchworm
        10 | jackrabbit
           |
          ::: /animals:1:1
           |
         1 | aardvark
           | -------- secondary
         2 | beetle
         3 | canary
           |
        ",
        );

        // This is a weirder case where there are multiple
        // snippets with primary annotations. We ensure that
        // all such snippets appear before any snippets with
        // zero primary annotations. Otherwise, the snippets
        // appear in source order.
        //
        // (We also drop the context so that we can squeeze
        // more snippets out of our test data.)
        env.context(0);
        let diag = env
            .err()
            .secondary("animals", "7", "7", "secondary 7")
            .primary("animals", "9", "9", "primary 9")
            .secondary("animals", "3", "3", "secondary 3")
            .secondary("animals", "1", "1", "secondary 1")
            .primary("animals", "5", "5", "primary 5")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /animals:5:1
          |
        5 | elephant
          | ^^^^^^^^ primary 5
          |
         ::: /animals:9:1
          |
        9 | inchworm
          | ^^^^^^^^ primary 9
          |
         ::: /animals:1:1
          |
        1 | aardvark
          | -------- secondary 1
          |
         ::: /animals:3:1
          |
        3 | canary
          | ------ secondary 3
          |
         ::: /animals:7:1
          |
        7 | gorilla
          | ------- secondary 7
          |
        ",
        );
    }

    #[test]
    fn annotation_many_files_primary_always_comes_first() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);
        env.add("fruits", FRUITS);

        let diag = env
            .err()
            .secondary("animals", "1", "1", "secondary")
            .primary("fruits", "1", "1", "primary")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
         --> /fruits:1:1
          |
        1 | apple
          | ^^^^^ primary
        2 | banana
        3 | cantelope
          |
         ::: /animals:1:1
          |
        1 | aardvark
          | -------- secondary
        2 | beetle
        3 | canary
          |
        ",
        );

        // Same as the single file test, we try adding
        // multiple primary annotations across multiple
        // files. Those should always appear first
        // *within* each file.
        env.context(0);
        let diag = env
            .err()
            .secondary("animals", "7", "7", "secondary animals 7")
            .secondary("fruits", "2", "2", "secondary fruits 2")
            .secondary("animals", "3", "3", "secondary animals 3")
            .secondary("animals", "1", "1", "secondary animals 1")
            .primary("animals", "11", "11", "primary animals 11")
            .primary("fruits", "10", "10", "primary fruits 10")
            .build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error: lint:test-diagnostic: main diagnostic message
          --> /animals:11:1
           |
        11 | kangaroo
           | ^^^^^^^^ primary animals 11
           |
          ::: /animals:1:1
           |
         1 | aardvark
           | -------- secondary animals 1
           |
          ::: /animals:3:1
           |
         3 | canary
           | ------ secondary animals 3
           |
          ::: /animals:7:1
           |
         7 | gorilla
           | ------- secondary animals 7
           |
          ::: /fruits:10:1
           |
        10 | watermelon
           | ^^^^^^^^^^ primary fruits 10
           |
          ::: /fruits:2:1
           |
         2 | banana
           | ------ secondary fruits 2
           |
        ",
        );
    }

    /// A small harness for setting up an environment specifically for testing
    /// diagnostic rendering.
    struct TestEnvironment {
        db: TestDb,
        config: DisplayDiagnosticConfig,
    }

    impl TestEnvironment {
        /// Create a new test harness.
        ///
        /// This uses the default diagnostic rendering configuration.
        fn new() -> TestEnvironment {
            TestEnvironment {
                db: TestDb::new(),
                config: DisplayDiagnosticConfig::default(),
            }
        }

        /// Set the number of contextual lines to include for each snippet
        /// in diagnostic rendering.
        fn context(&mut self, lines: usize) {
            // Kind of annoying. I considered making `DisplayDiagnosticConfig`
            // be `Copy` (which it could be, at time of writing, 2025-03-07),
            // but it seems likely to me that it will grow non-`Copy`
            // configuration. So just deal with this inconvenience for now.
            let mut config = std::mem::take(&mut self.config);
            config = config.context(lines);
            self.config = config;
        }

        /// Add a file with the given path and contents to this environment.
        fn add(&mut self, path: &str, contents: &str) {
            let path = SystemPath::new(path);
            self.db.write_file(path, contents).unwrap();
        }

        /// Conveniently create a `Span` that points into a file in this
        /// environment.
        ///
        /// The path given must have been added via `TestEnvironment::add`.
        ///
        /// The offset strings given should be in `{line}(:{offset})?` format.
        /// `line` is a 1-indexed offset corresponding to the line number,
        /// while `offset` is a 0-indexed *byte* offset starting from the
        /// beginning of the corresponding line. When `offset` is missing from
        /// the start of the span, it is assumed to be `0`. When `offset` is
        /// missing from the end of the span, it is assumed to be the length
        /// of the corresponding line minus one. (The "minus one" is because
        /// otherwise, the span will end where the next line begins, and this
        /// confuses `ruff_annotate_snippets` as of 2025-03-13.)
        fn span(&self, path: &str, line_offset_start: &str, line_offset_end: &str) -> Span {
            let file = system_path_to_file(&self.db, path).unwrap();

            let text = source_text(&self.db, file);
            let line_index = line_index(&self.db, file);
            let source = SourceCode::new(text.as_str(), &line_index);

            let (line_start, offset_start) = parse_line_offset(line_offset_start);
            let (line_end, offset_end) = parse_line_offset(line_offset_end);

            let start = match offset_start {
                None => source.line_start(line_start),
                Some(offset) => source.line_start(line_start) + offset,
            };
            let end = match offset_end {
                None => source.line_end(line_end) - TextSize::from(1),
                Some(offset) => source.line_start(line_end) + offset,
            };
            Span::from(file).with_range(TextRange::new(start, end))
        }

        /// A convenience function for returning a builder for a diagnostic
        /// with "error" severity and canned values for its identifier
        /// and message.
        fn err(&mut self) -> DiagnosticBuilder<'_> {
            self.builder(
                "test-diagnostic",
                Severity::Error,
                "main diagnostic message",
            )
        }

        /// A convenience function for returning a builder for a
        /// sub-diagnostic with "error" severity and canned values for
        /// its identifier and message.
        fn sub_warn(&mut self) -> SubDiagnosticBuilder<'_> {
            self.sub_builder(Severity::Warning, "sub-diagnostic message")
        }

        /// Returns a builder for tersely constructing diagnostics.
        fn builder(
            &mut self,
            identifier: &'static str,
            severity: Severity,
            message: &str,
        ) -> DiagnosticBuilder<'_> {
            let diag = Diagnostic::new(id(identifier), severity, message);
            DiagnosticBuilder { env: self, diag }
        }

        /// Returns a builder for tersely constructing sub-diagnostics.
        fn sub_builder(&mut self, severity: Severity, message: &str) -> SubDiagnosticBuilder<'_> {
            let subdiag = SubDiagnostic::new(severity, message);
            SubDiagnosticBuilder { env: self, subdiag }
        }

        /// Render the given diagnostic into a `String`.
        ///
        /// (This will set the "printed" flag on `Diagnostic`.)
        fn render(&self, diag: &Diagnostic) -> String {
            diag.display(&self.db, &self.config).to_string()
        }
    }

    /// A helper builder for tersely populating a `Diagnostic`.
    ///
    /// If you need to mutate the diagnostic in a way that isn't
    /// supported by this builder, and this only needs to be done
    /// infrequently, consider doing it more verbosely on `diag`
    /// itself.
    struct DiagnosticBuilder<'e> {
        env: &'e mut TestEnvironment,
        diag: Diagnostic,
    }

    impl<'e> DiagnosticBuilder<'e> {
        /// Return the built diagnostic.
        fn build(self) -> Diagnostic {
            self.diag
        }

        /// Add a primary annotation with a message.
        ///
        /// If the message is empty, then an annotation without any
        /// message be created.
        ///
        /// See the docs on `TestEnvironment::span` for the meaning of
        /// `path`, `line_offset_start` and `line_offset_end`.
        fn primary(
            mut self,
            path: &str,
            line_offset_start: &str,
            line_offset_end: &str,
            label: &str,
        ) -> DiagnosticBuilder<'e> {
            let span = self.env.span(path, line_offset_start, line_offset_end);
            let mut ann = Annotation::primary(span);
            if !label.is_empty() {
                ann = ann.message(label);
            }
            self.diag.annotate(ann);
            self
        }

        /// Add a secondary annotation with a message.
        ///
        /// If the message is empty, then an annotation without any
        /// message be created.
        ///
        /// See the docs on `TestEnvironment::span` for the meaning of
        /// `path`, `line_offset_start` and `line_offset_end`.
        fn secondary(
            mut self,
            path: &str,
            line_offset_start: &str,
            line_offset_end: &str,
            label: &str,
        ) -> DiagnosticBuilder<'e> {
            let span = self.env.span(path, line_offset_start, line_offset_end);
            let mut ann = Annotation::secondary(span);
            if !label.is_empty() {
                ann = ann.message(label);
            }
            self.diag.annotate(ann);
            self
        }
    }

    /// A helper builder for tersely populating a `SubDiagnostic`.
    ///
    /// If you need to mutate the sub-diagnostic in a way that isn't
    /// supported by this builder, and this only needs to be done
    /// infrequently, consider doing it more verbosely on `diag`
    /// itself.
    struct SubDiagnosticBuilder<'e> {
        env: &'e mut TestEnvironment,
        subdiag: SubDiagnostic,
    }

    impl<'e> SubDiagnosticBuilder<'e> {
        /// Return the built sub-diagnostic.
        fn build(self) -> SubDiagnostic {
            self.subdiag
        }

        /// Add a primary annotation with a message.
        ///
        /// If the message is empty, then an annotation without any
        /// message be created.
        ///
        /// See the docs on `TestEnvironment::span` for the meaning of
        /// `path`, `line_offset_start` and `line_offset_end`.
        fn primary(
            mut self,
            path: &str,
            line_offset_start: &str,
            line_offset_end: &str,
            label: &str,
        ) -> SubDiagnosticBuilder<'e> {
            let span = self.env.span(path, line_offset_start, line_offset_end);
            let mut ann = Annotation::primary(span);
            if !label.is_empty() {
                ann = ann.message(label);
            }
            self.subdiag.annotate(ann);
            self
        }

        /// Add a secondary annotation with a message.
        ///
        /// If the message is empty, then an annotation without any
        /// message be created.
        ///
        /// See the docs on `TestEnvironment::span` for the meaning of
        /// `path`, `line_offset_start` and `line_offset_end`.
        fn secondary(
            mut self,
            path: &str,
            line_offset_start: &str,
            line_offset_end: &str,
            label: &str,
        ) -> SubDiagnosticBuilder<'e> {
            let span = self.env.span(path, line_offset_start, line_offset_end);
            let mut ann = Annotation::secondary(span);
            if !label.is_empty() {
                ann = ann.message(label);
            }
            self.subdiag.annotate(ann);
            self
        }
    }

    fn id(lint_name: &'static str) -> DiagnosticId {
        DiagnosticId::lint(lint_name)
    }

    fn parse_line_offset(s: &str) -> (OneIndexed, Option<TextSize>) {
        let Some((line, offset)) = s.split_once(":") else {
            let line_number = OneIndexed::new(s.parse().unwrap()).unwrap();
            return (line_number, None);
        };
        let line_number = OneIndexed::new(line.parse().unwrap()).unwrap();
        let offset = TextSize::from(offset.parse::<u32>().unwrap());
        (line_number, Some(offset))
    }
}

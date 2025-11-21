use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use full::FullRenderer;
use ruff_annotate_snippets::{
    Annotation as AnnotateAnnotation, Level as AnnotateLevel, Message as AnnotateMessage,
    Snippet as AnnotateSnippet,
};
use ruff_notebook::{Notebook, NotebookIndex};
use ruff_source_file::{LineIndex, OneIndexed, SourceCode};
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::{
    Db,
    files::File,
    source::{SourceText, line_index, source_text},
};

use super::{
    Annotation, Diagnostic, DiagnosticFormat, DiagnosticSource, DisplayDiagnosticConfig,
    SubDiagnostic, UnifiedFile,
};

use azure::AzureRenderer;
use concise::ConciseRenderer;
use github::GithubRenderer;
use pylint::PylintRenderer;

mod azure;
mod concise;
mod full;
pub mod github;
#[cfg(feature = "serde")]
mod gitlab;
#[cfg(feature = "serde")]
mod json;
#[cfg(feature = "serde")]
mod json_lines;
#[cfg(feature = "junit")]
mod junit;
mod pylint;
#[cfg(feature = "serde")]
mod rdjson;

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
pub struct DisplayDiagnostic<'a> {
    config: &'a DisplayDiagnosticConfig,
    resolver: &'a dyn FileResolver,
    diag: &'a Diagnostic,
}

impl<'a> DisplayDiagnostic<'a> {
    pub(crate) fn new(
        resolver: &'a dyn FileResolver,
        config: &'a DisplayDiagnosticConfig,
        diag: &'a Diagnostic,
    ) -> DisplayDiagnostic<'a> {
        DisplayDiagnostic {
            config,
            resolver,
            diag,
        }
    }
}

impl std::fmt::Display for DisplayDiagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        DisplayDiagnostics::new(self.resolver, self.config, std::slice::from_ref(self.diag)).fmt(f)
    }
}

/// A type that implements `std::fmt::Display` for rendering a collection of diagnostics.
///
/// It is intended for collections of diagnostics that need to be serialized together, as is the
/// case for JSON, for example.
///
/// See [`DisplayDiagnostic`] for rendering individual `Diagnostic`s and details about the lifetime
/// constraints.
pub struct DisplayDiagnostics<'a> {
    config: &'a DisplayDiagnosticConfig,
    resolver: &'a dyn FileResolver,
    diagnostics: &'a [Diagnostic],
}

impl<'a> DisplayDiagnostics<'a> {
    pub fn new(
        resolver: &'a dyn FileResolver,
        config: &'a DisplayDiagnosticConfig,
        diagnostics: &'a [Diagnostic],
    ) -> DisplayDiagnostics<'a> {
        DisplayDiagnostics {
            config,
            resolver,
            diagnostics,
        }
    }
}

impl std::fmt::Display for DisplayDiagnostics<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.config.format {
            DiagnosticFormat::Concise => {
                ConciseRenderer::new(self.resolver, self.config).render(f, self.diagnostics)?;
            }
            DiagnosticFormat::Full => {
                FullRenderer::new(self.resolver, self.config).render(f, self.diagnostics)?;
            }
            DiagnosticFormat::Azure => {
                AzureRenderer::new(self.resolver).render(f, self.diagnostics)?;
            }
            #[cfg(feature = "serde")]
            DiagnosticFormat::Json => {
                json::JsonRenderer::new(self.resolver, self.config).render(f, self.diagnostics)?;
            }
            #[cfg(feature = "serde")]
            DiagnosticFormat::JsonLines => {
                json_lines::JsonLinesRenderer::new(self.resolver, self.config)
                    .render(f, self.diagnostics)?;
            }
            #[cfg(feature = "serde")]
            DiagnosticFormat::Rdjson => {
                rdjson::RdjsonRenderer::new(self.resolver).render(f, self.diagnostics)?;
            }
            DiagnosticFormat::Pylint => {
                PylintRenderer::new(self.resolver).render(f, self.diagnostics)?;
            }
            #[cfg(feature = "junit")]
            DiagnosticFormat::Junit => {
                junit::JunitRenderer::new(self.resolver).render(f, self.diagnostics)?;
            }
            #[cfg(feature = "serde")]
            DiagnosticFormat::Gitlab => {
                gitlab::GitlabRenderer::new(self.resolver).render(f, self.diagnostics)?;
            }
            DiagnosticFormat::Github => {
                GithubRenderer::new(self.resolver, "ty").render(f, self.diagnostics)?;
            }
        }

        Ok(())
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
    diagnostics: Vec<ResolvedDiagnostic<'a>>,
}

impl<'a> Resolved<'a> {
    /// Creates a new resolved set of diagnostics.
    fn new(
        resolver: &'a dyn FileResolver,
        diag: &'a Diagnostic,
        config: &DisplayDiagnosticConfig,
    ) -> Resolved<'a> {
        let mut diagnostics = vec![];
        diagnostics.push(ResolvedDiagnostic::from_diagnostic(resolver, config, diag));
        for sub in &diag.inner.subs {
            diagnostics.push(ResolvedDiagnostic::from_sub_diagnostic(resolver, sub));
        }
        Resolved { diagnostics }
    }

    /// Creates a value that is amenable to rendering directly.
    fn to_renderable(&self, context: usize) -> Renderable<'_> {
        Renderable {
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
    level: AnnotateLevel,
    id: Option<String>,
    documentation_url: Option<String>,
    message: String,
    annotations: Vec<ResolvedAnnotation<'a>>,
    is_fixable: bool,
    header_offset: usize,
}

impl<'a> ResolvedDiagnostic<'a> {
    /// Resolve a single diagnostic.
    fn from_diagnostic(
        resolver: &'a dyn FileResolver,
        config: &DisplayDiagnosticConfig,
        diag: &'a Diagnostic,
    ) -> ResolvedDiagnostic<'a> {
        let annotations: Vec<_> = diag
            .inner
            .annotations
            .iter()
            .filter_map(|ann| {
                let path = ann
                    .span
                    .file
                    .relative_path(resolver)
                    .to_str()
                    .unwrap_or_else(|| ann.span.file.path(resolver));
                let diagnostic_source = ann.span.file.diagnostic_source(resolver);
                ResolvedAnnotation::new(path, &diagnostic_source, ann, resolver)
            })
            .collect();

        let id = if config.hide_severity {
            // Either the rule code alone (e.g. `F401`), or the lint id with a colon (e.g.
            // `invalid-syntax:`). When Ruff gets real severities, we should put the colon back in
            // `DisplaySet::format_annotation` for both cases, but this is a small hack to improve
            // the formatting of syntax errors for now. This should also be kept consistent with the
            // concise formatting.
            diag.secondary_code().map_or_else(
                || format!("{id}:", id = diag.inner.id),
                |code| code.to_string(),
            )
        } else {
            diag.inner.id.to_string()
        };

        let level = if config.hide_severity {
            AnnotateLevel::None
        } else {
            diag.inner.severity.to_annotate()
        };

        ResolvedDiagnostic {
            level,
            id: Some(id),
            documentation_url: diag.documentation_url().map(ToString::to_string),
            message: diag.inner.message.as_str().to_string(),
            annotations,
            is_fixable: config.show_fix_status && diag.has_applicable_fix(config),
            header_offset: diag.inner.header_offset,
        }
    }

    /// Resolve a single sub-diagnostic.
    fn from_sub_diagnostic(
        resolver: &'a dyn FileResolver,
        diag: &'a SubDiagnostic,
    ) -> ResolvedDiagnostic<'a> {
        let annotations: Vec<_> = diag
            .inner
            .annotations
            .iter()
            .filter_map(|ann| {
                let path = ann
                    .span
                    .file
                    .relative_path(resolver)
                    .to_str()
                    .unwrap_or_else(|| ann.span.file.path(resolver));
                let diagnostic_source = ann.span.file.diagnostic_source(resolver);
                ResolvedAnnotation::new(path, &diagnostic_source, ann, resolver)
            })
            .collect();
        ResolvedDiagnostic {
            level: diag.inner.severity.to_annotate(),
            id: None,
            documentation_url: None,
            message: diag.inner.message.as_str().to_string(),
            annotations,
            is_fixable: false,
            header_offset: 0,
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

                let prev_context_ends = context_after(
                    &prev.diagnostic_source.as_source_code(),
                    context,
                    prev.line_end,
                    prev.notebook_index.as_ref(),
                )
                .get();
                let this_context_begins = context_before(
                    &ann.diagnostic_source.as_source_code(),
                    context,
                    ann.line_start,
                    ann.notebook_index.as_ref(),
                )
                .get();

                // For notebooks, check whether the end of the
                // previous annotation and the start of the current
                // annotation are in different cells.
                let prev_cell_index = prev.notebook_index.as_ref().map(|notebook_index| {
                    let prev_end = prev
                        .diagnostic_source
                        .as_source_code()
                        .line_column(prev.range.end());
                    notebook_index.cell(prev_end.line).unwrap_or_default().get()
                });
                let this_cell_index = ann.notebook_index.as_ref().map(|notebook_index| {
                    let this_start = ann
                        .diagnostic_source
                        .as_source_code()
                        .line_column(ann.range.start());
                    notebook_index
                        .cell(this_start.line)
                        .unwrap_or_default()
                        .get()
                });
                let in_different_cells = prev_cell_index != this_cell_index;

                // The boundary case here is when `prev_context_ends`
                // is exactly one less than `this_context_begins`. In
                // that case, the context windows are adjacent and we
                // should fall through below to add this annotation to
                // the existing snippet.
                //
                // For notebooks, also check that the context windows
                // are in the same cell. Windows from different cells
                // should never be considered adjacent.
                if in_different_cells || this_context_begins.saturating_sub(prev_context_ends) > 1 {
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
            level: self.level,
            id: self.id.as_deref(),
            documentation_url: self.documentation_url.as_deref(),
            message: &self.message,
            snippets_by_input,
            is_fixable: self.is_fixable,
            header_offset: self.header_offset,
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
    diagnostic_source: DiagnosticSource,
    range: TextRange,
    line_start: OneIndexed,
    line_end: OneIndexed,
    message: Option<&'a str>,
    is_primary: bool,
    hide_snippet: bool,
    notebook_index: Option<NotebookIndex>,
}

impl<'a> ResolvedAnnotation<'a> {
    /// Resolve an annotation.
    ///
    /// `path` is the path of the file that this annotation points to.
    ///
    /// `input` is the contents of the file that this annotation points to.
    fn new(
        path: &'a str,
        diagnostic_source: &DiagnosticSource,
        ann: &'a Annotation,
        resolver: &'a dyn FileResolver,
    ) -> Option<ResolvedAnnotation<'a>> {
        let source = diagnostic_source.as_source_code();
        let (range, line_start, line_end) = match (ann.span.range(), ann.message.is_some()) {
            // An annotation with no range AND no message is probably(?)
            // meaningless, but we should try to render it anyway.
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
            diagnostic_source: diagnostic_source.clone(),
            range,
            line_start,
            line_end,
            message: ann.get_message(),
            is_primary: ann.is_primary,
            hide_snippet: ann.hide_snippet,
            notebook_index: resolver.notebook_index(&ann.span.file),
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
    diagnostics: Vec<RenderableDiagnostic<'r>>,
}

/// A single diagnostic amenable to rendering.
#[derive(Debug)]
struct RenderableDiagnostic<'r> {
    /// The severity of the diagnostic.
    level: AnnotateLevel,
    /// The ID of the diagnostic. The ID can usually be used on the CLI or in a
    /// config file to change the severity of a lint.
    ///
    /// An ID is always present for top-level diagnostics and always absent for
    /// sub-diagnostics.
    id: Option<&'r str>,
    documentation_url: Option<&'r str>,
    /// The message emitted with the diagnostic, before any snippets are
    /// rendered.
    message: &'r str,
    /// A collection of collections of snippets. Each collection of snippets
    /// should be from the same file, and none of the snippets inside of a
    /// collection should overlap with one another or be directly adjacent.
    snippets_by_input: Vec<RenderableSnippets<'r>>,
    /// Whether or not the diagnostic is fixable.
    ///
    /// This is rendered as a `[*]` indicator after the diagnostic ID.
    is_fixable: bool,
    /// Offset to align the header sigil (`-->`) with the subsequent line number separators.
    ///
    /// This is only needed for formatter diagnostics where we don't render a snippet via
    /// `annotate-snippets` and thus the alignment isn't computed automatically.
    header_offset: usize,
}

impl RenderableDiagnostic<'_> {
    /// Convert this to an "annotate" snippet.
    fn to_annotate(&self) -> AnnotateMessage<'_> {
        let snippets = self.snippets_by_input.iter().flat_map(|snippets| {
            let path = snippets.path;
            snippets
                .snippets
                .iter()
                .map(|snippet| snippet.to_annotate(path))
        });
        let mut message = self
            .level
            .title(self.message)
            .is_fixable(self.is_fixable)
            .lineno_offset(self.header_offset);
        if let Some(id) = self.id {
            message = message.id_with_url(id, self.documentation_url);
        }
        message.snippets(snippets)
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
    snippet: Cow<'r, str>,
    /// The absolute line number corresponding to where this
    /// snippet begins.
    line_start: OneIndexed,
    /// A non-zero number of annotations on this snippet.
    annotations: Vec<RenderableAnnotation<'r>>,
    /// Whether this snippet contains at least one primary
    /// annotation.
    has_primary: bool,
    /// The cell index in a Jupyter notebook, if this snippet refers to a notebook.
    ///
    /// This is used for rendering annotations with offsets like `cell 1:2:3` instead of simple row
    /// and column numbers.
    cell_index: Option<usize>,
}

impl<'r> RenderableSnippet<'r> {
    /// Creates a new snippet with one or more annotations that is ready to be
    /// rendered.
    ///
    /// The first line of the snippet is the smallest line number on which one
    /// of the annotations begins, minus the context window size. The last line
    /// is the largest line number on which one of the annotations ends, plus
    /// the context window size.
    ///
    /// For Jupyter notebooks, the context window may also be truncated at cell
    /// boundaries. If multiple annotations are present, and they point to
    /// different cells, these will have already been split into separate
    /// snippets by `ResolvedDiagnostic::to_renderable`.
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
        let diagnostic_source = &anns[0].diagnostic_source;
        let notebook_index = anns[0].notebook_index.as_ref();
        let source = diagnostic_source.as_source_code();
        let has_primary = anns.iter().any(|ann| ann.is_primary);

        let content_start_index = anns.iter().map(|ann| ann.line_start).min().unwrap();
        let line_start = context_before(&source, context, content_start_index, notebook_index);

        let start = source.line_column(anns[0].range.start());
        let cell_index = notebook_index
            .map(|notebook_index| notebook_index.cell(start.line).unwrap_or_default().get());

        let content_end_index = anns.iter().map(|ann| ann.line_end).max().unwrap();
        let line_end = context_after(&source, context, content_end_index, notebook_index);

        let snippet_start = source.line_start(line_start);
        let snippet_end = source.line_end(line_end);
        let snippet = diagnostic_source
            .as_source_code()
            .slice(TextRange::new(snippet_start, snippet_end));

        // Strip the BOM from the beginning of the snippet, if present. Doing this here saves us the
        // trouble of updating the annotation ranges in `replace_unprintable`, and also allows us to
        // check that the BOM is at the very beginning of the file, not just the beginning of the
        // snippet.
        const BOM: char = '\u{feff}';
        let bom_len = BOM.text_len();
        let (snippet, snippet_start) =
            if snippet_start == TextSize::ZERO && snippet.starts_with(BOM) {
                (
                    &snippet[bom_len.to_usize()..],
                    snippet_start + TextSize::new(bom_len.to_u32()),
                )
            } else {
                (snippet, snippet_start)
            };

        let annotations = anns
            .iter()
            .map(|ann| RenderableAnnotation::new(snippet_start, ann))
            .collect();

        let EscapedSourceCode {
            text: snippet,
            annotations,
        } = replace_unprintable(snippet, annotations).fix_up_empty_spans_after_line_terminator();

        let line_start = notebook_index.map_or(line_start, |notebook_index| {
            notebook_index
                .cell_row(line_start)
                .unwrap_or(OneIndexed::MIN)
        });

        RenderableSnippet {
            snippet,
            line_start,
            annotations,
            has_primary,
            cell_index,
        }
    }

    /// Convert this to an "annotate" snippet.
    fn to_annotate<'a>(&'a self, path: &'a str) -> AnnotateSnippet<'a> {
        AnnotateSnippet::source(&self.snippet)
            .origin(path)
            .line_start(self.line_start.get())
            .annotations(
                self.annotations
                    .iter()
                    .map(RenderableAnnotation::to_annotate),
            )
            .cell_index(self.cell_index)
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
    /// Whether the snippet for this annotation should be hidden instead of rendered.
    hide_snippet: bool,
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
        // This should only ever saturate if a BOM is present _and_ the annotation range points
        // before the BOM (i.e. at offset 0). In Ruff this typically results from the use of
        // `TextRange::default()` for a diagnostic range instead of a range relative to file
        // contents.
        let range = ann.range.checked_sub(snippet_start).unwrap_or(ann.range);
        RenderableAnnotation {
            range,
            message: ann.message,
            is_primary: ann.is_primary,
            hide_snippet: ann.hide_snippet,
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
        ann.hide_snippet(self.hide_snippet)
    }
}

/// A trait that facilitates the retrieval of source code from a `Span`.
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
/// to be entirely separate from the Salsa interner used by ty, and so,
/// callers will need to pass in a different "resolver" for turning `Span`s
/// into actual file paths/contents. The infrastructure for this isn't fully in
/// place, but this type serves to demarcate the intended abstraction boundary.
pub trait FileResolver {
    /// Returns the path associated with the file given.
    fn path(&self, file: File) -> &str;

    /// Returns the input contents associated with the file given.
    fn input(&self, file: File) -> Input;

    /// Returns the [`NotebookIndex`] associated with the file given, if it's a Jupyter notebook.
    fn notebook_index(&self, file: &UnifiedFile) -> Option<NotebookIndex>;

    /// Returns whether the file given is a Jupyter notebook.
    fn is_notebook(&self, file: &UnifiedFile) -> bool;

    /// Returns the current working directory.
    fn current_directory(&self) -> &Path;
}

impl<T> FileResolver for T
where
    T: Db,
{
    fn path(&self, file: File) -> &str {
        file.path(self).as_str()
    }

    fn input(&self, file: File) -> Input {
        Input {
            text: source_text(self, file),
            line_index: line_index(self, file),
        }
    }

    fn notebook_index(&self, file: &UnifiedFile) -> Option<NotebookIndex> {
        match file {
            UnifiedFile::Ty(file) => self
                .input(*file)
                .text
                .as_notebook()
                .map(Notebook::index)
                .cloned(),
            UnifiedFile::Ruff(_) => unimplemented!("Expected an interned ty file"),
        }
    }

    fn is_notebook(&self, file: &UnifiedFile) -> bool {
        match file {
            UnifiedFile::Ty(file) => self.input(*file).text.as_notebook().is_some(),
            UnifiedFile::Ruff(_) => unimplemented!("Expected an interned ty file"),
        }
    }

    fn current_directory(&self) -> &Path {
        self.system().current_directory().as_std_path()
    }
}

impl FileResolver for &dyn Db {
    fn path(&self, file: File) -> &str {
        file.path(*self).as_str()
    }

    fn input(&self, file: File) -> Input {
        Input {
            text: source_text(*self, file),
            line_index: line_index(*self, file),
        }
    }

    fn notebook_index(&self, file: &UnifiedFile) -> Option<NotebookIndex> {
        match file {
            UnifiedFile::Ty(file) => self
                .input(*file)
                .text
                .as_notebook()
                .map(Notebook::index)
                .cloned(),
            UnifiedFile::Ruff(_) => unimplemented!("Expected an interned ty file"),
        }
    }

    fn is_notebook(&self, file: &UnifiedFile) -> bool {
        match file {
            UnifiedFile::Ty(file) => self.input(*file).text.as_notebook().is_some(),
            UnifiedFile::Ruff(_) => unimplemented!("Expected an interned ty file"),
        }
    }

    fn current_directory(&self) -> &Path {
        self.system().current_directory().as_std_path()
    }
}

/// An abstraction over a unit of user input.
///
/// A single unit of user input usually corresponds to a `File`.
/// This contains the actual content of that input as well as a
/// line index for efficiently querying its contents.
#[derive(Clone, Debug)]
pub struct Input {
    pub(crate) text: SourceText,
    pub(crate) line_index: LineIndex,
}

/// Returns the line number accounting for the given `len`
/// number of preceding context lines.
///
/// The line number returned is guaranteed to be less than
/// or equal to `start`.
///
/// In Jupyter notebooks, lines outside the cell containing
/// `start` will be omitted.
fn context_before(
    source: &SourceCode<'_, '_>,
    len: usize,
    start: OneIndexed,
    notebook_index: Option<&NotebookIndex>,
) -> OneIndexed {
    let mut line = start.saturating_sub(len);
    // Trim leading empty lines.
    while line < start {
        if !source.line_text(line).trim().is_empty() {
            break;
        }
        line = line.saturating_add(1);
    }

    if let Some(index) = notebook_index {
        let content_start_cell = index.cell(start).unwrap_or(OneIndexed::MIN);
        while line < start {
            if index.cell(line).unwrap_or(OneIndexed::MIN) == content_start_cell {
                break;
            }
            line = line.saturating_add(1);
        }
    }

    line
}

/// Returns the line number accounting for the given `len`
/// number of following context lines.
///
/// The line number returned is guaranteed to be greater
/// than or equal to `start` and no greater than the
/// number of lines in `source`.
///
/// In Jupyter notebooks, lines outside the cell containing
/// `start` will be omitted.
fn context_after(
    source: &SourceCode<'_, '_>,
    len: usize,
    start: OneIndexed,
    notebook_index: Option<&NotebookIndex>,
) -> OneIndexed {
    let max_lines = OneIndexed::from_zero_indexed(source.line_count());
    let mut line = start.saturating_add(len).min(max_lines);
    // Trim trailing empty lines.
    while line > start {
        if !source.line_text(line).trim().is_empty() {
            break;
        }
        line = line.saturating_sub(1);
    }

    if let Some(index) = notebook_index {
        let content_end_cell = index.cell(start).unwrap_or(OneIndexed::MIN);
        while line > start {
            if index.cell(line).unwrap_or(OneIndexed::MIN) == content_end_cell {
                break;
            }
            line = line.saturating_sub(1);
        }
    }

    line
}

/// Given some source code and annotation ranges, this routine replaces
/// unprintable characters with printable representations of them.
///
/// The source code and annotations returned are updated to reflect changes made
/// to the source code (if any).
///
/// We don't need to normalize whitespace, such as converting tabs to spaces,
/// because `annotate-snippets` handles that internally. Similarly, it's safe to
/// modify the annotation ranges by inserting 3-byte Unicode replacements
/// because `annotate-snippets` will account for their actual width when
/// rendering and displaying the column to the user.
fn replace_unprintable<'r>(
    source: &'r str,
    mut annotations: Vec<RenderableAnnotation<'r>>,
) -> EscapedSourceCode<'r> {
    // Updates the annotation ranges given by the caller whenever a single byte (at `index` in
    // `source`) is replaced with `len` bytes.
    //
    // When the index occurs before the start of the range, the range is
    // offset by `len`. When the range occurs after or at the start but before
    // the end, then the end of the range only is offset by `len`.
    let mut update_ranges = |index: usize, len: u32| {
        for ann in &mut annotations {
            if index < usize::from(ann.range.start()) {
                ann.range += TextSize::new(len - 1);
            } else if index < usize::from(ann.range.end()) {
                ann.range = ann.range.add_end(TextSize::new(len - 1));
            }
        }
    };

    // If `c` is an unprintable character, then this returns a printable
    // representation of it (using a fancier Unicode codepoint).
    let unprintable_replacement = |c: char| -> Option<char> {
        match c {
            '\x07' => Some('␇'),
            '\x08' => Some('␈'),
            '\x1b' => Some('␛'),
            '\x7f' => Some('␡'),
            _ => None,
        }
    };

    let mut last_end = 0;
    let mut result = String::new();
    for (index, c) in source.char_indices() {
        // normalize `\r` line endings but don't double `\r\n`
        if c == '\r' && !source[index + 1..].starts_with("\n") {
            result.push_str(&source[last_end..index]);
            result.push('\n');
            last_end = index + 1;
        } else if let Some(printable) = unprintable_replacement(c) {
            result.push_str(&source[last_end..index]);

            let len = printable.text_len().to_u32();
            update_ranges(result.text_len().to_usize(), len);

            result.push(printable);
            last_end = index + 1;
        }
    }

    // No tabs or unprintable chars
    if result.is_empty() {
        EscapedSourceCode {
            annotations,
            text: Cow::Borrowed(source),
        }
    } else {
        result.push_str(&source[last_end..]);
        EscapedSourceCode {
            annotations,
            text: Cow::Owned(result),
        }
    }
}

struct EscapedSourceCode<'r> {
    text: Cow<'r, str>,
    annotations: Vec<RenderableAnnotation<'r>>,
}

impl<'r> EscapedSourceCode<'r> {
    // This attempts to "fix up" the spans on each annotation  in the case where
    // it's an empty span immediately following a line terminator.
    //
    // At present, `annotate-snippets` (both upstream and our vendored copy)
    // will render annotations of such spans to point to the space immediately
    // following the previous line. But ideally, this should point to the space
    // immediately preceding the next line.
    //
    // After attempting to fix `annotate-snippets` and giving up after a couple
    // hours, this routine takes a different tact: it adjusts the span to be
    // non-empty and it will cover the first codepoint of the following line.
    // This forces `annotate-snippets` to point to the right place.
    //
    // See also: <https://github.com/astral-sh/ruff/issues/15509> and
    // `ruff_linter::message::text::SourceCode::fix_up_empty_spans_after_line_terminator`,
    // from which this was adapted.
    fn fix_up_empty_spans_after_line_terminator(mut self) -> EscapedSourceCode<'r> {
        for ann in &mut self.annotations {
            let range = ann.range;
            if !range.is_empty()
                || range.start() == TextSize::from(0)
                || range.start() >= self.text.text_len()
            {
                continue;
            }
            if !matches!(
                self.text.as_bytes()[range.start().to_usize() - 1],
                b'\n' | b'\r'
            ) {
                continue;
            }
            let start = range.start();
            let end = ceil_char_boundary(&self.text, start + TextSize::from(1));
            ann.range = TextRange::new(start, end);
        }

        self
    }
}

/// Finds the closest [`TextSize`] not less than the offset given for which
/// `is_char_boundary` is `true`. Unless the offset given is greater than
/// the length of the underlying contents, in which case, the length of the
/// contents is returned.
///
/// Can be replaced with `str::ceil_char_boundary` once it's stable.
///
/// # Examples
///
/// From `std`:
///
/// ```
/// use ruff_db::diagnostic::ceil_char_boundary;
/// use ruff_text_size::{Ranged, TextLen, TextSize};
///
/// let source = "❤️🧡💛💚💙💜";
/// assert_eq!(source.text_len(), TextSize::from(26));
/// assert!(!source.is_char_boundary(13));
///
/// let closest = ceil_char_boundary(source, TextSize::from(13));
/// assert_eq!(closest, TextSize::from(14));
/// assert_eq!(&source[..closest.to_usize()], "❤️🧡💛");
/// ```
///
/// Additional examples:
///
/// ```
/// use ruff_db::diagnostic::ceil_char_boundary;
/// use ruff_text_size::{Ranged, TextRange, TextSize};
///
/// let source = "Hello";
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(0)),
///     TextSize::from(0)
/// );
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(5)),
///     TextSize::from(5)
/// );
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(6)),
///     TextSize::from(5)
/// );
///
/// let source = "α";
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(0)),
///     TextSize::from(0)
/// );
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(1)),
///     TextSize::from(2)
/// );
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(2)),
///     TextSize::from(2)
/// );
///
/// assert_eq!(
///     ceil_char_boundary(source, TextSize::from(3)),
///     TextSize::from(2)
/// );
/// ```
pub fn ceil_char_boundary(text: &str, offset: TextSize) -> TextSize {
    let upper_bound = offset
        .to_u32()
        .saturating_add(4)
        .min(text.text_len().to_u32());
    (offset.to_u32()..upper_bound)
        .map(TextSize::from)
        .find(|offset| text.is_char_boundary(offset.to_usize()))
        .unwrap_or_else(|| TextSize::from(upper_bound))
}

/// A stub implementation of [`FileResolver`] intended for testing.
pub struct DummyFileResolver;

impl FileResolver for DummyFileResolver {
    fn path(&self, _file: File) -> &str {
        unimplemented!()
    }

    fn input(&self, _file: File) -> Input {
        unimplemented!()
    }

    fn notebook_index(&self, _file: &UnifiedFile) -> Option<NotebookIndex> {
        None
    }

    fn is_notebook(&self, _file: &UnifiedFile) -> bool {
        false
    }

    fn current_directory(&self) -> &Path {
        Path::new(".")
    }
}

#[cfg(test)]
mod tests {

    use ruff_diagnostics::{Applicability, Edit, Fix};

    use crate::diagnostic::{
        Annotation, DiagnosticId, IntoDiagnosticMessage, SecondaryCode, Severity, Span,
        SubDiagnosticSeverity,
    };
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
cantaloupe
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        warning[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        info[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
    fn no_range() {
        let mut env = TestEnvironment::new();
        env.add("animals", ANIMALS);

        let mut builder = env.err();
        builder
            .diag
            .annotate(Annotation::primary(builder.env.path("animals")));
        let diag = builder.build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
          |
        1 | aardvark
          | ^
        2 | beetle
        3 | canary
          |
        ",
        );

        let mut builder = env.err();
        builder.diag.annotate(
            Annotation::primary(builder.env.path("animals")).message("primary annotation message"),
        );
        let diag = builder.build();
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
          |
        1 | aardvark
          | ^ primary annotation message
        2 | beetle
        3 | canary
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
        error[test-diagnostic]: main diagnostic message
         --> non-ascii:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> non-ascii:2:2
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:11:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:1:1
           |
         1 | aardvark
           | ^^^^^^^^
         2 | beetle
         3 | canary
           |
          ::: animals:11:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
          |
        1 | aardvark
          | ^^^^^^^^
        2 | beetle
          |
         ::: animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:1:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:1:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:1:1
           |
         1 | aardvark
           | ^^^^^^^^
         2 | beetle
         3 | canary
         4 | dog
           |
          ::: animals:9:1
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
        error[test-diagnostic]: main diagnostic message
         --> spacey-animals:8:1
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
        error[test-diagnostic]: main diagnostic message
          --> spacey-animals:12:1
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
        error[test-diagnostic]: main diagnostic message
          --> spacey-animals:13:1
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
        error[test-diagnostic]: main diagnostic message
         --> spacey-animals:3:1
          |
        3 | beetle
          | ^^^^^^
          |
         ::: spacey-animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
         ::: fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantaloupe
          | ^^^^^^^^^^
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
            env.sub_builder(SubDiagnosticSeverity::Info, "this is a helpful note")
                .build(),
        );
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
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
            env.sub_builder(SubDiagnosticSeverity::Info, "this is a helpful note")
                .build(),
        );
        diag.sub(
            env.sub_builder(SubDiagnosticSeverity::Info, "another helpful note")
                .build(),
        );
        diag.sub(
            env.sub_builder(SubDiagnosticSeverity::Info, "and another helpful note")
                .build(),
        );
        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
         --> fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantaloupe
          | ^^^^^^^^^^
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
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
         --> fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantaloupe
          | ^^^^^^^^^^
        4 | lime
        5 | orange
          |
        warning: sub-diagnostic message
          --> animals:11:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
          --> animals:11:1
           |
         9 | inchworm
        10 | jackrabbit
        11 | kangaroo
           | ^^^^^^^^
           |
        warning: sub-diagnostic message
         --> fruits:3:1
          |
        1 | apple
        2 | banana
        3 | cantaloupe
          | ^^^^^^^^^^
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
        error[test-diagnostic]: main diagnostic message
         --> animals:3:1
          |
        1 | aardvark
        2 | beetle
        3 | canary
          | ^^^^^^
        4 | dog
        5 | elephant
          |
        warning: sub-diagnostic message
         --> animals:3:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:5:4
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
        error[test-diagnostic]: main diagnostic message
          --> animals:5:4
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
        error[test-diagnostic]: main diagnostic message
         --> animals:4:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:4:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:3
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:3
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
        error[test-diagnostic]: main diagnostic message
          --> animals:8:1
           |
         6 | finch
         7 | gorilla
         8 | hippopotamus
           | ^^^^^^^^^^^^ primary
         9 | inchworm
        10 | jackrabbit
           |
          ::: animals:1:1
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
        error[test-diagnostic]: main diagnostic message
         --> animals:5:1
          |
        5 | elephant
          | ^^^^^^^^ primary 5
          |
         ::: animals:9:1
          |
        9 | inchworm
          | ^^^^^^^^ primary 9
          |
         ::: animals:1:1
          |
        1 | aardvark
          | -------- secondary 1
          |
         ::: animals:3:1
          |
        3 | canary
          | ------ secondary 3
          |
         ::: animals:7:1
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
        error[test-diagnostic]: main diagnostic message
         --> fruits:1:1
          |
        1 | apple
          | ^^^^^ primary
        2 | banana
        3 | cantaloupe
          |
         ::: animals:1:1
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
        error[test-diagnostic]: main diagnostic message
          --> animals:11:1
           |
        11 | kangaroo
           | ^^^^^^^^ primary animals 11
           |
          ::: animals:1:1
           |
         1 | aardvark
           | -------- secondary animals 1
           |
          ::: animals:3:1
           |
         3 | canary
           | ------ secondary animals 3
           |
          ::: animals:7:1
           |
         7 | gorilla
           | ------- secondary animals 7
           |
          ::: fruits:10:1
           |
        10 | watermelon
           | ^^^^^^^^^^ primary fruits 10
           |
          ::: fruits:2:1
           |
         2 | banana
           | ------ secondary fruits 2
           |
        ",
        );
    }

    /// A small harness for setting up an environment specifically for testing
    /// diagnostic rendering.
    pub(super) struct TestEnvironment {
        db: TestDb,
        config: DisplayDiagnosticConfig,
    }

    impl TestEnvironment {
        /// Create a new test harness.
        ///
        /// This uses the default diagnostic rendering configuration.
        pub(super) fn new() -> TestEnvironment {
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

        /// Set the output format to use in diagnostic rendering.
        pub(super) fn format(&mut self, format: DiagnosticFormat) {
            let mut config = std::mem::take(&mut self.config);
            config = config.format(format);
            self.config = config;
        }

        /// Enable preview functionality for diagnostic rendering.
        #[allow(
            dead_code,
            reason = "This is currently only used for JSON but will be needed soon for other formats"
        )]
        pub(super) fn preview(&mut self, yes: bool) {
            let mut config = std::mem::take(&mut self.config);
            config = config.preview(yes);
            self.config = config;
        }

        /// Hide diagnostic severity when rendering.
        pub(super) fn hide_severity(&mut self, yes: bool) {
            let mut config = std::mem::take(&mut self.config);
            config = config.hide_severity(yes);
            self.config = config;
        }

        /// Show fix availability when rendering.
        pub(super) fn show_fix_status(&mut self, yes: bool) {
            let mut config = std::mem::take(&mut self.config);
            config = config.with_show_fix_status(yes);
            self.config = config;
        }

        /// Show a diff for the fix when rendering.
        pub(super) fn show_fix_diff(&mut self, yes: bool) {
            let mut config = std::mem::take(&mut self.config);
            config = config.show_fix_diff(yes);
            self.config = config;
        }

        /// The lowest fix applicability to show when rendering.
        pub(super) fn fix_applicability(&mut self, applicability: Applicability) {
            let mut config = std::mem::take(&mut self.config);
            config = config.with_fix_applicability(applicability);
            self.config = config;
        }

        /// Add a file with the given path and contents to this environment.
        pub(super) fn add(&mut self, path: &str, contents: &str) {
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
        pub(super) fn span(
            &self,
            path: &str,
            line_offset_start: &str,
            line_offset_end: &str,
        ) -> Span {
            let span = self.path(path);

            let file = span.expect_ty_file();
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
            span.with_range(TextRange::new(start, end))
        }

        /// Like `span`, but only attaches a file path.
        pub(super) fn path(&self, path: &str) -> Span {
            let file = system_path_to_file(&self.db, path).unwrap();
            Span::from(file)
        }

        /// A convenience function for returning a builder for a diagnostic
        /// with "error" severity and canned values for its identifier
        /// and message.
        pub(super) fn err(&mut self) -> DiagnosticBuilder<'_> {
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
            self.sub_builder(SubDiagnosticSeverity::Warning, "sub-diagnostic message")
        }

        /// Returns a builder for tersely constructing diagnostics.
        pub(super) fn builder(
            &mut self,
            identifier: &'static str,
            severity: Severity,
            message: &str,
        ) -> DiagnosticBuilder<'_> {
            let diag = Diagnostic::new(id(identifier), severity, message);
            DiagnosticBuilder { env: self, diag }
        }

        /// A convenience function for returning a builder for an invalid syntax diagnostic.
        fn invalid_syntax(&mut self, message: &str) -> DiagnosticBuilder<'_> {
            let diag = Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Error, message);
            DiagnosticBuilder { env: self, diag }
        }

        /// Returns a builder for tersely constructing sub-diagnostics.
        fn sub_builder(
            &mut self,
            severity: SubDiagnosticSeverity,
            message: &str,
        ) -> SubDiagnosticBuilder<'_> {
            let subdiag = SubDiagnostic::new(severity, message);
            SubDiagnosticBuilder { env: self, subdiag }
        }

        /// Render the given diagnostic into a `String`.
        ///
        /// (This will set the "printed" flag on `Diagnostic`.)
        pub(super) fn render(&self, diag: &Diagnostic) -> String {
            diag.display(&self.db, &self.config).to_string()
        }

        /// Render the given diagnostics into a `String`.
        ///
        /// See `render` for rendering a single diagnostic.
        ///
        /// (This will set the "printed" flag on `Diagnostic`.)
        pub(super) fn render_diagnostics(&self, diagnostics: &[Diagnostic]) -> String {
            DisplayDiagnostics::new(&self.db, &self.config, diagnostics).to_string()
        }
    }

    /// A helper builder for tersely populating a `Diagnostic`.
    ///
    /// If you need to mutate the diagnostic in a way that isn't
    /// supported by this builder, and this only needs to be done
    /// infrequently, consider doing it more verbosely on `diag`
    /// itself.
    pub(super) struct DiagnosticBuilder<'e> {
        env: &'e mut TestEnvironment,
        diag: Diagnostic,
    }

    impl<'e> DiagnosticBuilder<'e> {
        /// Return the built diagnostic.
        pub(super) fn build(self) -> Diagnostic {
            self.diag
        }

        /// Add a primary annotation with a message.
        ///
        /// If the message is empty, then an annotation without any
        /// message be created.
        ///
        /// See the docs on `TestEnvironment::span` for the meaning of
        /// `path`, `line_offset_start` and `line_offset_end`.
        pub(super) fn primary(
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
        pub(super) fn secondary(
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

        /// Set the secondary code on the diagnostic.
        fn secondary_code(mut self, secondary_code: &str) -> DiagnosticBuilder<'e> {
            self.diag
                .set_secondary_code(SecondaryCode::new(secondary_code.to_string()));
            self
        }

        /// Set the fix on the diagnostic.
        pub(super) fn fix(mut self, fix: Fix) -> DiagnosticBuilder<'e> {
            self.diag.set_fix(fix);
            self
        }

        /// Set the noqa offset on the diagnostic.
        fn noqa_offset(mut self, noqa_offset: TextSize) -> DiagnosticBuilder<'e> {
            self.diag.set_noqa_offset(noqa_offset);
            self
        }

        /// Adds a "help" sub-diagnostic with the given message.
        pub(super) fn help(mut self, message: impl IntoDiagnosticMessage) -> DiagnosticBuilder<'e> {
            self.diag.help(message);
            self
        }

        /// Set the documentation URL for the diagnostic.
        pub(super) fn documentation_url(mut self, url: impl Into<String>) -> DiagnosticBuilder<'e> {
            self.diag.set_documentation_url(Some(url.into()));
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

    /// Create Ruff-style diagnostics for testing the various output formats.
    pub(crate) fn create_diagnostics(
        format: DiagnosticFormat,
    ) -> (TestEnvironment, Vec<Diagnostic>) {
        let mut env = TestEnvironment::new();
        env.add(
            "fib.py",
            r#"import os


def fibonacci(n):
    """Compute the nth number in the Fibonacci sequence."""
    x = 1
    if n == 0:
        return 0
    elif n == 1:
        return 1
    else:
        return fibonacci(n - 1) + fibonacci(n - 2)
"#,
        );
        env.add("undef.py", r"if a == 1: pass");
        env.format(format);

        let diagnostics = vec![
            env.builder("unused-import", Severity::Error, "`os` imported but unused")
                .primary("fib.py", "1:7", "1:9", "")
                .help("Remove unused import: `os`")
                .secondary_code("F401")
                .fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                    TextSize::from(0),
                    TextSize::from(10),
                ))))
                .noqa_offset(TextSize::from(7))
                .documentation_url("https://docs.astral.sh/ruff/rules/unused-import")
                .build(),
            env.builder(
                "unused-variable",
                Severity::Error,
                "Local variable `x` is assigned to but never used",
            )
            .primary("fib.py", "6:4", "6:5", "")
            .help("Remove assignment to unused variable `x`")
            .secondary_code("F841")
            .fix(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(99),
            )))
            .noqa_offset(TextSize::from(94))
            .documentation_url("https://docs.astral.sh/ruff/rules/unused-variable")
            .build(),
            env.builder("undefined-name", Severity::Error, "Undefined name `a`")
                .primary("undef.py", "1:3", "1:4", "")
                .secondary_code("F821")
                .noqa_offset(TextSize::from(3))
                .documentation_url("https://docs.astral.sh/ruff/rules/undefined-name")
                .build(),
        ];

        (env, diagnostics)
    }

    /// Create Ruff-style syntax error diagnostics for testing the various output formats.
    pub(crate) fn create_syntax_error_diagnostics(
        format: DiagnosticFormat,
    ) -> (TestEnvironment, Vec<Diagnostic>) {
        let mut env = TestEnvironment::new();
        env.add(
            "syntax_errors.py",
            r"from os import

if call(foo
    def bar():
        pass
",
        );
        env.format(format);

        let diagnostics = vec![
            env.invalid_syntax("Expected one or more symbol names after import")
                .primary("syntax_errors.py", "1:14", "1:15", "")
                .build(),
            env.invalid_syntax("Expected ')', found newline")
                .primary("syntax_errors.py", "3:11", "3:12", "")
                .build(),
        ];

        (env, diagnostics)
    }

    /// A Jupyter notebook for testing diagnostics.
    ///
    ///
    /// The concatenated cells look like this:
    ///
    /// ```python
    /// # cell 1
    /// import os
    /// # cell 2
    /// import math
    ///
    /// print('hello world')
    /// # cell 3
    /// def foo():
    ///     print()
    ///     x = 1
    /// ```
    ///
    /// The first diagnostic is on the unused `os` import with location cell 1, row 2, column 8
    /// (`cell 1:2:8`). The second diagnostic is the unused `math` import at `cell 2:2:8`, and the
    /// third diagnostic is an unfixable unused variable at `cell 3:4:5`.
    pub(super) static NOTEBOOK: &str = r##"
        {
 "cells": [
  {
   "cell_type": "code",
   "metadata": {},
   "outputs": [],
   "source": [
    "# cell 1\n",
    "import os"
   ]
  },
  {
   "cell_type": "code",
   "metadata": {},
   "outputs": [],
   "source": [
    "# cell 2\n",
    "import math\n",
    "\n",
    "print('hello world')"
   ]
  },
  {
   "cell_type": "code",
   "metadata": {},
   "outputs": [],
   "source": [
    "# cell 3\n",
    "def foo():\n",
    "    print()\n",
    "    x = 1\n"
   ]
  }
 ],
 "metadata": {},
 "nbformat": 4,
 "nbformat_minor": 5
}
"##;

    /// Create Ruff-style diagnostics for testing the various output formats for a notebook.
    pub(crate) fn create_notebook_diagnostics(
        format: DiagnosticFormat,
    ) -> (TestEnvironment, Vec<Diagnostic>) {
        let mut env = TestEnvironment::new();
        env.add("notebook.ipynb", NOTEBOOK);
        env.format(format);

        let diagnostics = vec![
            env.builder("unused-import", Severity::Error, "`os` imported but unused")
                .primary("notebook.ipynb", "2:7", "2:9", "")
                .help("Remove unused import: `os`")
                .secondary_code("F401")
                .fix(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                    TextSize::from(9),
                    TextSize::from(19),
                ))))
                .noqa_offset(TextSize::from(16))
                .documentation_url("https://docs.astral.sh/ruff/rules/unused-import")
                .build(),
            env.builder(
                "unused-import",
                Severity::Error,
                "`math` imported but unused",
            )
            .primary("notebook.ipynb", "4:7", "4:11", "")
            .help("Remove unused import: `math`")
            .secondary_code("F401")
            .fix(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(28),
                TextSize::from(40),
            ))))
            .noqa_offset(TextSize::from(35))
            .documentation_url("https://docs.astral.sh/ruff/rules/unused-import")
            .build(),
            env.builder(
                "unused-variable",
                Severity::Error,
                "Local variable `x` is assigned to but never used",
            )
            .primary("notebook.ipynb", "10:4", "10:5", "")
            .help("Remove assignment to unused variable `x`")
            .secondary_code("F841")
            .fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(94),
                TextSize::from(104),
            ))))
            .noqa_offset(TextSize::from(98))
            .documentation_url("https://docs.astral.sh/ruff/rules/unused-variable")
            .build(),
        ];

        (env, diagnostics)
    }
}

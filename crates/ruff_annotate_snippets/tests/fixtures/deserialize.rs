use serde::Deserialize;
use std::ops::Range;

use ruff_annotate_snippets::renderer::DEFAULT_TERM_WIDTH;
use ruff_annotate_snippets::{Annotation, Level, Message, Renderer, Snippet};

#[derive(Deserialize)]
pub(crate) struct Fixture {
    #[serde(default)]
    pub(crate) renderer: RendererDef,
    pub(crate) message: MessageDef,
}

#[derive(Deserialize)]
pub struct MessageDef {
    #[serde(with = "LevelDef")]
    pub level: Level,
    pub title: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub footer: Vec<MessageDef>,
    pub snippets: Vec<SnippetDef>,
}

impl<'a> From<&'a MessageDef> for Message<'a> {
    fn from(val: &'a MessageDef) -> Self {
        let MessageDef {
            level,
            title,
            id,
            footer,
            snippets,
        } = val;
        let mut message = level.title(title);
        if let Some(id) = id {
            message = message.id(id);
        }
        message = message.snippets(snippets.iter().map(Snippet::from));
        message = message.footers(footer.iter().map(Into::into));
        message
    }
}

#[derive(Deserialize)]
pub struct SnippetDef {
    pub source: String,
    pub line_start: usize,
    pub origin: Option<String>,
    pub annotations: Vec<AnnotationDef>,
    #[serde(default)]
    pub fold: bool,
}

impl<'a> From<&'a SnippetDef> for Snippet<'a> {
    fn from(val: &'a SnippetDef) -> Self {
        let SnippetDef {
            source,
            line_start,
            origin,
            annotations,
            fold,
        } = val;
        let mut snippet = Snippet::source(source).line_start(*line_start).fold(*fold);
        if let Some(origin) = origin {
            snippet = snippet.origin(origin);
        }
        snippet = snippet.annotations(annotations.iter().map(Into::into));
        snippet
    }
}

#[derive(Deserialize)]
pub struct AnnotationDef {
    pub range: Range<usize>,
    pub label: String,
    #[serde(with = "LevelDef")]
    pub level: Level,
}

impl<'a> From<&'a AnnotationDef> for Annotation<'a> {
    fn from(val: &'a AnnotationDef) -> Self {
        let AnnotationDef {
            range,
            label,
            level,
        } = val;
        level.span(range.start..range.end).label(label)
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "Level")]
enum LevelDef {
    Error,
    Warning,
    Info,
    Note,
    Help,
}

#[derive(Default, Deserialize)]
pub struct RendererDef {
    #[serde(default)]
    anonymized_line_numbers: bool,
    #[serde(default)]
    term_width: Option<usize>,
    #[serde(default)]
    color: bool,
}

impl From<RendererDef> for Renderer {
    fn from(val: RendererDef) -> Self {
        let RendererDef {
            anonymized_line_numbers,
            term_width,
            color,
        } = val;

        let renderer = if color {
            Renderer::styled()
        } else {
            Renderer::plain()
        };
        renderer
            .anonymized_line_numbers(anonymized_line_numbers)
            .term_width(term_width.unwrap_or(DEFAULT_TERM_WIDTH))
    }
}

use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;

use ruff_python_ast::docstrings::{leading_space, leading_words};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use strum_macros::EnumIter;

use ruff_source_file::{Line, NewlineWithTrailingNewline, UniversalNewlines};

use crate::docstrings::styles::SectionStyle;
use crate::docstrings::{Docstring, DocstringBody};

#[derive(EnumIter, PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum SectionKind {
    Args,
    Arguments,
    Attention,
    Attributes,
    Caution,
    Danger,
    Error,
    Example,
    Examples,
    ExtendedSummary,
    Hint,
    Important,
    KeywordArgs,
    KeywordArguments,
    Methods,
    Note,
    Notes,
    OtherArgs,
    OtherArguments,
    OtherParams,
    OtherParameters,
    Parameters,
    Raises,
    References,
    Return,
    Returns,
    SeeAlso,
    ShortSummary,
    Tip,
    Todo,
    Warning,
    Warnings,
    Warns,
    Yield,
    Yields,
}

impl SectionKind {
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "args" => Some(Self::Args),
            "arguments" => Some(Self::Arguments),
            "attention" => Some(Self::Attention),
            "attributes" => Some(Self::Attributes),
            "caution" => Some(Self::Caution),
            "danger" => Some(Self::Danger),
            "error" => Some(Self::Error),
            "example" => Some(Self::Example),
            "examples" => Some(Self::Examples),
            "extended summary" => Some(Self::ExtendedSummary),
            "hint" => Some(Self::Hint),
            "important" => Some(Self::Important),
            "keyword args" => Some(Self::KeywordArgs),
            "keyword arguments" => Some(Self::KeywordArguments),
            "methods" => Some(Self::Methods),
            "note" => Some(Self::Note),
            "notes" => Some(Self::Notes),
            "other args" => Some(Self::OtherArgs),
            "other arguments" => Some(Self::OtherArguments),
            "other params" => Some(Self::OtherParams),
            "other parameters" => Some(Self::OtherParameters),
            "parameters" => Some(Self::Parameters),
            "raises" => Some(Self::Raises),
            "references" => Some(Self::References),
            "return" => Some(Self::Return),
            "returns" => Some(Self::Returns),
            "see also" => Some(Self::SeeAlso),
            "short summary" => Some(Self::ShortSummary),
            "tip" => Some(Self::Tip),
            "todo" => Some(Self::Todo),
            "warning" => Some(Self::Warning),
            "warnings" => Some(Self::Warnings),
            "warns" => Some(Self::Warns),
            "yield" => Some(Self::Yield),
            "yields" => Some(Self::Yields),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Args => "Args",
            Self::Arguments => "Arguments",
            Self::Attention => "Attention",
            Self::Attributes => "Attributes",
            Self::Caution => "Caution",
            Self::Danger => "Danger",
            Self::Error => "Error",
            Self::Example => "Example",
            Self::Examples => "Examples",
            Self::ExtendedSummary => "Extended Summary",
            Self::Hint => "Hint",
            Self::Important => "Important",
            Self::KeywordArgs => "Keyword Args",
            Self::KeywordArguments => "Keyword Arguments",
            Self::Methods => "Methods",
            Self::Note => "Note",
            Self::Notes => "Notes",
            Self::OtherArgs => "Other Args",
            Self::OtherArguments => "Other Arguments",
            Self::OtherParams => "Other Params",
            Self::OtherParameters => "Other Parameters",
            Self::Parameters => "Parameters",
            Self::Raises => "Raises",
            Self::References => "References",
            Self::Return => "Return",
            Self::Returns => "Returns",
            Self::SeeAlso => "See Also",
            Self::ShortSummary => "Short Summary",
            Self::Tip => "Tip",
            Self::Todo => "Todo",
            Self::Warning => "Warning",
            Self::Warnings => "Warnings",
            Self::Warns => "Warns",
            Self::Yield => "Yield",
            Self::Yields => "Yields",
        }
    }

    /// Returns `true` if a section can contain subsections, as in:
    /// ```python
    /// Yields
    /// ------
    /// int
    ///     Description of the anonymous integer return value.
    /// ```
    ///
    /// For NumPy, see: <https://numpydoc.readthedocs.io/en/latest/format.html>
    ///
    /// For Google, see: <https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings>
    pub(crate) fn has_subsections(self) -> bool {
        matches!(
            self,
            Self::Args
                | Self::Arguments
                | Self::OtherArgs
                | Self::OtherParameters
                | Self::OtherParams
                | Self::Parameters
                | Self::Raises
                | Self::Returns
                | Self::SeeAlso
                | Self::Warns
                | Self::Yields
        )
    }
}

pub(crate) struct SectionContexts<'a> {
    contexts: Vec<SectionContextData>,
    docstring: &'a Docstring<'a>,
    style: SectionStyle,
}

impl<'a> SectionContexts<'a> {
    /// Extract all `SectionContext` values from a docstring.
    pub(crate) fn from_docstring(docstring: &'a Docstring<'a>, style: SectionStyle) -> Self {
        let contents = docstring.body();

        let mut contexts = Vec::new();
        let mut last: Option<SectionContextData> = None;

        let mut lines = contents.universal_newlines().peekable();

        // Skip the first line, which is the summary.
        let mut previous_line = lines.next();

        while let Some(line) = lines.next() {
            if let Some(section_kind) = suspected_as_section(&line, style) {
                let indent = leading_space(&line);
                let indent_size = indent.text_len();

                let section_name = leading_words(&line);
                let section_name_size = section_name.text_len();

                if is_docstring_section(
                    &line,
                    indent_size,
                    section_name_size,
                    section_kind,
                    last.as_ref(),
                    previous_line.as_ref(),
                    lines.peek(),
                ) {
                    if let Some(mut last) = last.take() {
                        last.range = TextRange::new(last.start(), line.start());
                        contexts.push(last);
                    }

                    last = Some(SectionContextData {
                        kind: section_kind,
                        indent_size: indent.text_len(),
                        name_range: TextRange::at(line.start() + indent_size, section_name_size),
                        range: TextRange::empty(line.start()),
                        summary_full_end: line.full_end(),
                    });
                }
            }

            previous_line = Some(line);
        }

        if let Some(mut last) = last.take() {
            last.range = TextRange::new(last.start(), contents.text_len());
            contexts.push(last);
        }

        Self {
            contexts,
            docstring,
            style,
        }
    }

    pub(crate) fn style(&self) -> SectionStyle {
        self.style
    }

    pub(crate) fn len(&self) -> usize {
        self.contexts.len()
    }

    pub(crate) fn iter(&self) -> SectionContextsIter {
        SectionContextsIter {
            docstring_body: self.docstring.body(),
            inner: self.contexts.iter(),
        }
    }
}

impl<'a> IntoIterator for &'a SectionContexts<'a> {
    type Item = SectionContext<'a>;
    type IntoIter = SectionContextsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Debug for SectionContexts<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

pub(crate) struct SectionContextsIter<'a> {
    docstring_body: DocstringBody<'a>,
    inner: std::slice::Iter<'a, SectionContextData>,
}

impl<'a> Iterator for SectionContextsIter<'a> {
    type Item = SectionContext<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;

        Some(SectionContext {
            data: next,
            docstring_body: self.docstring_body,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a> DoubleEndedIterator for SectionContextsIter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let back = self.inner.next_back()?;
        Some(SectionContext {
            data: back,
            docstring_body: self.docstring_body,
        })
    }
}

impl FusedIterator for SectionContextsIter<'_> {}
impl ExactSizeIterator for SectionContextsIter<'_> {}

#[derive(Debug)]
struct SectionContextData {
    kind: SectionKind,

    /// The size of the indentation of the section name.
    indent_size: TextSize,

    /// Range of the section name, relative to the [`Docstring::body`]
    name_range: TextRange,

    /// Range from the start to the end of the section, relative to the [`Docstring::body`]
    range: TextRange,

    /// End of the summary, relative to the [`Docstring::body`]
    summary_full_end: TextSize,
}

impl Ranged for SectionContextData {
    fn range(&self) -> TextRange {
        self.range
    }
}

pub(crate) struct SectionContext<'a> {
    data: &'a SectionContextData,
    docstring_body: DocstringBody<'a>,
}

impl<'a> SectionContext<'a> {
    /// The `kind` of the section, e.g. [`SectionKind::Args`] or [`SectionKind::Returns`].
    pub(crate) const fn kind(&self) -> SectionKind {
        self.data.kind
    }

    /// The name of the section as it appears in the docstring, e.g. "Args" or "Returns".
    pub(crate) fn section_name(&self) -> &'a str {
        &self.docstring_body.as_str()[self.data.name_range]
    }

    /// Returns the rest of the summary line after the section name.
    pub(crate) fn summary_after_section_name(&self) -> &'a str {
        &self.summary_line()[usize::from(self.data.name_range.end() - self.data.range.start())..]
    }

    fn offset(&self) -> TextSize {
        self.docstring_body.start()
    }

    /// The absolute range of the section name
    pub(crate) fn section_name_range(&self) -> TextRange {
        self.data.name_range + self.offset()
    }

    /// The absolute range of the summary line, excluding any trailing newline character.
    pub(crate) fn summary_range(&self) -> TextRange {
        TextRange::at(self.range().start(), self.summary_line().text_len())
    }

    /// Range of the summary line relative to [`Docstring::body`], including the trailing newline character.
    fn summary_full_range_relative(&self) -> TextRange {
        TextRange::new(self.range_relative().start(), self.data.summary_full_end)
    }

    /// Returns the range of this section relative to [`Docstring::body`]
    const fn range_relative(&self) -> TextRange {
        self.data.range
    }

    /// The absolute range of the full-section.
    pub(crate) fn range(&self) -> TextRange {
        self.range_relative() + self.offset()
    }

    /// Summary line without the trailing newline characters
    pub(crate) fn summary_line(&self) -> &'a str {
        let full_summary = &self.docstring_body.as_str()[self.summary_full_range_relative()];

        let mut bytes = full_summary.bytes().rev();

        let newline_width = match bytes.next() {
            Some(b'\n') => {
                if bytes.next() == Some(b'\r') {
                    2
                } else {
                    1
                }
            }
            Some(b'\r') => 1,
            _ => 0,
        };

        &full_summary[..full_summary.len() - newline_width]
    }

    /// Returns the text of the last line of the previous section or an empty string if it is the first section.
    pub(crate) fn previous_line(&self) -> Option<&'a str> {
        let previous =
            &self.docstring_body.as_str()[TextRange::up_to(self.range_relative().start())];
        previous
            .universal_newlines()
            .last()
            .map(|line| line.as_str())
    }

    /// Returns the lines belonging to this section after the summary line.
    pub(crate) fn following_lines(&self) -> NewlineWithTrailingNewline<'a> {
        let lines = self.following_lines_str();
        NewlineWithTrailingNewline::with_offset(lines, self.offset() + self.data.summary_full_end)
    }

    pub(crate) fn following_lines_str(&self) -> &'a str {
        &self.docstring_body.as_str()[self.following_range_relative()]
    }

    /// Returns the range to the following lines relative to [`Docstring::body`].
    const fn following_range_relative(&self) -> TextRange {
        TextRange::new(self.data.summary_full_end, self.range_relative().end())
    }

    /// Returns the absolute range of the following lines.
    pub(crate) fn following_range(&self) -> TextRange {
        self.following_range_relative() + self.offset()
    }
}

impl Ranged for SectionContext<'_> {
    fn range(&self) -> TextRange {
        self.range()
    }
}

impl Debug for SectionContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SectionContext")
            .field("kind", &self.kind())
            .field("section_name", &self.section_name())
            .field("summary_line", &self.summary_line())
            .field("following_lines", &&self.following_lines_str())
            .finish()
    }
}

fn suspected_as_section(line: &str, style: SectionStyle) -> Option<SectionKind> {
    if let Some(kind) = SectionKind::from_str(leading_words(line)) {
        if style.sections().contains(&kind) {
            return Some(kind);
        }
    }
    None
}

/// Check if the suspected context is really a section header.
fn is_docstring_section(
    line: &Line,
    indent_size: TextSize,
    section_name_size: TextSize,
    section_kind: SectionKind,
    previous_section: Option<&SectionContextData>,
    previous_line: Option<&Line>,
    next_line: Option<&Line>,
) -> bool {
    // Determine whether the current line looks like a section header, e.g., "Args:".
    let section_name_suffix = line[usize::from(indent_size + section_name_size)..].trim();
    let this_looks_like_a_section_name =
        section_name_suffix == ":" || section_name_suffix.is_empty();
    if !this_looks_like_a_section_name {
        return false;
    }

    // Determine whether the next line is an underline, e.g., "-----".
    let next_line_is_underline = next_line.is_some_and(|next_line| {
        let next_line = next_line.trim();
        if next_line.is_empty() {
            false
        } else {
            let next_line_is_underline = next_line.chars().all(|char| matches!(char, '-' | '='));
            next_line_is_underline
        }
    });
    if next_line_is_underline {
        return true;
    }

    // Determine whether the previous line looks like the end of a paragraph.
    let previous_line_looks_like_end_of_paragraph = previous_line.map_or(true, |previous_line| {
        let previous_line = previous_line.trim();
        let previous_line_ends_with_punctuation = [',', ';', '.', '-', '\\', '/', ']', '}', ')']
            .into_iter()
            .any(|char| previous_line.ends_with(char));
        previous_line_ends_with_punctuation || previous_line.is_empty()
    });
    if !previous_line_looks_like_end_of_paragraph {
        return false;
    }

    // Determine if this is a sub-section within another section, like `args` in:
    // ```python
    // def func(args: tuple[int]):
    //     """Toggle the gizmo.
    //
    //     Args:
    //         args: The arguments to the function.
    //     """
    // ```
    // Or `parameters` in:
    // ```python
    // def func(parameters: tuple[int]):
    //     """Toggle the gizmo.
    //
    //     Parameters:
    //     -----
    //     parameters:
    //         The arguments to the function.
    //     """
    // ```
    // However, if the header is an _exact_ match (like `Returns:`, as opposed to `returns:`), then
    // continue to treat it as a section header.
    if section_kind.has_subsections() {
        if let Some(previous_section) = previous_section {
            let verbatim = &line[TextRange::at(indent_size, section_name_size)];

            // If the section is more deeply indented, assume it's a subsection, as in:
            // ```python
            // def func(args: tuple[int]):
            //     """Toggle the gizmo.
            //
            //     Args:
            //         args: The arguments to the function.
            //     """
            // ```
            if previous_section.indent_size < indent_size {
                if section_kind.as_str() != verbatim {
                    return false;
                }
            }

            // If the section isn't underlined, and isn't title-cased, assume it's a subsection,
            // as in:
            // ```python
            // def func(parameters: tuple[int]):
            //     """Toggle the gizmo.
            //
            //     Parameters:
            //     -----
            //     parameters:
            //         The arguments to the function.
            //     """
            // ```
            if !next_line_is_underline && verbatim.chars().next().is_some_and(char::is_lowercase) {
                if section_kind.as_str() != verbatim {
                    return false;
                }
            }
        }
    }

    true
}

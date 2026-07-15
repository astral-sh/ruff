//! Renders "direct" HTTP(S) reST hyperlinks in prose (i.e., those links whose
//! destination appears directly in the link markup).
//!
//! The following forms are unsupported and are passed through this rendered
//! without modification:
//!
//! - Sphinx roles such as `:ref:`, because they depend on Sphinx context
//! - Relative targets, because a standalone docstring has no document base
//! - Named references, because they require collecting and resolving target
//!   definitions across the entire docstring
//!
//! The set of links we've chosen to support was informed by a point-in-time
//! review of 475 direct links that appeared in a sample of popular public
//! Python repos (including pytorch, scikit-learn, pandas, and numpy). The scope
//! remains deliberately limited to this link family rather than allowing an
//! expansion into a fully-general reST parser.
//!
//! # Examples
//!
//! We support multi-line links with line breaks that appear anywher in the
//! link label or URI:
//!
//! ```text
//! `Sesame <https://cds.unistra.fr/cgi-bin/Sesame>`_
//!
//! `Schechter
//! 1976 <https://example.com/paper>`_
//!
//! `Citation author,
//! "Citation title,"
//! Journal and year.
//! <https://example.com/paper>`_
//!
//! `strftime documentation <https://docs.python.org/3/
//! library/datetime.html#strftime-and-strptime-behavior>`_
//! ```
//!
//! These render as:
//!
//! ```text
//! [Sesame](https://cds.unistra.fr/cgi-bin/Sesame)
//!
//! [Schechter 1976](https://example.com/paper)
//!
//! [Citation author, "Citation title," Journal and year.](https://example.com/paper)
//!
//! [strftime documentation](https://docs.python.org/3/library/datetime.html#strftime-and-strptime-behavior)
//! ```

use std::borrow::Cow;

use ruff_text_size::TextSize;

use crate::docstring::document::syntax::{
    find_backtick_run, is_backtick_run_escaped, markdown_code_span,
};

/// Exposes an interface for rendering a line of prose that may contain a hyperlink.
#[derive(Default)]
pub(super) struct Renderer {
    pending_link: Option<PendingLink>,
}

impl Renderer {
    /// Renders a prose line, buffering a supported wrapped hyperlink when necessary.
    pub(super) fn render_line(&mut self, output: &mut String, line: Line<'_>) {
        if let Some(pending_link) = self.pending_link.take() {
            self.render_line_with_link_pending(output, pending_link, line);
        } else {
            self.render_line_with_no_link_pending(output, line);
        }
    }

    /// Renders a line without converting hyperlinks.
    pub(super) fn render_line_without_link_conversion(
        &mut self,
        output: &mut String,
        line: Line<'_>,
    ) {
        self.flush_pending_link(output);
        output.push_str(line.rendered_prefix);
        render_inline_text(output, line.text);
    }

    /// Returns whether a wrapped hyperlink candidate is buffered.
    pub(super) fn has_pending_link(&self) -> bool {
        self.pending_link.is_some()
    }

    /// Emits a buffered hyperlink candidate without converting it.
    pub(super) fn flush_pending_link(&mut self, output: &mut String) {
        if let Some(pending_link) = self.pending_link.take() {
            output.push_str(&pending_link.fallback);
        }
    }

    fn render_line_with_link_pending(
        &mut self,
        output: &mut String,
        mut pending_link: PendingLink,
        mut line: Line<'_>,
    ) {
        // A dedent or implausible continuation ends the link candidate.
        // Emit its buffered fallback, then render the current line normally.
        if line.source_indentation < pending_link.minimum_indentation
            || !pending_link.can_continue_with(line.text)
        {
            output.push_str(&pending_link.fallback);
            self.render_line_with_no_link_pending(output, line);
            return;
        }

        // Record where the current line begins in the buffered source. A
        // completed link's `len` is its end offset in that same source, so
        // `link.len - line_start` is where rendering resumes within `line`.
        let line_start = pending_link.source.len() + 1;

        pending_link.target_started |= line.text.contains('<');
        pending_link.source.push('\n');
        pending_link.source.push_str(line.text);

        // Buffer ordinary continuation lines without reparsing the entire
        // source, which would make long wrapped links increasingly expensive.
        if !line.text.contains(['`', '\\']) {
            pending_link.push_fallback_line(line);
            self.pending_link = Some(pending_link);
            return;
        }

        // Reaching this point means the line contains either a possible closing
        // backtick or a disallowed backslash. Reparse now to complete or reject
        // the candidate.
        match parse_candidate(&pending_link.source) {
            Some(Candidate::Complete(link)) if link.len >= line_start => {
                // Replace the speculative source with the rendered link, then
                // scan the rest of this line for additional links.
                output.push_str(&pending_link.rendered_before);
                link.render_markdown(output);
                line.rendered_prefix = "";
                line.text = &line.text[link.len - line_start..];
                self.render_line_with_no_link_pending(output, line);
            }
            Some(Candidate::Pending) => {
                // Keep the parsed source and fallback rendering in sync while
                // the candidate remains unresolved.
                pending_link.push_fallback_line(line);
                self.pending_link = Some(pending_link);
            }
            Some(Candidate::Complete(_)) | None => {
                // Abandon the candidate, emit its buffered fallback, and
                // process the current line from scratch.
                output.push_str(&pending_link.fallback);
                self.render_line_with_no_link_pending(output, line);
            }
        }
    }

    fn render_line_with_no_link_pending(&mut self, output: &mut String, line: Line<'_>) {
        let mut prefix = line.rendered_prefix;
        let mut rest = line.text;

        loop {
            match find_link(rest) {
                Some((start, Candidate::Complete(link))) => {
                    output.push_str(prefix);
                    prefix = "";
                    render_inline_text(output, &rest[..start]);
                    link.render_markdown(output);
                    rest = &rest[start + link.len..];
                }
                Some((start, Candidate::Pending)) => {
                    self.pending_link = Some(PendingLink::new(
                        prefix,
                        rest,
                        start,
                        line.source_indentation,
                    ));
                    return;
                }
                None => {
                    output.push_str(prefix);
                    render_inline_text(output, rest);
                    return;
                }
            }
        }
    }
}

/// A prose line and the Markdown prefix that precedes it.
#[derive(Clone, Copy)]
pub(super) struct Line<'a> {
    /// Prefix to emit directly into the rendered Markdown before `text`.
    pub(super) rendered_prefix: &'a str,
    /// Number of leading spaces in the source line.
    pub(super) source_indentation: usize,
    /// The line's text after removing leading indentation.
    pub(super) text: &'a str,
}

/// Represents a potential link that spans multiple lines.
struct PendingLink {
    /// Source text from the opening backtick through the last buffered line.
    source: String,
    /// Whether the candidate link has begun its target URI.
    target_started: bool,
    /// Markdown to be rendered prior to the link if parsing succeeds.
    rendered_before: String,
    /// Speculative Markdown rendering of every buffered line. It is accumulated
    /// alongside the candidate, then discarded if link conversion succeeds but
    /// emitted if conversion is abandoned.
    fallback: String,
    /// Minimum indentation required to continue link parsing rather than
    /// aborting because of a dedent.
    minimum_indentation: usize,
}

impl PendingLink {
    /// Starts buffering a wrapped link candidate found in `line`.
    fn new(
        rendered_prefix: &str,
        line: &str,
        candidate_start: usize,
        minimum_indentation: usize,
    ) -> Self {
        let mut rendered_before = String::with_capacity(rendered_prefix.len() + candidate_start);
        rendered_before.push_str(rendered_prefix);
        render_inline_text(&mut rendered_before, &line[..candidate_start]);

        let mut fallback = String::with_capacity(rendered_prefix.len() + line.len());
        fallback.push_str(rendered_prefix);
        render_inline_text(&mut fallback, line);

        let source = line[candidate_start..].to_owned();
        Self {
            target_started: source.contains('<'),
            source,
            rendered_before,
            fallback,
            minimum_indentation,
        }
    }

    /// Returns whether `line` can remain part of the candidate.
    fn can_continue_with(&self, line: &str) -> bool {
        if line.trim_ascii_end().is_empty() {
            return false;
        }

        // Once the target has begun, its contents may wrap anywhere. The full
        // parser validates the completed URI before rendering it as a link.
        if self.target_started {
            return true;
        }

        // Before the target begins, remain conservative about what can extend
        // a label. If this line also opens the target, require the text seen so
        // far to be a possible HTTP(S) scheme.
        let Some((label, uri)) = line.split_once('<') else {
            return is_label_continuation(line);
        };
        (label.trim_ascii_end().is_empty() || is_label_continuation(label))
            && is_supported_uri_prefix(uri)
    }

    /// Adds `line` to the speculative rendering used if conversion is abandoned.
    fn push_fallback_line(&mut self, line: Line<'_>) {
        self.fallback.push_str(line.rendered_prefix);
        render_inline_text(&mut self.fallback, line.text);
    }
}

/// A supported link or plausible wrapped candidate.
enum Candidate<'a> {
    Complete(Hyperlink<'a>),
    Pending,
}

/// Finds the first complete hyperlink or plausible wrapped candidate in `input`.
fn find_link(input: &str) -> Option<(usize, Candidate<'_>)> {
    let mut offset = TextSize::ZERO;

    // Visit each backtick run that could delimit inline markup.
    while let Some(run) = find_backtick_run(input, offset) {
        let index = run.start().to_usize();

        // An escaped run is literal text, so continue immediately after it.
        if is_backtick_run_escaped(input, index) {
            offset = run.end();
            continue;
        }

        // Try parsing a link only when a single backtick has valid surrounding characters.
        if run.len() == TextSize::new(1)
            && is_link_start(input, index)
            && let Some(candidate) = parse_candidate(&input[index..])
        {
            return Some((index, candidate));
        }

        // Skip other backtick-delimited spans rather than searching inside them.
        offset = markdown_code_span(input, run)?.end();
    }

    None
}

/// Parses a possible link at the start of `input`.
///
/// Plausible wrapped labels without a closing backtick remain pending;
/// malformed or unsupported forms return `None`.
fn parse_candidate(input: &str) -> Option<Candidate<'_>> {
    let after_opening = input.strip_prefix('`')?;
    if after_opening
        .chars()
        .next()
        .is_none_or(|char| char == '`' || char.is_whitespace())
    {
        return None;
    }

    let Some(closing) = find_backtick_run(input, TextSize::new(1)) else {
        // Eliminate candidates whose content already contains a disallowed
        // backslash or closing `>`, or whose target cannot become HTTP(S). A
        // partial URI scheme remains valid so it can wrap immediately after
        // `<` or anywhere within `http://` or `https://`.
        if after_opening.contains(['\\', '>'])
            || after_opening
                .rsplit_once('<')
                .is_some_and(|(_, uri)| !is_supported_uri_prefix(uri))
        {
            return None;
        }
        return Some(Candidate::Pending);
    };
    if closing.len() != TextSize::new(1) {
        return None;
    }

    let content = &input[1..closing.start().to_usize()];
    if content.contains('\\') {
        return None;
    }

    let after_closing = &input[closing.end().to_usize()..];
    let underscore_count = after_closing
        .bytes()
        .take_while(|byte| *byte == b'_')
        .count();
    let len = closing.end().to_usize() + underscore_count;
    if !(1..=2).contains(&underscore_count) || !is_link_suffix(&after_closing[underscore_count..]) {
        return None;
    }

    Hyperlink::parse(content, len).map(Candidate::Complete)
}

struct Hyperlink<'a> {
    label: Option<&'a str>,
    uri: Cow<'a, str>,
    len: usize,
}

impl<'a> Hyperlink<'a> {
    fn parse(content: &'a str, len: usize) -> Option<Self> {
        let target_start = content.rfind('<')?;
        if !content.ends_with('>') {
            return None;
        }

        let uri = normalize_uri(&content[target_start + 1..content.len() - 1])?;

        let label = if target_start == 0 {
            None
        } else {
            let before_target = &content[..target_start];
            if !before_target.ends_with(|char: char| char.is_ascii_whitespace()) {
                return None;
            }
            let label = before_target.trim_end_matches(|char: char| char.is_ascii_whitespace());
            if label.is_empty() || label.contains(['<', '>']) {
                return None;
            }
            Some(label)
        };

        Some(Self { label, uri, len })
    }

    fn render_markdown(&self, output: &mut String) {
        render_markdown_link(output, self.label, self.uri.as_ref());
    }
}

fn normalize_uri(uri: &str) -> Option<Cow<'_, str>> {
    // Keep the common case borrowed. This also ensures that whitespace within
    // a single physical line remains invalid rather than being normalized.
    if !uri.contains('\n') {
        return is_supported_uri(uri).then_some(Cow::Borrowed(uri));
    }

    // Whitespace at a physical line boundary comes from wrapping rather than
    // the destination itself, so remove it while joining the URI lines.
    let mut normalized = String::with_capacity(uri.len());
    for line in uri.lines() {
        normalized.push_str(line.trim_ascii());
    }
    is_supported_uri(&normalized).then_some(Cow::Owned(normalized))
}

fn is_supported_uri(uri: &str) -> bool {
    !uri.is_empty()
        && !uri.chars().any(|char| {
            matches!(char, '\\' | '<' | '>' | '[' | ']')
                || char.is_control()
                || char.is_whitespace()
        })
        && ["http://", "https://"].into_iter().any(|scheme| {
            uri.get(..scheme.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
        })
}

/// Returns whether `uri` can grow into a supported URI scheme.
///
/// The empty prefix is intentionally accepted to allow a line break directly
/// after the target's opening `<`.
fn is_supported_uri_prefix(uri: &str) -> bool {
    ["http://", "https://"].into_iter().any(|scheme| {
        let comparison_len = uri.len().min(scheme.len());
        uri.get(..comparison_len)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&scheme[..comparison_len]))
    })
}

fn is_label_continuation(line: &str) -> bool {
    let line = line.trim_end_matches(|char: char| char.is_ascii_whitespace());
    !line.is_empty() && !line.contains(['`', '\\', '<', '>']) && !starts_obvious_block(line)
}

/// Identifies obvious reST or Markdown block starts that cannot continue a
/// wrapped link label.
///
/// This conservative heuristic prevents a pending link from consuming adjacent
/// block syntax without requiring a full block parser.
fn starts_obvious_block(line: &str) -> bool {
    if line.starts_with(".. ")
        || line.starts_with([':', '|', '#'])
        || is_rst_section_adornment(line)
    {
        return true;
    }

    let Some(first_word) = line.split_ascii_whitespace().next() else {
        return false;
    };
    matches!(first_word, "-" | "+" | "*" | "•" | "‣" | "⁃")
        || first_word
            .strip_suffix(['.', ')'])
            .is_some_and(|marker| !marker.is_empty() && marker.chars().all(char::is_alphanumeric))
}

fn is_rst_section_adornment(line: &str) -> bool {
    let mut characters = line.chars();
    let Some(marker) = characters.next() else {
        return false;
    };
    marker.is_ascii_punctuation() && characters.all(|character| character == marker)
}

/// Returns whether the backtick at `index` may begin inline markup.
///
/// Uses the supported ASCII subset of reStructuredText's
/// [inline markup recognition rules] for start-string boundaries.
///
/// [inline markup recognition rules]: https://www.docutils.org/docs/ref/rst/restructuredtext.html#inline-markup-recognition-rules
fn is_link_start(input: &str, index: usize) -> bool {
    let (before, after) = input.split_at(index);
    let after = &after[1..];

    (before.is_empty()
        || before.ends_with(|char: char| {
            char.is_ascii_whitespace()
                || matches!(char, '-' | '/' | ':' | '"' | '\'' | '(' | '<' | '[' | '{')
        }))
        && !after.is_empty()
        && !after.starts_with(|char: char| char == '`' || char.is_ascii_whitespace())
}

/// Returns whether `input` may follow a phrase hyperlink's `_` or `__` suffix.
///
/// Uses the supported ASCII subset of reStructuredText's
/// [inline markup recognition rules] for end-string boundaries.
///
/// [inline markup recognition rules]: https://www.docutils.org/docs/ref/rst/restructuredtext.html#inline-markup-recognition-rules
fn is_link_suffix(input: &str) -> bool {
    input.chars().next().is_none_or(|char| {
        char.is_ascii_whitespace()
            || matches!(
                char,
                '-' | '/' | ':' | '.' | ',' | ';' | '!' | '?' | '"' | '\'' | ')' | '>' | ']' | '}'
            )
    })
}

fn render_markdown_link(output: &mut String, label: Option<&str>, uri: &str) {
    output.push('[');
    if let Some(label) = label {
        push_link_label(output, label);
    } else {
        for char in uri.chars() {
            push_link_text_char(output, char);
        }
    }
    output.push_str("](");
    push_link_destination(output, uri);
    output.push(')');
}

/// Escapes underscores and HTML-sensitive characters in prose outside inline
/// code spans.
///
/// For example, `__init__` becomes `\_\_init\_\_`, while `` `__init__` ``
/// remains unchanged.
///
/// Conveniently, both reST and Markdown delimit inline code with backticks, so
/// we only have to detect one type of code span.
///
/// Inline code is assumed not to span lines.
fn render_inline_text(output: &mut String, text: &str) {
    let mut in_inline_code = false;
    let mut first_chunk = true;
    let mut opening_tick_count = 0;
    let mut current_tick_count = 0;

    for chunk in text.split('`') {
        // First chunk is definitionally not in inline-code and so always plaintext.
        if first_chunk {
            first_chunk = false;
            push_escaped_markdown_text(output, chunk);
            continue;
        }

        // Not in first chunk, emit the ` between the last chunk and this one.
        output.push('`');
        current_tick_count += 1;

        // If we're in an inline block and have enough close-ticks to terminate it, do so.
        // TODO: we parse ``hello```there` as (hello)(there) which probably isn't correct
        // (definitely not for Markdown) but it's close enough for horse grenades in this
        // MVP impl. Notably we're verbatim emitting all the backticks so as long as reST and
        // Markdown agree we're *fine*. The accuracy of this parsing only affects the
        // accuracy of where we apply escaping (so we need to misparse and see escapables
        // for any of this to matter).
        if opening_tick_count > 0 && current_tick_count >= opening_tick_count {
            opening_tick_count = 0;
            current_tick_count = 0;
            in_inline_code = false;
        }

        // If this chunk is completely empty we're just in a run of ticks.
        if chunk.is_empty() {
            continue;
        }

        // Ok the chunk is non-empty, our run of ticks is complete.
        if in_inline_code {
            // The previous check for >= opening_tick_count didn't trip, so these can't close
            // and these ticks will be verbatim rendered in the content.
            current_tick_count = 0;
        } else if current_tick_count > 0 {
            // Ok we're now in inline code.
            opening_tick_count = current_tick_count;
            current_tick_count = 0;
            in_inline_code = true;
        }

        // Finally include the content either escaped or not.
        if in_inline_code {
            output.push_str(chunk);
        } else {
            push_escaped_markdown_text(output, chunk);
        }
    }
    // NOTE: explicitly not "flushing" the ticks here.
    // We respect however the user closed their inline code.
}

fn push_escaped_markdown_text(output: &mut String, input: &str) {
    for char in input.chars() {
        match char {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '_' => output.push_str("\\_"),
            _ => output.push(char),
        }
    }
}

fn push_link_label(output: &mut String, input: &str) {
    let mut pending_whitespace = false;

    for char in input.chars() {
        if char.is_ascii_whitespace() {
            pending_whitespace = true;
            continue;
        }
        if pending_whitespace {
            output.push(' ');
            pending_whitespace = false;
        }
        push_link_text_char(output, char);
    }
}

fn push_link_text_char(output: &mut String, char: char) {
    match char {
        '*' | '[' | ']' | '`' | '|' | '~' | '\\' => {
            output.push('\\');
            output.push(char);
        }
        '&' => output.push_str("&amp;"),
        '<' => output.push_str("&lt;"),
        '>' => output.push_str("&gt;"),
        '_' => output.push_str("\\_"),
        _ => output.push(char),
    }
}

fn push_link_destination(output: &mut String, input: &str) {
    for char in input.chars() {
        match char {
            '(' | ')' | '\\' => {
                output.push('\\');
                output.push(char);
            }
            '&' => output.push_str("&amp;"),
            _ => output.push(char),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render_inline_text;

    #[test]
    fn renders_supported_single_line_links() {
        assert_rendered(&[
            (
                "See `datetime-like <https://numpy.org/doc/stable/reference/arrays.datetime.html>`_ values.",
                "See [datetime-like](https://numpy.org/doc/stable/reference/arrays.datetime.html) values.",
            ),
            (
                "`project docs <https://example.com/docs>`__",
                "[project docs](https://example.com/docs)",
            ),
            (
                "`HTTP docs <http://example.com/docs>`_",
                "[HTTP docs](http://example.com/docs)",
            ),
            (
                "`<https://example.com/_under_/*>`_",
                r"[https://example.com/\_under\_/\*](https://example.com/_under_/*)",
            ),
            (
                "`code` and `link <https://example.com>`_",
                "`code` and [link](https://example.com)",
            ),
            (
                "[outer](https://outer.example), `matrix[i][j]`, and `inner <https://inner.example>`_",
                "[outer](https://outer.example), `matrix[i][j]`, and [inner](https://inner.example)",
            ),
            (
                "`not a link <https://inside.example>` and `link <https://example.com>`_",
                "`not a link <https://inside.example>` and [link](https://example.com)",
            ),
            (
                "(`parenthesized <HTTPS://example.com/a_(b)?x=1&y=2>`_)",
                "([parenthesized](HTTPS://example.com/a_\\(b\\)?x=1&amp;y=2))",
            ),
        ]);
    }

    #[test]
    fn renders_supported_multiline_links() {
        assert_rendered(&[
            (
                "See `the documentation\n<https://example.com/docs>`_ for details.",
                "See [the documentation](https://example.com/docs) for details.",
            ),
            (
                "`Sanjoy Dasgupta and Anupam Gupta, 1999,\n\"An elementary proof of the\nJohnson-Lindenstrauss Lemma.\"\n<https://example.com/paper>`_",
                "[Sanjoy Dasgupta and Anupam Gupta, 1999, \"An elementary proof of the Johnson-Lindenstrauss Lemma.\"](https://example.com/paper)",
            ),
            (
                "See `a BCP47\nlanguage code <https://example.com/language-tags>`_ here.",
                "See [a BCP47 language code](https://example.com/language-tags) here.",
            ),
            (
                "`invalid_row_handler\n<https://arrow.apache.org/docs/python\n/generated/pyarrow.csv.ParseOptions.html\n#pyarrow.csv.ParseOptions.invalid_row_handler>`_",
                "[invalid\\_row\\_handler](https://arrow.apache.org/docs/python/generated/pyarrow.csv.ParseOptions.html#pyarrow.csv.ParseOptions.invalid_row_handler)",
            ),
            (
                "`PDF <https://example.com/articles/\narticle.pdf>`_",
                "[PDF](https://example.com/articles/article.pdf)",
            ),
            (
                "`docs <ht\ntps://example.com>`_",
                "[docs](https://example.com)",
            ),
            (
                "\
`strftime documentation
<https://docs.python.org/3/library/datetime.html
#strftime-and-strptime-behavior>`_ for more information.",
                "[strftime documentation](https://docs.python.org/3/library/datetime.html#strftime-and-strptime-behavior) for more information.",
            ),
            (
                "\
See `timezone conversion and
localization
<https://pandas.pydata.org/pandas-docs/stable/user_guide/timeseries.html
#time-zone-handling>`_.",
                "See [timezone conversion and localization](https://pandas.pydata.org/pandas-docs/stable/user_guide/timeseries.html#time-zone-handling).",
            ),
            (
                "`first\n<https://one.example>`_, `second <https://two.example>`_, and `third\n<https://three.example>`_",
                "[first](https://one.example), [second](https://two.example), and [third](https://three.example)",
            ),
            (
                "`anonymous\n<http://example.com>`__",
                "[anonymous](http://example.com)",
            ),
            (
                "References\n----------\n.. [1] `Cubic Spline Interpolation\n    <https://en.wikiversity.org/wiki/Cubic_Spline_Interpolation>`_",
                "References  \n----------  \n.. [1] [Cubic Spline Interpolation](https://en.wikiversity.org/wiki/Cubic_Spline_Interpolation)",
            ),
            (
                "- `wrapped\n  <https://example.com>`_",
                "- [wrapped](https://example.com)",
            ),
            (
                "1. `wrapped\n   <https://example.com>`_",
                "1. [wrapped](https://example.com)",
            ),
        ]);
    }

    #[test]
    fn does_not_convert_candidates_outside_inline_markup_boundaries() {
        for source in [
            "word`link <https://example.com>`_",
            "`link <https://example.com>`_word",
            r"\`link <https://example.com>`_",
            r"`escaped \label <https://example.com>`_",
        ] {
            assert_eq!(render_docstring(source), render_plain_docstring(source));
        }

        assert!(
            !render_fragment("Unclosed `comparison < value\n    `literal <https://example.com>`_")
                .contains("[literal](")
        );
    }

    #[test]
    fn does_not_convert_links_with_unsupported_uris() {
        for source in [
            "`link <>`_",
            "`link <../../docs.html>`_",
            "`link <ftp://example.com>`_",
            "`link <https://example.com/a b>`_",
            "`link <https://example.com/\u{7f}>`_",
            r"`link <https://example.com/\path>`_",
            "`link <https://example.com/<tag>>`_",
            "`link <https://example.com/[id]>`_",
        ] {
            assert_eq!(render_docstring(source), render_plain_docstring(source));
        }
    }

    #[test]
    fn does_not_convert_links_with_unsupported_label_continuations() {
        for line in [
            "",
            "   ",
            "`code`",
            "<target>",
            r"escaped \ label",
            ".. note::",
            ":field:",
            "| substitution |",
            "# heading",
            "----",
            "- list item",
            "1. list item",
            "• list item",
        ] {
            let source = format!("`label\n{line}\n<https://example.com>`_");
            assert!(
                !render_docstring(&source).contains("](https://example.com)"),
                "line should be rejected: {line:?}"
            );
        }
    }

    #[test]
    fn does_not_convert_malformed_uri_continuations() {
        for source in [
            "\
`docs
<https://example.com/path
#fragment with space>`_",
            "\
`docs
<https://example.com/path
#fragment>`_word",
            "\
`docs
<https://example.com/path",
            r"  `docs
  <https://example.com/path
#fragment>`_",
        ] {
            assert_eq!(render_docstring(source), render_plain_docstring(source));
        }
    }

    #[test]
    fn does_not_convert_representative_links_outside_the_supported_subset() {
        // These are representative fallbacks, not an exhaustive list. The
        // `Renderer` contract defines the complete supported subset.
        for source in [
            "`label\n- list item\n<https://example.com>`_",
            "  `docs\n<https://example.com>`_",
            "`Table Visualization <../../user_guide/style.ipynb>`_",
            "`generic type`_\n\n.. _generic type: https://example.com/generics",
        ] {
            assert_eq!(render_docstring(source), render_plain_docstring(source));
        }
    }

    #[test]
    fn does_not_convert_links_in_preformatted_blocks() {
        assert_eq!(
            render_docstring(
                "Example::\n\n    `literal <https://inner.example>`_\n\n`docs <https://example.com>`_"
            ),
            "Example:    \n```````````python\n    `literal <https://inner.example>`_\n\n```````````\n[docs](https://example.com)"
        );
    }

    #[test]
    fn preserves_existing_inline_rendering() {
        assert_rendered(&[
            ("__init__", r"\_\_init\_\_"),
            ("`__init__`", "`__init__`"),
            ("``C:\\`` and __dunder__", r"``C:\`` and \_\_dunder\_\_"),
            ("This is `unclosed", "This is `unclosed"),
            (r"\` literal `__dunder__", r"\` literal `\_\_dunder\_\_"),
        ]);
    }

    fn assert_rendered(cases: &[(&str, &str)]) {
        for &(source, expected) in cases {
            assert_eq!(render_docstring(source), expected, "source: {source:?}");
        }
    }

    fn render_docstring(source: &str) -> String {
        let mut output = String::new();
        super::super::render_into(&mut output, source);
        output
    }

    fn render_fragment(source: &str) -> String {
        let mut output = String::new();
        super::super::render_fragment_into(&mut output, source);
        output
    }

    fn render_plain_docstring(source: &str) -> String {
        let mut output = String::new();
        let mut first_line = true;
        for line in source.lines() {
            if !first_line {
                output.push_str("  \n");
            }
            first_line = false;
            let text = line.trim_start_matches(' ');
            for _ in 0..line.len() - text.len() {
                output.push_str("&nbsp;");
            }
            render_inline_text(&mut output, text);
        }
        output
    }
}

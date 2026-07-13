use ruff_text_size::TextSize;

use super::super::document::{
    preformatted::MarkdownFence,
    syntax::{RestDirective, RestDirectiveKind, parse_rest_directive},
};

mod inline;

// Here lies a monument to robust parsing and escaping:
// a codefence with SO MANY backticks that surely no one will ever accidentally
// break out of it, even if they're writing python documentation about markdown
// code fences and are showing off how you can use more than 3 backticks.
const FENCE: &str = "```````````";

/// Renders normalized docstring source as Markdown.
///
/// Outside recognized code blocks, leading indentation is encoded as
/// non-breaking spaces. This preserves the source's visual alignment without
/// allowing Markdown to interpret the indentation as block syntax. For example,
/// a continuation line can align with the start of a parameter description
/// without becoming an indented code block.
///
/// See [`render_fragment_into`] for fragments whose indentation should remain
/// available to the Markdown parser.
pub(super) fn render_into(output: &mut String, docstring: &str) {
    render_with_indentation_mode(output, docstring, LeadingIndentation::DisplayOnly);
}

/// Renders a normalized, extracted docstring fragment as Markdown.
///
/// Outside recognized code blocks, leading indentation remains ordinary spaces,
/// allowing Markdown to interpret fragment-relative block structure such as
/// nested lists and indented code blocks. [`render_into`] instead preserves
/// indentation for display only.
pub(super) fn render_fragment_into(output: &mut String, fragment: &str) {
    render_with_indentation_mode(output, fragment, LeadingIndentation::MarkdownSyntax);
}

/// Renders normalized docstring source with the selected treatment of leading indentation.
///
/// Callers must expand tabs and normalize line endings to `\n`, as done by
/// `docstring::documentation_trim` and `docstring::documentation_fragment_trim`.
/// This function treats leading ASCII spaces as indentation. The indentation
/// mode only applies outside recognized code blocks; code-block indentation
/// always remains ordinary spaces.
///
/// The general approach here is:
///
/// * Depending on the value of `leading_indentation`, encode line indentation
///   and breaks so that the rendered output mirrors the source layout
/// * Escape problematic things where necessary (bare `__dunder__` => `\_\_dunder\_\_`)
/// * Introduce code fences where appropriate
///
/// The first rule is significant in ensuring various docstring idioms render
/// clearly e.g.:
///
/// ```text
/// param1 -- a good parameter
/// param2 -- another good parameter
///           with longer docs
/// ```
///
/// Without that encoding, Markdown would render inputs like that into abominations like:
///
/// ```html
/// <p>
/// param1 -- a good parameter param2 -- another good parameter
/// </p>
///
/// <code>
/// with longer docs
/// </code>
/// ```
fn render_with_indentation_mode(
    output: &mut String,
    source: &str,
    leading_indentation: LeadingIndentation,
) {
    // TODO: there is a convention that `singletick` is for items that can
    // be looked up in-scope while ``multitick`` is for opaque inline code.
    // While rendering this we should make note of all the `singletick` locations
    // and (possibly in a higher up piece of logic) try to resolve the names for
    // cross-linking. (Similar to `TypeDetails` in the type formatting code.)
    let mut first_line = true;
    let mut renderer = Renderer::new(output);
    let mut temp_owned_line;
    for line in source.lines() {
        // We can assume leading whitespace has been normalized
        let trimmed_source_line = line.trim_start_matches(' ');
        let mut rendered_line = trimmed_source_line;
        let line_indent = TextSize::of(line) - TextSize::of(trimmed_source_line);

        // First things first, prepare the prefix for the new line.
        renderer.prepare_line(first_line);
        first_line = false;

        if let Some(marker_indent) = renderer.block_state.rest_control_marker_indent() {
            if rendered_line.is_empty() || line_indent > marker_indent {
                renderer.discard_pending_line();
                continue;
            }
            renderer.finish_rest_control();
        }

        // If we're in a literal block and we find a non-empty dedented line, end the block
        // TODO: we should remove all the trailing blank lines
        // (Just pop all trailing `\n` from `output`?)
        if let Some(indent) = renderer.block_state.rst_literal_indent()
            && !rendered_line.is_empty()
            && line_indent < indent
        {
            renderer.finish_rest_literal();
        }

        if let Some(marker_indent) = renderer.block_state.rest_directive_literal_marker_indent()
            && !rendered_line.is_empty()
            && line_indent <= marker_indent
        {
            renderer.finish_rest_literal();
        }

        // We previously entered a literal block and we just found our first non-blank line
        // So now we're actually in the literal block
        if let Some(language) = renderer.block_state.pending_rst_literal_language()
            && !rendered_line.is_empty()
        {
            renderer.start_rest_literal(language, line_indent);
        }

        if let Some((language, marker_indent)) =
            renderer.block_state.pending_rest_directive_literal()
            && !rendered_line.is_empty()
        {
            if line_indent > marker_indent {
                renderer.start_rest_directive_literal(language, marker_indent);
            } else {
                renderer.cancel_pending_rest_directive_literal();
            }
        }

        // If we're not in a codeblock and we see something that signals a doctest, start one
        if renderer.block_state.is_prose() && rendered_line.starts_with(">>>") {
            renderer.start_doctest();
        }

        // If we're not in a codeblock and we see a markdown codefence, start one
        if renderer.block_state.is_prose()
            && let Some(fence) = MarkdownFence::find(trimmed_source_line)
        {
            // Unlike other blocks we don't need to emit fences because it's already markdown
            // Render the line verbatim without its indent and move on.
            //
            // If there's any indent this is really just Bad Syntax but it "makes sense"
            // to someone writing docs like this:
            //
            // Returns:
            //     Some details...
            //     ```
            //     some_example()
            //     ```
            //     etc etc...
            //
            // We "make this work" by stripping the indent on the fences but preserving the
            // full indent of the lines between the fences
            renderer.start_markdown_fence(fence, rendered_line);
            continue;
        // If we're in a markdown code fence and this line seems to terminate it, end the block
        } else if renderer
            .block_state
            .markdown_fence_is_closed_by(rendered_line)
        {
            // Render the line without its indent and move on.
            renderer.finish_markdown_fence(rendered_line);
            continue;
        }

        // Directives classify their content independently from paragraph literal blocks.
        if renderer.block_state.is_prose()
            && let Some((prefix, directive)) = find_rest_directive(trimmed_source_line)
        {
            renderer.flush_pending_link();

            match directive.kind() {
                RestDirectiveKind::Code => {
                    rendered_line = prefix.trim_end();
                    renderer.start_pending_rest_directive_literal(
                        code_directive_language(directive),
                        line_indent,
                    );
                }
                RestDirectiveKind::Preformatted => {
                    let language = preformatted_directive_language(directive);
                    if prefix.trim().is_empty()
                        && let Some(content) = preformatted_directive_inline_content(directive)
                    {
                        renderer.start_rest_directive_literal(language, line_indent);
                        rendered_line = content;
                    } else {
                        temp_owned_line = preformatted_directive_title(prefix, directive);
                        rendered_line = temp_owned_line.as_str();
                        renderer.start_pending_rest_directive_literal(language, line_indent);
                    }
                }
                RestDirectiveKind::Prose => {
                    temp_owned_line = render_prose_directive(prefix, directive);
                    rendered_line = temp_owned_line.as_str();
                }
                RestDirectiveKind::Control => {
                    rendered_line = prefix.trim_end();
                    if rendered_line.is_empty() {
                        renderer.start_rest_control(line_indent);
                    }
                }
            }
        // If we're not in a codeblock and we see a paragraph literal marker, start a block.
        } else if renderer.block_state.is_prose()
            && let Some((without_literal, language)) = parse_paragraph_literal(trimmed_source_line)
        {
            let include_colon = language.is_none()
                && without_literal
                    .chars()
                    .next_back()
                    .is_some_and(|character| !character.is_whitespace());
            rendered_line = if include_colon {
                rendered_line.strip_suffix(':').unwrap_or(rendered_line)
            } else {
                without_literal.trim_end()
            };
            renderer.start_pending_rest_literal(language.unwrap_or("python"));
        }

        // Add this line's indentation.
        // We could subtract the literal block's indentation here but in practice it's uglier
        // TODO: should we not do this if the `line.is_empty()`? When would it matter?
        renderer.push_indentation(line_indent, leading_indentation);

        if renderer.block_state.is_doctest() && rendered_line.is_empty() {
            renderer.finish_doctest();
            continue;
        }

        let is_indented_markdown_code =
            matches!(leading_indentation, LeadingIndentation::MarkdownSyntax)
                && line_indent >= TextSize::from(4)
                && !renderer.inline.has_pending_link();

        renderer.render_line(
            rendered_line,
            line_indent.to_usize(),
            !is_indented_markdown_code,
        );
    }
    renderer.finish_document();
}

fn find_rest_directive(line: &str) -> Option<(&str, RestDirective<'_>)> {
    if let Some(directive) = parse_rest_directive(line) {
        return Some(("", directive));
    }

    // Preserve the renderer's historical best-effort handling of invalid markers that have
    // prose before the directive.
    let marker_start = line.rfind(".. ")?;
    let directive = parse_rest_directive(&line[marker_start..])?;
    Some((&line[..marker_start], directive))
}

fn parse_paragraph_literal(line: &str) -> Option<(&str, Option<&str>)> {
    line.strip_suffix("::")
        .map(|prefix| (prefix, None))
        .or_else(|| {
            let (prefix, language) = line.rsplit_once(' ')?;
            let prefix = prefix.trim_end().strip_suffix("::")?;
            Some((prefix, Some(language)))
        })
}

fn code_directive_language(directive: RestDirective<'_>) -> &str {
    if directive.is_named("code")
        || directive.is_named("code-block")
        || directive.is_named("sourcecode")
    {
        first_argument_word(directive.argument()).unwrap_or("python")
    } else if directive.is_named("testoutput") {
        "text"
    } else {
        "python"
    }
}

fn preformatted_directive_language(directive: RestDirective<'_>) -> &str {
    if directive.is_named("raw") {
        first_argument_word(directive.argument()).unwrap_or("text")
    } else if directive.is_named("csv-table") {
        "csv"
    } else if directive.is_named("graphviz") {
        "dot"
    } else {
        "text"
    }
}

fn preformatted_directive_inline_content(directive: RestDirective<'_>) -> Option<&str> {
    directive
        .is_named("math")
        .then_some(directive.argument())
        .filter(|argument| !argument.is_empty())
}

fn preformatted_directive_title(prefix: &str, directive: RestDirective<'_>) -> String {
    if directive.is_named("csv-table") && !directive.argument().is_empty() {
        format!("{prefix}**{}**", directive.argument())
    } else {
        prefix.trim_end().to_owned()
    }
}

fn render_prose_directive(prefix: &str, directive: RestDirective<'_>) -> String {
    let name = directive.name();
    let argument = directive.argument();

    let label = [
        ("attention", "Attention"),
        ("caution", "Caution"),
        ("danger", "Danger"),
        ("error", "Error"),
        ("hint", "Hint"),
        ("important", "Important"),
        ("note", "Note"),
        ("tip", "Tip"),
        ("warning", "Warning"),
        ("seealso", "See also"),
    ]
    .into_iter()
    .find_map(|(name, label)| directive.is_named(name).then_some(label));
    if let Some(label) = label {
        return render_labeled_directive(prefix, label, argument);
    }

    if directive.is_named("admonition") {
        let label = if argument.is_empty() {
            "Admonition"
        } else {
            argument
        };
        return render_labeled_directive(prefix, label, "");
    }

    let version_label = [
        ("versionadded", "Added in version"),
        ("version-added", "Added in version"),
        ("versionchanged", "Changed in version"),
        ("version-changed", "Changed in version"),
        ("deprecated", "Deprecated since version"),
        ("version-deprecated", "Deprecated since version"),
        ("versionremoved", "Removed in version"),
        ("version-removed", "Removed in version"),
    ]
    .into_iter()
    .find_map(|(name, label)| directive.is_named(name).then_some(label));
    if let Some(label) = version_label {
        let (version, explanation) = split_first_word(argument);
        let label = if version.is_empty() {
            label.to_owned()
        } else {
            format!("{label} {version}")
        };
        return render_labeled_directive(prefix, &label, explanation);
    }

    let mut rendered = format!("{prefix}*{name}*");
    if !argument.is_empty() {
        rendered.push(' ');
        rendered.push_str(argument);
    }
    rendered
}

fn render_labeled_directive(prefix: &str, label: &str, content: &str) -> String {
    let mut rendered = format!("**{prefix}{label}:**");
    if !content.is_empty() {
        rendered.push(' ');
        rendered.push_str(content);
    }
    rendered
}

fn split_first_word(argument: &str) -> (&str, &str) {
    let Some(split) = argument.find(char::is_whitespace) else {
        return (argument, "");
    };
    (&argument[..split], argument[split..].trim_start())
}

fn first_argument_word(argument: &str) -> Option<&str> {
    argument.split_whitespace().next()
}

/// How to emit leading indentation outside recognized code blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeadingIndentation {
    /// Emit `&nbsp;` so that indentation affects display without activating Markdown syntax.
    DisplayOnly,
    /// Emit ordinary spaces that Markdown can interpret as block syntax.
    MarkdownSyntax,
}

/// Renders docstring lines while preserving state across block and line boundaries.
struct Renderer<'source, 'output> {
    line_prefix: LinePrefix,
    block_state: BlockState<'source>,
    inline: inline::Renderer,
    output: &'output mut String,
}

impl<'source, 'output> Renderer<'source, 'output> {
    fn new(output: &'output mut String) -> Self {
        Self {
            line_prefix: LinePrefix::default(),
            block_state: BlockState::default(),
            inline: inline::Renderer::default(),
            output,
        }
    }

    fn prepare_line(&mut self, first_line: bool) {
        self.line_prefix.prepare(&self.block_state, first_line);
    }

    fn push_indentation(&mut self, indentation: TextSize, leading_indentation: LeadingIndentation) {
        self.line_prefix.push_indentation(
            indentation.to_usize(),
            &self.block_state,
            leading_indentation,
        );
    }

    /// Flushes content buffered for the current line.
    fn flush_pending_line(&mut self) {
        self.inline.flush_pending_link(self.output);
        self.line_prefix.emit(self.output);
    }

    fn discard_pending_line(&mut self) {
        self.line_prefix.rendered.clear();
    }

    /// Flushes a wrapped hyperlink candidate.
    fn flush_pending_link(&mut self) {
        self.inline.flush_pending_link(self.output);
    }

    fn finish_rest_literal(&mut self) {
        debug_assert!(matches!(
            self.block_state,
            BlockState::RestLiteral { .. } | BlockState::RestDirectiveLiteral { .. }
        ));
        self.flush_pending_line();
        self.block_state = BlockState::Prose;
        self.output.push_str(FENCE);
        self.output.push('\n');
    }

    fn start_pending_rest_literal(&mut self, language: &'source str) {
        self.block_state = BlockState::PendingRestLiteral { language };
    }

    fn start_pending_rest_directive_literal(
        &mut self,
        language: &'source str,
        marker_indent: TextSize,
    ) {
        self.block_state = BlockState::PendingRestDirectiveLiteral {
            language,
            marker_indent,
        };
    }

    fn start_rest_literal(&mut self, language: &str, indent: TextSize) {
        self.flush_pending_line();
        self.block_state = BlockState::RestLiteral { indent };
        self.output.push('\n');
        self.output.push_str(FENCE);
        self.output.push_str(language);
        self.output.push('\n');
    }

    fn start_rest_directive_literal(&mut self, language: &str, marker_indent: TextSize) {
        self.flush_pending_line();
        self.block_state = BlockState::RestDirectiveLiteral { marker_indent };
        self.output.push('\n');
        self.output.push_str(FENCE);
        self.output.push_str(language);
        self.output.push('\n');
    }

    fn cancel_pending_rest_directive_literal(&mut self) {
        debug_assert!(matches!(
            self.block_state,
            BlockState::PendingRestDirectiveLiteral { .. }
        ));
        self.block_state = BlockState::Prose;
    }

    fn start_rest_control(&mut self, marker_indent: TextSize) {
        self.block_state = BlockState::RestControl { marker_indent };
    }

    fn finish_rest_control(&mut self) {
        debug_assert!(matches!(self.block_state, BlockState::RestControl { .. }));
        self.block_state = BlockState::Prose;
    }

    fn start_doctest(&mut self) {
        self.flush_pending_line();
        self.block_state = BlockState::Doctest;
        // TODO: is there something more specific? `pycon`?
        self.output.push_str(FENCE);
        self.output.push_str("python\n");
    }

    fn finish_doctest(&mut self) {
        debug_assert!(matches!(self.block_state, BlockState::Doctest));
        self.flush_pending_line();
        self.block_state = BlockState::Prose;
        self.output.push_str(FENCE);
    }

    fn start_markdown_fence(&mut self, fence: MarkdownFence<'source>, line: &str) {
        self.flush_pending_line();
        self.block_state = BlockState::MarkdownFence(fence);
        self.output.push_str(line);
    }

    fn finish_markdown_fence(&mut self, line: &str) {
        debug_assert!(matches!(self.block_state, BlockState::MarkdownFence(_)));
        self.flush_pending_line();
        self.block_state = BlockState::Prose;
        self.output.push_str(line);
    }

    fn render_line(&mut self, line: &str, source_indentation: usize, render_rst_links: bool) {
        if self.block_state.is_code() {
            self.flush_pending_line();
            self.output.push_str(line);
        } else {
            self.render_inline(line, source_indentation, render_rst_links);
        }
    }

    fn finish_document(mut self) {
        self.flush_pending_line();
        match self.block_state {
            BlockState::Prose
            | BlockState::PendingRestLiteral { .. }
            | BlockState::PendingRestDirectiveLiteral { .. }
            | BlockState::RestControl { .. } => {}
            BlockState::RestLiteral { .. }
            | BlockState::RestDirectiveLiteral { .. }
            | BlockState::Doctest => {
                self.output.push('\n');
                self.output.push_str(FENCE);
            }
            BlockState::MarkdownFence(fence) => {
                self.output.push('\n');
                self.output.push_str(fence.marker());
            }
        }
    }

    fn render_inline(&mut self, text: &str, source_indentation: usize, render_rst_links: bool) {
        let line = inline::Line {
            rendered_prefix: &self.line_prefix.rendered,
            source_indentation,
            text,
        };
        if render_rst_links {
            self.inline.render_line(self.output, line);
        } else {
            self.inline
                .render_line_without_link_conversion(self.output, line);
        }
        self.line_prefix.rendered.clear();
    }
}

/// Buffers the separator and indentation that must be emitted before each rendered line.
#[derive(Default)]
struct LinePrefix {
    rendered: String,
}

impl LinePrefix {
    /// Prepares us to render the next line by buffering the separator required
    /// to delineate it from the previous line.
    fn prepare(&mut self, block_state: &BlockState<'_>, first_line: bool) {
        self.rendered.clear();
        if first_line {
            // If this is the first source line of the fragment that we're rendering,
            // then there is no previous line to delineate from the next one.
            return;
        }

        if !block_state.is_code() {
            // Two trailing spaces turn the next newline into a Markdown hard
            // break, thereby preserving the source line boundary.
            self.rendered.push_str("  ");
        }

        // Suppress source line boundaries after a reST literal marker while we
        // wait for its content. This allows us to collapse any intervening
        // newlines that are purely structural.
        if !block_state.suppresses_source_line_boundary() {
            self.rendered.push('\n');
        }
    }

    /// Adds the next line's indentation to the buffered prefix.
    fn push_indentation(
        &mut self,
        indentation: usize,
        block_state: &BlockState<'_>,
        leading_indentation: LeadingIndentation,
    ) {
        for _ in 0..indentation {
            if !block_state.is_code()
                && matches!(leading_indentation, LeadingIndentation::DisplayOnly)
            {
                // TODO: would the raw unicode codepoint be handled *better* or *worse*
                // by various IDEs? VS Code handles this approach well, at least.
                self.rendered.push_str("&nbsp;");
            } else {
                self.rendered.push(' ');
            }
        }
    }

    /// Writes the buffered prefix to `output` and clears it.
    fn emit(&mut self, output: &mut String) {
        output.push_str(&self.rendered);
        self.rendered.clear();
    }
}

#[derive(Default)]
enum BlockState<'a> {
    #[default]
    Prose,
    PendingRestLiteral {
        language: &'a str,
    },
    PendingRestDirectiveLiteral {
        language: &'a str,
        marker_indent: TextSize,
    },
    RestLiteral {
        indent: TextSize,
    },
    RestDirectiveLiteral {
        marker_indent: TextSize,
    },
    RestControl {
        marker_indent: TextSize,
    },
    Doctest,
    MarkdownFence(MarkdownFence<'a>),
}

impl<'docstring> BlockState<'docstring> {
    const fn is_prose(&self) -> bool {
        matches!(self, Self::Prose)
    }

    const fn is_code(&self) -> bool {
        matches!(
            self,
            Self::RestLiteral { .. }
                | Self::RestDirectiveLiteral { .. }
                | Self::Doctest
                | Self::MarkdownFence(_)
        )
    }

    const fn suppresses_source_line_boundary(&self) -> bool {
        matches!(
            self,
            Self::PendingRestLiteral { .. } | Self::PendingRestDirectiveLiteral { .. }
        )
    }

    const fn is_doctest(&self) -> bool {
        matches!(self, Self::Doctest)
    }

    const fn rst_literal_indent(&self) -> Option<TextSize> {
        match self {
            Self::RestLiteral { indent } => Some(*indent),
            _ => None,
        }
    }

    const fn pending_rst_literal_language(&self) -> Option<&'docstring str> {
        match self {
            Self::PendingRestLiteral { language } => Some(*language),
            _ => None,
        }
    }

    const fn rest_directive_literal_marker_indent(&self) -> Option<TextSize> {
        match self {
            Self::RestDirectiveLiteral { marker_indent } => Some(*marker_indent),
            _ => None,
        }
    }

    const fn pending_rest_directive_literal(&self) -> Option<(&'docstring str, TextSize)> {
        match self {
            Self::PendingRestDirectiveLiteral {
                language,
                marker_indent,
                ..
            } => Some((*language, *marker_indent)),
            _ => None,
        }
    }

    const fn rest_control_marker_indent(&self) -> Option<TextSize> {
        match self {
            Self::RestControl { marker_indent } => Some(*marker_indent),
            _ => None,
        }
    }

    fn markdown_fence_is_closed_by(&self, line: &str) -> bool {
        matches!(self, Self::MarkdownFence(fence) if fence.is_closed_by(line))
    }
}

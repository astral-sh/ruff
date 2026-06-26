use std::borrow::Cow;

use ruff_text_size::TextSize;

use super::super::document::preformatted::MarkdownFence;

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

        // If we're in a literal block and we find a non-empty dedented line, end the block
        // TODO: we should remove all the trailing blank lines
        // (Just pop all trailing `\n` from `output`?)
        if let Some(indent) = renderer.block_state.rst_literal_indent()
            && !rendered_line.is_empty()
            && line_indent < indent
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

        // If we're not in a codeblock and we see something that signals a literal block, start one
        let parsed_literal = trimmed_source_line
            // first check for a line ending with `::`
            .strip_suffix("::")
            .map(|prefix| (prefix, None))
            // if that fails, look for a line ending with `:: lang`
            .or_else(|| {
                let (prefix, lang) = trimmed_source_line.rsplit_once(' ')?;
                let prefix = prefix.trim_end().strip_suffix("::")?;
                Some((prefix, Some(lang)))
            });
        if renderer.block_state.is_prose()
            && let Some((without_literal, lang)) = parsed_literal
        {
            let mut without_directive = without_literal;
            let mut directive = None;
            // Parse out a directive like `.. warning::`
            if let Some((prefix, directive_str)) = without_literal.rsplit_once(' ')
                && let Some(without_directive_str) = prefix.strip_suffix("..")
            {
                directive = Some(directive_str);
                without_directive = without_directive_str;
            }

            // Whether the `::` should become `:` or be erased
            let include_colon = if let Some(character) = without_directive.chars().next_back() {
                // If lang is set then we're either deleting the whole line or
                // the special rendering below will add it itself
                lang.is_none() && !character.is_whitespace()
            } else {
                // Delete whole line
                false
            };

            if include_colon {
                rendered_line = rendered_line.strip_suffix(":").unwrap();
            } else {
                rendered_line = without_directive.trim_end();
            }

            match directive {
                // Special directives that should be plaintext
                Some(
                    "attention" | "caution" | "danger" | "error" | "hint" | "important" | "note"
                    | "tip" | "warning" | "admonition" | "versionadded" | "version-added"
                    | "versionchanged" | "version-changed" | "version-deprecated" | "deprecated"
                    | "version-removed" | "versionremoved",
                ) => {
                    // Map version directives to human-readable phrases (matching Sphinx output)
                    let pretty_directive = match directive.unwrap() {
                        "versionadded" | "version-added" => Cow::Borrowed("Added in version"),
                        "versionchanged" | "version-changed" => Cow::Borrowed("Changed in version"),
                        "deprecated" | "version-deprecated" => {
                            Cow::Borrowed("Deprecated since version")
                        }
                        "versionremoved" | "version-removed" => Cow::Borrowed("Removed in version"),
                        other => Cow::Owned(
                            other
                                .char_indices()
                                .map(|(index, c)| {
                                    if index == 0 {
                                        c.to_ascii_uppercase()
                                    } else {
                                        c
                                    }
                                })
                                .collect(),
                        ),
                    };

                    // Render the argument of things like `.. version-added:: 4.0`
                    let suffix = if let Some(lang) = lang {
                        format!(" {lang}")
                    } else {
                        String::new()
                    };
                    // We prepend without_directive here out of caution for preserving input.
                    // This is probably gibberish/invalid syntax? But it's a no-op in normal cases.
                    temp_owned_line = format!("**{without_directive}{pretty_directive}{suffix}:**");

                    rendered_line = temp_owned_line.as_str();
                }
                // All other directives are code and default to Python.
                _ => renderer.start_pending_rest_literal(lang.unwrap_or("python")),
            }
        }

        // Add this line's indentation.
        // We could subtract the literal block's indentation here but in practice it's uglier
        // TODO: should we not do this if the `line.is_empty()`? When would it matter?
        renderer.push_indentation(line_indent, leading_indentation);

        if renderer.block_state.is_doctest() && rendered_line.is_empty() {
            renderer.finish_doctest();
            continue;
        }

        renderer.render_line(rendered_line);
    }
    renderer.finish_document();
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
    output: &'output mut String,
}

impl<'source, 'output> Renderer<'source, 'output> {
    fn new(output: &'output mut String) -> Self {
        Self {
            line_prefix: LinePrefix::default(),
            block_state: BlockState::default(),
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
        self.line_prefix.emit(self.output);
    }

    fn finish_rest_literal(&mut self) {
        debug_assert!(matches!(self.block_state, BlockState::RestLiteral { .. }));
        self.flush_pending_line();
        self.block_state = BlockState::Prose;
        self.output.push_str(FENCE);
        self.output.push('\n');
    }

    fn start_pending_rest_literal(&mut self, language: &'source str) {
        self.block_state = BlockState::PendingRestLiteral { language };
    }

    fn start_rest_literal(&mut self, language: &str, indent: TextSize) {
        self.flush_pending_line();
        self.block_state = BlockState::RestLiteral { indent };
        self.output.push('\n');
        self.output.push_str(FENCE);
        self.output.push_str(language);
        self.output.push('\n');
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

    fn render_line(&mut self, line: &str) {
        self.flush_pending_line();
        if self.block_state.is_code() {
            self.output.push_str(line);
        } else {
            inline::render_line(self.output, line);
        }
    }

    fn finish_document(mut self) {
        self.flush_pending_line();
        match self.block_state {
            BlockState::Prose | BlockState::PendingRestLiteral { .. } => {}
            BlockState::RestLiteral { .. } | BlockState::Doctest => {
                self.output.push('\n');
                self.output.push_str(FENCE);
            }
            BlockState::MarkdownFence(fence) => {
                self.output.push('\n');
                self.output.push_str(fence.marker());
            }
        }
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
        if !block_state.is_pending_rst_literal() {
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
    RestLiteral {
        indent: TextSize,
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
            Self::RestLiteral { .. } | Self::Doctest | Self::MarkdownFence(_)
        )
    }

    const fn is_pending_rst_literal(&self) -> bool {
        matches!(self, Self::PendingRestLiteral { .. })
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

    fn markdown_fence_is_closed_by(&self, line: &str) -> bool {
        matches!(self, Self::MarkdownFence(fence) if fence.is_closed_by(line))
    }
}

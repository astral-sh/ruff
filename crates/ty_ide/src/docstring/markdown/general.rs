use std::borrow::Cow;

use super::super::document::preformatted::MarkdownFence;

/// Applies whole-docstring Markdown escaping and code-fence handling.
///
/// This function assumes the input has had its whitespace normalized by
/// `docstring::documentation_trim`, so leading whitespace is always a space,
/// and newlines are always `\n`.
///
/// The general approach here is:
///
/// * Encode line indentation and breaks so that the rendered output mirrors the source layout
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
pub(super) fn render_into(output: &mut String, docstring: &str) {
    render_with_indentation_mode(output, docstring, LeadingIndentation::DisplayOnly);
}

/// Renders an extracted docstring fragment with ordinary spaces for indentation.
///
/// Unlike [`render_into`], this emits leading indentation outside code blocks as ordinary spaces,
/// allowing Markdown to interpret it as block syntax such as nested lists.
pub(super) fn render_fragment_into(output: &mut String, fragment: &str) {
    render_with_indentation_mode(output, fragment, LeadingIndentation::MarkdownSyntax);
}

/// How to emit leading indentation outside recognized code blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeadingIndentation {
    /// Emit `&nbsp;` so that indentation affects display without activating Markdown syntax.
    DisplayOnly,
    /// Emit ordinary spaces that Markdown can interpret as block syntax.
    MarkdownSyntax,
}

fn render_with_indentation_mode(
    output: &mut String,
    docstring: &str,
    leading_indentation: LeadingIndentation,
) {
    // Here lies a monumemnt to robust parsing and escaping:
    // a codefence with SO MANY backticks that surely no one will ever accidentally
    // break out of it, even if they're writing python documentation about markdown
    // code fences and are showing off how you can use more than 3 backticks.
    const FENCE: &str = "```````````";
    // TODO: there is a convention that `singletick` is for items that can
    // be looked up in-scope while ``multitick`` is for opaque inline code.
    // While rendering this we should make note of all the `singletick` locations
    // and (possibly in a higher up piece of logic) try to resolve the names for
    // cross-linking. (Similar to `TypeDetails` in the type formatting code.)
    let mut first_line = true;
    let mut block_indent = 0;
    let mut in_doctest = false;
    let mut in_markdown_with_fence = None;
    let mut starting_literal = None;
    let mut in_literal = false;
    let mut in_any_code = false;
    let mut temp_owned_line;
    for line in docstring.lines() {
        // We can assume leading whitespace has been normalized
        let trimmed_source_line = line.trim_start_matches(' ');
        let mut rendered_line = trimmed_source_line;
        let line_indent = line.len() - trimmed_source_line.len();

        // First thing's first, add a newline to start the new line
        if !first_line {
            // If we're not in a codeblock, add trailing space to the line to authentically wrap it
            // (Lines ending with two spaces tell markdown to preserve a linebreak)
            if !in_any_code {
                output.push_str("  ");
            }
            // Only push newlines if we're not scanning for a real line
            if starting_literal.is_none() {
                output.push('\n');
            }
        }
        first_line = false;

        // If we're in a literal block and we find a non-empty dedented line, end the block
        // TODO: we should remove all the trailing blank lines
        // (Just pop all trailing `\n` from `output`?)
        if in_literal && line_indent < block_indent && !rendered_line.is_empty() {
            in_literal = false;
            in_any_code = false;
            block_indent = 0;
            output.push_str(FENCE);
            output.push('\n');
        }

        // We previously entered a literal block and we just found our first non-blank line
        // So now we're actually in the literal block
        if let Some(literal) = starting_literal
            && !rendered_line.is_empty()
        {
            starting_literal = None;
            in_literal = true;
            in_any_code = true;
            block_indent = line_indent;
            output.push('\n');
            output.push_str(FENCE);
            output.push_str(literal);
            output.push('\n');
        }

        // If we're not in a codeblock and we see something that signals a doctest, start one
        if !in_any_code && rendered_line.starts_with(">>>") {
            block_indent = line_indent;
            in_doctest = true;
            in_any_code = true;
            // TODO: is there something more specific? `pycon`?
            output.push_str(FENCE);
            output.push_str("python\n");
        }

        // If we're not in a codeblock and we see a markdown codefence, start one
        if !in_any_code && let Some(fence) = MarkdownFence::find(trimmed_source_line) {
            // Unlike other blocks we don't need to emit fences because it's already markdown
            block_indent = line_indent;
            in_any_code = true;
            in_markdown_with_fence = Some(fence);
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
            output.push_str(rendered_line);
            continue;
        // If we're in a markdown code fence and this line seems to terminate it, end the block
        } else if let Some(fence) = in_markdown_with_fence
            && fence.is_closed_by(rendered_line)
        {
            in_any_code = false;
            block_indent = 0;
            in_markdown_with_fence = None;
            // Render the line without its indent and move on.
            output.push_str(rendered_line);
            continue;
        }

        // If we're not in a codeblock and we see something that signals a literal block, start one
        let parsed_lit = rendered_line
            // first check for a line ending with `::`
            .strip_suffix("::")
            .map(|prefix| (prefix, None))
            // if that fails, look for a line ending with `:: lang`
            .or_else(|| {
                let (prefix, lang) = rendered_line.rsplit_once(' ')?;
                let prefix = prefix.trim_end().strip_suffix("::")?;
                Some((prefix, Some(lang)))
            });
        if !in_any_code && let Some((without_lit, lang)) = parsed_lit {
            let mut without_directive = without_lit;
            let mut directive = None;
            // Parse out a directive like `.. warning::`
            if let Some((prefix, directive_str)) = without_lit.rsplit_once(' ')
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

            starting_literal = match directive {
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
                    None
                }
                // Things that just mean "it's code"
                Some(
                    "code-block" | "sourcecode" | "code" | "testcode" | "testsetup" | "testcleanup",
                ) => lang.or(Some("python")),
                // Unknown (python I guess?)
                Some(_) => lang.or(Some("python")),
                // default to python
                None => lang.or(Some("python")),
            };
        }

        // Add this line's indentation.
        // We could subtract the block_indent here but in practice it's uglier
        // TODO: should we not do this if the `line.is_empty()`? When would it matter?
        for _ in 0..line_indent {
            // Outside code blocks, emit indentation according to the selected mode.
            if !in_any_code && matches!(leading_indentation, LeadingIndentation::DisplayOnly) {
                // TODO: would the raw unicode codepoint be handled *better* or *worse*
                // by various IDEs? VS Code handles this approach well, at least.
                output.push_str("&nbsp;");
            } else {
                output.push(' ');
            }
        }

        if !in_any_code {
            // This line is plain text, so we need to escape things that are inert in reST
            // but active syntax in markdown... but not if it's inside `inline code`.
            // Inline-code syntax is shared by reST and markdown which is really convenient
            // except we need to find and parse it anyway to do this escaping properly! :(
            // For now we assume `inline code` does not span a line (I'm not even sure if can).
            //
            // Things that need to be escaped: underscores and HTML-sensitive characters.
            //
            // e.g. we want __init__ => \_\_init\_\_ but `__init__` => `__init__`
            let escape = |input: &str| {
                input
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('_', "\\_")
            };

            let mut in_inline_code = false;
            let mut first_chunk = true;
            let mut opening_tick_count = 0;
            let mut current_tick_count = 0;
            for chunk in rendered_line.split('`') {
                // First chunk is definitionally not in inline-code and so always plaintext
                if first_chunk {
                    first_chunk = false;
                    output.push_str(&escape(chunk));
                    continue;
                }
                // Not in first chunk, emit the ` between the last chunk and this one
                output.push('`');
                current_tick_count += 1;

                // If we're in an inline block and have enough close-ticks to terminate it, do so.
                // TODO: we parse ``hello```there` as (hello)(there) which probably isn't correct
                // (definitely not for markdown) but it's close enough for horse grenades in this
                // MVP impl. Notably we're verbatime emitting all the `'s so as long as reST and
                // markdown agree we're *fine*. The accuracy of this parsing only affects the
                // accuracy of where we apply escaping (so we need to misparse and see escapables
                // for any of this to matter).
                if opening_tick_count > 0 && current_tick_count >= opening_tick_count {
                    opening_tick_count = 0;
                    current_tick_count = 0;
                    in_inline_code = false;
                }

                // If this chunk is completely empty we're just in a run of ticks, continue
                if chunk.is_empty() {
                    continue;
                }

                // Ok the chunk is non-empty, our run of ticks is complete
                if in_inline_code {
                    // The previous check for >= open_tick_count didn't trip, so these can't close
                    // and these ticks will be verbatim rendered in the content
                    current_tick_count = 0;
                } else if current_tick_count > 0 {
                    // Ok we're now in inline code
                    opening_tick_count = current_tick_count;
                    current_tick_count = 0;
                    in_inline_code = true;
                }

                // Finally include the content either escaped or not
                if in_inline_code {
                    output.push_str(chunk);
                } else {
                    output.push_str(&escape(chunk));
                }
            }
            // NOTE: explicitly not "flushing" the ticks here.
            // We respect however the user closed their inline code.
        } else if rendered_line.is_empty() {
            if in_doctest {
                // This is the end of a doctest
                block_indent = 0;
                in_any_code = false;
                in_doctest = false;
                output.push_str(FENCE);
            }
        } else {
            // Print the line verbatim, it's in code
            output.push_str(rendered_line);
        }
    }
    // Flush codeblock
    if in_any_code {
        output.push('\n');
        if let Some(fence) = in_markdown_with_fence {
            output.push_str(fence.marker());
        } else {
            output.push_str(FENCE);
        }
    }
}

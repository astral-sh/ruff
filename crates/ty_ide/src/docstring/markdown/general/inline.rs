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
pub(super) fn render_line(output: &mut String, line: &str) {
    let mut in_inline_code = false;
    let mut first_chunk = true;
    let mut opening_tick_count = 0;
    let mut current_tick_count = 0;

    for chunk in line.split('`') {
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

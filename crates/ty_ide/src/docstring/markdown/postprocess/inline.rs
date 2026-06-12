/// Renders inline markup within prose lines.
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
            current_tick_count = 0;
        } else if current_tick_count > 0 {
            opening_tick_count = current_tick_count;
            current_tick_count = 0;
            in_inline_code = true;
        }

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

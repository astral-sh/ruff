/// Renders inline markup within prose lines.
#[derive(Default)]
pub(super) struct Renderer {
    pending_hyperlink: Option<PendingHyperlink>,
}

#[derive(Clone, Copy)]
pub(super) struct Line<'a> {
    pub(super) markdown_prefix: &'a str,
    pub(super) source_prefix: &'a str,
    pub(super) text: &'a str,
}

impl Renderer {
    pub(super) fn render_line(&mut self, output: &mut String, line: Line<'_>) {
        if self.pending_hyperlink.is_some() {
            self.render_pending_line(output, line);
        } else {
            output.push_str(line.markdown_prefix);
            self.render_fragment(output, line.text);
        }
    }

    pub(super) fn flush_pending_as_plain(&mut self, output: &mut String) {
        if let Some(pending_hyperlink) = self.pending_hyperlink.take() {
            pending_hyperlink.render_as_plain(output);
        }
    }

    fn render_pending_line(&mut self, output: &mut String, line: Line<'_>) {
        let Some(mut pending_hyperlink) = self.pending_hyperlink.take() else {
            return;
        };

        let Some(candidate_end) = pending_line_candidate_end(line.text) else {
            pending_hyperlink.push_line(line.markdown_prefix, line.source_prefix, line.text);
            if line.text.is_empty() {
                pending_hyperlink.render_as_plain(output);
            } else {
                self.pending_hyperlink = Some(pending_hyperlink);
            }
            return;
        };

        pending_hyperlink.push_line(
            line.markdown_prefix,
            line.source_prefix,
            &line.text[..candidate_end],
        );
        if let Some(hyperlink) = Hyperlink::parse(&pending_hyperlink.candidate) {
            hyperlink.render_markdown(output);
        } else {
            pending_hyperlink.render_as_plain(output);
        }

        self.render_fragment(output, &line.text[candidate_end..]);
    }

    fn render_fragment(&mut self, output: &mut String, line: &str) {
        let mut rest = line;

        while let Some(opening_index) = rest.find('`') {
            push_escaped_markdown_text(output, &rest[..opening_index]);
            rest = &rest[opening_index..];

            if let Some(hyperlink) = Hyperlink::parse(rest) {
                hyperlink.render_markdown(output);
                rest = &rest[hyperlink.len..];
                continue;
            }

            if Hyperlink::is_unclosed_wrapped_candidate(rest) {
                self.pending_hyperlink = Some(PendingHyperlink::new(rest));
                return;
            }

            rest = render_inline_code_or_text(output, rest);
        }

        push_escaped_markdown_text(output, rest);
    }
}

fn pending_line_candidate_end(line: &str) -> Option<usize> {
    let closing_index = line.find('`')?;
    let tick_count = line[closing_index..]
        .bytes()
        .take_while(|byte| *byte == b'`')
        .count();
    let after_ticks = closing_index + tick_count;
    let underscore_count = line[after_ticks..]
        .bytes()
        .take_while(|byte| *byte == b'_')
        .count();

    Some(after_ticks + underscore_count)
}

struct PendingHyperlink {
    candidate: String,
    fallback: String,
}

impl PendingHyperlink {
    fn new(first_line: &str) -> Self {
        let mut fallback = String::new();
        render_inline_markup_line(&mut fallback, first_line);

        Self {
            candidate: first_line.to_owned(),
            fallback,
        }
    }

    fn push_line(&mut self, markdown_prefix: &str, source_prefix: &str, line: &str) {
        self.candidate.push_str(source_prefix);
        self.candidate.push_str(line);
        self.fallback.push_str(markdown_prefix);
        render_inline_markup_line(&mut self.fallback, line);
    }

    fn render_as_plain(self, output: &mut String) {
        output.push_str(&self.fallback);
    }
}

fn render_inline_markup_line(output: &mut String, line: &str) {
    let mut rest = line;

    while let Some(opening_index) = rest.find('`') {
        push_escaped_markdown_text(output, &rest[..opening_index]);
        rest = &rest[opening_index..];

        if let Some(hyperlink) = Hyperlink::parse(rest) {
            hyperlink.render_markdown(output);
            rest = &rest[hyperlink.len..];
            continue;
        }

        rest = render_inline_code_or_text(output, rest);
    }

    push_escaped_markdown_text(output, rest);
}

fn render_inline_code_or_text<'a>(output: &mut String, input: &'a str) -> &'a str {
    let tick_count = input.bytes().take_while(|byte| *byte == b'`').count();
    let delimiter = &input[..tick_count];
    let after_opening = &input[tick_count..];

    output.push_str(delimiter);

    let Some(closing_index) = find_closing_backtick_run(after_opening, tick_count) else {
        output.push_str(after_opening);
        return "";
    };

    output.push_str(&after_opening[..closing_index]);
    output.push_str(delimiter);
    &after_opening[closing_index + tick_count..]
}

fn find_closing_backtick_run(input: &str, opening_tick_count: usize) -> Option<usize> {
    let mut offset = 0;

    while let Some(index) = input[offset..].find('`') {
        let index = offset + index;
        let tick_count = input[index..]
            .bytes()
            .take_while(|byte| *byte == b'`')
            .count();

        if tick_count >= opening_tick_count {
            return Some(index);
        }

        offset = index + tick_count;
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hyperlink<'a> {
    text: &'a str,
    target: &'a str,
    len: usize,
}

impl<'a> Hyperlink<'a> {
    fn parse(input: &'a str) -> Option<Self> {
        if !input.starts_with('`') || input.as_bytes().get(1) == Some(&b'`') {
            return None;
        }

        let after_opening = &input[1..];
        let closing_index = after_opening.find('`')?;
        let after_closing = &after_opening[closing_index + 1..];
        let underscore_count = after_closing
            .bytes()
            .take_while(|byte| *byte == b'_')
            .count();
        if !(1..=2).contains(&underscore_count) {
            return None;
        }

        let content = &after_opening[..closing_index];
        let (text, target) = Self::parse_text_and_target(content)?;
        Some(Self {
            text,
            target,
            len: 1 + closing_index + 1 + underscore_count,
        })
    }

    fn parse_text_and_target(content: &'a str) -> Option<(&'a str, &'a str)> {
        let content = content.trim();
        let target_start = content.rfind('<')?;
        if !content.ends_with('>') {
            return None;
        }

        let before_target = &content[..target_start];
        if !before_target
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        {
            return None;
        }

        let text = before_target.trim();
        let target = content[target_start + 1..content.len() - 1].trim();
        (!text.is_empty() && !target.is_empty()).then_some((text, target))
    }

    fn is_unclosed_wrapped_candidate(input: &str) -> bool {
        input.starts_with('`')
            && input.as_bytes().get(1) != Some(&b'`')
            && !input[1..].contains('`')
    }

    fn render_markdown(&self, output: &mut String) {
        output.push('[');
        push_escaped_markdown_link_text(output, self.text);
        output.push_str("](");
        push_markdown_link_destination(output, self.target);
        output.push(')');
    }
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

fn push_escaped_markdown_link_text(output: &mut String, input: &str) {
    let mut pending_whitespace = false;

    for char in input.chars() {
        if char.is_whitespace() {
            pending_whitespace = true;
            continue;
        }

        if pending_whitespace {
            output.push(' ');
            pending_whitespace = false;
        }

        match char {
            '[' | ']' | '\\' => {
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
}

fn push_markdown_link_destination(output: &mut String, input: &str) {
    let mut chars = input.chars().peekable();

    while let Some(char) = chars.next() {
        if char.is_whitespace() {
            let mut has_line_break = matches!(char, '\n' | '\r');
            let mut whitespace_count = 1;

            while let Some(char) = chars.next_if(|char| char.is_whitespace()) {
                has_line_break |= matches!(char, '\n' | '\r');
                whitespace_count += 1;
            }

            if !has_line_break {
                for _ in 0..whitespace_count {
                    output.push_str("%20");
                }
            }

            continue;
        }

        match char {
            '(' | ')' | '\\' => {
                output.push('\\');
                output.push(char);
            }
            '<' => output.push_str("%3C"),
            '>' => output.push_str("%3E"),
            _ => output.push(char),
        }
    }
}

mod postprocess;

/// Represents a fenced code block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MarkdownFence<'a> {
    /// The string used to denote the start and end of the fenced code block (e.g., triple backticks).
    marker: &'a str,
}

impl<'a> MarkdownFence<'a> {
    pub(super) fn marker(&self) -> &'a str {
        self.marker
    }

    /// Recognizes the beginning of a fenced code block if one is present on the given line.
    pub(super) fn find(line: &'a str) -> Option<Self> {
        let line = line.trim_start_matches(' ');
        let has_tick_fence = line.starts_with("```");
        let has_tilde_fence = line.starts_with("~~~");
        if !has_tick_fence && !has_tilde_fence {
            return None;
        }

        let without_leading_fence = if has_tick_fence {
            line.trim_start_matches('`')
        } else {
            line.trim_start_matches('~')
        };
        let fence_len = line.len() - without_leading_fence.len();
        let fence = &line[..fence_len];

        // We *don't* want to consider ```hello``` as a codefence; that's inline code!
        (!without_leading_fence.contains(fence)).then_some(Self { marker: fence })
    }

    /// Returns whether `line` closes this fenced code block.
    pub(super) fn is_closed_by(&self, line: &str) -> bool {
        line.trim_start_matches(' ').starts_with(self.marker)
    }
}

pub(super) fn render(raw: &str) -> String {
    postprocess::render(raw)
}

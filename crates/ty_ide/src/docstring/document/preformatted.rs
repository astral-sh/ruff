use ruff_python_trivia::leading_indentation;
use ruff_text_size::TextSize;

use super::indentation as visual_indentation;

/// Represents a fenced Markdown code block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) struct MarkdownFence<'a> {
    /// The string used to denote the start and end of the fenced code block.
    marker: &'a str,
}

impl<'a> MarkdownFence<'a> {
    pub(in crate::docstring) fn marker(&self) -> &'a str {
        self.marker
    }

    /// Recognizes the beginning of a fenced code block if one is present on the given line.
    pub(in crate::docstring) fn find(line: &'a str) -> Option<Self> {
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
    pub(in crate::docstring) fn is_closed_by(&self, line: &str) -> bool {
        line.trim_start_matches(' ').starts_with(self.marker)
    }
}

/// Recognizes preformatted blocks that may occur within a docstring.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PreformattedBlockScanner<'a> {
    active_markdown_fence: Option<MarkdownFence<'a>>,
    active_doctest_indent: Option<TextSize>,
    rest_literal_blocks: RestLiteralBlockScanner,
}

/// The set of characters that can each be used to denote a block quote.
///
/// <https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#quoted-literal-blocks>
const QUOTED_LITERAL_BLOCK_QUOTE_CHARACTERS: &str = r##"!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"##;

impl<'a> PreformattedBlockScanner<'a> {
    /// Returns whether the scanner is currently inside an accepted preformatted block.
    pub(super) fn is_active(&self) -> bool {
        self.active_markdown_fence.is_some()
            || self.active_doctest_indent.is_some()
            || matches!(
                self.rest_literal_blocks.state,
                RestLiteralBlockState::Active(_)
            )
    }

    /// Updates internal state to reflect the given line and returns whether or
    /// not the given line is contained within a preformatted block.
    pub(super) fn consume_preformatted_line(&mut self, line: &'a str) -> bool {
        if let Some(fence) = self.active_markdown_fence {
            if fence.is_closed_by(line) {
                self.active_markdown_fence = None;
            }
            return true;
        }

        if self.rest_literal_blocks.consume_line(line) {
            return true;
        }

        if let Some(doctest_indent) = self.active_doctest_indent {
            if line.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
                self.active_doctest_indent = None;
                return true;
            } else if visual_indentation(line) < doctest_indent {
                self.active_doctest_indent = None;
            } else {
                return true;
            }
        }

        if Self::line_starts_doctest(line) {
            self.active_doctest_indent = Some(visual_indentation(line));
            return true;
        }

        if let Some(fence) = MarkdownFence::find(line) {
            self.active_markdown_fence = Some(fence);
            return true;
        }

        false
    }

    /// Updates internal state for a line outside of any active preformatted block.
    pub(super) fn observe_line_outside_preformatted_block(&mut self, line: &str) {
        self.rest_literal_blocks.observe_marker_in_line(line);
    }

    /// Whether or not the given line marks the start of a doctest.
    fn line_starts_doctest(line: &str) -> bool {
        line.trim_start_matches(' ').starts_with(">>>")
    }
}

/// Recognizes literal blocks introduced by reST syntax.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct RestLiteralBlockScanner {
    state: RestLiteralBlockState,
}

impl RestLiteralBlockScanner {
    /// Updates internal state for a possible reST literal block marker.
    pub(super) fn observe_marker_in_line(&mut self, line: &str) {
        self.observe_marker(line, indentation(line));
    }

    /// Updates internal state for a possible reST literal block marker whose text has already
    /// been split out from its source line.
    pub(super) fn observe_marker(&mut self, line: &str, marker_indent: TextSize) {
        let line = line.trim_start();
        if matches!(self.state, RestLiteralBlockState::Inactive)
            && Self::line_starts_literal_block(line)
        {
            self.state = RestLiteralBlockState::Pending {
                marker_indent,
                allows_quoted_literal_block: Self::allows_quoted_literal_block(line),
            };
        }
    }

    /// Consumes a line if it is inside a reST literal block already observed by `observe_marker`.
    pub(super) fn consume_line(&mut self, line: &str) -> bool {
        let current_indent = indentation(line);
        let line_is_empty = line.trim_start().is_empty();

        match self.state {
            RestLiteralBlockState::Active(RestLiteralBlockKind::Indented { marker_indent }) => {
                if !line_is_empty && current_indent <= marker_indent {
                    // We've reached the de-dent that marks the end of the literal block.
                    self.state = RestLiteralBlockState::Inactive;
                    false
                } else {
                    true
                }
            }
            RestLiteralBlockState::Active(RestLiteralBlockKind::Quoted { indent, quote }) => {
                if line_is_empty {
                    self.state = RestLiteralBlockState::Inactive;
                    false
                } else if Self::quote_character(line, indent) == Some(quote) {
                    true
                } else {
                    self.state = RestLiteralBlockState::Inactive;
                    false
                }
            }
            RestLiteralBlockState::Pending {
                marker_indent,
                allows_quoted_literal_block,
            } if !line_is_empty => {
                if current_indent > marker_indent {
                    // We just entered a new literal block.
                    self.state = RestLiteralBlockState::Active(RestLiteralBlockKind::Indented {
                        marker_indent,
                    });
                    true
                } else if allows_quoted_literal_block
                    && let Some(quote) = Self::quote_character(line, marker_indent)
                {
                    self.state = RestLiteralBlockState::Active(RestLiteralBlockKind::Quoted {
                        indent: marker_indent,
                        quote,
                    });
                    true
                } else {
                    self.state = RestLiteralBlockState::Inactive;
                    false
                }
            }
            RestLiteralBlockState::Pending { .. } | RestLiteralBlockState::Inactive => false,
        }
    }

    /// Whether or not the given line marks the start of a reST literal block.
    fn line_starts_literal_block(line: &str) -> bool {
        let Some(marker) = Self::literal_block_marker(line) else {
            return false;
        };

        !matches!(
            marker,
            RestLiteralBlockMarker::Directive(
                "attention"
                    | "caution"
                    | "danger"
                    | "error"
                    | "hint"
                    | "important"
                    | "note"
                    | "tip"
                    | "warning"
                    | "admonition"
                    | "seealso"
                    | "versionadded"
                    | "version-added"
                    | "versionchanged"
                    | "version-changed"
                    | "version-deprecated"
                    | "deprecated"
                    | "version-removed"
                    | "versionremoved",
            )
        )
    }

    /// Tries to identify a marker that introduces a reST literal block.
    fn literal_block_marker(line: &str) -> Option<RestLiteralBlockMarker<'_>> {
        let marker = if let Some(marker) = line.strip_suffix("::") {
            marker
        } else {
            let (before_language, _language) = line.rsplit_once(' ')?;
            before_language.trim_end().strip_suffix("::")?
        };

        if let Some(directive) = marker.strip_prefix(".. ") {
            Some(RestLiteralBlockMarker::Directive(directive))
        } else {
            Some(RestLiteralBlockMarker::Paragraph)
        }
    }

    /// Whether or not a particular literal block can contain an unindented quoted literal block.
    fn allows_quoted_literal_block(line: &str) -> bool {
        line.ends_with("::")
            && matches!(
                Self::literal_block_marker(line),
                Some(RestLiteralBlockMarker::Paragraph)
            )
    }

    /// Returns the quote character for a quoted literal block line.
    fn quote_character(line: &str, indent: TextSize) -> Option<char> {
        if indentation(line) != indent {
            return None;
        }

        let quote = line.get(indent.to_usize()..)?.chars().next()?;
        QUOTED_LITERAL_BLOCK_QUOTE_CHARACTERS
            .contains(quote)
            .then_some(quote)
    }
}

/// Identifies the syntax that introduced a potential reST literal block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestLiteralBlockMarker<'a> {
    Paragraph,
    Directive(&'a str),
}

/// Tracks the state of a literal block introduced by reST syntax.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum RestLiteralBlockState {
    #[default]
    Inactive,
    Pending {
        marker_indent: TextSize,
        allows_quoted_literal_block: bool,
    },
    Active(RestLiteralBlockKind),
}

/// Tracks the type of an active reST literal block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestLiteralBlockKind {
    Indented { marker_indent: TextSize },
    Quoted { indent: TextSize, quote: char },
}

fn indentation(line: &str) -> TextSize {
    TextSize::of(leading_indentation(line))
}

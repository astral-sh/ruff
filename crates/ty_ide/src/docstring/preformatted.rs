use ruff_python_trivia::leading_indentation;
use ruff_text_size::TextSize;

use super::markdown;

/// Recognizes preformatted blocks that may occur within a docstring.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PreformattedBlockScanner<'a> {
    active_markdown_fence: Option<markdown::MarkdownFence<'a>>,
    active_doctest: bool,
    preformatted_block_state: PreformattedBlockState,
}

/// The set of characters that can each be used to denote a block quote.
///
/// <https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#quoted-literal-blocks>
const QUOTED_LITERAL_BLOCK_QUOTE_CHARACTERS: &str = r##"!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"##;

impl<'a> PreformattedBlockScanner<'a> {
    /// Returns whether the given line opens an accepted preformatted block.
    pub(super) fn line_starts_preformatted_block(line: &'a str) -> bool {
        markdown::MarkdownFence::find(line).is_some()
            || Self::line_starts_doctest(line)
            || Self::line_starts_rest_preformatted_block(line.trim_start_matches(' '))
    }

    /// Returns whether the scanner is currently inside an accepted preformatted block.
    #[expect(
        dead_code,
        reason = "used by parsed docstring rendering in follow-up changes"
    )]
    pub(super) fn is_active(&self) -> bool {
        self.active_markdown_fence.is_some()
            || self.active_doctest
            || matches!(
                self.preformatted_block_state,
                PreformattedBlockState::Active(_)
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

        if self.is_within_preformatted_block(line) {
            return true;
        }

        if self.active_doctest {
            if line.trim_start_matches(' ').is_empty() {
                self.active_doctest = false;
            }
            return true;
        }

        if Self::line_starts_doctest(line) {
            self.active_doctest = true;
            return true;
        }

        if !Self::line_starts_preformatted_block(line) {
            return false;
        }

        if let Some(fence) = markdown::MarkdownFence::find(line) {
            self.active_markdown_fence = Some(fence);
            return true;
        }

        false
    }

    /// Updates internal state that allows us to detect preformatted blocks introduced by reST
    /// syntax.
    pub(super) fn observe_non_preformatted_line(&mut self, line: &str) {
        if matches!(
            self.preformatted_block_state,
            PreformattedBlockState::Inactive
        ) && Self::line_starts_rest_preformatted_block(line.trim_start())
        {
            self.preformatted_block_state = PreformattedBlockState::Pending {
                marker_indent: indentation(line),
                allows_quoted_literal_block: Self::allows_quoted_literal_block(line.trim_start()),
            };
        }
    }

    /// Whether or not the given line is specifically within a preformatted block
    /// introduced by reST syntax.
    fn is_within_preformatted_block(&mut self, line: &str) -> bool {
        let current_indent = indentation(line);
        let line_is_empty = line.trim_start().is_empty();

        match self.preformatted_block_state {
            PreformattedBlockState::Active(PreformattedBlockKind::Indented { marker_indent }) => {
                if !line_is_empty && current_indent <= marker_indent {
                    // We've reached the de-dent that marks the end of the preformatted block.
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                } else {
                    true
                }
            }
            PreformattedBlockState::Active(PreformattedBlockKind::QuotedLiteral {
                indent,
                quote,
            }) => {
                if line_is_empty {
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                } else if Self::quote_character(line, indent) == Some(quote) {
                    true
                } else {
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                }
            }
            PreformattedBlockState::Pending {
                marker_indent,
                allows_quoted_literal_block,
            } if !line_is_empty => {
                if current_indent > marker_indent {
                    // We just entered a new preformatted block.
                    self.preformatted_block_state =
                        PreformattedBlockState::Active(PreformattedBlockKind::Indented {
                            marker_indent,
                        });
                    true
                } else if allows_quoted_literal_block
                    && let Some(quote) = Self::quote_character(line, marker_indent)
                {
                    self.preformatted_block_state =
                        PreformattedBlockState::Active(PreformattedBlockKind::QuotedLiteral {
                            indent: marker_indent,
                            quote,
                        });
                    true
                } else {
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                }
            }
            PreformattedBlockState::Pending { .. } | PreformattedBlockState::Inactive => false,
        }
    }

    /// Whether or not the given line marks the start of a reST preformatted block.
    fn line_starts_rest_preformatted_block(line: &str) -> bool {
        let Some(marker) = Self::preformatted_block_marker(line) else {
            return false;
        };

        !matches!(
            marker,
            PreformattedBlockMarker::Directive(
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

    /// Tries to identify a marker that introduces a preformatted block.
    fn preformatted_block_marker(line: &str) -> Option<PreformattedBlockMarker<'_>> {
        let marker = if let Some(marker) = line.strip_suffix("::") {
            marker
        } else {
            let (before_language, _language) = line.rsplit_once(' ')?;
            before_language.trim_end().strip_suffix("::")?
        };

        if let Some(directive) = marker.strip_prefix(".. ") {
            Some(PreformattedBlockMarker::Directive(directive))
        } else {
            Some(PreformattedBlockMarker::Paragraph)
        }
    }

    /// Whether or not the given line marks the start of a doctest.
    fn line_starts_doctest(line: &str) -> bool {
        line.trim_start_matches(' ').starts_with(">>>")
    }

    /// Whether or not a particular preformatted block can contain an unindented quoted literal block.
    fn allows_quoted_literal_block(line: &str) -> bool {
        line.ends_with("::")
            && matches!(
                Self::preformatted_block_marker(line),
                Some(PreformattedBlockMarker::Paragraph)
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

/// Identifies the syntax that introduced a potential preformatted block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreformattedBlockMarker<'a> {
    Paragraph,
    Directive(&'a str),
}

/// Tracks the state of a preformatted block introduced by reST syntax.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PreformattedBlockState {
    #[default]
    Inactive,
    Pending {
        marker_indent: TextSize,
        allows_quoted_literal_block: bool,
    },
    Active(PreformattedBlockKind),
}

/// Tracks the type of an active preformatted block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreformattedBlockKind {
    Indented { marker_indent: TextSize },
    QuotedLiteral { indent: TextSize, quote: char },
}

fn indentation(line: &str) -> TextSize {
    TextSize::of(leading_indentation(line))
}

use std::fmt::{Debug, Formatter, Write};

use itertools::Itertools;

use ruff_formatter::SourceCode;
use ruff_text_size::Ranged;

use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::{CommentsMap, SourceComment};

/// Prints a debug representation of [`SourceComment`] that includes the comment's text
pub(crate) struct DebugComment<'a> {
    comment: &'a SourceComment,
    source_code: SourceCode<'a>,
}

impl<'a> DebugComment<'a> {
    pub(super) fn new(comment: &'a SourceComment, source_code: SourceCode<'a>) -> Self {
        Self {
            comment,
            source_code,
        }
    }
}

impl Debug for DebugComment<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut strut = f.debug_struct("SourceComment");

        strut
            .field("text", &self.comment.slice.text(self.source_code))
            .field("position", &self.comment.line_position)
            .field("formatted", &self.comment.formatted.get());

        strut.finish()
    }
}

/// Pretty-printed debug representation of [`Comments`].
pub(crate) struct DebugComments<'a> {
    comments: &'a CommentsMap<'a>,
    source_code: SourceCode<'a>,
}

impl<'a> DebugComments<'a> {
    pub(super) fn new(comments: &'a CommentsMap, source_code: SourceCode<'a>) -> Self {
        Self {
            comments,
            source_code,
        }
    }
}

impl Debug for DebugComments<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for node in self
            .comments
            .keys()
            .sorted_by_key(|key| (key.node().start(), key.node().end()))
        {
            map.entry(
                &NodeKindWithSource {
                    key: *node,
                    source: self.source_code,
                },
                &DebugNodeComments {
                    comments: self.comments,
                    source_code: self.source_code,
                    key: *node,
                },
            );
        }

        map.finish()
    }
}

/// Prints the source code up to the first new line character. Truncates the text if it exceeds 40 characters.
struct NodeKindWithSource<'a> {
    key: NodeRefEqualityKey<'a>,
    source: SourceCode<'a>,
}

impl Debug for NodeKindWithSource<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct TruncatedSource<'a>(&'a str);

        impl Debug for TruncatedSource<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_char('`')?;
                let first_line = if let Some(line_end_pos) = self.0.find(['\n', '\r']) {
                    &self.0[..line_end_pos]
                } else {
                    self.0
                };

                if first_line.len() > 40 {
                    let (head, rest) = first_line.split_at(27);

                    f.write_str(head)?;
                    f.write_str("...")?;

                    // Take the last 10 characters
                    let tail = &rest[rest.len().saturating_sub(10)..];
                    f.write_str(tail)?;
                } else {
                    f.write_str(first_line)?;
                }

                if first_line.len() < self.0.len() {
                    f.write_str("\u{23ce}")?;
                }

                f.write_char('`')
            }
        }

        let kind = self.key.node().kind();
        let source = self.source.slice(self.key.node().range()).text(self.source);

        f.debug_struct("Node")
            .field("kind", &kind)
            .field("range", &self.key.node().range())
            .field("source", &TruncatedSource(source))
            .finish()
    }
}

struct DebugNodeComments<'a> {
    comments: &'a CommentsMap<'a>,
    source_code: SourceCode<'a>,
    key: NodeRefEqualityKey<'a>,
}

impl Debug for DebugNodeComments<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entry(
                &"leading",
                &DebugNodeCommentSlice {
                    node_comments: self.comments.leading(&self.key),
                    source_code: self.source_code,
                },
            )
            .entry(
                &"dangling",
                &DebugNodeCommentSlice {
                    node_comments: self.comments.dangling(&self.key),
                    source_code: self.source_code,
                },
            )
            .entry(
                &"trailing",
                &DebugNodeCommentSlice {
                    node_comments: self.comments.trailing(&self.key),
                    source_code: self.source_code,
                },
            )
            .finish()
    }
}

struct DebugNodeCommentSlice<'a> {
    node_comments: &'a [SourceComment],
    source_code: SourceCode<'a>,
}

impl Debug for DebugNodeCommentSlice<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();

        for comment in self.node_comments {
            list.entry(&comment.debug(self.source_code));
        }

        list.finish()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use ruff_formatter::SourceCode;
    use ruff_python_ast::AnyNodeRef;
    use ruff_python_ast::{StmtBreak, StmtContinue};
    use ruff_python_trivia::{CommentLinePosition, CommentRanges};
    use ruff_text_size::{TextRange, TextSize};

    use crate::comments::map::MultiMap;
    use crate::comments::{Comments, CommentsMap, SourceComment};

    #[test]
    fn debug() {
        let continue_statement = StmtContinue {
            range: TextRange::new(TextSize::new(18), TextSize::new(26)),
        };

        let break_statement = StmtBreak {
            range: TextRange::new(TextSize::new(55), TextSize::new(60)),
        };

        let source = r"# leading comment
continue; # trailing
# break leading
break;
";

        let source_code = SourceCode::new(source);

        let mut comments_map: CommentsMap = MultiMap::new();

        comments_map.push_leading(
            AnyNodeRef::from(&continue_statement).into(),
            SourceComment::new(
                source_code.slice(TextRange::at(TextSize::new(0), TextSize::new(17))),
                CommentLinePosition::OwnLine,
            ),
        );

        comments_map.push_trailing(
            AnyNodeRef::from(&continue_statement).into(),
            SourceComment::new(
                source_code.slice(TextRange::at(TextSize::new(28), TextSize::new(10))),
                CommentLinePosition::EndOfLine,
            ),
        );

        comments_map.push_leading(
            AnyNodeRef::from(&break_statement).into(),
            SourceComment::new(
                source_code.slice(TextRange::at(TextSize::new(39), TextSize::new(15))),
                CommentLinePosition::OwnLine,
            ),
        );

        let comment_ranges = CommentRanges::default();
        let comments = Comments::new(comments_map, &comment_ranges);

        assert_debug_snapshot!(comments.debug(source_code));
    }
}

use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::{CommentsMap, SourceComment};
use ruff_formatter::SourceCode;
use std::fmt::{Debug, Formatter};

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

        strut.field("text", &self.comment.slice.text(self.source_code));

        #[cfg(debug_assertions)]
        strut.field("formatted", &self.comment.formatted.get());

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

        for node in self.comments.keys() {
            map.entry(
                &node,
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
    use crate::comments::map::MultiMap;
    use crate::comments::node_key::NodeRefEqualityKey;
    use crate::comments::{node_key, Comments, CommentsData};
    use crate::comments::{CommentsMap, SourceComment};
    use insta::assert_debug_snapshot;
    use ruff_formatter::SourceCode;
    use ruff_python_ast::node::AnyNode;
    use ruff_python_ast::source_code;
    use ruff_text_size::{TextRange, TextSize};
    use rustpython_parser::ast::{StmtBreak, StmtContinue};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn debug() {
        let continue_statement = AnyNode::from(StmtContinue {
            range: TextRange::default(),
        });

        let break_statement = AnyNode::from(StmtBreak {
            range: TextRange::default(),
        });

        let source = r#"# leading comment
continue; # trailing
# break leading
break;
"#;

        let source_code = SourceCode::new(source);

        let mut comments_map: CommentsMap = MultiMap::new();

        comments_map.push_leading(
            continue_statement.as_ref().into(),
            SourceComment {
                slice: source_code.slice(TextRange::at(TextSize::new(0), TextSize::new(17))),
                formatted: Cell::new(false),
            },
        );

        comments_map.push_trailing(
            continue_statement.as_ref().into(),
            SourceComment {
                slice: source_code.slice(TextRange::at(TextSize::new(28), TextSize::new(10))),
                formatted: Cell::new(false),
            },
        );

        comments_map.push_leading(
            break_statement.as_ref().into(),
            SourceComment {
                slice: source_code.slice(TextRange::at(TextSize::new(39), TextSize::new(15))),
                formatted: Cell::new(false),
            },
        );

        let comments = Comments {
            data: Rc::new(CommentsData {
                comments: comments_map,
            }),
        };

        assert_debug_snapshot!(comments.debug(source_code));
    }
}

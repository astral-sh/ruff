use crate::comments::visitor::{CommentPlacement, DecoratedComment};

use crate::comments::CommentTextPosition;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::whitespace;
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Ranged;
use std::cmp::Ordering;

/// Implements the custom comment placement logic.
pub(super) fn place_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    handle_in_between_excepthandlers_or_except_handler_and_else_or_finally_comment(comment, locator)
        .or_else(|comment| handle_match_comment(comment, locator))
        .or_else(|comment| handle_in_between_bodies_comment(comment, locator))
        .or_else(|comment| handle_trailing_body_comment(comment, locator))
        .or_else(|comment| handle_positional_only_arguments_separator_comment(comment, locator))
}

/// Handles leading comments in front of a match case or a trailing comment of the `match` statement.
/// ```python
/// match pt:
///     # Leading `case(x, y)` comment
///     case (x, y):
///         return Point3d(x, y, 0)
///     # Leading `case (x, y, z)` comment
///     case _:
/// ```
fn handle_match_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // Must be an own line comment after the last statement in a match case
    if comment.text_position().is_end_of_line() || comment.following_node().is_some() {
        return CommentPlacement::Default(comment);
    }

    // Get the enclosing match case
    let Some(match_case) = comment.enclosing_node().match_case() else {
        return CommentPlacement::Default(comment)
    };

    // And its parent match statement.
    let Some(match_stmt) = comment
        .enclosing_parent()
        .and_then(AnyNodeRef::stmt_match) else {
        return CommentPlacement::Default(comment)
    };

    // Get the next sibling (sibling traversal would be really nice)
    let current_case_index = match_stmt
        .cases
        .iter()
        .position(|case| case == match_case)
        .expect("Expected case to belong to parent match statement.");

    let next_case = match_stmt.cases.get(current_case_index + 1);

    let comment_indentation =
        whitespace::indentation_at_offset(locator, comment.slice().range().start())
            .map(str::len)
            .unwrap_or_default();
    let match_case_indentation = whitespace::indentation(locator, match_case).unwrap().len();

    if let Some(next_case) = next_case {
        // The comment's indentation is less or equal to the `case` indention and there's a following
        // `case` arm.
        // ```python
        // match pt:
        //     case (x, y):
        //         return Point3d(x, y, 0)
        //     # Leading `case (x, y, z)` comment
        //     case _:
        //         pass
        // ```
        // Attach the `comment` as leading comment to the next case.
        if comment_indentation <= match_case_indentation {
            CommentPlacement::leading(next_case.into(), comment)
        } else {
            // Otherwise, delegate to `handle_trailing_body_comment`
            // ```python
            // match pt:
            //     case (x, y):
            //         return Point3d(x, y, 0)
            //         # Trailing case body comment
            //     case _:
            //         pass
            // ```
            CommentPlacement::Default(comment)
        }
    } else {
        // Comment after the last statement in a match case...
        let match_stmt_indentation = whitespace::indentation(locator, match_stmt)
            .unwrap_or_default()
            .len();

        if comment_indentation <= match_case_indentation
            && comment_indentation > match_stmt_indentation
        {
            // The comment's indent matches the `case` indent (or is larger than the `match`'s indent).
            // ```python
            // match pt:
            //     case (x, y):
            //         return Point3d(x, y, 0)
            //     case _:
            //         pass
            //     # Trailing match comment
            // ```
            // This is a trailing comment of the last case.
            CommentPlacement::trailing(match_case.into(), comment)
        } else {
            // Delegate to `handle_trailing_body_comment` because it's either a trailing indent
            // for the last statement in the `case` body or a comment for the parent of the `match`
            //
            // ```python
            // match pt:
            //     case (x, y):
            //         return Point3d(x, y, 0)
            //     case _:
            //         pass
            //         # trailing case comment
            // ```
            CommentPlacement::Default(comment)
        }
    }
}

/// Handles comments between excepthandlers and between the last except handler and any following `else` or `finally` block.
fn handle_in_between_excepthandlers_or_except_handler_and_else_or_finally_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    if comment.text_position().is_end_of_line() || comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    if let Some(AnyNodeRef::ExcepthandlerExceptHandler(except_handler)) = comment.preceding_node() {
        // it now depends on the indentation level of the comment if it is a leading comment for e.g.
        // the following `elif` or indeed a trailing comment of the previous body's last statement.
        let comment_indentation =
            whitespace::indentation_at_offset(locator, comment.slice().range().start())
                .map(str::len)
                .unwrap_or_default();

        if let Some(except_indentation) =
            whitespace::indentation(locator, except_handler).map(str::len)
        {
            return if comment_indentation <= except_indentation {
                // It has equal, or less indent than the `except` handler. It must be a comment
                // of the following `finally` or `else` block
                //
                // ```python
                // try:
                //     pass
                // except Exception:
                //     print("noop")
                // # leading
                // finally:
                //     pass
                // ```
                // Attach it to the `try` statement.
                CommentPlacement::dangling(comment.enclosing_node(), comment)
            } else {
                // Delegate to `handle_trailing_body_comment`
                CommentPlacement::Default(comment)
            };
        }
    }

    CommentPlacement::Default(comment)
}

/// Handles comments between the last statement and the first statement of two bodies.
///
/// ```python
/// if x == y:
///     pass
///     # This should be a trailing comment of `pass` and not a leading comment of the `print`
///     # in the `else` branch
/// else:
///     print("I have no comments")
/// ```
fn handle_in_between_bodies_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    use ruff_python_ast::prelude::*;

    // The rule only applies to own line comments. The default logic associates end of line comments
    // correctly.
    if comment.text_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    // The comment must be between two statements...
    if let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    {
        // ...and the following statement must be the first statement in an alternate body of the parent...
        let is_following_the_first_statement_in_a_parents_alternate_body =
            match comment.enclosing_node() {
                AnyNodeRef::StmtIf(StmtIf { orelse, .. })
                | AnyNodeRef::StmtFor(StmtFor { orelse, .. })
                | AnyNodeRef::StmtAsyncFor(StmtAsyncFor { orelse, .. })
                | AnyNodeRef::StmtWhile(StmtWhile { orelse, .. }) => {
                    are_same_optional(following, orelse.first())
                }

                AnyNodeRef::StmtTry(StmtTry {
                    handlers,
                    orelse,
                    finalbody,
                    ..
                })
                | AnyNodeRef::StmtTryStar(StmtTryStar {
                    handlers,
                    orelse,
                    finalbody,
                    ..
                }) => {
                    are_same_optional(following, handlers.first())
                    // Comments between the handlers and the `else`, or comments between the `handlers` and the `finally`
                    // are already handled by `handle_in_between_excepthandlers_or_except_handler_and_else_or_finally_comment`
                    || handlers.is_empty() && are_same_optional(following, orelse.first())
                    || (handlers.is_empty() || !orelse.is_empty())
                        && are_same_optional(following, finalbody.first())
                }

                _ => false,
            };

        if !is_following_the_first_statement_in_a_parents_alternate_body {
            // ```python
            // if test:
            //     a
            //     # comment
            //     b
            // ```
            return CommentPlacement::Default(comment);
        }

        // it now depends on the indentation level of the comment if it is a leading comment for e.g.
        // the following `elif` or indeed a trailing comment of the previous body's last statement.
        let comment_indentation =
            whitespace::indentation_at_offset(locator, comment.slice().range().start())
                .map(str::len)
                .unwrap_or_default();

        if let Some(preceding_indentation) =
            whitespace::indentation(locator, &preceding).map(str::len)
        {
            return if comment_indentation >= preceding_indentation {
                // `# comment` has the same or a larger indent than the `pass` statement.
                // It likely is a trailing comment of the `pass` statement.
                // ```python
                // if x == y:
                //     pass
                //     # comment
                // else:
                //     print("noop")
                // ```
                CommentPlacement::trailing(preceding, comment)
            } else {
                // Otherwise it has less indent than the previous statement. Meaning that it is a leading comment
                // of the following block.
                //
                // ```python
                // if x == y:
                //     pass
                // # I'm a leading comment of the `elif` statement.
                // elif:
                //     print("nooop")
                // ```
                if following.is_stmt_if() || following.is_except_handler() {
                    // The `elif` or except handlers have their own body to which we can attach the leading comment
                    CommentPlacement::leading(following, comment)
                } else {
                    // There are no bodies for the "else" branch and other bodies that are represented as a `Vec<Stmt>`.
                    // This means, there's no good place to attach the comments to.
                    // That's why we make these dangling comments and format them  manually
                    // in the enclosing node's formatting logic. For `try`, it's the formatters responsibility
                    // to correctly identify the comments for the `finally` and `orelse` block by looking
                    // at the comment's range.
                    //
                    // ```python
                    // if x == y:
                    //     pass
                    // # I'm a leading comment of the `else` branch but there's no `else` node.
                    // else:
                    //     print("nooop")
                    // ```
                    CommentPlacement::dangling(comment.enclosing_node(), comment)
                }
            };
        }
    }

    CommentPlacement::Default(comment)
}

/// Handles trailing comments at the end of a body block (or any other block that is indented).
/// ```python
/// def test():
///     pass
///     # This is a trailing comment that belongs to the function `test`
///     # and not to the next statement.
///
/// print("I have no comments")
/// ```
fn handle_trailing_body_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    if comment.text_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    // Only do something if the preceding node has a body (has indented statements).
    let Some(last_child) = comment.preceding_node().and_then(last_child_in_body) else {
        return CommentPlacement::Default(comment);
    };

    let Some(comment_indentation) = whitespace::indentation_at_offset(locator, comment.slice().range().start()) else {
        // The comment can't be a comment for the previous block if it isn't indented..
        return CommentPlacement::Default(comment);
    };

    // We only care about the length because indentations with mixed spaces and tabs are only valid if
    // the indent-level doesn't depend on the tab width (the indent level must be the same if the tab width is 1 or 8).
    let comment_indentation_len = comment_indentation.len();

    let mut current_child = last_child;
    let mut parent_body = comment.preceding_node();
    let mut grand_parent_body = None;

    loop {
        let child_indentation = whitespace::indentation(locator, &current_child)
            .map(str::len)
            .unwrap_or_default();

        match comment_indentation_len.cmp(&child_indentation) {
            Ordering::Less => {
                break if let Some(parent_block) = grand_parent_body {
                    // Comment belongs to the parent block.
                    // ```python
                    // if test:
                    //      if other:
                    //          pass
                    //        # comment
                    // ```
                    CommentPlacement::trailing(parent_block, comment)
                } else {
                    // The comment does not belong to this block.
                    // ```python
                    // if test:
                    //      pass
                    // # comment
                    // ```
                    CommentPlacement::Default(comment)
                };
            }
            Ordering::Equal => {
                // The comment belongs to this block.
                // ```python
                // if test:
                //     pass
                //     # comment
                // ```
                break CommentPlacement::trailing(current_child, comment);
            }
            Ordering::Greater => {
                if let Some(nested_child) = last_child_in_body(current_child) {
                    // The comment belongs to the inner block.
                    // ```python
                    // def a():
                    //     if test:
                    //         pass
                    //         # comment
                    // ```
                    // Comment belongs to the `if`'s inner body
                    grand_parent_body = parent_body;
                    parent_body = Some(current_child);
                    current_child = nested_child;
                } else {
                    // The comment belongs to this block.
                    // ```python
                    // if test:
                    //     pass
                    //         # comment
                    // ```
                    break CommentPlacement::trailing(current_child, comment);
                }
            }
        }
    }
}

/// Attaches comments for the positional-only arguments separator `/` as trailing comments to the
/// enclosing [`Arguments`] node.
///
/// ```python
/// def test(
///     a,
///     # Positional arguments only after here
///     /, # trailing positional argument comment.
///     b,
/// ): pass
/// ```
fn handle_positional_only_arguments_separator_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let AnyNodeRef::Arguments(arguments) = comment.enclosing_node() else {
        return CommentPlacement::Default(comment);
    };

    // Using the `/` without any leading arguments is a syntax error.
    let Some(last_argument_or_default) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    let is_last_positional_argument = are_same_optional(last_argument_or_default, arguments.posonlyargs.last())
            // If the preceding node is the default for the last positional argument
            // ```python
            // def test(a=10, /, b): pass
            // ```
            || arguments
                .defaults
                .iter()
                .position(|default| AnyNodeRef::from(default).ptr_eq(last_argument_or_default))
                == Some(arguments.posonlyargs.len().saturating_sub(1));

    if !is_last_positional_argument {
        return CommentPlacement::Default(comment);
    }

    let trivia_end = comment
        .following_node()
        .map_or(arguments.end(), |following| following.start());
    let trivia_range = TextRange::new(last_argument_or_default.end(), trivia_end);

    if let Some(slash_offset) = find_pos_only_slash_offset(trivia_range, locator) {
        let comment_start = comment.slice().range().start();
        let is_slash_comment = match comment.text_position() {
            CommentTextPosition::EndOfLine => {
                let preceding_end_line = locator.line_end(last_argument_or_default.end());
                let slash_comments_start = preceding_end_line.min(slash_offset);

                comment_start >= slash_comments_start
                    && locator.line_end(slash_offset) > comment_start
            }
            CommentTextPosition::OwnLine => comment_start < slash_offset,
        };

        if is_slash_comment {
            CommentPlacement::dangling(comment.enclosing_node(), comment)
        } else {
            CommentPlacement::Default(comment)
        }
    } else {
        // Should not happen, but let's go with it
        CommentPlacement::Default(comment)
    }
}

fn find_pos_only_slash_offset(trivia_range: TextRange, locator: &Locator) -> Option<TextSize> {
    let mut in_comment = false;

    for (offset, c) in locator.slice(trivia_range).char_indices() {
        match c {
            '\n' | '\r' => {
                in_comment = false;
            }
            '/' if !in_comment => {
                return Some(trivia_range.start() + TextSize::try_from(offset).unwrap());
            }
            '#' => {
                // SAFE because we know there's only trivia. So all content is either whitespace,
                // or comments but never strings.
                in_comment = true;
            }
            _ => {}
        }
    }

    None
}

fn are_same_optional<'a, T>(left: AnyNodeRef, right: Option<T>) -> bool
where
    T: Into<AnyNodeRef<'a>>,
{
    right.map_or(false, |right| left.ptr_eq(right.into()))
}

fn last_child_in_body(node: AnyNodeRef) -> Option<AnyNodeRef> {
    use ruff_python_ast::prelude::*;

    let body = match node {
        AnyNodeRef::StmtFunctionDef(StmtFunctionDef { body, .. })
        | AnyNodeRef::StmtAsyncFunctionDef(StmtAsyncFunctionDef { body, .. })
        | AnyNodeRef::StmtClassDef(StmtClassDef { body, .. })
        | AnyNodeRef::StmtWith(StmtWith { body, .. })
        | AnyNodeRef::StmtAsyncWith(StmtAsyncWith { body, .. })
        | AnyNodeRef::MatchCase(MatchCase { body, .. })
        | AnyNodeRef::ExcepthandlerExceptHandler(ExcepthandlerExceptHandler { body, .. }) => body,

        AnyNodeRef::StmtIf(StmtIf { body, orelse, .. })
        | AnyNodeRef::StmtFor(StmtFor { body, orelse, .. })
        | AnyNodeRef::StmtAsyncFor(StmtAsyncFor { body, orelse, .. })
        | AnyNodeRef::StmtWhile(StmtWhile { body, orelse, .. }) => {
            if orelse.is_empty() {
                body
            } else {
                orelse
            }
        }

        AnyNodeRef::StmtMatch(StmtMatch { cases, .. }) => {
            return cases.last().map(AnyNodeRef::from)
        }

        AnyNodeRef::StmtTry(StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        })
        | AnyNodeRef::StmtTryStar(StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            if finalbody.is_empty() {
                if orelse.is_empty() {
                    if handlers.is_empty() {
                        body
                    } else {
                        return handlers.last().map(AnyNodeRef::from);
                    }
                } else {
                    orelse
                }
            } else {
                finalbody
            }
        }

        // Not a node that contains an indented child node.
        _ => return None,
    };

    body.last().map(AnyNodeRef::from)
}

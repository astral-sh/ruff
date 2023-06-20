use std::cmp::Ordering;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Ranged;

use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::whitespace;
use ruff_python_whitespace::{PythonWhitespace, UniversalNewlines};

use crate::comments::visitor::{CommentPlacement, DecoratedComment};
use crate::comments::CommentTextPosition;
use crate::trivia::{SimpleTokenizer, Token, TokenKind};

/// Implements the custom comment placement logic.
pub(super) fn place_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    handle_in_between_except_handlers_or_except_handler_and_else_or_finally_comment(
        comment, locator,
    )
    .or_else(|comment| handle_match_comment(comment, locator))
    .or_else(|comment| handle_in_between_bodies_own_line_comment(comment, locator))
    .or_else(|comment| handle_in_between_bodies_end_of_line_comment(comment, locator))
    .or_else(|comment| handle_trailing_body_comment(comment, locator))
    .or_else(handle_trailing_end_of_line_body_comment)
    .or_else(|comment| handle_trailing_end_of_line_condition_comment(comment, locator))
    .or_else(|comment| {
        handle_module_level_own_line_comment_before_class_or_function_comment(comment, locator)
    })
    .or_else(|comment| handle_positional_only_arguments_separator_comment(comment, locator))
    .or_else(|comment| handle_trailing_binary_expression_left_or_operator_comment(comment, locator))
    .or_else(handle_leading_function_with_decorators_comment)
    .or_else(|comment| handle_dict_unpacking_comment(comment, locator))
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
        let match_stmt_indentation =
            whitespace::indentation(locator, match_stmt).map_or(usize::MAX, str::len);

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

/// Handles comments between except handlers and between the last except handler and any following `else` or `finally` block.
fn handle_in_between_except_handlers_or_except_handler_and_else_or_finally_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    if comment.text_position().is_end_of_line() || comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    if let Some(AnyNodeRef::ExceptHandlerExceptHandler(except_handler)) = comment.preceding_node() {
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

/// Handles own line comments between the last statement and the first statement of two bodies.
///
/// ```python
/// if x == y:
///     pass
///     # This should be a trailing comment of `pass` and not a leading comment of the `print`
///     # in the `else` branch
/// else:
///     print("I have no comments")
/// ```
fn handle_in_between_bodies_own_line_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    if !comment.text_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    // The comment must be between two statements...
    if let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    {
        // ...and the following statement must be the first statement in an alternate body of the parent...
        if !is_first_statement_in_enclosing_alternate_body(following, comment.enclosing_node()) {
            // ```python
            // if test:
            //     a
            //     # comment
            //     b
            // ```
            return CommentPlacement::Default(comment);
        }

        // If there's any non-trivia token between the preceding node and the comment, than it means that
        // we're past the case of the alternate branch, defer to the default rules
        // ```python
        // if a:
        //      pass
        //  else:
        //      # leading comment
        //      def inline_after_else(): ...
        // ```
        if SimpleTokenizer::new(
            locator.contents(),
            TextRange::new(preceding.end(), comment.slice().start()),
        )
        .skip_trivia()
        .next()
        .is_some()
        {
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

/// Handles end of line comments comments between the last statement and the first statement of two bodies.
///
/// ```python
/// if x == y:
///     pass # trailing comment of pass
/// else: # trailing comment of `else`
///     print("I have no comments")
/// ```
fn handle_in_between_bodies_end_of_line_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    if !comment.text_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    // The comment must be between two statements...
    if let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    {
        // ...and the following statement must be the first statement in an alternate body of the parent...
        if !is_first_statement_in_enclosing_alternate_body(following, comment.enclosing_node()) {
            // ```python
            // if test:
            //     a
            //     # comment
            //     b
            // ```
            return CommentPlacement::Default(comment);
        }

        if !locator.contains_line_break(TextRange::new(preceding.end(), comment.slice().start())) {
            // Trailing comment of the preceding statement
            // ```python
            // while test:
            //     a # comment
            // else:
            //     b
            // ```
            if preceding.is_node_with_body() {
                // We can't set this as a trailing comment of the function declaration because it
                // will then move behind the function block instead of sticking with the pass
                // ```python
                // if True:
                //     def f():
                //         pass  # a
                // else:
                //     pass
                // ```
                CommentPlacement::Default(comment)
            } else {
                CommentPlacement::trailing(preceding, comment)
            }
        } else if following.is_stmt_if() || following.is_except_handler() {
            // The `elif` or except handlers have their own body to which we can attach the trailing comment
            // ```python
            // if test:
            //     a
            // elif c: # comment
            //     b
            // ```
            CommentPlacement::trailing(following, comment)
        } else {
            // There are no bodies for the "else" branch and other bodies that are represented as a `Vec<Stmt>`.
            // This means, there's no good place to attach the comments to.
            // Make this a dangling comments and manually format the comment in
            // in the enclosing node's formatting logic. For `try`, it's the formatters responsibility
            // to correctly identify the comments for the `finally` and `orelse` block by looking
            // at the comment's range.
            //
            // ```python
            // while x == y:
            //     pass
            // else: # trailing
            //     print("nooop")
            // ```
            CommentPlacement::dangling(comment.enclosing_node(), comment)
        }
    } else {
        CommentPlacement::Default(comment)
    }
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
        let child_indentation =
            whitespace::indentation(locator, &current_child).map_or(usize::MAX, str::len);

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

/// Handles end of line comments of the last statement in an indented body:
///
/// ```python
/// while True:
///     if something.changed:
///         do.stuff()  # trailing comment
/// ```
fn handle_trailing_end_of_line_body_comment(comment: DecoratedComment<'_>) -> CommentPlacement<'_> {
    // Must be an end of line comment
    if comment.text_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    // Must be *after* a statement
    let Some(preceding) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    // Recursively get the last child of statements with a body.
    let last_children = std::iter::successors(last_child_in_body(preceding), |parent| {
        last_child_in_body(*parent)
    });

    if let Some(last_child) = last_children.last() {
        CommentPlacement::trailing(last_child, comment)
    } else {
        // End of line comment of a statement that has no body. This is not what we're looking for.
        // ```python
        // a # trailing comment
        // b
        //  ```
        CommentPlacement::Default(comment)
    }
}

/// Handles end of line comments after the `:` of a condition
///
/// ```python
/// while True: # comment
///     pass
/// ```
///
/// It attaches the comment as dangling comment to the enclosing `while` statement.
fn handle_trailing_end_of_line_condition_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    use ruff_python_ast::prelude::*;

    // Must be an end of line comment
    if comment.text_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    // Must be between the condition expression and the first body element
    let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node()) else {
        return CommentPlacement::Default(comment);
    };

    let expression_before_colon = match comment.enclosing_node() {
        AnyNodeRef::StmtIf(StmtIf { test: expr, .. })
        | AnyNodeRef::StmtWhile(StmtWhile { test: expr, .. })
        | AnyNodeRef::StmtFor(StmtFor { iter: expr, .. })
        | AnyNodeRef::StmtAsyncFor(StmtAsyncFor { iter: expr, .. }) => {
            Some(AnyNodeRef::from(expr.as_ref()))
        }

        AnyNodeRef::StmtWith(StmtWith { items, .. })
        | AnyNodeRef::StmtAsyncWith(StmtAsyncWith { items, .. }) => {
            items.last().map(AnyNodeRef::from)
        }
        AnyNodeRef::StmtFunctionDef(StmtFunctionDef { returns, args, .. })
        | AnyNodeRef::StmtAsyncFunctionDef(StmtAsyncFunctionDef { returns, args, .. }) => returns
            .as_deref()
            .map(AnyNodeRef::from)
            .or_else(|| Some(AnyNodeRef::from(args.as_ref()))),
        _ => None,
    };

    let Some(last_before_colon) = expression_before_colon else {
        return CommentPlacement::Default(comment);
    };

    // If the preceding is the node before the `colon`
    // `while true:` The node before the `colon` is the `true` constant.
    if preceding.ptr_eq(last_before_colon) {
        let tokens = SimpleTokenizer::new(
            locator.contents(),
            TextRange::new(preceding.end(), following.start()),
        )
        .skip_trivia();

        for token in tokens {
            match token.kind() {
                TokenKind::Colon => {
                    if comment.slice().start() > token.start() {
                        // Comment comes after the colon
                        // ```python
                        // while a: # comment
                        //      ...
                        // ```
                        return CommentPlacement::dangling(comment.enclosing_node(), comment);
                    }

                    // Comment comes before the colon
                    // ```python
                    // while (
                    //  a # comment
                    // ):
                    //      ...
                    // ```
                    break;
                }
                TokenKind::RParen => {
                    // Skip over any closing parentheses
                }
                _ => {
                    unreachable!("Only ')' or ':' should follow the condition")
                }
            }
        }
    }

    CommentPlacement::Default(comment)
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

    let is_last_positional_argument =
        are_same_optional(last_argument_or_default, arguments.posonlyargs.last());

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

/// Handles comments between the left side and the operator of a binary expression (trailing comments of the left),
/// and trailing end-of-line comments that are on the same line as the operator.
///
/// ```python
/// a = (
///     5 # trailing left comment
///     + # trailing operator comment
///     # leading right comment
///     3
/// )
/// ```
fn handle_trailing_binary_expression_left_or_operator_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let Some(binary_expression) = comment.enclosing_node().expr_bin_op() else {
        return CommentPlacement::Default(comment);
    };

    // Only if there's a preceding node (in which case, the preceding node is `left`).
    if comment.preceding_node().is_none() || comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    let between_operands_range = TextRange::new(
        binary_expression.left.end(),
        binary_expression.right.start(),
    );

    let mut tokens = SimpleTokenizer::new(locator.contents(), between_operands_range).skip_trivia();
    let operator_offset = if let Some(non_r_paren) = tokens.find(|t| t.kind() != TokenKind::RParen)
    {
        non_r_paren.start()
    } else {
        return CommentPlacement::Default(comment);
    };

    let comment_range = comment.slice().range();

    if comment_range.end() < operator_offset {
        // ```python
        // a = (
        //      5
        //      # comment
        //      +
        //      3
        // )
        // ```
        CommentPlacement::trailing(AnyNodeRef::from(binary_expression.left.as_ref()), comment)
    } else if comment.text_position().is_end_of_line() {
        // Is the operator on its own line.
        if locator.contains_line_break(TextRange::new(
            binary_expression.left.end(),
            operator_offset,
        )) && locator.contains_line_break(TextRange::new(
            operator_offset,
            binary_expression.right.start(),
        )) {
            // ```python
            // a = (
            //      5
            //      + # comment
            //      3
            // )
            // ```
            CommentPlacement::dangling(binary_expression.into(), comment)
        } else {
            // ```python
            // a = (
            //      5
            //      +
            //      3 # comment
            // )
            // ```
            // OR
            // ```python
            // a = (
            //      5 # comment
            //      +
            //      3
            // )
            // ```
            CommentPlacement::Default(comment)
        }
    } else {
        // ```python
        // a = (
        //      5
        //      +
        //      # comment
        //      3
        // )
        // ```
        CommentPlacement::Default(comment)
    }
}

/// Handles own line comments on the module level before a class or function statement.
/// A comment only becomes the leading comment of a class or function if it isn't separated by an empty
/// line from the class. Comments that are separated by at least one empty line from the header of the
/// class are considered trailing comments of the previous statement.
///
/// This handling is necessary because Ruff inserts two empty lines before each class or function.
/// Let's take this example:
///
/// ```python
/// some = statement
/// # This should be stick to the statement above
///
///
/// # This should be split from the above by two lines
/// class MyClassWithComplexLeadingComments:
///     pass
/// ```
///
/// By default, the `# This should be stick to the statement above` would become a leading comment
/// of the `class` AND the `Suite` formatting separates the comment by two empty lines from the
/// previous statement, so that the result becomes:
///
/// ```python
/// some = statement
///
///
/// # This should be stick to the statement above
///
///
/// # This should be split from the above by two lines
/// class MyClassWithComplexLeadingComments:
///     pass
/// ```
///
/// Which is not what we want. The work around is to make the `# This should be stick to the statement above`
/// a trailing comment of the previous statement.
fn handle_module_level_own_line_comment_before_class_or_function_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // Only applies for own line comments on the module level...
    if !comment.text_position().is_own_line() || !comment.enclosing_node().is_module() {
        return CommentPlacement::Default(comment);
    }

    // ... for comments with a preceding and following node,
    let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node()) else {
        return CommentPlacement::Default(comment)
    };

    // ... where the following is a function or class statement.
    if !matches!(
        following,
        AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
    ) {
        return CommentPlacement::Default(comment);
    }

    // Make the comment a leading comment if there's no empty line between the comment and the function / class header
    if max_empty_lines(locator.slice(TextRange::new(comment.slice().end(), following.start()))) == 0
    {
        CommentPlacement::leading(following, comment)
    } else {
        // Otherwise attach the comment as trailing comment to the previous statement
        CommentPlacement::trailing(preceding, comment)
    }
}

/// Finds the offset of the `/` that separates the positional only and arguments from the other arguments.
/// Returns `None` if the positional only separator `/` isn't present in the specified range.
fn find_pos_only_slash_offset(
    between_arguments_range: TextRange,
    locator: &Locator,
) -> Option<TextSize> {
    let mut tokens =
        SimpleTokenizer::new(locator.contents(), between_arguments_range).skip_trivia();

    if let Some(comma) = tokens.next() {
        debug_assert_eq!(comma.kind(), TokenKind::Comma);

        if let Some(maybe_slash) = tokens.next() {
            if maybe_slash.kind() == TokenKind::Slash {
                return Some(maybe_slash.start());
            }

            debug_assert_eq!(
                maybe_slash.kind(),
                TokenKind::RParen,
                "{:?}",
                maybe_slash.kind()
            );
        }
    }

    None
}

/// Handles own line comments between the last function decorator and the *header* of the function.
/// It attaches these comments as dangling comments to the function instead of making them
/// leading argument comments.
///
/// ```python
/// @decorator
/// # leading function comment
/// def test():
///      ...
/// ```
fn handle_leading_function_with_decorators_comment(comment: DecoratedComment) -> CommentPlacement {
    let is_preceding_decorator = comment
        .preceding_node()
        .map_or(false, |node| node.is_decorator());

    let is_following_arguments = comment
        .following_node()
        .map_or(false, |node| node.is_arguments());

    if comment.text_position().is_own_line() && is_preceding_decorator && is_following_arguments {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handles comments between `**` and the variable name in dict unpacking
/// It attaches these to the appropriate value node
///
/// ```python
/// {
///     **  # comment between `**` and the variable name
///     value
///     ...
/// }
/// ```
fn handle_dict_unpacking_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    match comment.enclosing_node() {
        // TODO: can maybe also add AnyNodeRef::Arguments here, but tricky to test due to
        // https://github.com/astral-sh/ruff/issues/5176
        AnyNodeRef::ExprDict(_) => {}
        _ => {
            return CommentPlacement::Default(comment);
        }
    };

    // no node after our comment so we can't be between `**` and the name (node)
    let Some(following) = comment.following_node() else {
        return CommentPlacement::Default(comment);
    };

    // we look at tokens between the previous node (or the start of the dict)
    // and the comment
    let preceding_end = match comment.preceding_node() {
        Some(preceding) => preceding.end(),
        None => comment.enclosing_node().start(),
    };
    if preceding_end > comment.slice().start() {
        return CommentPlacement::Default(comment);
    }
    let mut tokens = SimpleTokenizer::new(
        locator.contents(),
        TextRange::new(preceding_end, comment.slice().start()),
    )
    .skip_trivia();

    // we start from the preceding node but we skip its token
    if let Some(first) = tokens.next() {
        debug_assert!(matches!(
            first,
            Token {
                kind: TokenKind::LBrace | TokenKind::Comma | TokenKind::Colon,
                ..
            }
        ));
    }

    // if the remaining tokens from the previous node is exactly `**`,
    // re-assign the comment to the one that follows the stars
    let mut count = 0;
    for token in tokens {
        if token.kind != TokenKind::Star {
            return CommentPlacement::Default(comment);
        }
        count += 1;
    }
    if count == 2 {
        return CommentPlacement::trailing(following, comment);
    }

    CommentPlacement::Default(comment)
}

/// Returns `true` if `right` is `Some` and `left` and `right` are referentially equal.
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
        | AnyNodeRef::ExceptHandlerExceptHandler(ExceptHandlerExceptHandler { body, .. }) => body,

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

/// Returns `true` if `following` is the first statement in an alternate `body` (e.g. the else of an if statement) of the `enclosing` node.
fn is_first_statement_in_enclosing_alternate_body(
    following: AnyNodeRef,
    enclosing: AnyNodeRef,
) -> bool {
    use ruff_python_ast::prelude::*;

    match enclosing {
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
                // are already handled by `handle_in_between_except_handlers_or_except_handler_and_else_or_finally_comment`
                || handlers.is_empty() && are_same_optional(following, orelse.first())
                || (handlers.is_empty() || !orelse.is_empty())
                && are_same_optional(following, finalbody.first())
        }

        _ => false,
    }
}

/// Counts the number of newlines in `contents`.
fn max_empty_lines(contents: &str) -> usize {
    let mut empty_lines = 0;
    let mut max_empty_lines = 0;

    for line in contents.universal_newlines().skip(1) {
        if line.trim_whitespace().is_empty() {
            empty_lines += 1;
        } else {
            max_empty_lines = max_empty_lines.max(empty_lines);
            empty_lines = 0;
        }
    }

    max_empty_lines
}

#[cfg(test)]
mod tests {
    use crate::comments::placement::max_empty_lines;

    #[test]
    fn count_empty_lines_in_trivia() {
        assert_eq!(max_empty_lines(""), 0);
        assert_eq!(max_empty_lines("# trailing comment\n # other comment\n"), 0);
        assert_eq!(
            max_empty_lines("# trailing comment\n# own line comment\n"),
            0
        );
        assert_eq!(
            max_empty_lines("# trailing comment\n\n# own line comment\n"),
            1
        );

        assert_eq!(
            max_empty_lines(
                "# trailing comment\n\n# own line comment\n\n# an other own line comment"
            ),
            1
        );

        assert_eq!(
            max_empty_lines(
                "# trailing comment\n\n# own line comment\n\n# an other own line comment\n# block"
            ),
            1
        );

        assert_eq!(
            max_empty_lines(
                "# trailing comment\n\n# own line comment\n\n\n# an other own line comment\n# block"
            ),
            2
        );

        assert_eq!(
            max_empty_lines(
                r#"# This multiline comments section
# should be split from the statement
# above by two lines.
"#
            ),
            0
        );
    }
}

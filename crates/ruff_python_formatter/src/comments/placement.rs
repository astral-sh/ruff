use std::cmp::Ordering;

use ruff_text_size::TextRange;
use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, ExprSlice, Ranged};

use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::whitespace;
use ruff_python_whitespace::{PythonWhitespace, UniversalNewlines};

use crate::comments::visitor::{CommentPlacement, DecoratedComment};
use crate::expression::expr_slice::{assign_comment_in_slice, ExprSliceCommentSection};
use crate::other::arguments::{
    assign_argument_separator_comment_placement, find_argument_separators,
};
use crate::trivia::{first_non_trivia_token_rev, SimpleTokenizer, Token, TokenKind};

/// Implements the custom comment placement logic.
pub(super) fn place_comment<'a>(
    mut comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    static HANDLERS: &[for<'a> fn(DecoratedComment<'a>, &Locator) -> CommentPlacement<'a>] = &[
        handle_in_between_except_handlers_or_except_handler_and_else_or_finally_comment,
        handle_match_comment,
        handle_in_between_bodies_own_line_comment,
        handle_in_between_bodies_end_of_line_comment,
        handle_trailing_body_comment,
        handle_trailing_end_of_line_body_comment,
        handle_trailing_end_of_line_condition_comment,
        handle_trailing_end_of_line_except_comment,
        handle_module_level_own_line_comment_before_class_or_function_comment,
        handle_arguments_separator_comment,
        handle_trailing_binary_expression_left_or_operator_comment,
        handle_leading_function_with_decorators_comment,
        handle_dict_unpacking_comment,
        handle_slice_comments,
        handle_attribute_comment,
    ];
    for handler in HANDLERS {
        comment = match handler(comment, locator) {
            CommentPlacement::Default(comment) => comment,
            placement => return placement,
        };
    }
    CommentPlacement::Default(comment)
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
    if comment.line_position().is_end_of_line() || comment.following_node().is_some() {
        return CommentPlacement::Default(comment);
    }

    // Get the enclosing match case
    let Some(match_case) = comment.enclosing_node().match_case() else {
        return CommentPlacement::Default(comment);
    };

    // And its parent match statement.
    let Some(match_stmt) = comment.enclosing_parent().and_then(AnyNodeRef::stmt_match) else {
        return CommentPlacement::Default(comment);
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
    if comment.line_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    let (Some(AnyNodeRef::ExceptHandlerExceptHandler(preceding_except_handler)), Some(following)) =
        (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

    // it now depends on the indentation level of the comment if it is a leading comment for e.g.
    // the following `finally` or indeed a trailing comment of the previous body's last statement.
    let comment_indentation =
        whitespace::indentation_at_offset(locator, comment.slice().range().start())
            .map(str::len)
            .unwrap_or_default();

    let Some(except_indentation) =
        whitespace::indentation(locator, preceding_except_handler).map(str::len)
    else {
        return CommentPlacement::Default(comment);
    };

    if comment_indentation > except_indentation {
        // Delegate to `handle_trailing_body_comment`
        return CommentPlacement::Default(comment);
    }

    // It has equal, or less indent than the `except` handler. It must be a comment of a subsequent
    // except handler or of the following `finally` or `else` block
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

    if following.is_except_handler() {
        // Attach it to the following except handler (which has a node) as leading
        CommentPlacement::leading(following, comment)
    } else {
        // No following except handler; attach it to the `try` statement.as dangling
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    }
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
    if !comment.line_position().is_own_line() {
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
    if !comment.line_position().is_end_of_line() {
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

        if locator.contains_line_break(TextRange::new(preceding.end(), comment.slice().start())) {
            // The `elif` or except handlers have their own body to which we can attach the trailing comment
            // ```python
            // if test:
            //     a
            // elif c: # comment
            //     b
            // ```
            if following.is_except_handler() {
                return CommentPlacement::trailing(following, comment);
            } else if following.is_stmt_if() {
                // We have to exclude for following if statements that are not elif by checking the
                // indentation
                // ```python
                // if True:
                //     pass
                // else:  # Comment
                //     if False:
                //         pass
                //     pass
                // ```
                let base_if_indent =
                    whitespace::indentation_at_offset(locator, following.range().start());
                let maybe_elif_indent = whitespace::indentation_at_offset(
                    locator,
                    comment.enclosing_node().range().start(),
                );
                if base_if_indent == maybe_elif_indent {
                    return CommentPlacement::trailing(following, comment);
                }
            }
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
        } else {
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
    if comment.line_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    // Only do something if the preceding node has a body (has indented statements).
    let Some(preceding_node) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    let Some(last_child) = last_child_in_body(preceding_node) else {
        return CommentPlacement::Default(comment);
    };

    let Some(comment_indentation) =
        whitespace::indentation_at_offset(locator, comment.slice().range().start())
    else {
        // The comment can't be a comment for the previous block if it isn't indented..
        return CommentPlacement::Default(comment);
    };

    // We only care about the length because indentations with mixed spaces and tabs are only valid if
    // the indent-level doesn't depend on the tab width (the indent level must be the same if the tab width is 1 or 8).
    let comment_indentation_len = comment_indentation.len();

    // Keep the comment on the entire statement in case it's a trailing comment
    // ```python
    // if "first if":
    //     pass
    // elif "first elif":
    //     pass
    // # Trailing if comment
    // ```
    // Here we keep the comment a trailing comment of the `if`
    let Some(preceding_node_indentation) =
        whitespace::indentation_at_offset(locator, preceding_node.start())
    else {
        return CommentPlacement::Default(comment);
    };
    if comment_indentation_len == preceding_node_indentation.len() {
        return CommentPlacement::Default(comment);
    }

    let mut current_child = last_child;
    let mut parent_body = Some(preceding_node);
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
fn handle_trailing_end_of_line_body_comment<'a>(
    comment: DecoratedComment<'a>,
    _locator: &Locator,
) -> CommentPlacement<'a> {
    // Must be an end of line comment
    if comment.line_position().is_own_line() {
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
    // Must be an end of line comment
    if comment.line_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    // Must be between the condition expression and the first body element
    let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

    let expression_before_colon = match comment.enclosing_node() {
        AnyNodeRef::StmtIf(ast::StmtIf { test: expr, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { test: expr, .. })
        | AnyNodeRef::StmtFor(ast::StmtFor { iter: expr, .. })
        | AnyNodeRef::StmtAsyncFor(ast::StmtAsyncFor { iter: expr, .. }) => {
            Some(AnyNodeRef::from(expr.as_ref()))
        }

        AnyNodeRef::StmtWith(ast::StmtWith { items, .. })
        | AnyNodeRef::StmtAsyncWith(ast::StmtAsyncWith { items, .. }) => {
            items.last().map(AnyNodeRef::from)
        }
        AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { returns, args, .. })
        | AnyNodeRef::StmtAsyncFunctionDef(ast::StmtAsyncFunctionDef { returns, args, .. }) => {
            returns
                .as_deref()
                .map(AnyNodeRef::from)
                .or_else(|| Some(AnyNodeRef::from(args.as_ref())))
        }
        AnyNodeRef::StmtClassDef(ast::StmtClassDef {
            bases, keywords, ..
        }) => keywords
            .last()
            .map(AnyNodeRef::from)
            .or_else(|| bases.last().map(AnyNodeRef::from)),
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
                TokenKind::Comma => {
                    // Skip over any trailing comma
                }
                kind => {
                    unreachable!(
                        "Only ')' or ':' should follow the condition but encountered {kind:?}"
                    )
                }
            }
        }
    }

    CommentPlacement::Default(comment)
}

/// Handles end of line comments after the `:` of an except clause
///
/// ```python
/// try:
///    ...
/// except: # comment
///     pass
/// ```
///
/// It attaches the comment as dangling comment to the enclosing except handler.
fn handle_trailing_end_of_line_except_comment<'a>(
    comment: DecoratedComment<'a>,
    _locator: &Locator,
) -> CommentPlacement<'a> {
    let AnyNodeRef::ExceptHandlerExceptHandler(handler) = comment.enclosing_node() else {
        return CommentPlacement::Default(comment);
    };

    // Must be an end of line comment
    if comment.line_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    let Some(first_body_statement) = handler.body.first() else {
        return CommentPlacement::Default(comment);
    };

    if comment.slice().start() < first_body_statement.range().start() {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Attaches comments for the positional only arguments separator `/` or the keywords only arguments
/// separator `*` as dangling comments to the enclosing [`Arguments`] node.
///
/// See [`assign_argument_separator_comment_placement`]
fn handle_arguments_separator_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let AnyNodeRef::Arguments(arguments) = comment.enclosing_node() else {
        return CommentPlacement::Default(comment);
    };

    let (slash, star) = find_argument_separators(locator.contents(), arguments);
    let comment_range = comment.slice().range();
    let placement = assign_argument_separator_comment_placement(
        slash.as_ref(),
        star.as_ref(),
        comment_range,
        comment.line_position(),
    );
    if placement.is_some() {
        return CommentPlacement::dangling(comment.enclosing_node(), comment);
    }

    CommentPlacement::Default(comment)
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
    } else if comment.line_position().is_end_of_line() {
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
    if !comment.line_position().is_own_line() || !comment.enclosing_node().is_module() {
        return CommentPlacement::Default(comment);
    }

    // ... for comments with a preceding and following node,
    let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
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

/// Handles the attaching comments left or right of the colon in a slice as trailing comment of the
/// preceding node or leading comment of the following node respectively.
/// ```python
/// a = "input"[
///     1 # c
///     # d
///     :2
/// ]
/// ```
fn handle_slice_comments<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let expr_slice = match comment.enclosing_node() {
        AnyNodeRef::ExprSlice(expr_slice) => expr_slice,
        AnyNodeRef::ExprSubscript(expr_subscript) => {
            if expr_subscript.value.end() < expr_subscript.slice.start() {
                if let Expr::Slice(expr_slice) = expr_subscript.slice.as_ref() {
                    expr_slice
                } else {
                    return CommentPlacement::Default(comment);
                }
            } else {
                return CommentPlacement::Default(comment);
            }
        }
        _ => return CommentPlacement::Default(comment),
    };

    let ExprSlice {
        range: _,
        lower,
        upper,
        step,
    } = expr_slice;

    // Check for `foo[ # comment`, but only if they are on the same line
    let after_lbracket = matches!(
        first_non_trivia_token_rev(comment.slice().start(), locator.contents()),
        Some(Token {
            kind: TokenKind::LBracket,
            ..
        })
    );
    if comment.line_position().is_end_of_line() && after_lbracket {
        // Keep comments after the opening bracket there by formatting them outside the
        // soft block indent
        // ```python
        // "a"[ # comment
        //     1:
        // ]
        // ```
        debug_assert!(
            matches!(comment.enclosing_node(), AnyNodeRef::ExprSubscript(_)),
            "{:?}",
            comment.enclosing_node()
        );
        return CommentPlacement::dangling(comment.enclosing_node(), comment);
    }

    let assignment =
        assign_comment_in_slice(comment.slice().range(), locator.contents(), expr_slice);
    let node = match assignment {
        ExprSliceCommentSection::Lower => lower,
        ExprSliceCommentSection::Upper => upper,
        ExprSliceCommentSection::Step => step,
    };

    if let Some(node) = node {
        if comment.slice().start() < node.start() {
            CommentPlacement::leading(node.as_ref().into(), comment)
        } else {
            // If a trailing comment is an end of line comment that's fine because we have a node
            // ahead of it
            CommentPlacement::trailing(node.as_ref().into(), comment)
        }
    } else {
        CommentPlacement::dangling(expr_slice.as_any_node_ref(), comment)
    }
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
fn handle_leading_function_with_decorators_comment<'a>(
    comment: DecoratedComment<'a>,
    _locator: &Locator,
) -> CommentPlacement<'a> {
    let is_preceding_decorator = comment
        .preceding_node()
        .map_or(false, |node| node.is_decorator());

    let is_following_arguments = comment
        .following_node()
        .map_or(false, |node| node.is_arguments());

    if comment.line_position().is_own_line() && is_preceding_decorator && is_following_arguments {
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
        AnyNodeRef::ExprDict(_) | AnyNodeRef::Keyword(_) => {}
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

    // if the remaining tokens from the previous node are exactly `**`,
    // re-assign the comment to the one that follows the stars
    let mut count = 0;

    // we start from the preceding node but we skip its token
    for token in tokens.by_ref() {
        // Skip closing parentheses that are not part of the node range
        if token.kind == TokenKind::RParen {
            continue;
        }
        // The Keyword case
        if token.kind == TokenKind::Star {
            count += 1;
            break;
        }
        // The dict case
        debug_assert!(
            matches!(
                token,
                Token {
                    kind: TokenKind::LBrace | TokenKind::Comma | TokenKind::Colon,
                    ..
                }
            ),
            "{token:?}",
        );
        break;
    }

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

/// Own line comments coming after the node are always dangling comments
/// ```python
/// (
///      a
///      # trailing a comment
///      . # dangling comment
///      # or this
///      b
/// )
/// ```
fn handle_attribute_comment<'a>(
    comment: DecoratedComment<'a>,
    _locator: &Locator,
) -> CommentPlacement<'a> {
    let Some(attribute) = comment.enclosing_node().expr_attribute() else {
        return CommentPlacement::Default(comment);
    };

    // It must be a comment AFTER the name
    if comment.preceding_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    if TextRange::new(attribute.value.end(), attribute.attr.start())
        .contains(comment.slice().start())
    {
        // ```text
        // value   .   attr
        //      ^^^^^^^ the range of dangling comments
        // ```
        if comment.line_position().is_end_of_line() {
            // Attach to node with b
            // ```python
            // x322 = (
            //     a
            //     . # end-of-line dot comment 2
            //     b
            // )
            // ```
            CommentPlacement::trailing(comment.enclosing_node(), comment)
        } else {
            CommentPlacement::dangling(attribute.into(), comment)
        }
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Returns `true` if `right` is `Some` and `left` and `right` are referentially equal.
fn are_same_optional<'a, T>(left: AnyNodeRef, right: Option<T>) -> bool
where
    T: Into<AnyNodeRef<'a>>,
{
    right.map_or(false, |right| left.ptr_eq(right.into()))
}

fn last_child_in_body(node: AnyNodeRef) -> Option<AnyNodeRef> {
    let body = match node {
        AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
        | AnyNodeRef::StmtAsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
        | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. })
        | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
        | AnyNodeRef::StmtAsyncWith(ast::StmtAsyncWith { body, .. })
        | AnyNodeRef::MatchCase(ast::MatchCase { body, .. })
        | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
            body, ..
        }) => body,

        AnyNodeRef::StmtIf(ast::StmtIf { body, orelse, .. })
        | AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
        | AnyNodeRef::StmtAsyncFor(ast::StmtAsyncFor { body, orelse, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
            if orelse.is_empty() {
                body
            } else {
                orelse
            }
        }

        AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
            return cases.last().map(AnyNodeRef::from)
        }

        AnyNodeRef::StmtTry(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        })
        | AnyNodeRef::StmtTryStar(ast::StmtTryStar {
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
    match enclosing {
        AnyNodeRef::StmtIf(ast::StmtIf { orelse, .. })
        | AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
        | AnyNodeRef::StmtAsyncFor(ast::StmtAsyncFor { orelse, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { orelse, .. }) => {
            are_same_optional(following, orelse.first())
        }

        AnyNodeRef::StmtTry(ast::StmtTry {
            handlers,
            orelse,
            finalbody,
            ..
        })
        | AnyNodeRef::StmtTryStar(ast::StmtTryStar {
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

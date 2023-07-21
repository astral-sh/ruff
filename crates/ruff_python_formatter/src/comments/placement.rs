use std::cmp::Ordering;

use ruff_text_size::TextRange;
use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, ExprIfExp, ExprSlice, Ranged};

use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::whitespace;
use ruff_python_trivia::{
    PythonWhitespace, SimpleToken, SimpleTokenKind, SimpleTokenizer, UniversalNewlines,
};

use crate::comments::visitor::{CommentPlacement, DecoratedComment};
use crate::expression::expr_slice::{assign_comment_in_slice, ExprSliceCommentSection};
use crate::other::arguments::{
    assign_argument_separator_comment_placement, find_argument_separators,
};

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
        handle_expr_if_comment,
        handle_comprehension_comment,
        handle_trailing_expression_starred_star_end_of_line_comment,
        handle_with_item_comment,
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
                // elif True:
                //     print("nooop")
                // ```
                if following.is_except_handler() {
                    // The except handlers have their own body to which we can attach the leading comment
                    CommentPlacement::leading(following, comment)
                } else if let AnyNodeRef::StmtIf(stmt_if) = comment.enclosing_node() {
                    if let Some(clause) = stmt_if
                        .elif_else_clauses
                        .iter()
                        .find(|clause| are_same_optional(following, clause.test.as_ref()))
                    {
                        CommentPlacement::leading(clause.into(), comment)
                    } else {
                        // Since we know we're between bodies and we know that the following node is
                        // not the condition of any `elif`, we know the next node must be the `else`
                        let else_clause = stmt_if.elif_else_clauses.last().unwrap();
                        debug_assert!(else_clause.test.is_none());
                        CommentPlacement::leading(else_clause.into(), comment)
                    }
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
    let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

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
        // The  except handlers have their own body to which we can attach the trailing comment
        // ```python
        // try:
        //     f()  # comment
        // except RuntimeError:
        //     raise
        // ```
        if following.is_except_handler() {
            return CommentPlacement::trailing(following, comment);
        }

        // Handle the `else` of an `if`. It is special because we don't have a test but unlike other
        // `else` (e.g. for `while`), we have a dedicated node.
        // ```python
        // if x == y:
        //     pass
        // elif x < y:
        //     pass
        // else:  # 12 trailing else condition
        //     pass
        // ```
        if let AnyNodeRef::StmtIf(stmt_if) = comment.enclosing_node() {
            if let Some(else_clause) = stmt_if.elif_else_clauses.last() {
                if else_clause.test.is_none()
                    && following.ptr_eq(else_clause.body.first().unwrap().into())
                {
                    return CommentPlacement::dangling(else_clause.into(), comment);
                }
            }
        }

        // There are no bodies for the "else" branch (only `Vec<Stmt>`) expect for StmtIf, so
        // we make this a dangling comments of the node containing the alternate branch and
        // manually format the comment in that node's formatting logic. For `try`, it's the
        // formatters responsibility to correctly identify the comments for the `finally` and
        // `orelse` block by looking at the comment's range.
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
}

/// Without the `StmtIf` special, this function would just be the following:
/// ```ignore
/// if let Some(preceding_node) = comment.preceding_node() {
///     Some((preceding_node, last_child_in_body(preceding_node)?))
/// } else {
///     None
/// }
/// ```
/// We handle two special cases here:
/// ```python
/// if True:
///     pass
///     # Comment between if and elif/else clause, needs to be manually attached to the `StmtIf`
/// else:
///     pass
///     # Comment after the `StmtIf`, needs to be manually attached to the ElifElseClause
/// ```
/// The problem is that `StmtIf` spans the whole range (there is no "inner if" node), so the first
/// comment doesn't see it as preceding node, and the second comment takes the entire `StmtIf` when
/// it should only take the `ElifElseClause`
fn find_preceding_and_handle_stmt_if_special_cases<'a>(
    comment: &DecoratedComment<'a>,
) -> Option<(AnyNodeRef<'a>, AnyNodeRef<'a>)> {
    if let (stmt_if @ AnyNodeRef::StmtIf(stmt_if_inner), Some(AnyNodeRef::ElifElseClause(..))) =
        (comment.enclosing_node(), comment.following_node())
    {
        if let Some(preceding_node @ AnyNodeRef::ElifElseClause(..)) = comment.preceding_node() {
            // We're already after and elif or else, defaults work
            Some((preceding_node, last_child_in_body(preceding_node)?))
        } else {
            // Special case 1: The comment is between if body and an elif/else clause. We have
            // to handle this separately since StmtIf spans the entire range, so it's not the
            // preceding node
            Some((
                stmt_if,
                AnyNodeRef::from(stmt_if_inner.body.last().unwrap()),
            ))
        }
    } else if let Some(preceding_node @ AnyNodeRef::StmtIf(stmt_if_inner)) =
        comment.preceding_node()
    {
        if let Some(clause) = stmt_if_inner.elif_else_clauses.last() {
            // Special case 2: We're after an if statement and need to narrow the preceding
            // down to the elif/else clause
            Some((clause.into(), last_child_in_body(clause.into())?))
        } else {
            // After an if without any elif/else, defaults work
            Some((preceding_node, last_child_in_body(preceding_node)?))
        }
    } else if let Some(preceding_node) = comment.preceding_node() {
        // The normal case
        Some((preceding_node, last_child_in_body(preceding_node)?))
    } else {
        // Only do something if the preceding node has a body (has indented statements).
        None
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

    let Some((preceding_node, last_child)) =
        find_preceding_and_handle_stmt_if_special_cases(&comment)
    else {
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

    // Handle the StmtIf special case
    // ```python
    // if True:
    //     pass
    // elif True:
    //     pass # 14 end-of-line trailing `pass` comment, set preceding to the ElifElseClause
    // ```
    let preceding = if let AnyNodeRef::StmtIf(stmt_if) = preceding {
        stmt_if
            .elif_else_clauses
            .last()
            .map_or(preceding, AnyNodeRef::from)
    } else {
        preceding
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

    // We handle trailing else comments separately because we the preceding node is None for their
    // case
    // ```python
    // if True:
    //     pass
    // else: # 12 trailing else condition
    //     pass
    // ```
    if let AnyNodeRef::ElifElseClause(ast::ElifElseClause {
        body, test: None, ..
    }) = comment.enclosing_node()
    {
        if comment.start() < body.first().unwrap().start() {
            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }
    }

    // Must be between the condition expression and the first body element
    let (Some(preceding), Some(following)) = (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

    let enclosing_node = comment.enclosing_node();
    let expression_before_colon = match enclosing_node {
        AnyNodeRef::ElifElseClause(ast::ElifElseClause {
            test: Some(expr), ..
        }) => Some(AnyNodeRef::from(expr)),
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
    if !preceding.ptr_eq(last_before_colon) {
        return CommentPlacement::Default(comment);
    }
    let mut colon_token = SimpleTokenizer::new(
        locator.contents(),
        TextRange::new(preceding.end(), following.start()),
    )
    .skip_trivia()
    // Skip over any closing parentheses and any trailing comma
    .skip_while(|token| {
        token.kind == SimpleTokenKind::RParen || token.kind == SimpleTokenKind::Comma
    });

    match colon_token.next() {
        Some(token) if token.kind == SimpleTokenKind::Colon => {
            if comment.slice().start() > token.start() {
                // Comment comes after the colon
                // ```python
                // while a: # comment
                //      ...
                // ```
                return CommentPlacement::dangling(enclosing_node, comment);
            }

            // Comment comes before the colon
            // ```python
            // while (
            //  a # comment
            // ):
            //      ...
            // ```
            return CommentPlacement::Default(comment);
        }
        Some(token) => {
            unreachable!(
                "Only ')' or ':' should follow the condition but encountered {:?}",
                token.kind
            )
        }
        None => {
            unreachable!("Expected trailing condition comment to be preceded by a token",)
        }
    }
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
    let operator_offset =
        if let Some(non_r_paren) = tokens.find(|t| t.kind() != SimpleTokenKind::RParen) {
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
        SimpleTokenizer::up_to_without_back_comment(comment.slice().start(), locator.contents())
            .skip_trivia()
            .next_back(),
        Some(SimpleToken {
            kind: SimpleTokenKind::LBracket,
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
    .skip_trivia()
    .skip_while(|token| token.kind == SimpleTokenKind::RParen);

    // if the remaining tokens from the previous node are exactly `**`,
    // re-assign the comment to the one that follows the stars
    let mut count = 0;

    // we start from the preceding node but we skip its token
    if let Some(token) = tokens.next() {
        // The Keyword case
        if token.kind == SimpleTokenKind::Star {
            count += 1;
        } else {
            // The dict case
            debug_assert!(
                matches!(
                    token,
                    SimpleToken {
                        kind: SimpleTokenKind::LBrace
                            | SimpleTokenKind::Comma
                            | SimpleTokenKind::Colon,
                        ..
                    }
                ),
                "{token:?}",
            );
        }
    }

    for token in tokens {
        if token.kind != SimpleTokenKind::Star {
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

/// Assign comments between `if` and `test` and `else` and `orelse` as leading to the respective
/// node.
///
/// ```python
/// x = (
///     "a"
///     if # leading comment of `True`
///     True
///     else # leading comment of `"b"`
///     "b"
/// )
/// ```
///
/// This placement ensures comments remain in their previous order. This an edge case that only
/// happens if the comments are in a weird position but it also doesn't hurt handling it.
fn handle_expr_if_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let Some(expr_if) = comment.enclosing_node().expr_if_exp() else {
        return CommentPlacement::Default(comment);
    };
    let ExprIfExp {
        range: _,
        test,
        body,
        orelse,
    } = expr_if;

    if comment.line_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    // Find the if and the else
    let if_token = find_only_token_in_range(
        TextRange::new(body.end(), test.start()),
        locator,
        SimpleTokenKind::If,
    );
    let else_token = find_only_token_in_range(
        TextRange::new(test.end(), orelse.start()),
        locator,
        SimpleTokenKind::Else,
    );

    // Between `if` and `test`
    if if_token.range.start() < comment.slice().start() && comment.slice().start() < test.start() {
        return CommentPlacement::leading(test.as_ref().into(), comment);
    }

    // Between `else` and `orelse`
    if else_token.range.start() < comment.slice().start()
        && comment.slice().start() < orelse.start()
    {
        return CommentPlacement::leading(orelse.as_ref().into(), comment);
    }

    CommentPlacement::Default(comment)
}

fn handle_trailing_expression_starred_star_end_of_line_comment<'a>(
    comment: DecoratedComment<'a>,
    _locator: &Locator,
) -> CommentPlacement<'a> {
    if comment.line_position().is_own_line() || comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    let AnyNodeRef::ExprStarred(starred) = comment.enclosing_node() else {
        return CommentPlacement::Default(comment);
    };

    CommentPlacement::leading(starred.as_any_node_ref(), comment)
}

/// Handles trailing own line comments before the `as` keyword of a with item and
/// end of line comments that are on the same line as the `as` keyword:
///
/// ```python
/// with (
///     a
///     # trailing a own line comment
///     as # trailing as same line comment
///     b
// ): ...
/// ```
fn handle_with_item_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    if !comment.enclosing_node().is_with_item() {
        return CommentPlacement::Default(comment);
    }

    // Needs to be a with item with an `as` expression.
    let (Some(context_expr), Some(optional_vars)) =
        (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

    let as_token = find_only_token_in_range(
        TextRange::new(context_expr.end(), optional_vars.start()),
        locator,
        SimpleTokenKind::As,
    );

    // If before the `as` keyword, then it must be a trailing comment of the context expression.
    if comment.end() < as_token.start() {
        CommentPlacement::trailing(context_expr, comment)
    }
    // Trailing end of line comment coming after the `as` keyword`.
    else if comment.line_position().is_end_of_line() {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Looks for a token in the range that contains no other tokens except for parentheses outside
/// the expression ranges
fn find_only_token_in_range(
    range: TextRange,
    locator: &Locator,
    token_kind: SimpleTokenKind,
) -> SimpleToken {
    let mut tokens = SimpleTokenizer::new(locator.contents(), range)
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let token = tokens.next().expect("Expected a token");
    debug_assert_eq!(token.kind(), token_kind);
    let mut tokens = tokens.skip_while(|token| token.kind == SimpleTokenKind::LParen);
    debug_assert_eq!(tokens.next(), None);
    token
}

// Handle comments inside comprehensions, e.g.
//
// ```python
// [
//      a
//      for  # dangling on the comprehension
//      b
//      # dangling on the comprehension
//      in  # dangling on comprehension.iter
//      # leading on the iter
//      c
//      # dangling on comprehension.if.n
//      if  # dangling on comprehension.if.n
//      d
// ]
// ```
fn handle_comprehension_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let AnyNodeRef::Comprehension(comprehension) = comment.enclosing_node() else {
        return CommentPlacement::Default(comment);
    };
    let is_own_line = comment.line_position().is_own_line();

    // Comments between the `for` and target
    // ```python
    // [
    //      a
    //      for  # attache as dangling on the comprehension
    //      b in c
    //  ]
    // ```
    if comment.slice().end() < comprehension.target.range().start() {
        return if is_own_line {
            // own line comments are correctly assigned as leading the target
            CommentPlacement::Default(comment)
        } else {
            // after the `for`
            CommentPlacement::dangling(comment.enclosing_node(), comment)
        };
    }

    let in_token = find_only_token_in_range(
        TextRange::new(
            comprehension.target.range().end(),
            comprehension.iter.range().start(),
        ),
        locator,
        SimpleTokenKind::In,
    );

    // Comments between the target and the `in`
    // ```python
    // [
    //      a for b
    //      # attach as dangling on the target
    //      # (to be rendered as leading on the "in")
    //      in c
    //  ]
    // ```
    if comment.slice().start() < in_token.start() {
        // attach as dangling comments on the target
        // (to be rendered as leading on the "in")
        return if is_own_line {
            CommentPlacement::dangling(comment.enclosing_node(), comment)
        } else {
            // correctly trailing on the target
            CommentPlacement::Default(comment)
        };
    }

    // Comments between the `in` and the iter
    // ```python
    // [
    //      a for b
    //      in  #  attach as dangling on the iter
    //      c
    //  ]
    // ```
    if comment.slice().start() < comprehension.iter.range().start() {
        return if is_own_line {
            CommentPlacement::Default(comment)
        } else {
            // after the `in` but same line, turn into trailing on the `in` token
            CommentPlacement::dangling((&comprehension.iter).into(), comment)
        };
    }

    let mut last_end = comprehension.iter.range().end();

    for if_node in &comprehension.ifs {
        // ```python
        // [
        //     a
        //     for
        //     c
        //     in
        //     e
        //     # above if   <-- find these own-line between previous and `if` token
        //     if  # if     <-- find these end-of-line between `if` and if node (`f`)
        //     # above f    <-- already correctly assigned as leading `f`
        //     f  # f       <-- already correctly assigned as trailing `f`
        //     # above if2
        //     if  # if2
        //     # above g
        //     g  # g
        // ]
        // ```
        let if_token = find_only_token_in_range(
            TextRange::new(last_end, if_node.range().start()),
            locator,
            SimpleTokenKind::If,
        );
        if is_own_line {
            if last_end < comment.slice().start() && comment.slice().start() < if_token.start() {
                return CommentPlacement::dangling((if_node).into(), comment);
            }
        } else {
            if if_token.start() < comment.slice().start()
                && comment.slice().start() < if_node.range().start()
            {
                return CommentPlacement::dangling((if_node).into(), comment);
            }
        }
        last_end = if_node.range().end();
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
    let body = match node {
        AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
        | AnyNodeRef::StmtAsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
        | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. })
        | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
        | AnyNodeRef::StmtAsyncWith(ast::StmtAsyncWith { body, .. })
        | AnyNodeRef::MatchCase(ast::MatchCase { body, .. })
        | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
            body, ..
        })
        | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. }) => body,
        AnyNodeRef::StmtIf(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => elif_else_clauses.last().map_or(body, |clause| &clause.body),

        AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
        | AnyNodeRef::StmtAsyncFor(ast::StmtAsyncFor { body, orelse, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
            if orelse.is_empty() {
                body
            } else {
                orelse
            }
        }

        AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
            return cases.last().map(AnyNodeRef::from);
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
        AnyNodeRef::StmtIf(ast::StmtIf {
            elif_else_clauses, ..
        }) => {
            for clause in elif_else_clauses {
                if let Some(test) = &clause.test {
                    // `elif`, the following node is the test
                    if following.ptr_eq(test.into()) {
                        return true;
                    }
                } else {
                    // `else`, there is no test and the following node is the first entry in the
                    // body
                    if following.ptr_eq(clause.body.first().unwrap().into()) {
                        return true;
                    }
                }
            }
            false
        }
        AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
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

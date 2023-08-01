use std::cmp::Ordering;

use ruff_python_ast::{
    self as ast, Arguments, Comprehension, Expr, ExprAttribute, ExprBinOp, ExprIfExp, ExprSlice,
    ExprStarred, MatchCase, Ranged,
};
use ruff_text_size::TextRange;

use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::whitespace::indentation;
use ruff_python_trivia::{
    indentation_at_offset, PythonWhitespace, SimpleToken, SimpleTokenKind, SimpleTokenizer,
};
use ruff_source_file::{Locator, UniversalNewlines};

use crate::comments::visitor::{CommentPlacement, DecoratedComment};
use crate::expression::expr_slice::{assign_comment_in_slice, ExprSliceCommentSection};
use crate::other::arguments::{
    assign_argument_separator_comment_placement, find_argument_separators,
};

/// Manually attach comments to nodes that the default placement gets wrong.
pub(super) fn place_comment<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // Handle comments before and after bodies such as the different branches of an if statement
    let comment = if comment.line_position().is_own_line() {
        match handle_own_line_comment_after_body(comment, locator) {
            CommentPlacement::Default(comment) => comment,
            placement => {
                return placement;
            }
        }
    } else {
        match handle_end_of_line_comment_around_body(comment, locator) {
            CommentPlacement::Default(comment) => comment,
            placement => {
                return placement;
            }
        }
    };

    // Change comment placement depending on the node type. These can be seen as node-specific
    // fixups.
    match comment.enclosing_node() {
        AnyNodeRef::Arguments(arguments) => {
            handle_arguments_separator_comment(comment, arguments, locator)
        }
        AnyNodeRef::Comprehension(comprehension) => {
            handle_comprehension_comment(comment, comprehension, locator)
        }
        AnyNodeRef::ExprAttribute(attribute) => handle_attribute_comment(comment, attribute),
        AnyNodeRef::ExprBinOp(binary_expression) => {
            handle_trailing_binary_expression_left_or_operator_comment(
                comment,
                binary_expression,
                locator,
            )
        }
        AnyNodeRef::ExprDict(_) | AnyNodeRef::Keyword(_) => {
            handle_dict_unpacking_comment(comment, locator)
        }
        AnyNodeRef::ExprIfExp(expr_if) => handle_expr_if_comment(comment, expr_if, locator),
        AnyNodeRef::ExprSlice(expr_slice) => handle_slice_comments(comment, expr_slice, locator),
        AnyNodeRef::ExprStarred(starred) => {
            handle_trailing_expression_starred_star_end_of_line_comment(comment, starred)
        }
        AnyNodeRef::ExprSubscript(expr_subscript) => {
            if let Expr::Slice(expr_slice) = expr_subscript.slice.as_ref() {
                handle_slice_comments(comment, expr_slice, locator)
            } else {
                CommentPlacement::Default(comment)
            }
        }
        AnyNodeRef::MatchCase(match_case) => handle_match_comment(comment, match_case, locator),
        AnyNodeRef::ModModule(_) => {
            handle_module_level_own_line_comment_before_class_or_function_comment(comment, locator)
        }
        AnyNodeRef::WithItem(_) => handle_with_item_comment(comment, locator),
        AnyNodeRef::StmtFunctionDef(_) | AnyNodeRef::StmtAsyncFunctionDef(_) => {
            handle_leading_function_with_decorators_comment(comment)
        }
        _ => CommentPlacement::Default(comment),
    }
}

fn handle_end_of_line_comment_around_body<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // Handle comments before the first statement in a body
    // ```python
    // for x in range(10): # in the main body ...
    //     pass
    // else: # ... and in alternative bodies
    //     pass
    // ```
    if let Some(following) = comment.following_node() {
        if is_first_statement_in_body(following, comment.enclosing_node())
            && SimpleTokenizer::new(
                locator.contents(),
                TextRange::new(comment.end(), following.start()),
            )
            .skip_trivia()
            .next()
            .is_none()
        {
            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }
    }

    // Handle comments after a body
    // ```python
    // if True:
    //     pass # after the main body ...
    //
    // try:
    //     1 / 0
    // except ZeroDivisionError:
    //     print("Error") # ...  and after alternative bodies
    // ```
    // The first earlier branch filters out ambiguities e.g. around try-except-finally.
    if let Some(preceding) = comment.preceding_node() {
        if let Some(last_child) = last_child_in_body(preceding) {
            let innermost_child =
                std::iter::successors(Some(last_child), |parent| last_child_in_body(*parent))
                    .last()
                    .unwrap_or(last_child);
            return CommentPlacement::trailing(innermost_child, comment);
        }
    }

    CommentPlacement::Default(comment)
}

/// Check if the given statement is the first statement after the colon of a branch, be it in if
/// statements, for statements, after each part of a try-except-else-finally or function/class
/// definitions.
///
///
/// ```python
/// if True:    <- has body
///     a       <- first statement
///     b
/// elif b:     <- has body
///     c       <- first statement
///     d
/// else:       <- has body
///     e       <- first statement
///     f
///
/// class:      <- has body
///     a: int  <- first statement
///     b: int
///
/// ```
///
/// For nodes with multiple bodies, we check all bodies that don't have their own node. For
/// try-except-else-finally, each except branch has it's own node, so for the `StmtTry`, we check
/// the `try:`, `else:` and `finally:`, bodies, while `ExceptHandlerExceptHandler` has it's own
/// check. For for-else and while-else, we check both branches for the whole statement.
///
/// ```python
/// try:        <- has body (a)
///     6/8     <- first statement (a)
///     1/0
/// except:     <- has body (b)
///     a       <- first statement (b)
///     b
/// else:
///     c       <- first statement (a)
///     d
/// finally:
///     e       <- first statement (a)
///     f
/// ```
fn is_first_statement_in_body(statement: AnyNodeRef, has_body: AnyNodeRef) -> bool {
    match has_body {
        AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
        | AnyNodeRef::StmtAsyncFor(ast::StmtAsyncFor { body, orelse, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
            are_same_optional(statement, body.first())
                || are_same_optional(statement, orelse.first())
        }

        AnyNodeRef::StmtTry(ast::StmtTry {
            body,
            orelse,
            finalbody,
            ..
        })
        | AnyNodeRef::StmtTryStar(ast::StmtTryStar {
            body,
            orelse,
            finalbody,
            ..
        }) => {
            are_same_optional(statement, body.first())
                || are_same_optional(statement, orelse.first())
                || are_same_optional(statement, finalbody.first())
        }

        AnyNodeRef::StmtIf(ast::StmtIf { body, .. })
        | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. })
        | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
        | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
            body, ..
        })
        | AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
        | AnyNodeRef::StmtAsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
        | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. }) => {
            are_same_optional(statement, body.first())
        }

        _ => false,
    }
}

/// Handles own line comments after a body, either at the end or between bodies.
/// ```python
/// for x in y:
///     pass
///     # This should be a trailing comment of `pass` and not a leading comment of the `print`
/// # This is a dangling comment that should be remain before the `else`
/// else:
///     print("I have no comments")
///     # This should be a trailing comment of the print
/// # This is a trailing comment of the entire statement
/// ```
fn handle_own_line_comment_after_body<'a>(
    comment: DecoratedComment<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    debug_assert!(comment.line_position().is_own_line());

    // If the following is the first child in an alternative body, this must be the last child in
    // the previous one
    let Some(preceding) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    // If there's any non-trivia token between the preceding node and the comment, than it means
    // we're past the case of the alternate branch, defer to the default rules
    // ```python
    // if a:
    //     preceding()
    //     # comment we place
    // else:
    //     # default placement comment
    //     def inline_after_else(): ...
    // ```
    let maybe_token = SimpleTokenizer::new(
        locator.contents(),
        TextRange::new(preceding.end(), comment.slice().start()),
    )
    .skip_trivia()
    .next();
    if maybe_token.is_some() {
        return CommentPlacement::Default(comment);
    }

    // Check if we're between bodies and should attach to the following body. If that is not the
    // case, either because there is no following branch or because the indentation is too deep,
    // attach to the recursively last statement in the preceding body with the matching indentation.
    match handle_own_line_comment_between_branches(comment, preceding, locator) {
        CommentPlacement::Default(comment) => {
            // Knowing the comment is not between branches, handle comments after the last branch
            handle_own_line_comment_after_branch(comment, preceding, locator)
        }
        placement => placement,
    }
}

/// Handles own line comments between two branches of a node.
/// ```python
/// for x in y:
///     pass
/// # This one ...
/// else:
///     print("I have no comments")
/// # ... but not this one
/// ```
fn handle_own_line_comment_between_branches<'a>(
    comment: DecoratedComment<'a>,
    preceding: AnyNodeRef<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // The following statement must be the first statement in an alternate body, otherwise check
    // if it's a comment after the final body and handle that case
    let Some(following) = comment.following_node() else {
        return CommentPlacement::Default(comment);
    };
    if !is_first_statement_in_alternate_body(following, comment.enclosing_node()) {
        return CommentPlacement::Default(comment);
    }

    // It depends on the indentation level of the comment if it is a leading comment for the
    // following branch or if it a trailing comment of the previous body's last statement.
    let comment_indentation = indentation_at_offset(comment.slice().range().start(), locator)
        .unwrap_or_default()
        .len();

    let preceding_indentation = indentation(locator, &preceding).unwrap_or_default().len();

    // Compare to the last statement in the body
    match comment_indentation.cmp(&preceding_indentation) {
        Ordering::Greater => {
            // The comment might belong to an arbitrarily deeply nested inner statement
            // ```python
            // while True:
            //     def f_inner():
            //         pass
            //         # comment
            // else:
            //     print("noop")
            // ```
            CommentPlacement::Default(comment)
        }
        Ordering::Equal => {
            // The comment belongs to the last statement, unless the preceding branch has a body.
            // ```python
            // try:
            //     pass
            //     # I'm a trailing comment of the `pass`
            // except ZeroDivisionError:
            //     print()
            // # I'm a dangling comment of the try, even if the indentation matches the except
            // else:
            //     pass
            // ```
            if preceding.is_alternative_branch_with_node() {
                // The indentation is equal, but only because the preceding branch has a node. The
                // comment still belongs to the following branch, which may not have a node.
                CommentPlacement::dangling(comment.enclosing_node(), comment)
            } else {
                CommentPlacement::trailing(preceding, comment)
            }
        }
        Ordering::Less => {
            // The comment is leading on the following block
            if following.is_alternative_branch_with_node() {
                // For some alternative branches, there are nodes ...
                // ```python
                // try:
                //     pass
                // # I'm a leading comment of the `except` statement.
                // except ZeroDivisionError:
                //     print()
                // ```
                CommentPlacement::leading(following, comment)
            } else {
                // ... while for others, such as "else" of for loops and finally branches, the bodies
                // that are represented as a `Vec<Stmt>`, lacking a no node for the branch that we could
                // attach the comments to. We mark these as dangling comments and format them manually
                // in the enclosing node's formatting logic. For `try`, it's the formatters
                // responsibility to correctly identify the comments for the `finally` and `orelse`
                // block by looking at the comment's range.
                // ```python
                // for x in y:
                //     pass
                // # I'm a leading comment of the `else` branch but there's no `else` node.
                // else:
                //     print()
                // ```
                CommentPlacement::dangling(comment.enclosing_node(), comment)
            }
        }
    }
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
    match_case: &'a MatchCase,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // Must be an own line comment after the last statement in a match case
    if comment.line_position().is_end_of_line() || comment.following_node().is_some() {
        return CommentPlacement::Default(comment);
    }

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

    let comment_indentation = indentation_at_offset(comment.slice().range().start(), locator)
        .unwrap_or_default()
        .len();
    let match_case_indentation = indentation(locator, match_case).unwrap().len();

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
            CommentPlacement::leading(next_case, comment)
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
        let match_stmt_indentation = indentation(locator, match_stmt).unwrap_or_default().len();

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
            CommentPlacement::trailing(match_case, comment)
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

/// Determine where to attach an own line comment after a branch depending on its indentation
fn handle_own_line_comment_after_branch<'a>(
    comment: DecoratedComment<'a>,
    preceding_node: AnyNodeRef<'a>,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let Some(last_child) = last_child_in_body(preceding_node) else {
        return CommentPlacement::Default(comment);
    };

    // We only care about the length because indentations with mixed spaces and tabs are only valid if
    // the indent-level doesn't depend on the tab width (the indent level must be the same if the tab width is 1 or 8).
    let comment_indentation = indentation_at_offset(comment.slice().range().start(), locator)
        .unwrap_or_default()
        .len();

    // Keep the comment on the entire statement in case it's a trailing comment
    // ```python
    // if "first if":
    //     pass
    // elif "first elif":
    //     pass
    // # Trailing if comment
    // ```
    // Here we keep the comment a trailing comment of the `if`
    let preceding_indentation = indentation_at_offset(preceding_node.start(), locator)
        .unwrap_or_default()
        .len();
    if comment_indentation == preceding_indentation {
        return CommentPlacement::Default(comment);
    }

    let mut parent_body = None;
    let mut current_body = Some(preceding_node);
    let mut last_child_in_current_body = last_child;

    loop {
        let child_indentation = indentation(locator, &last_child_in_current_body)
            .unwrap_or_default()
            .len();

        // There a three cases:
        // ```python
        // if parent_body:
        //     if current_body:
        //         child_in_body()
        //         last_child_in_current_body # may or may not have children on its own
        // # less: Comment belongs to the parent block.
        //   # less
        //     # equal: The comment belongs to this block.
        //        # greater
        //          # greater: The comment belongs to the inner block.
        match comment_indentation.cmp(&child_indentation) {
            Ordering::Less => {
                return if let Some(parent_block) = parent_body {
                    // Comment belongs to the parent block.
                    CommentPlacement::trailing(parent_block, comment)
                } else {
                    // The comment does not belong to this block.
                    // ```python
                    // if test:
                    //     pass
                    // # comment
                    // ```
                    CommentPlacement::Default(comment)
                };
            }
            Ordering::Equal => {
                // The comment belongs to this block.
                return CommentPlacement::trailing(last_child_in_current_body, comment);
            }
            Ordering::Greater => {
                if let Some(nested_child) = last_child_in_body(last_child_in_current_body) {
                    // The comment belongs to the inner block.
                    parent_body = current_body;
                    current_body = Some(last_child_in_current_body);
                    last_child_in_current_body = nested_child;
                } else {
                    // The comment is overindented, we assign it to the most indented child we have.
                    // ```python
                    // if test:
                    //     pass
                    //       # comment
                    // ```
                    return CommentPlacement::trailing(last_child_in_current_body, comment);
                }
            }
        }
    }
}

/// Attaches comments for the positional only arguments separator `/` or the keywords only arguments
/// separator `*` as dangling comments to the enclosing [`Arguments`] node.
///
/// See [`assign_argument_separator_comment_placement`]
fn handle_arguments_separator_comment<'a>(
    comment: DecoratedComment<'a>,
    arguments: &Arguments,
    locator: &Locator,
) -> CommentPlacement<'a> {
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
    binary_expression: &'a ExprBinOp,
    locator: &Locator,
) -> CommentPlacement<'a> {
    // Only if there's a preceding node (in which case, the preceding node is `left`).
    if comment.preceding_node().is_none() || comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    let between_operands_range = TextRange::new(
        binary_expression.left.end(),
        binary_expression.right.start(),
    );

    let mut tokens = SimpleTokenizer::new(locator.contents(), between_operands_range)
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let operator_offset = tokens
        .next()
        .expect("Expected a token for the operator")
        .start();

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
        CommentPlacement::trailing(binary_expression.left.as_ref(), comment)
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
            CommentPlacement::dangling(binary_expression, comment)
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
    debug_assert!(comment.enclosing_node().is_module());
    // Only applies for own line comments on the module level...
    if comment.line_position().is_end_of_line() {
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
    expr_slice: &'a ExprSlice,
    locator: &Locator,
) -> CommentPlacement<'a> {
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
            CommentPlacement::leading(node.as_ref(), comment)
        } else {
            // If a trailing comment is an end of line comment that's fine because we have a node
            // ahead of it
            CommentPlacement::trailing(node.as_ref(), comment)
        }
    } else {
        CommentPlacement::dangling(expr_slice, comment)
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
fn handle_leading_function_with_decorators_comment(comment: DecoratedComment) -> CommentPlacement {
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
    debug_assert!(matches!(
        comment.enclosing_node(),
        AnyNodeRef::ExprDict(_) | AnyNodeRef::Keyword(_)
    ));

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
        debug_assert!(token.kind == SimpleTokenKind::Star, "Expected star token");
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
    attribute: &'a ExprAttribute,
) -> CommentPlacement<'a> {
    debug_assert!(
        comment.preceding_node().is_some(),
        "The enclosing node an attribute expression, expected to be at least after the name"
    );

    // ```text
    // value   .   attr
    //      ^^^^^^^ we're in this range
    // ```
    debug_assert!(
        TextRange::new(attribute.value.end(), attribute.attr.start())
            .contains(comment.slice().start())
    );
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
        CommentPlacement::dangling(attribute, comment)
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
    expr_if: &'a ExprIfExp,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let ExprIfExp {
        range: _,
        test,
        body,
        orelse,
    } = expr_if;

    if comment.line_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    let if_token = find_only_token_in_range(
        TextRange::new(body.end(), test.start()),
        SimpleTokenKind::If,
        locator,
    );
    // Between `if` and `test`
    if if_token.range.start() < comment.slice().start() && comment.slice().start() < test.start() {
        return CommentPlacement::leading(test.as_ref(), comment);
    }

    let else_token = find_only_token_in_range(
        TextRange::new(test.end(), orelse.start()),
        SimpleTokenKind::Else,
        locator,
    );
    // Between `else` and `orelse`
    if else_token.range.start() < comment.slice().start()
        && comment.slice().start() < orelse.start()
    {
        return CommentPlacement::leading(orelse.as_ref(), comment);
    }

    CommentPlacement::Default(comment)
}

/// Moving
/// ``` python
/// call(
///     # Leading starred comment
///     * # Trailing star comment
///     []
/// )
/// ```
/// to
/// ``` python
/// call(
///     # Leading starred comment
///     # Trailing star comment
///     * []
/// )
/// ```
fn handle_trailing_expression_starred_star_end_of_line_comment<'a>(
    comment: DecoratedComment<'a>,
    starred: &'a ExprStarred,
) -> CommentPlacement<'a> {
    if comment.line_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    if comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    CommentPlacement::leading(starred, comment)
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
    debug_assert!(comment.enclosing_node().is_with_item());

    // Needs to be a with item with an `as` expression.
    let (Some(context_expr), Some(optional_vars)) =
        (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

    let as_token = find_only_token_in_range(
        TextRange::new(context_expr.end(), optional_vars.start()),
        SimpleTokenKind::As,
        locator,
    );

    if comment.end() < as_token.start() {
        // If before the `as` keyword, then it must be a trailing comment of the context expression.
        CommentPlacement::trailing(context_expr, comment)
    }
    // Trailing end of line comment coming after the `as` keyword`.
    else if comment.line_position().is_end_of_line() {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::leading(optional_vars, comment)
    }
}

/// Looks for a token in the range that contains no other tokens except for parentheses outside
/// the expression ranges
fn find_only_token_in_range(
    range: TextRange,
    token_kind: SimpleTokenKind,
    locator: &Locator,
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
    comprehension: &'a Comprehension,
    locator: &Locator,
) -> CommentPlacement<'a> {
    let is_own_line = comment.line_position().is_own_line();

    // Comments between the `for` and target
    // ```python
    // [
    //      a
    //      for  # attach as dangling on the comprehension
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
        SimpleTokenKind::In,
        locator,
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
            CommentPlacement::dangling(&comprehension.iter, comment)
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
            SimpleTokenKind::If,
            locator,
        );
        if is_own_line {
            if last_end < comment.slice().start() && comment.slice().start() < if_token.start() {
                return CommentPlacement::dangling(if_node, comment);
            }
        } else if if_token.start() < comment.slice().start()
            && comment.slice().start() < if_node.range().start()
        {
            return CommentPlacement::dangling(if_node, comment);
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

/// The last child of the last branch, if the node hs multiple branches.
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

/// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
fn is_first_statement_in_alternate_body(statement: AnyNodeRef, has_body: AnyNodeRef) -> bool {
    match has_body {
        AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
        | AnyNodeRef::StmtAsyncFor(ast::StmtAsyncFor { orelse, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { orelse, .. }) => {
            are_same_optional(statement, orelse.first())
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
            are_same_optional(statement, handlers.first())
                || are_same_optional(statement, orelse.first())
                || are_same_optional(statement, finalbody.first())
        }

        AnyNodeRef::StmtIf(ast::StmtIf {
            elif_else_clauses, ..
        }) => are_same_optional(statement, elif_else_clauses.first()),
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

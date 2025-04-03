use ast::helpers::comment_indentation_after;
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{
    self as ast, AnyNodeRef, Comprehension, Expr, ModModule, Parameter, Parameters, StringLike,
};
use ruff_python_trivia::{
    find_only_token_in_range, first_non_trivia_token, indentation_at_offset, BackwardsTokenizer,
    CommentRanges, SimpleToken, SimpleTokenKind, SimpleTokenizer,
};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange};
use std::cmp::Ordering;

use crate::comments::visitor::{CommentPlacement, DecoratedComment};
use crate::expression::expr_slice::{assign_comment_in_slice, ExprSliceCommentSection};
use crate::expression::parentheses::is_expression_parenthesized;
use crate::other::parameters::{
    assign_argument_separator_comment_placement, find_parameter_separators,
};
use crate::pattern::pattern_match_sequence::SequenceType;

/// Manually attach comments to nodes that the default placement gets wrong.
pub(super) fn place_comment<'a>(
    comment: DecoratedComment<'a>,
    comment_ranges: &CommentRanges,
    source: &str,
) -> CommentPlacement<'a> {
    handle_parenthesized_comment(comment, source)
        .or_else(|comment| handle_end_of_line_comment_around_body(comment, source))
        .or_else(|comment| handle_own_line_comment_around_body(comment, source))
        .or_else(|comment| handle_enclosed_comment(comment, comment_ranges, source))
}

/// Handle parenthesized comments. A parenthesized comment is a comment that appears within a
/// parenthesis, but not within the range of the expression enclosed by the parenthesis.
/// For example, the comment here is a parenthesized comment:
/// ```python
/// if (
///     # comment
///     True
/// ):
///     ...
/// ```
/// The parentheses enclose `True`, but the range of `True` doesn't include the `# comment`.
///
/// Default handling can get parenthesized comments wrong in a number of ways. For example, the
/// comment here is marked (by default) as a trailing comment of `x`, when it should be a leading
/// comment of `y`:
/// ```python
/// assert (
///     x
/// ), ( # comment
///     y
/// )
/// ```
///
/// Similarly, this is marked as a leading comment of `y`, when it should be a trailing comment of
/// `x`:
/// ```python
/// if (
///     x
///     # comment
/// ):
///    y
/// ```
///
/// As a generalized solution, if a comment has a preceding node and a following node, we search for
/// opening and closing parentheses between the two nodes. If we find a closing parenthesis between
/// the preceding node and the comment, then the comment is a trailing comment of the preceding
/// node. If we find an opening parenthesis between the comment and the following node, then the
/// comment is a leading comment of the following node.
fn handle_parenthesized_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    // As a special-case, ignore comments within f-strings, like:
    // ```python
    // (
    //     f'{1}' # comment
    //     f'{2}'
    // )
    // ```
    // These can't be parenthesized, as they must fall between two string tokens in an implicit
    // concatenation. But the expression ranges only include the `1` and `2` above, so we also
    // can't lex the contents between them.
    if comment.enclosing_node().is_expr_f_string() {
        return CommentPlacement::Default(comment);
    }

    let Some(preceding) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    let Some(following) = comment.following_node() else {
        return CommentPlacement::Default(comment);
    };

    // TODO(charlie): Assert that there are no bogus tokens in these ranges. There are a few cases
    // where we _can_ hit bogus tokens, but the parentheses need to come before them. For example:
    // ```python
    // try:
    //     some_call()
    // except (
    //     UnformattedError
    //     # trailing comment
    // ) as err:
    //     handle_exception()
    // ```
    // Here, we lex from the end of `UnformattedError` to the start of `handle_exception()`, which
    // means we hit an "other" token at `err`. We know the parentheses must precede the `err`, but
    // this could be fixed by including `as err` in the node range.
    //
    // Another example:
    // ```python
    // @deco
    // # comment
    // def decorated():
    //     pass
    // ```
    // Here, we lex from the end of `deco` to the start of the arguments of `decorated`. We hit an
    // "other" token at `decorated`, but any parentheses must precede that.
    //
    // For now, we _can_ assert, but to do so, we stop lexing when we hit a token that precedes an
    // identifier.

    // Search for comments that to the right of a parenthesized node, e.g.:
    // ```python
    // [
    //     x  # comment,
    //     (
    //         y,
    //     ),
    // ]
    // ```
    let range = TextRange::new(preceding.end(), comment.start());
    let tokenizer = SimpleTokenizer::new(source, range);
    if tokenizer
        .skip_trivia()
        .take_while(|token| {
            !matches!(
                token.kind,
                SimpleTokenKind::As | SimpleTokenKind::Def | SimpleTokenKind::Class
            )
        })
        .any(|token| {
            debug_assert!(
                !matches!(token.kind, SimpleTokenKind::Bogus),
                "Unexpected token between nodes: `{:?}`",
                &source[range]
            );
            token.kind() == SimpleTokenKind::LParen
        })
    {
        return CommentPlacement::leading(following, comment);
    }

    // Search for comments that to the right of a parenthesized node, e.g.:
    // ```python
    // [
    //     (
    //         x  # comment,
    //     ),
    //     y
    // ]
    // ```
    let range = TextRange::new(comment.end(), following.start());
    let tokenizer = SimpleTokenizer::new(source, range);
    if tokenizer
        .skip_trivia()
        .take_while(|token| {
            !matches!(
                token.kind,
                SimpleTokenKind::As | SimpleTokenKind::Def | SimpleTokenKind::Class
            )
        })
        .any(|token| {
            debug_assert!(
                !matches!(token.kind, SimpleTokenKind::Bogus),
                "Unexpected token between nodes: `{:?}`",
                &source[range]
            );
            token.kind() == SimpleTokenKind::RParen
        })
    {
        return CommentPlacement::trailing(preceding, comment);
    }

    CommentPlacement::Default(comment)
}

/// Handle a comment that is enclosed by a node.
fn handle_enclosed_comment<'a>(
    comment: DecoratedComment<'a>,
    comment_ranges: &CommentRanges,
    source: &str,
) -> CommentPlacement<'a> {
    match comment.enclosing_node() {
        AnyNodeRef::Parameters(parameters) => {
            handle_parameters_separator_comment(comment, parameters, source).or_else(|comment| {
                if are_parameters_parenthesized(parameters, source) {
                    handle_bracketed_end_of_line_comment(comment, source)
                } else {
                    CommentPlacement::Default(comment)
                }
            })
        }
        AnyNodeRef::Parameter(parameter) => handle_parameter_comment(comment, parameter, source),
        AnyNodeRef::Arguments(_) | AnyNodeRef::TypeParams(_) | AnyNodeRef::PatternArguments(_) => {
            handle_bracketed_end_of_line_comment(comment, source)
        }
        AnyNodeRef::Comprehension(comprehension) => {
            handle_comprehension_comment(comment, comprehension, source)
        }

        AnyNodeRef::ExprAttribute(attribute) => {
            handle_attribute_comment(comment, attribute, source)
        }
        AnyNodeRef::ExprBinOp(binary_expression) => {
            handle_trailing_binary_expression_left_or_operator_comment(
                comment,
                binary_expression,
                source,
            )
        }
        AnyNodeRef::ExprBoolOp(_) | AnyNodeRef::ExprCompare(_) => {
            handle_trailing_binary_like_comment(comment, source)
        }
        AnyNodeRef::Keyword(keyword) => handle_keyword_comment(comment, keyword, source),
        AnyNodeRef::PatternKeyword(pattern_keyword) => {
            handle_pattern_keyword_comment(comment, pattern_keyword, source)
        }
        AnyNodeRef::ExprUnaryOp(unary_op) => handle_unary_op_comment(comment, unary_op, source),
        AnyNodeRef::ExprNamed(_) => handle_named_expr_comment(comment, source),
        AnyNodeRef::ExprLambda(lambda) => handle_lambda_comment(comment, lambda, source),
        AnyNodeRef::ExprDict(_) => handle_dict_unpacking_comment(comment, source)
            .or_else(|comment| handle_bracketed_end_of_line_comment(comment, source))
            .or_else(|comment| handle_key_value_comment(comment, source)),
        AnyNodeRef::ExprDictComp(_) => handle_key_value_comment(comment, source)
            .or_else(|comment| handle_bracketed_end_of_line_comment(comment, source)),
        AnyNodeRef::ExprIf(expr_if) => handle_expr_if_comment(comment, expr_if, source),
        AnyNodeRef::ExprSlice(expr_slice) => {
            handle_slice_comments(comment, expr_slice, comment_ranges, source)
        }
        AnyNodeRef::ExprStarred(starred) => {
            handle_trailing_expression_starred_star_end_of_line_comment(comment, starred, source)
        }
        AnyNodeRef::ExprSubscript(expr_subscript) => {
            if let Expr::Slice(expr_slice) = expr_subscript.slice.as_ref() {
                return handle_slice_comments(comment, expr_slice, comment_ranges, source);
            }

            // Handle non-slice subscript end-of-line comments coming after the `[`
            // ```python
            // repro(
            //     "some long string that takes up some space"
            //  )[  # some long comment also taking up space
            //     0
            // ]
            // ```
            if comment.line_position().is_end_of_line()
                && expr_subscript.value.end() < comment.start()
            {
                // Ensure that there are no tokens between the open bracket and the comment.
                let mut lexer = SimpleTokenizer::new(
                    source,
                    TextRange::new(expr_subscript.value.end(), comment.start()),
                )
                .skip_trivia();

                // Skip to after the opening parenthesis (may skip some closing parentheses of value)
                if !lexer
                    .by_ref()
                    .any(|token| token.kind() == SimpleTokenKind::LBracket)
                {
                    return CommentPlacement::Default(comment);
                }

                // If there are no additional tokens between the open parenthesis and the comment, then
                // it should be attached as a dangling comment on the brackets, rather than a leading
                // comment on the first argument.
                if lexer.next().is_none() {
                    return CommentPlacement::dangling(expr_subscript, comment);
                }
            }

            CommentPlacement::Default(comment)
        }
        AnyNodeRef::ModModule(module) => {
            handle_trailing_module_comment(module, comment).or_else(|comment| {
                handle_module_level_own_line_comment_before_class_or_function_comment(
                    comment, source,
                )
            })
        }
        AnyNodeRef::WithItem(_) => handle_with_item_comment(comment, source),
        AnyNodeRef::PatternMatchSequence(pattern_match_sequence) => {
            if SequenceType::from_pattern(pattern_match_sequence, source).is_parenthesized() {
                handle_bracketed_end_of_line_comment(comment, source)
            } else {
                CommentPlacement::Default(comment)
            }
        }
        AnyNodeRef::PatternMatchClass(class) => handle_pattern_match_class_comment(comment, class),
        AnyNodeRef::PatternMatchAs(_) => handle_pattern_match_as_comment(comment, source),
        AnyNodeRef::PatternMatchStar(_) => handle_pattern_match_star_comment(comment),
        AnyNodeRef::PatternMatchMapping(pattern) => {
            handle_bracketed_end_of_line_comment(comment, source)
                .or_else(|comment| handle_pattern_match_mapping_comment(comment, pattern, source))
        }
        AnyNodeRef::StmtFunctionDef(_) => handle_leading_function_with_decorators_comment(comment),
        AnyNodeRef::StmtClassDef(class_def) => {
            handle_leading_class_with_decorators_comment(comment, class_def)
        }
        AnyNodeRef::StmtImportFrom(import_from) => handle_import_from_comment(comment, import_from),
        AnyNodeRef::StmtWith(with_) => handle_with_comment(comment, with_),
        AnyNodeRef::ExprCall(_) => handle_call_comment(comment),
        AnyNodeRef::ExprStringLiteral(_) => {
            if let Some(AnyNodeRef::FString(fstring)) = comment.enclosing_parent() {
                CommentPlacement::dangling(fstring, comment)
            } else {
                CommentPlacement::Default(comment)
            }
        }
        AnyNodeRef::FString(fstring) => CommentPlacement::dangling(fstring, comment),
        AnyNodeRef::FStringExpressionElement(_) => {
            // Handle comments after the format specifier (should be rare):
            //
            // ```python
            // f"literal {
            //     expr:.3f
            //     # comment
            // }"
            // ```
            //
            // This is a valid comment placement.
            if matches!(
                comment.preceding_node(),
                Some(
                    AnyNodeRef::FStringExpressionElement(_) | AnyNodeRef::FStringLiteralElement(_)
                )
            ) {
                CommentPlacement::trailing(comment.enclosing_node(), comment)
            } else {
                handle_bracketed_end_of_line_comment(comment, source)
            }
        }
        AnyNodeRef::ExprList(_)
        | AnyNodeRef::ExprSet(_)
        | AnyNodeRef::ExprListComp(_)
        | AnyNodeRef::ExprSetComp(_) => handle_bracketed_end_of_line_comment(comment, source),
        AnyNodeRef::ExprTuple(ast::ExprTuple {
            parenthesized: true,
            ..
        }) => handle_bracketed_end_of_line_comment(comment, source),
        AnyNodeRef::ExprGenerator(generator) if generator.parenthesized => {
            handle_bracketed_end_of_line_comment(comment, source)
        }
        AnyNodeRef::StmtReturn(_) => {
            handle_trailing_implicit_concatenated_string_comment(comment, comment_ranges, source)
        }
        AnyNodeRef::StmtAssign(assignment)
            if comment.preceding_node().is_some_and(|preceding| {
                preceding.ptr_eq(AnyNodeRef::from(&*assignment.value))
            }) =>
        {
            handle_trailing_implicit_concatenated_string_comment(comment, comment_ranges, source)
        }
        AnyNodeRef::StmtAnnAssign(assignment)
            if comment.preceding_node().is_some_and(|preceding| {
                assignment
                    .value
                    .as_deref()
                    .is_some_and(|value| preceding.ptr_eq(value.into()))
            }) =>
        {
            handle_trailing_implicit_concatenated_string_comment(comment, comment_ranges, source)
        }
        AnyNodeRef::StmtAugAssign(assignment)
            if comment.preceding_node().is_some_and(|preceding| {
                preceding.ptr_eq(AnyNodeRef::from(&*assignment.value))
            }) =>
        {
            handle_trailing_implicit_concatenated_string_comment(comment, comment_ranges, source)
        }
        AnyNodeRef::StmtTypeAlias(assignment)
            if comment.preceding_node().is_some_and(|preceding| {
                preceding.ptr_eq(AnyNodeRef::from(&*assignment.value))
            }) =>
        {
            handle_trailing_implicit_concatenated_string_comment(comment, comment_ranges, source)
        }

        _ => CommentPlacement::Default(comment),
    }
}

/// Handle an end-of-line comment around a body.
fn handle_end_of_line_comment_around_body<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    if comment.line_position().is_own_line() {
        return CommentPlacement::Default(comment);
    }

    // Handle comments before the first statement in a body
    // ```python
    // for x in range(10): # in the main body ...
    //     pass
    // else: # ... and in alternative bodies
    //     pass
    // ```
    if let Some(following) = comment.following_node() {
        if following.is_first_statement_in_body(comment.enclosing_node())
            && SimpleTokenizer::new(source, TextRange::new(comment.end(), following.start()))
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
        if let Some(last_child) = preceding.last_child_in_body() {
            let innermost_child =
                std::iter::successors(Some(last_child), AnyNodeRef::last_child_in_body)
                    .last()
                    .unwrap_or(last_child);
            return CommentPlacement::trailing(innermost_child, comment);
        }
    }

    CommentPlacement::Default(comment)
}

/// Handles own-line comments around a body (at the end of the body, at the end of the header
/// preceding the body, or between bodies):
///
/// ```python
/// for x in y:
///     pass
///     # This should be a trailing comment of `pass` and not a leading comment of the `print`
/// # This is a dangling comment that should be remain before the `else`
/// else:
///     print("I have no comments")
///     # This should be a trailing comment of the print
/// # This is a trailing comment of the entire statement
///
/// if (
///     True
///     # This should be a trailing comment of `True` and not a leading comment of `pass`
/// ):
///     pass
/// ```
fn handle_own_line_comment_around_body<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    if comment.line_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    // If the following is the first child in an alternative body, this must be the last child in
    // the previous one
    let Some(preceding) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    // If there's any non-trivia token between the preceding node and the comment, then it means
    // we're past the case of the alternate branch, defer to the default rules
    // ```python
    // if a:
    //     preceding()
    //     # comment we place
    // else:
    //     # default placement comment
    //     def inline_after_else(): ...
    // ```
    let maybe_token =
        SimpleTokenizer::new(source, TextRange::new(preceding.end(), comment.start()))
            .skip_trivia()
            .next();
    if maybe_token.is_some() {
        return CommentPlacement::Default(comment);
    }

    // Check if we're between bodies and should attach to the following body.
    handle_own_line_comment_between_branches(comment, preceding, source)
        .or_else(|comment| {
            // Otherwise, there's no following branch or the indentation is too deep, so attach to the
            // recursively last statement in the preceding body with the matching indentation.
            handle_own_line_comment_after_branch(comment, preceding, source)
        })
        .or_else(|comment| handle_own_line_comment_between_statements(comment, source))
}

/// Handles own-line comments between statements. If an own-line comment is between two statements,
/// it's treated as a leading comment of the following statement _if_ there are no empty lines
/// separating the comment and the statement; otherwise, it's treated as a trailing comment of the
/// preceding statement.
///
/// For example, this comment would be a trailing comment of `x = 1`:
/// ```python
/// x = 1
/// # comment
///
/// y = 2
/// ```
///
/// However, this comment would be a leading comment of `y = 2`:
/// ```python
/// x = 1
///
/// # comment
/// y = 2
/// ```
fn handle_own_line_comment_between_statements<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    let Some(preceding) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    let Some(following) = comment.following_node() else {
        return CommentPlacement::Default(comment);
    };

    // We're looking for comments between two statements, like:
    // ```python
    // x = 1
    // # comment
    // y = 2
    // ```
    if !preceding.is_statement() || !following.is_statement() {
        return CommentPlacement::Default(comment);
    }

    if comment.line_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    // If the comment is directly attached to the following statement; make it a leading
    // comment:
    // ```python
    // x = 1
    //
    // # leading comment
    // y = 2
    // ```
    //
    // Otherwise, if there's at least one empty line, make it a trailing comment:
    // ```python
    // x = 1
    // # trailing comment
    //
    // y = 2
    // ```
    if max_empty_lines(&source[TextRange::new(comment.end(), following.start())]) == 0 {
        CommentPlacement::leading(following, comment)
    } else {
        CommentPlacement::trailing(preceding, comment)
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
    source: &str,
) -> CommentPlacement<'a> {
    // The following statement must be the first statement in an alternate body, otherwise check
    // if it's a comment after the final body and handle that case
    let Some(following) = comment.following_node() else {
        return CommentPlacement::Default(comment);
    };
    if !following.is_first_statement_in_alternate_body(comment.enclosing_node()) {
        return CommentPlacement::Default(comment);
    }

    // It depends on the indentation level of the comment if it is a leading comment for the
    // following branch or if it a trailing comment of the previous body's last statement.
    let comment_indentation = comment_indentation_after(preceding, comment.range(), source);

    let preceding_indentation = indentation(source, &preceding)
        .unwrap_or_default()
        .text_len();

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

/// Determine where to attach an own line comment after a branch depending on its indentation
fn handle_own_line_comment_after_branch<'a>(
    comment: DecoratedComment<'a>,
    preceding: AnyNodeRef<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    let Some(last_child) = preceding.last_child_in_body() else {
        return CommentPlacement::Default(comment);
    };

    // We only care about the length because indentations with mixed spaces and tabs are only valid if
    // the indent-level doesn't depend on the tab width (the indent level must be the same if the tab width is 1 or 8).
    let comment_indentation = comment_indentation_after(preceding, comment.range(), source);

    // Keep the comment on the entire statement in case it's a trailing comment
    // ```python
    // if "first if":
    //     pass
    // elif "first elif":
    //     pass
    // # Trailing if comment
    // ```
    // Here we keep the comment a trailing comment of the `if`
    let preceding_indentation = indentation_at_offset(preceding.start(), source)
        .unwrap_or_default()
        .text_len();
    if comment_indentation == preceding_indentation {
        return CommentPlacement::Default(comment);
    }

    let mut parent = None;
    let mut last_child_in_parent = last_child;

    loop {
        let child_indentation = indentation(source, &last_child_in_parent)
            .unwrap_or_default()
            .text_len();

        // There a three cases:
        // ```python
        // if parent_body:
        //     if current_body:
        //         child_in_body()
        //         last_child_in_current_body  # may or may not have children on its own
        // # less: Comment belongs to the parent block.
        //   # less: Comment belongs to the parent block.
        //     # equal: The comment belongs to this block.
        //       # greater (but less in the next iteration)
        //         # greater: The comment belongs to the inner block.
        // ```
        match comment_indentation.cmp(&child_indentation) {
            Ordering::Less => {
                return if let Some(parent_block) = parent {
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
                return CommentPlacement::trailing(last_child_in_parent, comment);
            }
            Ordering::Greater => {
                if let Some(nested_child) = last_child_in_parent.last_child_in_body() {
                    // The comment belongs to the inner block.
                    parent = Some(last_child_in_parent);
                    last_child_in_parent = nested_child;
                } else {
                    // The comment is overindented, we assign it to the most indented child we have.
                    // ```python
                    // if test:
                    //     pass
                    //       # comment
                    // ```
                    return CommentPlacement::trailing(last_child_in_parent, comment);
                }
            }
        }
    }
}

/// Attaches comments for the positional-only parameters separator `/` or the keywords-only
/// parameters separator `*` as dangling comments to the enclosing [`Parameters`] node.
///
/// See [`assign_argument_separator_comment_placement`]
fn handle_parameters_separator_comment<'a>(
    comment: DecoratedComment<'a>,
    parameters: &Parameters,
    source: &str,
) -> CommentPlacement<'a> {
    let (slash, star) = find_parameter_separators(source, parameters);
    let placement = assign_argument_separator_comment_placement(
        slash.as_ref(),
        star.as_ref(),
        comment.range(),
        comment.line_position(),
    );
    if placement.is_some() {
        return CommentPlacement::dangling(comment.enclosing_node(), comment);
    }

    CommentPlacement::Default(comment)
}

/// Associate comments that come before the `:` starting the type annotation or before the
/// parameter's name for unannotated parameters as leading parameter-comments.
///
/// The parameter's name isn't a node to which comments can be associated.
/// That's why we pull out all comments that come before the expression name or the type annotation
/// and make them leading parameter comments. For example:
/// * `* # comment\nargs`
/// * `arg # comment\n : int`
///
/// Associate comments with the type annotation when possible.
fn handle_parameter_comment<'a>(
    comment: DecoratedComment<'a>,
    parameter: &'a Parameter,
    source: &str,
) -> CommentPlacement<'a> {
    if parameter.annotation().is_some() {
        let colon = first_non_trivia_token(parameter.name.end(), source).expect(
            "A annotated parameter should have a colon following its name when it is valid syntax.",
        );

        assert_eq!(colon.kind(), SimpleTokenKind::Colon);

        if comment.start() < colon.start() {
            // The comment is before the colon, pull it out and make it a leading comment of the parameter.
            CommentPlacement::leading(parameter, comment)
        } else {
            CommentPlacement::Default(comment)
        }
    } else if comment.start() < parameter.name.start() {
        CommentPlacement::leading(parameter, comment)
    } else {
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
    binary_expression: &'a ast::ExprBinOp,
    source: &str,
) -> CommentPlacement<'a> {
    // Only if there's a preceding node (in which case, the preceding node is `left`).
    if comment.preceding_node().is_none() || comment.following_node().is_none() {
        return CommentPlacement::Default(comment);
    }

    let between_operands_range = TextRange::new(
        binary_expression.left.end(),
        binary_expression.right.start(),
    );

    let mut tokens = SimpleTokenizer::new(source, between_operands_range)
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let operator_offset = tokens
        .next()
        .expect("Expected a token for the operator")
        .start();

    if comment.end() < operator_offset {
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
        if source.contains_line_break(TextRange::new(
            binary_expression.left.end(),
            operator_offset,
        )) && source.contains_line_break(TextRange::new(
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

/// Attaches comments between two bool or compare expression operands to the preceding operand if the comment is before the operator.
///
/// ```python
/// a = (
///     5 > 3
///     # trailing comment
///     and 3 == 3
/// )
/// ```
fn handle_trailing_binary_like_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    debug_assert!(
        comment.enclosing_node().is_expr_bool_op() || comment.enclosing_node().is_expr_compare()
    );

    // Only if there's a preceding node (in which case, the preceding node is `left` or middle node).
    let (Some(left_operand), Some(right_operand)) =
        (comment.preceding_node(), comment.following_node())
    else {
        return CommentPlacement::Default(comment);
    };

    let between_operands_range = TextRange::new(left_operand.end(), right_operand.start());

    let mut tokens = SimpleTokenizer::new(source, between_operands_range)
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let operator_offset = tokens
        .next()
        .expect("Expected a token for the operator")
        .start();

    if comment.end() < operator_offset {
        CommentPlacement::trailing(left_operand, comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handles trailing comments after the last statement in a module.
/// Ruff's parser sets the module range to exclude trailing comments and the result is that
/// [`CommentPlacement::Default`] makes these comments dangling comments.
///
/// This method overrides the handling to make these comments trailing comments of the last
/// statement instead.
///
/// ```python
/// a
///
/// # trailing comment
/// ```
///
/// Comments of an all empty module are leading module comments
fn handle_trailing_module_comment<'a>(
    module: &'a ModModule,
    comment: DecoratedComment<'a>,
) -> CommentPlacement<'a> {
    if comment.preceding_node().is_none() && comment.following_node().is_none() {
        if let Some(last_statement) = module.body.last() {
            CommentPlacement::trailing(last_statement, comment)
        } else {
            CommentPlacement::leading(comment.enclosing_node(), comment)
        }
    } else {
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
    source: &str,
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
        AnyNodeRef::StmtFunctionDef(_) | AnyNodeRef::StmtClassDef(_)
    ) {
        return CommentPlacement::Default(comment);
    }

    // Make the comment a leading comment if there's no empty line between the comment and the function / class header
    if max_empty_lines(&source[TextRange::new(comment.end(), following.start())]) == 0 {
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
    expr_slice: &'a ast::ExprSlice,
    comment_ranges: &CommentRanges,
    source: &str,
) -> CommentPlacement<'a> {
    let ast::ExprSlice {
        range: _,
        lower,
        upper,
        step,
    } = expr_slice;

    // Check for `foo[ # comment`, but only if they are on the same line
    let after_lbracket = matches!(
        BackwardsTokenizer::up_to(comment.start(), source, comment_ranges)
            .skip_trivia()
            .next(),
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

    let assignment = assign_comment_in_slice(comment.range(), source, expr_slice);
    let node = match assignment {
        ExprSliceCommentSection::Lower => lower,
        ExprSliceCommentSection::Upper => upper,
        ExprSliceCommentSection::Step => step,
    };

    if let Some(node) = node {
        if comment.start() < node.start() {
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
        .is_some_and(|node| node.is_decorator());

    let is_following_parameters = comment
        .following_node()
        .is_some_and(|node| node.is_parameters() || node.is_type_params());

    if comment.line_position().is_own_line() && is_preceding_decorator && is_following_parameters {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handle comments between decorators and the decorated node.
///
/// For example, given:
/// ```python
/// @dataclass
/// # comment
/// class Foo(Bar):
///     ...
/// ```
///
/// The comment should be attached to the enclosing [`ast::StmtClassDef`] as a dangling node,
/// as opposed to being treated as a leading comment on `Bar` or similar.
fn handle_leading_class_with_decorators_comment<'a>(
    comment: DecoratedComment<'a>,
    class_def: &'a ast::StmtClassDef,
) -> CommentPlacement<'a> {
    if comment.line_position().is_own_line() && comment.start() < class_def.name.start() {
        if let Some(decorator) = class_def.decorator_list.last() {
            if decorator.end() < comment.start() {
                return CommentPlacement::dangling(class_def, comment);
            }
        }
    }
    CommentPlacement::Default(comment)
}

/// Handles comments between a keyword's identifier and value:
/// ```python
/// func(
///     x  # dangling
///     =  # dangling
///     # dangling
///     1,
///     **  # dangling
///     y
/// )
/// ```
fn handle_keyword_comment<'a>(
    comment: DecoratedComment<'a>,
    keyword: &'a ast::Keyword,
    source: &str,
) -> CommentPlacement<'a> {
    let start = keyword.arg.as_ref().map_or(keyword.start(), Ranged::end);

    // If the comment is parenthesized, it should be attached to the value:
    // ```python
    // func(
    //     x=(  # comment
    //         1
    //     )
    // )
    // ```
    let mut tokenizer = SimpleTokenizer::new(source, TextRange::new(start, comment.start()));
    if tokenizer.any(|token| token.kind == SimpleTokenKind::LParen) {
        return CommentPlacement::Default(comment);
    }

    CommentPlacement::leading(comment.enclosing_node(), comment)
}

/// Handles comments between a pattern keyword's identifier and value:
/// ```python
/// case Point2D(
///     x  # dangling
///     =  # dangling
///     # dangling
///     1
/// )
/// ```
fn handle_pattern_keyword_comment<'a>(
    comment: DecoratedComment<'a>,
    pattern_keyword: &'a ast::PatternKeyword,
    source: &str,
) -> CommentPlacement<'a> {
    // If the comment is parenthesized, it should be attached to the value:
    // ```python
    // case Point2D(
    //     x=(  # comment
    //         1
    //     )
    // )
    // ```
    let mut tokenizer = SimpleTokenizer::new(
        source,
        TextRange::new(pattern_keyword.attr.end(), comment.start()),
    );
    if tokenizer.any(|token| token.kind == SimpleTokenKind::LParen) {
        return CommentPlacement::Default(comment);
    }

    CommentPlacement::leading(comment.enclosing_node(), comment)
}

/// Handles comments between `**` and the variable name in dict unpacking
/// It attaches these to the appropriate value node.
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
    source: &str,
) -> CommentPlacement<'a> {
    debug_assert!(matches!(comment.enclosing_node(), AnyNodeRef::ExprDict(_)));

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
    let mut tokens = SimpleTokenizer::new(source, TextRange::new(preceding_end, comment.start()))
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);

    // if the remaining tokens from the previous node are exactly `**`,
    // re-assign the comment to the one that follows the stars.
    if tokens.any(|token| token.kind == SimpleTokenKind::DoubleStar) {
        CommentPlacement::leading(following, comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handles comments around the `:` in a key-value pair:
///
/// ```python
/// {
///     key  # dangling
///     :  # dangling
///     # dangling
///     value
/// }
/// ```
fn handle_key_value_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    debug_assert!(matches!(
        comment.enclosing_node(),
        AnyNodeRef::ExprDict(_) | AnyNodeRef::ExprDictComp(_)
    ));

    let (Some(following), Some(preceding)) = (comment.following_node(), comment.preceding_node())
    else {
        return CommentPlacement::Default(comment);
    };

    // Ensure that the comment is between the key and the value by detecting the colon:
    // ```python
    // {
    //     key  # comment
    //     : value
    // }
    // ```
    // This prevents against detecting comments on starred expressions as key-value comments.
    let tokens = SimpleTokenizer::new(source, TextRange::new(preceding.end(), following.start()));
    if tokens
        .skip_trivia()
        .any(|token| token.kind == SimpleTokenKind::Colon)
    {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handle comments between a function call and its arguments. For example, attach the following as
/// dangling on the call:
/// ```python
/// (
///   func
///   # dangling
///   ()
/// )
/// ```
fn handle_call_comment(comment: DecoratedComment) -> CommentPlacement {
    if comment.line_position().is_own_line() {
        if comment.preceding_node().is_some_and(|preceding| {
            comment.following_node().is_some_and(|following| {
                preceding.end() < comment.start() && comment.end() < following.start()
            })
        }) {
            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }
    }

    CommentPlacement::Default(comment)
}

/// Own line comments coming after the node are always dangling comments
/// ```python
/// (
///      a  # trailing comment on `a`
///      # dangling comment on the attribute
///      . # dangling comment on the attribute
///      # dangling comment on the attribute
///      b
/// )
/// ```
fn handle_attribute_comment<'a>(
    comment: DecoratedComment<'a>,
    attribute: &'a ast::ExprAttribute,
    source: &str,
) -> CommentPlacement<'a> {
    if comment.preceding_node().is_none() {
        // ```text
        // (    value)   .   attr
        //  ^^^^ we're in this range
        // ```
        return CommentPlacement::leading(attribute.value.as_ref(), comment);
    }

    // If the comment is parenthesized, use the parentheses to either attach it as a trailing
    // comment on the value or a dangling comment on the attribute.
    // For example, treat this as trailing:
    // ```python
    // (
    //     (
    //         value
    //         # comment
    //     )
    //     .attribute
    // )
    // ```
    //
    // However, treat this as dangling:
    // ```python
    // (
    //     (value)
    //     # comment
    //     .attribute
    // )
    // ```
    if let Some(right_paren) = SimpleTokenizer::starts_at(attribute.value.end(), source)
        .skip_trivia()
        .take_while(|token| token.kind == SimpleTokenKind::RParen)
        .last()
    {
        if comment.start() < right_paren.start() {
            return CommentPlacement::trailing(attribute.value.as_ref(), comment);
        }
    }

    // If the comment precedes the `.`, treat it as trailing _if_ it's on the same line as the
    // value. For example, treat this as trailing:
    // ```python
    // (
    //     value  # comment
    //     .attribute
    // )
    // ```
    //
    // However, treat this as dangling:
    // ```python
    // (
    //     value
    //     # comment
    //     .attribute
    // )
    // ```
    if comment.line_position().is_end_of_line() {
        let dot_token = find_only_token_in_range(
            TextRange::new(attribute.value.end(), attribute.attr.start()),
            SimpleTokenKind::Dot,
            source,
        );
        if comment.end() < dot_token.start() {
            return CommentPlacement::trailing(attribute.value.as_ref(), comment);
        }
    }

    CommentPlacement::dangling(comment.enclosing_node(), comment)
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
    expr_if: &'a ast::ExprIf,
    source: &str,
) -> CommentPlacement<'a> {
    let ast::ExprIf {
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
        source,
    );
    // Between `if` and `test`
    if if_token.start() < comment.start() && comment.start() < test.start() {
        return CommentPlacement::leading(test.as_ref(), comment);
    }

    let else_token = find_only_token_in_range(
        TextRange::new(test.end(), orelse.start()),
        SimpleTokenKind::Else,
        source,
    );
    // Between `else` and `orelse`
    if else_token.start() < comment.start() && comment.start() < orelse.start() {
        return CommentPlacement::leading(orelse.as_ref(), comment);
    }

    CommentPlacement::Default(comment)
}

/// Handles trailing comments on between the `*` of a starred expression and the
/// expression itself. For example, attaches the first two comments here as leading
/// comments on the enclosing node, and the third to the `True` node.
/// ``` python
/// call(
///     *  # dangling end-of-line comment
///     # dangling own line comment
///     (  # leading comment on the expression
///        True
///     )
/// )
/// ```
fn handle_trailing_expression_starred_star_end_of_line_comment<'a>(
    comment: DecoratedComment<'a>,
    starred: &'a ast::ExprStarred,
    source: &str,
) -> CommentPlacement<'a> {
    if comment.following_node().is_some() {
        let tokenizer =
            SimpleTokenizer::new(source, TextRange::new(starred.start(), comment.start()));
        if !tokenizer
            .skip_trivia()
            .any(|token| token.kind() == SimpleTokenKind::LParen)
        {
            return CommentPlacement::leading(starred, comment);
        }
    }

    CommentPlacement::Default(comment)
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
/// ): ...
/// ```
fn handle_with_item_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
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
        source,
    );

    if comment.end() < as_token.start() {
        // If before the `as` keyword, then it must be a trailing comment of the context expression.
        CommentPlacement::trailing(context_expr, comment)
    } else if comment.line_position().is_end_of_line() {
        // Trailing end of line comment coming after the `as` keyword`.
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::leading(optional_vars, comment)
    }
}

/// Handles trailing comments between the class name and its arguments in:
/// ```python
/// case (
///     Pattern
///     # dangling
///     (...)
/// ): ...
/// ```
fn handle_pattern_match_class_comment<'a>(
    comment: DecoratedComment<'a>,
    class: &'a ast::PatternMatchClass,
) -> CommentPlacement<'a> {
    if class.cls.end() < comment.start() && comment.end() < class.arguments.start() {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handles trailing comments after the `as` keyword of a pattern match item:
///
/// ```python
/// case (
///     pattern
///     as # dangling end of line comment
///     # dangling own line comment
///     name
/// ): ...
/// ```
fn handle_pattern_match_as_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    debug_assert!(comment.enclosing_node().is_pattern_match_as());

    let Some(pattern) = comment.preceding_node() else {
        return CommentPlacement::Default(comment);
    };

    let mut tokens = SimpleTokenizer::starts_at(pattern.end(), source)
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);

    let Some(as_token) = tokens
        .next()
        .filter(|token| token.kind == SimpleTokenKind::As)
    else {
        return CommentPlacement::Default(comment);
    };

    if comment.end() < as_token.start() {
        // If before the `as` keyword, then it must be a trailing comment of the pattern.
        CommentPlacement::trailing(pattern, comment)
    } else {
        // Otherwise, must be a dangling comment. (Any comments that follow the name will be
        // trailing comments on the pattern match item, rather than enclosed by it.)
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    }
}

/// Handles dangling comments between the `*` token and identifier of a pattern match star:
///
/// ```python
/// case [
///   ...,
///   *  # dangling end of line comment
///   # dangling end of line comment
///   rest,
/// ]: ...
/// ```
fn handle_pattern_match_star_comment(comment: DecoratedComment) -> CommentPlacement {
    CommentPlacement::dangling(comment.enclosing_node(), comment)
}

/// Handles trailing comments after the `**` in a pattern match item. The comments can either
/// appear between the `**` and the identifier, or after the identifier (which is just an
/// identifier, not a node).
///
/// ```python
/// case {
///     **  # dangling end of line comment
///     # dangling own line comment
///     rest  # dangling end of line comment
///     # dangling own line comment
/// ): ...
/// ```
fn handle_pattern_match_mapping_comment<'a>(
    comment: DecoratedComment<'a>,
    pattern: &'a ast::PatternMatchMapping,
    source: &str,
) -> CommentPlacement<'a> {
    // The `**` has to come at the end, so there can't be another node after it. (The identifier,
    // like `rest` above, isn't a node.)
    if comment.following_node().is_some() {
        return CommentPlacement::Default(comment);
    }

    // If there's no rest pattern, no need to do anything special.
    let Some(rest) = pattern.rest.as_ref() else {
        return CommentPlacement::Default(comment);
    };

    // If the comment falls after the `**rest` entirely, treat it as dangling on the enclosing
    // node.
    if comment.start() > rest.end() {
        return CommentPlacement::dangling(comment.enclosing_node(), comment);
    }

    // Look at the tokens between the previous node (or the start of the pattern) and the comment.
    let preceding_end = match comment.preceding_node() {
        Some(preceding) => preceding.end(),
        None => comment.enclosing_node().start(),
    };
    let mut tokens =
        SimpleTokenizer::new(source, TextRange::new(preceding_end, comment.start())).skip_trivia();

    // If the remaining tokens from the previous node include `**`, mark as a dangling comment.
    if tokens.any(|token| token.kind == SimpleTokenKind::DoubleStar) {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handles comments around the `:=` token in a named expression (walrus operator).
///
/// For example, here, `# 1` and `# 2` will be marked as dangling comments on the named expression,
/// while `# 3` and `4` will be attached `y` (via our general parenthesized comment handling), and
/// `# 5` will be a trailing comment on the named expression.
///
/// ```python
/// if (
///     x
///     :=  # 1
///     # 2
///     (  # 3
///         y  # 4
///     ) # 5
/// ):
///     pass
/// ```
fn handle_named_expr_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    debug_assert!(comment.enclosing_node().is_expr_named());

    let (Some(target), Some(value)) = (comment.preceding_node(), comment.following_node()) else {
        return CommentPlacement::Default(comment);
    };

    let colon_equal = find_only_token_in_range(
        TextRange::new(target.end(), value.start()),
        SimpleTokenKind::ColonEqual,
        source,
    );

    if comment.end() < colon_equal.start() {
        // If the comment is before the `:=` token, then it must be a trailing comment of the
        // target.
        CommentPlacement::trailing(target, comment)
    } else {
        // Otherwise, treat it as dangling. We effectively treat it as a comment on the `:=` itself.
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    }
}

/// Handles comments around the `:` token in a lambda expression.
///
/// For parameterized lambdas, both the comments between the `lambda` and the parameters, and the
/// comments between the parameters and the body, are considered dangling, as is the case for all
/// of the following:
///
/// ```python
/// (
///     lambda  # 1
///     # 2
///     x
///     :  # 3
///     # 4
///     y
/// )
/// ```
///
/// For non-parameterized lambdas, all comments before the body are considered dangling, as is the
/// case for all of the following:
///
/// ```python
/// (
///     lambda  # 1
///     # 2
///     :  # 3
///     # 4
///     y
/// )
/// ```
fn handle_lambda_comment<'a>(
    comment: DecoratedComment<'a>,
    lambda: &'a ast::ExprLambda,
    source: &str,
) -> CommentPlacement<'a> {
    if let Some(parameters) = lambda.parameters.as_deref() {
        // Comments between the `lambda` and the parameters are dangling on the lambda:
        // ```python
        // (
        //     lambda  # comment
        //     x:
        //     y
        // )
        // ```
        if comment.start() < parameters.start() {
            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }

        // Comments between the parameters and the body are dangling on the lambda:
        // ```python
        // (
        //     lambda x:  # comment
        //     y
        // )
        // ```
        if parameters.end() < comment.start() && comment.start() < lambda.body.start() {
            // If the value is parenthesized, and the comment is within the parentheses, it should
            // be a leading comment on the value, not a dangling comment in the lambda, as in:
            // ```python
            // (
            //     lambda x:  (  # comment
            //         y
            //     )
            // )
            // ```
            let tokenizer =
                SimpleTokenizer::new(source, TextRange::new(parameters.end(), comment.start()));
            if tokenizer
                .skip_trivia()
                .any(|token| token.kind == SimpleTokenKind::LParen)
            {
                return CommentPlacement::Default(comment);
            }

            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }
    } else {
        // Comments between the lambda and the body are dangling on the lambda:
        // ```python
        // (
        //     lambda:  # comment
        //     y
        // )
        // ```
        if comment.start() < lambda.body.start() {
            // If the value is parenthesized, and the comment is within the parentheses, it should
            // be a leading comment on the value, not a dangling comment in the lambda, as in:
            // ```python
            // (
            //     lambda:  (  # comment
            //         y
            //     )
            // )
            // ```
            let tokenizer =
                SimpleTokenizer::new(source, TextRange::new(lambda.start(), comment.start()));
            if tokenizer
                .skip_trivia()
                .any(|token| token.kind == SimpleTokenKind::LParen)
            {
                return CommentPlacement::Default(comment);
            }

            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }
    }

    CommentPlacement::Default(comment)
}

/// Move comment between a unary op and its operand before the unary op by marking them as trailing.
///
/// For example, given:
/// ```python
/// (
///     not  # comment
///     True
/// )
/// ```
///
/// The `# comment` will be attached as a dangling comment on the enclosing node, to ensure that
/// it remains on the same line as the operator.
fn handle_unary_op_comment<'a>(
    comment: DecoratedComment<'a>,
    unary_op: &'a ast::ExprUnaryOp,
    source: &str,
) -> CommentPlacement<'a> {
    let mut tokenizer = SimpleTokenizer::new(
        source,
        TextRange::new(unary_op.start(), unary_op.operand.start()),
    )
    .skip_trivia();
    let op_token = tokenizer.next();
    debug_assert!(op_token.is_some_and(|token| matches!(
        token.kind,
        SimpleTokenKind::Tilde
            | SimpleTokenKind::Not
            | SimpleTokenKind::Plus
            | SimpleTokenKind::Minus
    )));
    let up_to = tokenizer
        .find(|token| token.kind == SimpleTokenKind::LParen)
        .map_or(unary_op.operand.start(), |lparen| lparen.start());
    if comment.end() < up_to {
        CommentPlacement::leading(unary_op, comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Attach an end-of-line comment immediately following an open bracket as a dangling comment on
/// enclosing node.
///
/// For example, given  the following function call:
/// ```python
/// foo(  # comment
///    bar,
/// )
/// ```
///
/// The comment will be attached to the [`Arguments`] node as a dangling comment, to ensure
/// that it remains on the same line as open parenthesis.
///
/// Similarly, given:
/// ```python
/// type foo[  # comment
///    bar,
/// ] = ...
/// ```
///
/// The comment will be attached to the [`TypeParams`] node as a dangling comment, to ensure
/// that it remains on the same line as open bracket.
fn handle_bracketed_end_of_line_comment<'a>(
    comment: DecoratedComment<'a>,
    source: &str,
) -> CommentPlacement<'a> {
    if comment.line_position().is_end_of_line() {
        // Ensure that there are no tokens between the open bracket and the comment.
        let mut lexer = SimpleTokenizer::new(
            source,
            TextRange::new(comment.enclosing_node().start(), comment.start()),
        )
        .skip_trivia();

        // Skip the opening parenthesis.
        let Some(paren) = lexer.next() else {
            return CommentPlacement::Default(comment);
        };
        debug_assert!(matches!(
            paren.kind(),
            SimpleTokenKind::LParen | SimpleTokenKind::LBrace | SimpleTokenKind::LBracket
        ));

        // If there are no additional tokens between the open parenthesis and the comment, then
        // it should be attached as a dangling comment on the brackets, rather than a leading
        // comment on the first argument.
        if lexer.next().is_none() {
            return CommentPlacement::dangling(comment.enclosing_node(), comment);
        }
    }

    CommentPlacement::Default(comment)
}

/// Attach an enclosed end-of-line comment to a [`ast::StmtImportFrom`].
///
/// For example, given:
/// ```python
/// from foo import (  # comment
///    bar,
/// )
/// ```
///
/// The comment will be attached to the [`ast::StmtImportFrom`] node as a dangling comment, to
/// ensure that it remains on the same line as the [`ast::StmtImportFrom`] itself.
fn handle_import_from_comment<'a>(
    comment: DecoratedComment<'a>,
    import_from: &'a ast::StmtImportFrom,
) -> CommentPlacement<'a> {
    // The comment needs to be on the same line, but before the first member. For example, we want
    // to treat this as a dangling comment:
    // ```python
    // from foo import (  # comment
    //     bar,
    //     baz,
    //     qux,
    // )
    // ```
    // However, this should _not_ be treated as a dangling comment:
    // ```python
    // from foo import (bar,  # comment
    //     baz,
    //     qux,
    // )
    // ```
    // Thus, we check whether the comment is an end-of-line comment _between_ the start of the
    // statement and the first member. If so, the only possible position is immediately following
    // the open parenthesis.
    if comment.line_position().is_end_of_line()
        && import_from.names.first().is_some_and(|first_name| {
            import_from.start() < comment.start() && comment.start() < first_name.start()
        })
    {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Attach an enclosed end-of-line comment to a [`ast::StmtWith`].
///
/// For example, given:
/// ```python
/// with ( # foo
///     CtxManager1() as example1,
///     CtxManager2() as example2,
///     CtxManager3() as example3,
/// ):
///     ...
/// ```
///
/// The comment will be attached to the [`ast::StmtWith`] node as a dangling comment, to ensure
/// that it remains on the same line as the [`ast::StmtWith`] itself.
fn handle_with_comment<'a>(
    comment: DecoratedComment<'a>,
    with_statement: &'a ast::StmtWith,
) -> CommentPlacement<'a> {
    if comment.line_position().is_end_of_line()
        && with_statement.items.first().is_some_and(|with_item| {
            with_statement.start() < comment.start() && comment.start() < with_item.start()
        })
    {
        CommentPlacement::dangling(comment.enclosing_node(), comment)
    } else {
        CommentPlacement::Default(comment)
    }
}

/// Handle comments inside comprehensions, e.g.
///
/// ```python
/// [
///      a
///      for  # dangling on the comprehension
///      b
///      # dangling on the comprehension
///      in  # dangling on comprehension.iter
///      # leading on the iter
///      c
///      # dangling on comprehension.if.n
///      if  # dangling on comprehension.if.n
///      d
/// ]
/// ```
fn handle_comprehension_comment<'a>(
    comment: DecoratedComment<'a>,
    comprehension: &'a Comprehension,
    source: &str,
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
    if comment.end() < comprehension.target.start() {
        return if is_own_line {
            // own line comments are correctly assigned as leading the target
            CommentPlacement::Default(comment)
        } else {
            // after the `for`
            CommentPlacement::dangling(comprehension, comment)
        };
    }

    let in_token = find_only_token_in_range(
        TextRange::new(comprehension.target.end(), comprehension.iter.start()),
        SimpleTokenKind::In,
        source,
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
    if comment.start() < in_token.start() {
        // attach as dangling comments on the target
        // (to be rendered as leading on the "in")
        return if is_own_line {
            CommentPlacement::dangling(comprehension, comment)
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
    if comment.start() < comprehension.iter.start() {
        return if is_own_line {
            CommentPlacement::Default(comment)
        } else {
            // after the `in` but same line, turn into trailing on the `in` token
            CommentPlacement::dangling(comprehension, comment)
        };
    }

    let mut last_end = comprehension.iter.end();

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
            TextRange::new(last_end, if_node.start()),
            SimpleTokenKind::If,
            source,
        );
        if is_own_line {
            if last_end < comment.start() && comment.start() < if_token.start() {
                return CommentPlacement::dangling(comprehension, comment);
            }
        } else if if_token.start() < comment.start() && comment.start() < if_node.start() {
            return CommentPlacement::dangling(comprehension, comment);
        }
        last_end = if_node.end();
    }

    CommentPlacement::Default(comment)
}

/// Handle end-of-line comments for parenthesized implicitly concatenated strings when used in
/// a `FormatStatementLastExpression` context:
///
/// ```python
/// a = (
///     "a"
///     "b"
///     "c"  # comment
/// )
/// ```
///
/// `# comment` is a trailing comment of the last part and not a trailing comment of the entire f-string.
/// Associating the comment with the last part is important or the assignment formatting might move
/// the comment at the end of the assignment, making it impossible to suppress an error for the last part.
///
/// On the other hand, `# comment` is a trailing end-of-line f-string comment for:
///
/// ```python
/// a = (
///     "a" "b" "c"  # comment
/// )
///
/// a = (
///     "a"
///     "b"
///     "c"
/// )  # comment
/// ```
///
/// Associating the comment with the f-string is desired in those cases because it allows
/// joining the string literals into a single string literal if it fits on the line.
fn handle_trailing_implicit_concatenated_string_comment<'a>(
    comment: DecoratedComment<'a>,
    comment_ranges: &CommentRanges,
    source: &str,
) -> CommentPlacement<'a> {
    if !comment.line_position().is_end_of_line() {
        return CommentPlacement::Default(comment);
    }

    let Some(string_like) = comment
        .preceding_node()
        .and_then(|preceding| StringLike::try_from(preceding).ok())
    else {
        return CommentPlacement::Default(comment);
    };

    let mut parts = string_like.parts();

    let (Some(last), Some(second_last)) = (parts.next_back(), parts.next_back()) else {
        return CommentPlacement::Default(comment);
    };

    if source.contains_line_break(TextRange::new(second_last.end(), last.start()))
        && is_expression_parenthesized(string_like.as_expression_ref(), comment_ranges, source)
    {
        let range = TextRange::new(last.end(), comment.start());

        if !SimpleTokenizer::new(source, range)
            .skip_trivia()
            .any(|token| token.kind() == SimpleTokenKind::RParen)
        {
            return CommentPlacement::trailing(AnyNodeRef::from(last), comment);
        }
    }

    CommentPlacement::Default(comment)
}

/// Returns `true` if the parameters are parenthesized (as in a function definition), or `false` if
/// not (as in a lambda).
fn are_parameters_parenthesized(parameters: &Parameters, contents: &str) -> bool {
    // A lambda never has parentheses around its parameters, but a function definition always does.
    contents[parameters.range()].starts_with('(')
}

/// Counts the number of empty lines in `contents`.
fn max_empty_lines(contents: &str) -> u32 {
    let mut newlines = 0u32;
    let mut max_new_lines = 0;

    for token in SimpleTokenizer::new(contents, TextRange::up_to(contents.text_len())) {
        match token.kind() {
            SimpleTokenKind::Newline => {
                newlines += 1;
            }

            SimpleTokenKind::Whitespace => {}

            SimpleTokenKind::Comment => {
                max_new_lines = newlines.max(max_new_lines);
                newlines = 0;
            }

            _ => {
                max_new_lines = newlines.max(max_new_lines);
                break;
            }
        }
    }

    max_new_lines = newlines.max(max_new_lines);
    max_new_lines.saturating_sub(1)
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
            max_empty_lines("# trailing comment\n\n# own line comment\n\n\n# an other own line comment\n# block"),
            2
        );

        assert_eq!(
            max_empty_lines(
                r"# This multiline comments section
# should be split from the statement
# above by two lines.
"
            ),
            0
        );
    }
}

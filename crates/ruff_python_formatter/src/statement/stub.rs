//! Spacing around groups of same-name decorated functions in stub files.

use ruff_formatter::{FormatContext, write};
use ruff_python_ast::{Stmt, StmtIf};
use ruff_python_trivia::{lines_after, lines_before};
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;

use crate::comments::format::format_comment;
use crate::comments::{has_skip_comment, leading_comments};
use crate::prelude::*;
use crate::preview::is_pyi_overload_group_blank_lines_enabled;
use crate::statement::suite::SuiteKind;

fn decorated_name<'a>(statement: &'a Stmt, context: &PyFormatContext) -> Option<&'a str> {
    let function = statement.as_function_def_stmt()?;
    if function.decorator_list.is_empty()
        || has_skip_comment(context.comments().trailing(statement), context.source())
        || function.body.last().is_some_and(|last| {
            has_skip_comment(context.comments().trailing(last), context.source())
        })
    {
        return None;
    }
    Some(function.name.as_str())
}

fn first_name<'a>(statement: &'a Stmt, context: &PyFormatContext) -> Option<&'a str> {
    if let Stmt::If(statement) = statement {
        decorated_name(statement.body.first()?, context)
    } else {
        decorated_name(statement, context)
    }
}

fn last_name<'a>(statement: &'a Stmt, context: &PyFormatContext) -> Option<&'a str> {
    if let Stmt::If(statement) = statement {
        let body = statement
            .elif_else_clauses
            .last()
            .map_or(&statement.body, |clause| &clause.body);
        decorated_name(body.last()?, context)
    } else {
        decorated_name(statement, context)
    }
}

fn conditional_group(statement: &StmtIf, context: &PyFormatContext) -> bool {
    let Some(name) = statement
        .body
        .first()
        .and_then(|first| decorated_name(first, context))
    else {
        return false;
    };
    statement.elif_else_clauses.iter().any(|clause| {
        clause
            .body
            .first()
            .and_then(|first| decorated_name(first, context))
            == Some(name)
    })
}

pub(super) fn conditional_groups(
    statement: &StmtIf,
    preceding: Option<&Stmt>,
    following: Option<&Stmt>,
    context: &PyFormatContext,
) -> Vec<bool> {
    let names: Vec<_> = std::iter::once(&statement.body)
        .chain(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| &clause.body),
        )
        .map(|body| {
            body.first()
                .and_then(|first| decorated_name(first, context))
        })
        .collect();
    let preceding = preceding.and_then(|statement| decorated_name(statement, context));
    let following = following.and_then(|statement| decorated_name(statement, context));
    let mut counts = FxHashMap::default();
    for name in names.iter().flatten() {
        *counts.entry(*name).or_insert(0usize) += 1;
    }
    names
        .into_iter()
        .map(|name| {
            name.is_some_and(|name| {
                Some(name) == preceding || Some(name) == following || counts[name] > 1
            })
        })
        .collect()
}

fn in_group(statement: &Stmt, statements: &[Stmt], context: &PyFormatContext) -> bool {
    let Ok(index) = statements.binary_search_by_key(&statement.start(), Ranged::start) else {
        return false;
    };
    let Some(name) = first_name(statement, context) else {
        return false;
    };
    index
        .checked_sub(1)
        .is_some_and(|previous| last_name(&statements[previous], context) == Some(name))
        || statements
            .get(index + 1)
            .is_some_and(|next| first_name(next, context) == Some(name))
        || matches!(statement, Stmt::If(statement) if conditional_group(statement, context))
}

fn enabled(context: &PyFormatContext) -> bool {
    context.options().source_type().is_stub() && is_pyi_overload_group_blank_lines_enabled(context)
}

/// `Some(true)` requires an empty line; `Some(false)` keeps declarations adjacent.
pub(super) fn group_spacing(
    preceding: &Stmt,
    following: &Stmt,
    statements: &[Stmt],
    context: &PyFormatContext,
) -> Option<bool> {
    if !enabled(context) {
        return None;
    }

    if let Some(name) = decorated_name(preceding, context)
        && first_name(following, context) == Some(name)
    {
        // Keep a user blank line above an intervening comment, but remove gaps
        // between the declarations themselves.
        return Some(
            context
                .comments()
                .leading(following)
                .first()
                .is_some_and(|comment| lines_before(comment.start(), context.source()) > 1)
                || (following.is_if_stmt()
                    && context
                        .comments()
                        .trailing(preceding)
                        .last()
                        .is_some_and(|comment| lines_after(comment.end(), context.source()) > 1)),
        );
    }

    let after_conditional_group = if let Stmt::If(conditional) = preceding {
        let body = conditional
            .elif_else_clauses
            .last()
            .map_or(&conditional.body, |clause| &clause.body);
        if let [.., previous, last] = body.as_slice() {
            decorated_name(previous, context)
                .is_some_and(|name| decorated_name(last, context) == Some(name))
        } else {
            false
        }
    } else {
        false
    };
    let before_group = in_group(following, statements, context)
        && (!following.is_if_stmt() || preceding.is_function_def_stmt());

    (in_group(preceding, statements, context) || after_conditional_group || before_group)
        .then_some(true)
}

pub(super) fn format_statement(
    statement: &Stmt,
    statements: &[Stmt],
    kind: SuiteKind,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    if !enabled(f.context()) {
        return statement.format().fmt(f);
    }

    if let Stmt::If(conditional) = statement {
        let index = statements
            .binary_search_by_key(&statement.start(), Ranged::start)
            .unwrap();
        let groups = conditional_groups(
            conditional,
            index.checked_sub(1).map(|index| &statements[index]),
            statements.get(index + 1),
            f.context(),
        );
        return conditional.format().with_options(groups).fmt(f);
    }

    if decorated_name(statement, f.context()).is_some() {
        let index = statements
            .binary_search_by_key(&statement.start(), Ranged::start)
            .unwrap();
        let continues_group = if index == 0 {
            kind.is_stub_overload_group()
        } else {
            decorated_name(&statements[index - 1], f.context())
                == decorated_name(statement, f.context())
        };
        if continues_group {
            let comments = f.context().comments().clone();
            let leading = comments.leading(statement);
            if index == 0
                && leading
                    .first()
                    .is_some_and(|comment| lines_before(comment.start(), f.context().source()) > 1)
            {
                empty_line().fmt(f)?;
            }
            if let Some((last, rest)) = leading.split_last() {
                leading_comments(rest).fmt(f)?;
                write!(f, [format_comment(last), hard_line_break()])?;
                last.mark_formatted();
            }
        }

        if let Stmt::FunctionDef(function) = statement {
            let continues = statements.get(index + 1).is_some_and(|next| {
                first_name(next, f.context()) == decorated_name(statement, f.context())
            });
            return function.format().with_options(continues).fmt(f);
        }
    }

    statement.format().fmt(f)
}

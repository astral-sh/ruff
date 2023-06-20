//! Interface for generating autofix edits from higher-level actions (e.g., "remove an argument").
use anyhow::{bail, Result};
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::{self, ExceptHandler, Expr, Keyword, Ranged, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers;
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};
use ruff_python_whitespace::{is_python_whitespace, NewlineWithTrailingNewline, PythonWhitespace};

use crate::autofix::codemods;

/// Return the `Fix` to use when deleting a `Stmt`.
///
/// In some cases, this is as simple as deleting the `Range` of the `Stmt`
/// itself. However, there are a few exceptions:
/// - If the `Stmt` is _not_ the terminal statement in a multi-statement line,
///   we need to delete up to the start of the next statement (and avoid
///   deleting any content that precedes the statement).
/// - If the `Stmt` is the terminal statement in a multi-statement line, we need
///   to avoid deleting any content that precedes the statement.
/// - If the `Stmt` has no trailing and leading content, then it's convenient to
///   remove the entire start and end lines.
/// - If the `Stmt` is the last statement in its parent body, replace it with a
///   `pass` instead.
pub(crate) fn delete_stmt(
    stmt: &Stmt,
    parent: Option<&Stmt>,
    locator: &Locator,
    indexer: &Indexer,
) -> Edit {
    if parent
        .map(|parent| is_lone_child(stmt, parent))
        .unwrap_or_default()
    {
        // If removing this node would lead to an invalid syntax tree, replace
        // it with a `pass`.
        Edit::range_replacement("pass".to_string(), stmt.range())
    } else {
        if let Some(semicolon) = trailing_semicolon(stmt.end(), locator) {
            let next = next_stmt_break(semicolon, locator);
            Edit::deletion(stmt.start(), next)
        } else if helpers::has_leading_content(stmt.start(), locator) {
            Edit::range_deletion(stmt.range())
        } else if let Some(start) =
            helpers::preceded_by_continuations(stmt.start(), locator, indexer)
        {
            Edit::range_deletion(TextRange::new(start, stmt.end()))
        } else {
            let range = locator.full_lines_range(stmt.range());
            Edit::range_deletion(range)
        }
    }
}

/// Generate a `Fix` to remove the specified imports from an `import` statement.
pub(crate) fn remove_unused_imports<'a>(
    unused_imports: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
) -> Result<Edit> {
    match codemods::remove_imports(unused_imports, stmt, locator, stylist)? {
        None => Ok(delete_stmt(stmt, parent, locator, indexer)),
        Some(content) => Ok(Edit::range_replacement(content, stmt.range())),
    }
}

/// Generic function to remove arguments or keyword arguments in function
/// calls and class definitions. (For classes `args` should be considered
/// `bases`)
///
/// Supports the removal of parentheses when this is the only (kw)arg left.
/// For this behavior, set `remove_parentheses` to `true`.
pub(crate) fn remove_argument(
    locator: &Locator,
    call_at: TextSize,
    expr_range: TextRange,
    args: &[Expr],
    keywords: &[Keyword],
    remove_parentheses: bool,
) -> Result<Edit> {
    // TODO(sbrugman): Preserve trailing comments.
    let contents = locator.after(call_at);

    let mut fix_start = None;
    let mut fix_end = None;

    let n_arguments = keywords.len() + args.len();
    if n_arguments == 0 {
        bail!("No arguments or keywords to remove");
    }

    if n_arguments == 1 {
        // Case 1: there is only one argument.
        let mut count = 0u32;
        for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, call_at).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(if remove_parentheses {
                        range.start()
                    } else {
                        range.start() + TextSize::from(1)
                    });
                }
                count = count.saturating_add(1);
            }

            if matches!(tok, Tok::Rpar) {
                count = count.saturating_sub(1);
                if count == 0 {
                    fix_end = Some(if remove_parentheses {
                        range.end()
                    } else {
                        range.end() - TextSize::from(1)
                    });
                    break;
                }
            }
        }
    } else if args
        .iter()
        .map(Expr::start)
        .chain(keywords.iter().map(Keyword::start))
        .any(|location| location > expr_range.start())
    {
        // Case 2: argument or keyword is _not_ the last node.
        let mut seen_comma = false;
        for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, call_at).flatten() {
            if seen_comma {
                if matches!(tok, Tok::NonLogicalNewline) {
                    // Also delete any non-logical newlines after the comma.
                    continue;
                }
                fix_end = Some(if matches!(tok, Tok::Newline) {
                    range.end()
                } else {
                    range.start()
                });
                break;
            }
            if range.start() == expr_range.start() {
                fix_start = Some(range.start());
            }
            if fix_start.is_some() && matches!(tok, Tok::Comma) {
                seen_comma = true;
            }
        }
    } else {
        // Case 3: argument or keyword is the last node, so we have to find the last
        // comma in the stmt.
        for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, call_at).flatten() {
            if range.start() == expr_range.start() {
                fix_end = Some(expr_range.end());
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(range.start());
            }
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Edit::deletion(start, end)),
        _ => {
            bail!("No fix could be constructed")
        }
    }
}

/// Determine if a vector contains only one, specific element.
fn is_only<T: PartialEq>(vec: &[T], value: &T) -> bool {
    vec.len() == 1 && vec[0] == *value
}

/// Determine if a child is the only statement in its body.
fn is_lone_child(child: &Stmt, parent: &Stmt) -> bool {
    match parent {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
        | Stmt::ClassDef(ast::StmtClassDef { body, .. })
        | Stmt::With(ast::StmtWith { body, .. })
        | Stmt::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
            if is_only(body, child) {
                return true;
            }
        }
        Stmt::For(ast::StmtFor { body, orelse, .. })
        | Stmt::AsyncFor(ast::StmtAsyncFor { body, orelse, .. })
        | Stmt::While(ast::StmtWhile { body, orelse, .. })
        | Stmt::If(ast::StmtIf { body, orelse, .. }) => {
            if is_only(body, child) || is_only(orelse, child) {
                return true;
            }
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            range: _,
        })
        | Stmt::TryStar(ast::StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
            range: _,
        }) => {
            if is_only(body, child)
                || is_only(orelse, child)
                || is_only(finalbody, child)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => is_only(body, child),
                })
            {
                return true;
            }
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => {
            if cases.iter().any(|case| is_only(&case.body, child)) {
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Return the location of a trailing semicolon following a `Stmt`, if it's part
/// of a multi-statement line.
fn trailing_semicolon(offset: TextSize, locator: &Locator) -> Option<TextSize> {
    let contents = locator.after(offset);

    for line in NewlineWithTrailingNewline::from(contents) {
        let trimmed = line.trim_whitespace_start();

        if trimmed.starts_with(';') {
            let colon_offset = line.text_len() - trimmed.text_len();
            return Some(offset + line.start() + colon_offset);
        }

        if !trimmed.starts_with('\\') {
            break;
        }
    }
    None
}

/// Find the next valid break for a `Stmt` after a semicolon.
fn next_stmt_break(semicolon: TextSize, locator: &Locator) -> TextSize {
    let start_location = semicolon + TextSize::from(1);

    let contents = &locator.contents()[usize::from(start_location)..];
    for line in NewlineWithTrailingNewline::from(contents) {
        let trimmed = line.trim_whitespace();
        // Skip past any continuations.
        if trimmed.starts_with('\\') {
            continue;
        }

        return start_location
            + if trimmed.is_empty() {
                // If the line is empty, then despite the previous statement ending in a
                // semicolon, we know that it's not a multi-statement line.
                line.start()
            } else {
                // Otherwise, find the start of the next statement. (Or, anything that isn't
                // whitespace.)
                let relative_offset = line.find(|c: char| !is_python_whitespace(c)).unwrap();
                line.start() + TextSize::try_from(relative_offset).unwrap()
            };
    }

    locator.line_end(start_location)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::TextSize;
    use rustpython_parser::ast::{Ranged, Suite};
    use rustpython_parser::Parse;

    use ruff_python_ast::source_code::Locator;

    use crate::autofix::edits::{next_stmt_break, trailing_semicolon};

    #[test]
    fn find_semicolon() -> Result<()> {
        let contents = "x = 1";
        let program = Suite::parse(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt.end(), &locator), None);

        let contents = "x = 1; y = 1";
        let program = Suite::parse(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt.end(), &locator),
            Some(TextSize::from(5))
        );

        let contents = "x = 1 ; y = 1";
        let program = Suite::parse(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt.end(), &locator),
            Some(TextSize::from(6))
        );

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let program = Suite::parse(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt.end(), &locator),
            Some(TextSize::from(10))
        );

        Ok(())
    }

    #[test]
    fn find_next_stmt_break() {
        let contents = "x = 1; y = 1";
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(4), &locator),
            TextSize::from(5)
        );

        let contents = "x = 1 ; y = 1";
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(5), &locator),
            TextSize::from(6)
        );

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(10), &locator),
            TextSize::from(12)
        );
    }
}

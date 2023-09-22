//! Interface for generating autofix edits from higher-level actions (e.g., "remove an argument").

use anyhow::{Context, Result};

use ruff_diagnostics::Edit;
use ruff_python_ast::{self as ast, Arguments, ExceptHandler, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_trivia::{
    has_leading_content, is_python_whitespace, PythonWhitespace, SimpleTokenKind, SimpleTokenizer,
};
use ruff_source_file::{Locator, NewlineWithTrailingNewline};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

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
        } else if has_leading_content(stmt.start(), locator) {
            Edit::range_deletion(stmt.range())
        } else if let Some(start) = indexer.preceded_by_continuations(stmt.start(), locator) {
            Edit::deletion(start, stmt.end())
        } else {
            let range = locator.full_lines_range(stmt.range());
            Edit::range_deletion(range)
        }
    }
}

/// Generate a `Fix` to remove the specified imports from an `import` statement.
pub(crate) fn remove_unused_imports<'a>(
    member_names: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
) -> Result<Edit> {
    match codemods::remove_imports(member_names, stmt, locator, stylist)? {
        None => Ok(delete_stmt(stmt, parent, locator, indexer)),
        Some(content) => Ok(Edit::range_replacement(content, stmt.range())),
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum Parentheses {
    /// Remove parentheses, if the removed argument is the only argument left.
    Remove,
    /// Preserve parentheses, even if the removed argument is the only argument
    Preserve,
}

/// Generic function to remove arguments or keyword arguments in function
/// calls and class definitions. (For classes `args` should be considered
/// `bases`)
///
/// Supports the removal of parentheses when this is the only (kw)arg left.
/// For this behavior, set `remove_parentheses` to `true`.
pub(crate) fn remove_argument<T: Ranged>(
    argument: &T,
    arguments: &Arguments,
    parentheses: Parentheses,
    source: &str,
) -> Result<Edit> {
    // Partition into arguments before and after the argument to remove.
    let (before, after): (Vec<_>, Vec<_>) = arguments
        .arguments_source_order()
        .map(|arg| arg.range())
        .filter(|range| argument.range() != *range)
        .partition(|range| range.start() < argument.start());

    if !after.is_empty() {
        // Case 1: argument or keyword is _not_ the last node, so delete from the start of the
        // argument to the end of the subsequent comma.
        let mut tokenizer = SimpleTokenizer::starts_at(argument.end(), source);

        // Find the trailing comma.
        tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        // Find the next non-whitespace token.
        let next = tokenizer
            .find(|token| {
                token.kind != SimpleTokenKind::Whitespace && token.kind != SimpleTokenKind::Newline
            })
            .context("Unable to find next token")?;

        Ok(Edit::deletion(argument.start(), next.start()))
    } else if let Some(previous) = before.iter().map(Ranged::end).max() {
        // Case 2: argument or keyword is the last node, so delete from the start of the
        // previous comma to the end of the argument.
        let mut tokenizer = SimpleTokenizer::starts_at(previous, source);

        // Find the trailing comma.
        let comma = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        Ok(Edit::deletion(comma.start(), argument.end()))
    } else {
        // Case 3: argument or keyword is the only node, so delete the arguments (but preserve
        // parentheses, if needed).
        Ok(match parentheses {
            Parentheses::Remove => Edit::range_deletion(arguments.range()),
            Parentheses::Preserve => Edit::range_replacement("()".to_string(), arguments.range()),
        })
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
        | Stmt::ClassDef(ast::StmtClassDef { body, .. })
        | Stmt::With(ast::StmtWith { body, .. }) => {
            if is_only(body, child) {
                return true;
            }
        }
        Stmt::For(ast::StmtFor { body, orelse, .. })
        | Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            if is_only(body, child) || is_only(orelse, child) {
                return true;
            }
        }
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            if is_only(body, child)
                || elif_else_clauses
                    .iter()
                    .any(|ast::ElifElseClause { body, .. }| is_only(body, child))
            {
                return true;
            }
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
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

    for line in
        NewlineWithTrailingNewline::with_offset(locator.after(start_location), start_location)
    {
        let trimmed = line.trim_whitespace();
        // Skip past any continuations.
        if trimmed.starts_with('\\') {
            continue;
        }

        return if trimmed.is_empty() {
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

/// Add leading whitespace to a snippet, if it's immediately preceded an identifier or keyword.
pub(crate) fn pad_start(mut content: String, start: TextSize, locator: &Locator) -> String {
    // Ex) When converting `except(ValueError,)` from a tuple to a single argument, we need to
    // insert a space before the fix, to achieve `except ValueError`.
    if locator
        .up_to(start)
        .chars()
        .last()
        .is_some_and(|char| char.is_ascii_alphabetic())
    {
        content.insert(0, ' ');
    }
    content
}

/// Add trailing whitespace to a snippet, if it's immediately followed by an identifier or keyword.
pub(crate) fn pad_end(mut content: String, end: TextSize, locator: &Locator) -> String {
    if locator
        .after(end)
        .chars()
        .next()
        .is_some_and(|char| char.is_ascii_alphabetic())
    {
        content.push(' ');
    }
    content
}

/// Add leading or trailing whitespace to a snippet, if it's immediately preceded or followed by
/// an identifier or keyword.
pub(crate) fn pad(content: String, range: TextRange, locator: &Locator) -> String {
    pad_start(
        pad_end(content, range.end(), locator),
        range.start(),
        locator,
    )
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_parser::parse_suite;
    use ruff_source_file::Locator;
    use ruff_text_size::{Ranged, TextSize};

    use crate::autofix::edits::{next_stmt_break, trailing_semicolon};

    #[test]
    fn find_semicolon() -> Result<()> {
        let contents = "x = 1";
        let program = parse_suite(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt.end(), &locator), None);

        let contents = "x = 1; y = 1";
        let program = parse_suite(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt.end(), &locator),
            Some(TextSize::from(5))
        );

        let contents = "x = 1 ; y = 1";
        let program = parse_suite(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt.end(), &locator),
            Some(TextSize::from(6))
        );

        let contents = r"
x = 1 \
  ; y = 1
"
        .trim();
        let program = parse_suite(contents, "<filename>")?;
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

        let contents = r"
x = 1 \
  ; y = 1
"
        .trim();
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(10), &locator),
            TextSize::from(12)
        );
    }
}

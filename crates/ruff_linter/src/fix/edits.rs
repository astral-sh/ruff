//! Interface for generating fix edits from higher-level actions (e.g., "remove an argument").

use anyhow::{Context, Result};

use ruff_diagnostics::Edit;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Arguments, ExceptHandler, Expr, ExprList, Parameters, Stmt};
use ruff_python_ast::{AnyNodeRef, ArgOrKeyword};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_trivia::textwrap::dedent_to;
use ruff_python_trivia::{
    has_leading_content, is_python_whitespace, CommentRanges, PythonWhitespace, SimpleTokenKind,
    SimpleTokenizer,
};
use ruff_source_file::{Locator, NewlineWithTrailingNewline, UniversalNewlines};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::cst::matchers::{match_function_def, match_indented_block, match_statement};
use crate::fix::codemods;
use crate::fix::codemods::CodegenStylist;
use crate::line_width::{IndentWidth, LineLength, LineWidthBuilder};

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
    if parent.is_some_and(|parent| is_lone_child(stmt, parent)) {
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

/// Generate a [`Edit`] to delete a comment (for example: a `noqa` directive).
pub(crate) fn delete_comment(range: TextRange, locator: &Locator) -> Edit {
    let line_range = locator.line_range(range.start());

    // Compute the leading space.
    let prefix = locator.slice(TextRange::new(line_range.start(), range.start()));
    let leading_space_len = prefix.text_len() - prefix.trim_whitespace_end().text_len();

    // Compute the trailing space.
    let suffix = locator.slice(TextRange::new(range.end(), line_range.end()));
    let trailing_space_len = suffix.text_len() - suffix.trim_whitespace_start().text_len();

    // Ex) `# noqa`
    if line_range
        == TextRange::new(
            range.start() - leading_space_len,
            range.end() + trailing_space_len,
        )
    {
        let full_line_end = locator.full_line_end(line_range.end());
        Edit::deletion(line_range.start(), full_line_end)
    }
    // Ex) `x = 1  # noqa`
    else if range.end() + trailing_space_len == line_range.end() {
        // Replace `x = 1  # noqa` with `x = 1`.
        Edit::deletion(range.start() - leading_space_len, line_range.end())
    }
    // Ex) `x = 1  # noqa  # type: ignore`
    else if locator
        .slice(TextRange::new(
            range.end() + trailing_space_len,
            line_range.end(),
        ))
        .starts_with('#')
    {
        // Replace `# noqa  # type: ignore` with `# type: ignore`.
        Edit::deletion(range.start(), range.end() + trailing_space_len)
    }
    // Ex) `x = 1  # noqa here`
    else {
        // Remove `# noqa here` and whitespace
        Edit::deletion(range.start() - leading_space_len, line_range.end())
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

/// Edits to make the specified imports explicit, e.g. change `import x` to `import x as x`.
pub(crate) fn make_redundant_alias<'a>(
    member_names: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
) -> Vec<Edit> {
    let aliases = match stmt {
        Stmt::Import(ast::StmtImport { names, .. }) => names,
        Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => names,
        _ => {
            return Vec::new();
        }
    };
    member_names
        .filter_map(|name| {
            aliases
                .iter()
                .find(|alias| alias.asname.is_none() && *name == alias.name.id)
                .map(|alias| Edit::range_replacement(format!("{name} as {name}"), alias.range))
        })
        .collect()
}

/// Fix to add the specified imports to the `__all__` export list.
pub(crate) fn add_to_dunder_all<'a>(
    names: impl Iterator<Item = &'a str>,
    expr: &Expr,
    stylist: &Stylist,
) -> Vec<Edit> {
    let (insertion_point, export_prefix_length) = match expr {
        Expr::List(ExprList { elts, .. }) => (
            elts.last().map_or(expr.end() - "]".text_len(), Ranged::end),
            elts.len(),
        ),
        Expr::Tuple(tup) if tup.parenthesized => (
            tup.elts
                .last()
                .map_or(tup.end() - ")".text_len(), Ranged::end),
            tup.len(),
        ),
        Expr::Tuple(tup) if !tup.parenthesized => (
            tup.elts
                .last()
                .expect("unparenthesized empty tuple is not possible")
                .range()
                .end(),
            tup.len(),
        ),
        _ => {
            // we don't know how to insert into this expression
            return vec![];
        }
    };
    let quote = stylist.quote();
    let mut edits: Vec<_> = names
        .enumerate()
        .map(|(offset, name)| match export_prefix_length + offset {
            0 => Edit::insertion(format!("{quote}{name}{quote}"), insertion_point),
            _ => Edit::insertion(format!(", {quote}{name}{quote}"), insertion_point),
        })
        .collect();
    if let Expr::Tuple(tup) = expr {
        if tup.parenthesized && export_prefix_length + edits.len() == 1 {
            edits.push(Edit::insertion(",".to_string(), insertion_point));
        }
    }
    edits
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum Parentheses {
    /// Remove parentheses, if the removed argument is the only argument left.
    Remove,
    /// Preserve parentheses, even if the removed argument is the only argument
    Preserve,
}

/// Generic function to remove arguments or keyword arguments in function
/// calls and class definitions. (For classes, `args` should be considered
/// `bases`.)
///
/// Supports the removal of parentheses when this is the only (kw)arg left.
/// For this behavior, set `parentheses` to `Parentheses::Remove`.
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

/// Generic function to add arguments or keyword arguments to function calls.
pub(crate) fn add_argument(
    argument: &str,
    arguments: &Arguments,
    comment_ranges: &CommentRanges,
    source: &str,
) -> Edit {
    if let Some(last) = arguments.arguments_source_order().last() {
        // Case 1: existing arguments, so append after the last argument.
        let last = parenthesized_range(
            match last {
                ArgOrKeyword::Arg(arg) => arg.into(),
                ArgOrKeyword::Keyword(keyword) => (&keyword.value).into(),
            },
            arguments.into(),
            comment_ranges,
            source,
        )
        .unwrap_or(last.range());
        Edit::insertion(format!(", {argument}"), last.end())
    } else {
        // Case 2: no arguments. Add argument, without any trailing comma.
        Edit::insertion(argument.to_string(), arguments.start() + TextSize::from(1))
    }
}

/// Generic function to add a (regular) parameter to a function definition.
pub(crate) fn add_parameter(parameter: &str, parameters: &Parameters, source: &str) -> Edit {
    if let Some(last) = parameters
        .args
        .iter()
        .filter(|arg| arg.default.is_none())
        .last()
    {
        // Case 1: at least one regular parameter, so append after the last one.
        Edit::insertion(format!(", {parameter}"), last.end())
    } else if parameters.args.first().is_some() {
        // Case 2: no regular parameters, but at least one keyword parameter, so add before the
        // first.
        let pos = parameters.start();
        let mut tokenizer = SimpleTokenizer::starts_at(pos, source);
        let name = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Name)
            .expect("Unable to find name token");
        Edit::insertion(format!("{parameter}, "), name.start())
    } else if let Some(last) = parameters.posonlyargs.last() {
        // Case 2: no regular parameter, but a positional-only parameter exists, so add after that.
        // We take care to add it *after* the `/` separator.
        let pos = last.end();
        let mut tokenizer = SimpleTokenizer::starts_at(pos, source);
        let slash = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Slash)
            .expect("Unable to find `/` token");
        // Try to find a comma after the slash.
        let comma = tokenizer.find(|token| token.kind == SimpleTokenKind::Comma);
        if let Some(comma) = comma {
            Edit::insertion(format!(" {parameter},"), comma.start() + TextSize::from(1))
        } else {
            Edit::insertion(format!(", {parameter}"), slash.start())
        }
    } else if parameters.kwonlyargs.first().is_some() {
        // Case 3: no regular parameter, but a keyword-only parameter exist, so add parameter before that.
        // We need to backtrack to before the `*` separator.
        // We know there is no non-keyword-only params, so we can safely assume that the `*` separator is the first
        let pos = parameters.start();
        let mut tokenizer = SimpleTokenizer::starts_at(pos, source);
        let star = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Star)
            .expect("Unable to find `*` token");
        Edit::insertion(format!("{parameter}, "), star.start())
    } else {
        // Case 4: no parameters at all, so add parameter after the opening parenthesis.
        Edit::insertion(
            parameter.to_string(),
            parameters.start() + TextSize::from(1),
        )
    }
}

/// Safely adjust the indentation of the indented block at [`TextRange`].
///
/// The [`TextRange`] is assumed to represent an entire indented block, including the leading
/// indentation of that block. For example, to dedent the body here:
/// ```python
/// if True:
///     print("Hello, world!")
/// ```
///
/// The range would be the entirety of `    print("Hello, world!")`.
pub(crate) fn adjust_indentation(
    range: TextRange,
    indentation: &str,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<String> {
    let contents = locator.slice(range);

    // If the range includes a multi-line string, use LibCST to ensure that we don't adjust the
    // whitespace _within_ the string.
    let contains_multiline_string =
        indexer.multiline_ranges().intersects(range) || indexer.fstring_ranges().intersects(range);

    // If the range has mixed indentation, we will use LibCST as well.
    let mixed_indentation = contents.universal_newlines().any(|line| {
        let trimmed = line.trim_whitespace_start();
        if trimmed.is_empty() {
            return false;
        }

        let line_indentation: &str = &line[..line.len() - trimmed.len()];
        line_indentation.contains('\t') && line_indentation.contains(' ')
    });

    // For simple cases, try to do a manual dedent.
    if !contains_multiline_string && !mixed_indentation {
        if let Some(dedent) = dedent_to(contents, indentation) {
            return Ok(dedent);
        }
    }

    let module_text = format!("def f():{}{contents}", stylist.line_ending().as_str());

    let mut tree = match_statement(&module_text)?;

    let embedding = match_function_def(&mut tree)?;

    let indented_block = match_indented_block(&mut embedding.body)?;
    indented_block.indent = Some(indentation);

    let module_text = indented_block.codegen_stylist(stylist);
    let module_text = module_text
        .strip_prefix(stylist.line_ending().as_str())
        .unwrap()
        .to_string();
    Ok(module_text)
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

/// Returns `true` if the fix fits within the maximum configured line length.
pub(crate) fn fits(
    fix: &str,
    node: AnyNodeRef,
    locator: &Locator,
    line_length: LineLength,
    tab_size: IndentWidth,
) -> bool {
    all_lines_fit(fix, node, locator, line_length.value() as usize, tab_size)
}

/// Returns `true` if all lines in the fix are shorter than the given line length.
fn all_lines_fit(
    fix: &str,
    node: AnyNodeRef,
    locator: &Locator,
    line_length: usize,
    tab_size: IndentWidth,
) -> bool {
    let prefix = locator.slice(TextRange::new(
        locator.line_start(node.start()),
        node.start(),
    ));

    // Ensure that all lines are shorter than the line length limit.
    fix.universal_newlines().enumerate().all(|(idx, line)| {
        // If `template` is a multiline string, `col_offset` should only be applied to the first
        // line:
        // ```
        // a = """{}        -> offset = col_offset (= 4)
        // {}               -> offset = 0
        // """.format(0, 1) -> offset = 0
        // ```
        let measured_length = if idx == 0 {
            LineWidthBuilder::new(tab_size)
                .add_str(prefix)
                .add_str(&line)
                .get()
        } else {
            LineWidthBuilder::new(tab_size).add_str(&line).get()
        };

        measured_length <= line_length
    })
}

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};
    use test_case::test_case;

    use ruff_diagnostics::{Diagnostic, Edit, Fix};
    use ruff_python_ast::Stmt;
    use ruff_python_codegen::Stylist;
    use ruff_python_parser::{parse_expression, parse_module};
    use ruff_source_file::Locator;
    use ruff_text_size::{Ranged, TextRange, TextSize};

    use crate::fix::apply_fixes;
    use crate::fix::edits::{
        add_to_dunder_all, make_redundant_alias, next_stmt_break, trailing_semicolon,
    };

    /// Parse the given source using [`Mode::Module`] and return the first statement.
    fn parse_first_stmt(source: &str) -> Result<Stmt> {
        let suite = parse_module(source)?.into_suite();
        Ok(suite.into_iter().next().unwrap())
    }

    #[test]
    fn find_semicolon() -> Result<()> {
        let contents = "x = 1";
        let stmt = parse_first_stmt(contents)?;
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt.end(), &locator), None);

        let contents = "x = 1; y = 1";
        let stmt = parse_first_stmt(contents)?;
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt.end(), &locator),
            Some(TextSize::from(5))
        );

        let contents = "x = 1 ; y = 1";
        let stmt = parse_first_stmt(contents)?;
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
        let stmt = parse_first_stmt(contents)?;
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

    #[test]
    fn redundant_alias() -> Result<()> {
        let contents = "import x, y as y, z as bees";
        let stmt = parse_first_stmt(contents)?;
        assert_eq!(
            make_redundant_alias(["x"].into_iter(), &stmt),
            vec![Edit::range_replacement(
                String::from("x as x"),
                TextRange::new(TextSize::new(7), TextSize::new(8)),
            )],
            "make just one item redundant"
        );
        assert_eq!(
            make_redundant_alias(vec!["x", "y"].into_iter(), &stmt),
            vec![Edit::range_replacement(
                String::from("x as x"),
                TextRange::new(TextSize::new(7), TextSize::new(8)),
            )],
            "the second item is already a redundant alias"
        );
        assert_eq!(
            make_redundant_alias(vec!["x", "z"].into_iter(), &stmt),
            vec![Edit::range_replacement(
                String::from("x as x"),
                TextRange::new(TextSize::new(7), TextSize::new(8)),
            )],
            "the third item is already aliased to something else"
        );
        Ok(())
    }

    #[test_case("()",             &["x", "y"], r#"("x", "y")"#             ; "2 into empty tuple")]
    #[test_case("()",             &["x"],      r#"("x",)"#                 ; "1 into empty tuple adding a trailing comma")]
    #[test_case("[]",             &["x", "y"], r#"["x", "y"]"#             ; "2 into empty list")]
    #[test_case("[]",             &["x"],      r#"["x"]"#                  ; "1 into empty list")]
    #[test_case(r#""a", "b""#,    &["x", "y"], r#""a", "b", "x", "y""#     ; "2 into unparenthesized tuple")]
    #[test_case(r#""a", "b""#,    &["x"],      r#""a", "b", "x""#          ; "1 into unparenthesized tuple")]
    #[test_case(r#""a", "b","#,   &["x", "y"], r#""a", "b", "x", "y","#    ; "2 into unparenthesized tuple w/trailing comma")]
    #[test_case(r#""a", "b","#,   &["x"],      r#""a", "b", "x","#         ; "1 into unparenthesized tuple w/trailing comma")]
    #[test_case(r#"("a", "b")"#,  &["x", "y"], r#"("a", "b", "x", "y")"#   ; "2 into nonempty tuple")]
    #[test_case(r#"("a", "b")"#,  &["x"],      r#"("a", "b", "x")"#        ; "1 into nonempty tuple")]
    #[test_case(r#"("a", "b",)"#, &["x", "y"], r#"("a", "b", "x", "y",)"#  ; "2 into nonempty tuple w/trailing comma")]
    #[test_case(r#"("a", "b",)"#, &["x"],      r#"("a", "b", "x",)"#       ; "1 into nonempty tuple w/trailing comma")]
    #[test_case(r#"["a", "b",]"#, &["x", "y"], r#"["a", "b", "x", "y",]"#  ; "2 into nonempty list w/trailing comma")]
    #[test_case(r#"["a", "b",]"#, &["x"],      r#"["a", "b", "x",]"#       ; "1 into nonempty list w/trailing comma")]
    #[test_case(r#"["a", "b"]"#,  &["x", "y"], r#"["a", "b", "x", "y"]"#   ; "2 into nonempty list")]
    #[test_case(r#"["a", "b"]"#,  &["x"],      r#"["a", "b", "x"]"#        ; "1 into nonempty list")]
    fn add_to_dunder_all_test(raw: &str, names: &[&str], expect: &str) -> Result<()> {
        let locator = Locator::new(raw);
        let edits = {
            let parsed = parse_expression(raw)?;
            let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
            add_to_dunder_all(names.iter().copied(), parsed.expr(), &stylist)
        };
        let diag = {
            use crate::rules::pycodestyle::rules::MissingNewlineAtEndOfFile;
            let mut iter = edits.into_iter();
            Diagnostic::new(
                MissingNewlineAtEndOfFile, // The choice of rule here is arbitrary.
                TextRange::default(),
            )
            .with_fix(Fix::safe_edits(
                iter.next().ok_or(anyhow!("expected edits nonempty"))?,
                iter,
            ))
        };
        assert_eq!(apply_fixes([diag].iter(), &locator).code, expect);
        Ok(())
    }
}

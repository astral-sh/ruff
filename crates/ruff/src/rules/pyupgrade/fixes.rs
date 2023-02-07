use anyhow::{bail, Result};
use libcst_native::{
    Codegen, CodegenState, CompoundStatement, Expression, ParenthesizableWhitespace,
    SmallStatement, Statement, Suite,
};
use rustpython_parser::ast::{Expr, Keyword, Location};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::helpers::remove_argument;
use crate::cst::matchers::match_module;
use crate::fix::Fix;
use crate::source_code::{Locator, Stylist};

/// Safely adjust the indentation of the indented block at [`Range`].
pub fn adjust_indentation(
    range: Range,
    indentation: &str,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let contents = locator.slice_source_code_range(&range);

    let module_text = format!("def f():{}{contents}", stylist.line_ending().as_str());

    let mut tree = match_module(&module_text)?;

    let [Statement::Compound(CompoundStatement::FunctionDef(embedding))] = &mut *tree.body else {
        bail!("Expected statement to be embedded in a function definition")
    };

    let Suite::IndentedBlock(indented_block) = &mut embedding.body else {
        bail!("Expected indented block")
    };
    indented_block.indent = Some(indentation);

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..Default::default()
    };
    indented_block.codegen(&mut state);

    let module_text = state.to_string();
    let module_text = module_text
        .strip_prefix(stylist.line_ending().as_str())
        .unwrap()
        .to_string();
    Ok(module_text)
}

/// Generate a fix to remove a base from a `ClassDef` statement.
pub fn remove_class_def_base(
    locator: &Locator,
    stmt_at: Location,
    expr_at: Location,
    expr_end: Location,
    bases: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    if let Ok(fix) = remove_argument(locator, stmt_at, expr_at, expr_end, bases, keywords, true) {
        Some(fix)
    } else {
        None
    }
}

/// Generate a fix to remove arguments from a `super` call.
pub fn remove_super_arguments(locator: &Locator, stylist: &Stylist, expr: &Expr) -> Option<Fix> {
    let range = Range::from_located(expr);
    let contents = locator.slice_source_code_range(&range);

    let mut tree = libcst_native::parse_module(contents, None).ok()?;

    let Statement::Simple(body) = tree.body.first_mut()? else {
        return None;
    };
    let SmallStatement::Expr(body) = body.body.first_mut()? else {
        return None;
    };
    let Expression::Call(body) = &mut body.value else {
        return None;
    };

    body.args = vec![];
    body.whitespace_before_args = ParenthesizableWhitespace::default();
    body.whitespace_after_func = ParenthesizableWhitespace::default();

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Some(Fix::replacement(
        state.to_string(),
        range.location,
        range.end_location,
    ))
}

/// Remove any imports matching `members` from an import-from statement.
pub fn remove_import_members(contents: &str, members: &[&str]) -> String {
    let mut names: Vec<Range> = vec![];
    let mut commas: Vec<Range> = vec![];
    let mut removal_indices: Vec<usize> = vec![];

    // Find all Tok::Name tokens that are not preceded by Tok::As, and all
    // Tok::Comma tokens.
    let mut prev_tok = None;
    for (start, tok, end) in lexer::make_tokenizer(contents)
        .flatten()
        .skip_while(|(_, tok, _)| !matches!(tok, Tok::Import))
    {
        if let Tok::Name { name } = &tok {
            if matches!(prev_tok, Some(Tok::As)) {
                // Adjust the location to take the alias into account.
                names.last_mut().unwrap().end_location = end;
            } else {
                if members.contains(&name.as_str()) {
                    removal_indices.push(names.len());
                }
                names.push(Range::new(start, end));
            }
        } else if matches!(tok, Tok::Comma) {
            commas.push(Range::new(start, end));
        }
        prev_tok = Some(tok);
    }

    // Reconstruct the source code by skipping any names that are in `members`.
    let locator = Locator::new(contents);
    let mut output = String::with_capacity(contents.len());
    let mut last_pos: Location = Location::new(1, 0);
    let mut is_first = true;
    for index in 0..names.len() {
        if !removal_indices.contains(&index) {
            is_first = false;
            continue;
        }

        let (start_location, end_location) = if is_first {
            (names[index].location, names[index + 1].location)
        } else {
            (commas[index - 1].location, names[index].end_location)
        };

        // Add all contents from `last_pos` to `fix.location`.
        // It's possible that `last_pos` is after `fix.location`, if we're removing the
        // first _two_ members.
        if start_location > last_pos {
            let slice = locator.slice_source_code_range(&Range::new(last_pos, start_location));
            output.push_str(slice);
        }

        last_pos = end_location;
    }

    // Add the remaining content.
    let slice = locator.slice_source_code_at(last_pos);
    output.push_str(slice);
    output
}

#[cfg(test)]
mod test {
    use crate::rules::pyupgrade::fixes::remove_import_members;

    #[test]
    fn once() {
        let source = r#"from foo import bar, baz, bop, qux as q"#;
        let expected = r#"from foo import bar, baz, qux as q"#;
        let actual = remove_import_members(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn twice() {
        let source = r#"from foo import bar, baz, bop, qux as q"#;
        let expected = r#"from foo import bar, qux as q"#;
        let actual = remove_import_members(source, &["baz", "bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn aliased() {
        let source = r#"from foo import bar, baz, bop as boop, qux as q"#;
        let expected = r#"from foo import bar, baz, qux as q"#;
        let actual = remove_import_members(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parenthesized() {
        let source = r#"from foo import (bar, baz, bop, qux as q)"#;
        let expected = r#"from foo import (bar, baz, qux as q)"#;
        let actual = remove_import_members(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn last_import() {
        let source = r#"from foo import bar, baz, bop, qux as q"#;
        let expected = r#"from foo import bar, baz, bop"#;
        let actual = remove_import_members(source, &["qux"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_import() {
        let source = r#"from foo import bar, baz, bop, qux as q"#;
        let expected = r#"from foo import baz, bop, qux as q"#;
        let actual = remove_import_members(source, &["bar"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_two_imports() {
        let source = r#"from foo import bar, baz, bop, qux as q"#;
        let expected = r#"from foo import bop, qux as q"#;
        let actual = remove_import_members(source, &["bar", "baz"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_two_imports_multiline() {
        let source = r#"from foo import (
    bar,
    baz,
    bop,
    qux as q
)"#;
        let expected = r#"from foo import (
    bop,
    qux as q
)"#;
        let actual = remove_import_members(source, &["bar", "baz"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiline_once() {
        let source = r#"from foo import (
    bar,
    baz,
    bop,
    qux as q,
)"#;
        let expected = r#"from foo import (
    bar,
    baz,
    qux as q,
)"#;
        let actual = remove_import_members(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiline_twice() {
        let source = r#"from foo import (
    bar,
    baz,
    bop,
    qux as q,
)"#;
        let expected = r#"from foo import (
    bar,
    qux as q,
)"#;
        let actual = remove_import_members(source, &["baz", "bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiline_comment() {
        let source = r#"from foo import (
    bar,
    baz,
    # This comment should be removed.
    bop,
    # This comment should be retained.
    qux as q,
)"#;
        let expected = r#"from foo import (
    bar,
    baz,
    # This comment should be retained.
    qux as q,
)"#;
        let actual = remove_import_members(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multi_comment_first_import() {
        let source = r#"from foo import (
    # This comment should be retained.
    bar,
    # This comment should be removed.
    baz,
    bop,
    qux as q,
)"#;
        let expected = r#"from foo import (
    # This comment should be retained.
    baz,
    bop,
    qux as q,
)"#;
        let actual = remove_import_members(source, &["bar"]);
        assert_eq!(expected, actual);
    }
}

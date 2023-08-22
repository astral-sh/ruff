use anyhow::Result;

use ruff_python_codegen::Stylist;
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};

use crate::autofix::codemods::CodegenStylist;
use crate::cst::matchers::{match_function_def, match_indented_block, match_statement};

/// Safely adjust the indentation of the indented block at [`TextRange`].
pub(crate) fn adjust_indentation(
    range: TextRange,
    indentation: &str,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let contents = locator.slice(range);

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

/// Remove any imports matching `members` from an import-from statement.
pub(crate) fn remove_import_members(contents: &str, members: &[&str]) -> String {
    let mut names: Vec<TextRange> = vec![];
    let mut commas: Vec<TextRange> = vec![];
    let mut removal_indices: Vec<usize> = vec![];

    // Find all Tok::Name tokens that are not preceded by Tok::As, and all
    // Tok::Comma tokens.
    let mut prev_tok = None;
    for (tok, range) in lexer::lex(contents, Mode::Module)
        .flatten()
        .skip_while(|(tok, _)| !matches!(tok, Tok::Import))
    {
        if let Tok::Name { name } = &tok {
            if matches!(prev_tok, Some(Tok::As)) {
                // Adjust the location to take the alias into account.
                let last_range = names.last_mut().unwrap();
                *last_range = TextRange::new(last_range.start(), range.end());
            } else {
                if members.contains(&name.as_str()) {
                    removal_indices.push(names.len());
                }
                names.push(range);
            }
        } else if matches!(tok, Tok::Comma) {
            commas.push(range);
        }
        prev_tok = Some(tok);
    }

    // Reconstruct the source code by skipping any names that are in `members`.
    let locator = Locator::new(contents);
    let mut output = String::with_capacity(contents.len());
    let mut last_pos = TextSize::default();
    let mut is_first = true;
    for index in 0..names.len() {
        if !removal_indices.contains(&index) {
            is_first = false;
            continue;
        }

        let range = if is_first {
            TextRange::new(names[index].start(), names[index + 1].start())
        } else {
            TextRange::new(commas[index - 1].start(), names[index].end())
        };

        // Add all contents from `last_pos` to `fix.location`.
        // It's possible that `last_pos` is after `fix.location`, if we're removing the
        // first _two_ members.
        if range.start() > last_pos {
            let slice = locator.slice(TextRange::new(last_pos, range.start()));
            output.push_str(slice);
        }

        last_pos = range.end();
    }

    // Add the remaining content.
    let slice = locator.after(last_pos);
    output.push_str(slice);
    output
}

#[cfg(test)]
mod tests {
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

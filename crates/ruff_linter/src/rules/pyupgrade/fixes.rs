use ruff_python_ast::StmtImportFrom;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

/// Remove any imports matching `members` from an import-from statement.
pub(crate) fn remove_import_members(
    locator: &Locator<'_>,
    import_from_stmt: &StmtImportFrom,
    tokens: &Tokens,
    members_to_remove: &[&str],
) -> String {
    let commas: Vec<TextRange> = tokens
        .in_range(import_from_stmt.range())
        .iter()
        .skip_while(|token| token.kind() != TokenKind::Import)
        .filter_map(|token| {
            if token.kind() == TokenKind::Comma {
                Some(token.range())
            } else {
                None
            }
        })
        .collect();

    // Reconstruct the source code by skipping any names that are in `members`.
    let mut output = String::with_capacity(import_from_stmt.range().len().to_usize());
    let mut last_pos = import_from_stmt.start();
    let mut is_first = true;

    for (index, member) in import_from_stmt.names.iter().enumerate() {
        if !members_to_remove.contains(&member.name.as_str()) {
            is_first = false;
            continue;
        }

        let range = if is_first {
            TextRange::new(
                import_from_stmt.names[index].start(),
                import_from_stmt.names[index + 1].start(),
            )
        } else {
            TextRange::new(
                commas[index - 1].start(),
                import_from_stmt.names[index].end(),
            )
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
    let slice = locator.slice(TextRange::new(last_pos, import_from_stmt.end()));
    output.push_str(slice);
    output
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::parse_module;
    use ruff_source_file::Locator;

    use super::remove_import_members;

    fn test_helper(source: &str, members_to_remove: &[&str]) -> String {
        let parsed = parse_module(source).unwrap();
        let import_from_stmt = parsed
            .suite()
            .first()
            .expect("source should have one statement")
            .as_import_from_stmt()
            .expect("first statement should be an import from statement");
        remove_import_members(
            &Locator::new(source),
            import_from_stmt,
            parsed.tokens(),
            members_to_remove,
        )
    }

    #[test]
    fn once() {
        let source = r"from foo import bar, baz, bop, qux as q";
        let expected = r"from foo import bar, baz, qux as q";
        let actual = test_helper(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn twice() {
        let source = r"from foo import bar, baz, bop, qux as q";
        let expected = r"from foo import bar, qux as q";
        let actual = test_helper(source, &["baz", "bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn aliased() {
        let source = r"from foo import bar, baz, bop as boop, qux as q";
        let expected = r"from foo import bar, baz, qux as q";
        let actual = test_helper(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parenthesized() {
        let source = r"from foo import (bar, baz, bop, qux as q)";
        let expected = r"from foo import (bar, baz, qux as q)";
        let actual = test_helper(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn last_import() {
        let source = r"from foo import bar, baz, bop, qux as q";
        let expected = r"from foo import bar, baz, bop";
        let actual = test_helper(source, &["qux"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_import() {
        let source = r"from foo import bar, baz, bop, qux as q";
        let expected = r"from foo import baz, bop, qux as q";
        let actual = test_helper(source, &["bar"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_two_imports() {
        let source = r"from foo import bar, baz, bop, qux as q";
        let expected = r"from foo import bop, qux as q";
        let actual = test_helper(source, &["bar", "baz"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_two_imports_multiline() {
        let source = r"from foo import (
    bar,
    baz,
    bop,
    qux as q
)";
        let expected = r"from foo import (
    bop,
    qux as q
)";
        let actual = test_helper(source, &["bar", "baz"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiline_once() {
        let source = r"from foo import (
    bar,
    baz,
    bop,
    qux as q,
)";
        let expected = r"from foo import (
    bar,
    baz,
    qux as q,
)";
        let actual = test_helper(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiline_twice() {
        let source = r"from foo import (
    bar,
    baz,
    bop,
    qux as q,
)";
        let expected = r"from foo import (
    bar,
    qux as q,
)";
        let actual = test_helper(source, &["baz", "bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiline_comment() {
        let source = r"from foo import (
    bar,
    baz,
    # This comment should be removed.
    bop,
    # This comment should be retained.
    qux as q,
)";
        let expected = r"from foo import (
    bar,
    baz,
    # This comment should be retained.
    qux as q,
)";
        let actual = test_helper(source, &["bop"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multi_comment_first_import() {
        let source = r"from foo import (
    # This comment should be retained.
    bar,
    # This comment should be removed.
    baz,
    bop,
    qux as q,
)";
        let expected = r"from foo import (
    # This comment should be retained.
    baz,
    bop,
    qux as q,
)";
        let actual = test_helper(source, &["bar"]);
        assert_eq!(expected, actual);
    }
}

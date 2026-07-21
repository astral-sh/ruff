use std::ops::Deref;

use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_trivia::Cursor;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSlice};

/// A shebang directive (e.g., `#!/usr/bin/env python3`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ShebangDirective<'a>(&'a str);

impl<'a> ShebangDirective<'a> {
    /// Parse a shebang directive from a line, or return `None` if the line does not contain a
    /// shebang directive.
    pub(crate) fn try_extract(line: &'a str) -> Option<Self> {
        let mut cursor = Cursor::new(line);

        // Trim the `#!` prefix.
        if !cursor.eat_char('#') {
            return None;
        }
        if !cursor.eat_char('!') {
            return None;
        }

        // A shebang must specify an absolute interpreter path, which always
        // begins with `/` (optionally preceded by spaces or tabs). We reject
        // bare `#!` comments like `#! regular comment` that are not shebangs.
        let rest = cursor.chars().as_str();
        if !rest.trim_start_matches([' ', '\t']).starts_with('/') {
            return None;
        }

        Some(Self(rest))
    }
}

impl Deref for ShebangDirective<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

/// Return the range of a shebang at the start of a file, including its line ending.
pub(crate) fn leading_shebang_range(source: &str, tokens: &Tokens) -> Option<TextRange> {
    let first_token = tokens.first()?;
    if first_token.kind() != TokenKind::Comment
        || ShebangDirective::try_extract(source.slice(first_token)).is_none()
    {
        return None;
    }

    Some(TextRange::new(
        first_token.start(),
        source.full_line_end(first_token.end()),
    ))
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use super::ShebangDirective;

    #[test]
    fn shebang_non_match() {
        let source = "not a match";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    #[test]
    fn shebang_end_of_line() {
        let source = "print('test')  #!/usr/bin/python";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    #[test]
    fn shebang_match() {
        let source = "#!/usr/bin/env python";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    #[test]
    fn shebang_match_trailing_comment() {
        let source = "#!/usr/bin/env python # trailing comment";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    #[test]
    fn shebang_leading_space() {
        let source = "  #!/usr/bin/env python";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    /// `#! /usr/bin/env python` — space between `!` and `/` is valid.
    #[test]
    fn shebang_space_before_path() {
        let source = "#! /usr/bin/env python";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    /// `#! regular comment` — not a shebang; no interpreter path.
    #[test]
    fn shebang_regular_comment() {
        let source = "#! regular comment";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    /// `#!python` — not a valid shebang; interpreter must be an absolute path.
    #[test]
    fn shebang_no_slash() {
        let source = "#!python";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }

    /// `#!` (empty) — not a shebang.
    #[test]
    fn shebang_empty() {
        let source = "#!";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }
}

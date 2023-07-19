use ruff_python_trivia::{is_python_whitespace, Cursor};
use ruff_text_size::{TextLen, TextSize};

/// A shebang directive (e.g., `#!/usr/bin/env python3`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ShebangDirective<'a> {
    /// The offset of the directive contents (e.g., `/usr/bin/env python3`) from the start of the
    /// line.
    pub(crate) offset: TextSize,
    /// The contents of the directive (e.g., `"/usr/bin/env python3"`).
    pub(crate) contents: &'a str,
}

impl<'a> ShebangDirective<'a> {
    /// Parse a shebang directive from a line, or return `None` if the line does not contain a
    /// shebang directive.
    pub(crate) fn try_extract(line: &'a str) -> Option<Self> {
        let mut cursor = Cursor::new(line);

        // Trim whitespace.
        cursor.eat_while(is_python_whitespace);

        // Trim the `#!` prefix.
        if !cursor.eat_char('#') {
            return None;
        }
        if !cursor.eat_char('!') {
            return None;
        }

        Some(Self {
            offset: line.text_len() - cursor.text_len(),
            contents: cursor.chars().as_str(),
        })
    }
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
    fn shebang_leading_space() {
        let source = "  #!/usr/bin/env python";
        assert_debug_snapshot!(ShebangDirective::try_extract(source));
    }
}

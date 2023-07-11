#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_family = "unix")]
use std::path::Path;

#[cfg(target_family = "unix")]
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_text_size::{TextLen, TextSize};

static SHEBANG_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?P<spaces>\s*)#!(?P<directive>.*)").unwrap());

/// A shebang directive (e.g., `#!/usr/bin/env python3`).
#[derive(Debug, PartialEq, Eq)]
pub struct ShebangDirective<'a> {
    /// The offset of the directive contents (e.g., `/usr/bin/env python3`) from the start of the
    /// line.
    pub(crate) offset: TextSize,
    /// The contents of the directive (e.g., `"/usr/bin/env python3"`).
    pub(crate) contents: &'a str,
}

impl<'a> ShebangDirective<'a> {
    ///
    pub fn try_extract(line: &'a str) -> Option<Self> {
        // Trim whitespace.
        let directive = Self::lex_whitespace(line);

        // Trim the `#!` prefix.
        let directive = Self::lex_char(directive, '#')?;
        let directive = Self::lex_char(directive, '!')?;

        Some(Self {
            offset: line.text_len() - directive.text_len(),
            contents: directive,
        })
    }

    /// Lex optional leading whitespace.
    #[inline]
    fn lex_whitespace(line: &str) -> &str {
        line.trim_start()
    }

    /// Lex a specific character, or return `None` if the character is not the first character in
    /// the line.
    #[inline]
    fn lex_char(line: &str, c: char) -> Option<&str> {
        let mut chars = line.chars();
        if chars.next() == Some(c) {
            Some(chars.as_str())
        } else {
            None
        }
    }
}

#[cfg(target_family = "unix")]
pub(super) fn is_executable(filepath: &Path) -> Result<bool> {
    let metadata = filepath.metadata()?;
    let permissions = metadata.permissions();
    Ok(permissions.mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use crate::rules::flake8_executable::helpers::ShebangDirective;

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

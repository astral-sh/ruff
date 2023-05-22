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

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ShebangDirective<'a> {
    None,
    // whitespace length, start of the shebang, contents
    Match(TextSize, TextSize, &'a str),
}

pub(crate) fn extract_shebang(line: &str) -> ShebangDirective {
    // Minor optimization to avoid matches in the common case.
    if !line.contains('!') {
        return ShebangDirective::None;
    }
    match SHEBANG_REGEX.captures(line) {
        Some(caps) => match caps.name("spaces") {
            Some(spaces) => match caps.name("directive") {
                Some(matches) => ShebangDirective::Match(
                    spaces.as_str().text_len(),
                    TextSize::try_from(matches.start()).unwrap(),
                    matches.as_str(),
                ),
                None => ShebangDirective::None,
            },
            None => ShebangDirective::None,
        },
        None => ShebangDirective::None,
    }
}

#[cfg(target_family = "unix")]
pub(crate) fn is_executable(filepath: &Path) -> Result<bool> {
    {
        let metadata = filepath.metadata()?;
        let permissions = metadata.permissions();
        Ok(permissions.mode() & 0o111 != 0)
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextSize;

    use crate::rules::flake8_executable::helpers::{
        extract_shebang, ShebangDirective, SHEBANG_REGEX,
    };

    #[test]
    fn shebang_regex() {
        // Positive cases
        assert!(SHEBANG_REGEX.is_match("#!/usr/bin/python"));
        assert!(SHEBANG_REGEX.is_match("#!/usr/bin/env python"));
        assert!(SHEBANG_REGEX.is_match("    #!/usr/bin/env python"));
        assert!(SHEBANG_REGEX.is_match("  #!/usr/bin/env python"));

        // Negative cases
        assert!(!SHEBANG_REGEX.is_match("hello world"));
    }

    #[test]
    fn shebang_extract_match() {
        assert_eq!(extract_shebang("not a match"), ShebangDirective::None);
        assert_eq!(
            extract_shebang("#!/usr/bin/env python"),
            ShebangDirective::Match(TextSize::from(0), TextSize::from(2), "/usr/bin/env python")
        );
        assert_eq!(
            extract_shebang("  #!/usr/bin/env python"),
            ShebangDirective::Match(TextSize::from(2), TextSize::from(4), "/usr/bin/env python")
        );
        assert_eq!(
            extract_shebang("print('test')  #!/usr/bin/python"),
            ShebangDirective::None
        );
    }
}

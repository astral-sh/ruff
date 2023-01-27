use once_cell::sync::Lazy;
use regex::Regex;

static SHEBANG_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?P<spaces>\s*)#!(?P<directive>.*)").unwrap());

#[derive(Debug)]
pub enum ShebangDirective<'a> {
    None,
    // whitespace length, start of shebang, end, shebang contents
    Match(usize, usize, usize, &'a str),
}

pub fn extract_shebang(line: &str) -> ShebangDirective {
    // Minor optimization to avoid matches in the common case.
    if !line.contains('!') {
        return ShebangDirective::None;
    }
    match SHEBANG_REGEX.captures(line) {
        Some(caps) => match caps.name("spaces") {
            Some(spaces) => match caps.name("directive") {
                Some(matches) => ShebangDirective::Match(
                    spaces.as_str().chars().count(),
                    matches.start(),
                    matches.end(),
                    matches.as_str(),
                ),
                None => ShebangDirective::None,
            },
            None => ShebangDirective::None,
        },
        None => ShebangDirective::None,
    }
}

#[cfg(test)]
mod tests {
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
        assert!(matches!(
            extract_shebang("not a match"),
            ShebangDirective::None
        ));
        assert!(matches!(
            extract_shebang("#!/usr/bin/env python"),
            ShebangDirective::Match(0, 2, 21, "/usr/bin/env python")
        ));
        assert!(matches!(
            extract_shebang("  #!/usr/bin/env python"),
            ShebangDirective::Match(2, 4, 23, "/usr/bin/env python")
        ));
        assert!(matches!(
            extract_shebang("print('test')  #!/usr/bin/python"),
            ShebangDirective::None
        ));
    }
}

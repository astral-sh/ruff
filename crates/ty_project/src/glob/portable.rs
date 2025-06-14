//! Cross-language glob syntax from
//! [PEP 639](https://packaging.python.org/en/latest/specifications/glob-patterns/).
//!
//! The glob syntax matches the `uv` variant of uv's `uv-globfilter` crate.
//! We intentionally use the same syntax to give users a consistent experience
//! across our tools.
//!
//! [Source](https://github.com/astral-sh/uv/blob/main/crates/uv-globfilter/src/portable_glob.rs)

use ruff_db::system::SystemPath;
use std::ops::Deref;
use std::{fmt::Write, path::MAIN_SEPARATOR};
use thiserror::Error;

/// Pattern that only uses cross-language glob syntax based on [PEP 639](https://packaging.python.org/en/latest/specifications/glob-patterns/):
///
/// - Alphanumeric characters, underscores (`_`), hyphens (`-`) and dots (`.`) are matched verbatim.
/// - The special glob characters are:
///   - `*`: Matches any number of characters except path separators
///   - `?`: Matches a single character except the path separator
///   - `**`: Matches any number of characters including path separators
///   - `[]`, containing only the verbatim matched characters: Matches a single of the characters contained. Within
///     `[...]`, the hyphen indicates a locale-agnostic range (e.g. `a-z`, order based on Unicode code points). Hyphens at
///     the start or end are matched literally.
///   - `\`: It escapes the following character to be matched verbatim (extension to PEP 639).
/// - The path separator is the forward slash character (`/`). Patterns are relative to the given directory, a leading slash
///   character for absolute paths is not supported.
/// - Parent directory indicators (`..`) are not allowed.
///
/// These rules mean that matching the backslash (`\`) is forbidden, which avoid collisions with the windows path separator.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct PortableGlobPattern<'a> {
    pattern: &'a str,
    is_exclude: bool,
}

impl<'a> PortableGlobPattern<'a> {
    /// Parses a portable glob pattern. Returns an error if the pattern isn't valid.
    pub(crate) fn parse(glob: &'a str, is_exclude: bool) -> Result<Self, PortableGlobError> {
        let mut chars = glob.chars().enumerate().peekable();

        if is_exclude {
            chars.next_if(|(_, c)| *c == '!');
        }

        // A `..` is on a parent directory indicator at the start of the string or after a directory
        // separator.
        let mut start_or_slash = true;
        // The number of consecutive stars before the current character.
        while let Some((offset, c)) = chars.next() {
            let pos = offset + 1;

            // `***` or `**literals` can be correctly represented with less stars. They are banned by
            // `glob`, they are allowed by `globset` and PEP 639 is ambiguous, so we're filtering them
            // out.
            if c == '*' {
                let mut star_run = 1;
                while let Some((_, c)) = chars.peek() {
                    if *c == '*' {
                        star_run += 1;
                        chars.next();
                    } else {
                        break;
                    }
                }
                if star_run >= 3 {
                    return Err(PortableGlobError::TooManyStars {
                        glob: glob.to_string(),
                        // We don't update pos for the stars.
                        pos,
                    });
                } else if star_run == 2 {
                    if chars.peek().is_some_and(|(_, c)| *c != '/') {
                        return Err(PortableGlobError::TooManyStars {
                            glob: glob.to_string(),
                            // We don't update pos for the stars.
                            pos,
                        });
                    }
                }
                start_or_slash = false;
            } else if c.is_alphanumeric() || matches!(c, '_' | '-' | '?') {
                start_or_slash = false;
            } else if c == '.' {
                if start_or_slash && matches!(chars.peek(), Some((_, '.'))) {
                    return Err(PortableGlobError::ParentDirectory {
                        pos,
                        glob: glob.to_string(),
                    });
                }
                start_or_slash = false;
            } else if c == '/' {
                start_or_slash = true;
            } else if c == '[' {
                for (pos, c) in chars.by_ref() {
                    if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') {
                        // Allowed.
                    } else if c == ']' {
                        break;
                    } else {
                        return Err(PortableGlobError::InvalidCharacterRange {
                            glob: glob.to_string(),
                            pos,
                            invalid: InvalidChar(c),
                        });
                    }
                }
                start_or_slash = false;
            } else if c == '\\' {
                match chars.next() {
                    Some((pos, '/' | '\\')) => {
                        // For cross-platform compatibility, we don't allow forward slashes or
                        // backslashes to be escaped.
                        return Err(PortableGlobError::InvalidEscapee {
                            glob: glob.to_string(),
                            pos,
                        });
                    }
                    Some(_) => {
                        // Escaped character
                    }
                    None => {
                        return Err(PortableGlobError::TrailingEscape {
                            glob: glob.to_string(),
                            pos,
                        });
                    }
                }
            } else {
                return Err(PortableGlobError::InvalidCharacter {
                    glob: glob.to_string(),
                    pos,
                    invalid: InvalidChar(c),
                });
            }
        }
        Ok(PortableGlobPattern {
            pattern: glob,
            is_exclude,
        })
    }

    /// Anchors pattern at `cwd`.
    ///
    /// `is_exclude` indicates whether this is a pattern in an exclude filter.
    ///
    /// This method similar to [`SystemPath::absolute`] but for a glob pattern.
    /// The main difference is that this method always uses `/` as path separator.
    pub(crate) fn into_absolute(self, cwd: impl AsRef<SystemPath>) -> AbsolutePortableGlobPattern {
        let mut pattern = self.pattern;
        let mut negated = false;

        if self.is_exclude {
            // If the pattern starts with `!`, we need to remove it and then anchor the rest.
            if let Some(after) = self.pattern.strip_prefix('!') {
                pattern = after;
                negated = true;
            }

            // Patterns that don't contain any `/`, e.g. `.venv` are unanchored patterns
            // that match anywhere.
            if !self.chars().any(|c| c == '/') {
                return AbsolutePortableGlobPattern(self.to_string());
            }
        }

        if pattern.starts_with('/') {
            return AbsolutePortableGlobPattern(pattern.to_string());
        }

        let mut rest = pattern;
        let mut prefix = cwd.as_ref().to_path_buf().into_utf8_path_buf();

        loop {
            if let Some(after) = rest.strip_prefix("./") {
                rest = after;
            } else if let Some(after) = rest.strip_prefix("../") {
                prefix.pop();
                rest = after;
            } else {
                break;
            }
        }

        let mut output = String::with_capacity(prefix.as_str().len() + rest.len());

        for component in prefix.components() {
            match component {
                camino::Utf8Component::Prefix(utf8_prefix_component) => {
                    output.push_str(&utf8_prefix_component.as_str().replace(MAIN_SEPARATOR, "/"));
                }

                camino::Utf8Component::RootDir => {
                    output.push('/');
                    continue;
                }
                camino::Utf8Component::CurDir => {}
                camino::Utf8Component::ParentDir => output.push_str("../"),
                camino::Utf8Component::Normal(component) => {
                    output.push_str(component);
                    output.push('/');
                }
            }
        }

        output.push_str(rest);
        if negated {
            // If the pattern is negated, we need to keep the leading `!`.
            AbsolutePortableGlobPattern(format!("!{output}"))
        } else {
            AbsolutePortableGlobPattern(output)
        }
    }
}

impl Deref for PortableGlobPattern<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.pattern
    }
}

/// A portable glob pattern that uses absolute paths.
///
/// E.g., `./src/**` becomes `/root/src/**` when anchored to `/root`.
#[derive(Debug, Eq, PartialEq, Hash)]
pub(crate) struct AbsolutePortableGlobPattern(String);

impl Deref for AbsolutePortableGlobPattern {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Error)]
pub(crate) enum PortableGlobError {
    /// Shows the failing glob in the error message.
    #[error(transparent)]
    GlobError(#[from] globset::Error),

    #[error(
        "The parent directory operator (`..`) at position {pos} is not allowed in glob: `{glob}`"
    )]
    ParentDirectory { glob: String, pos: usize },

    #[error(
        "Invalid character `{invalid}` at position {pos} in glob: `{glob}`. hint: Characters can be escaped with a backslash"
    )]
    InvalidCharacter {
        glob: String,
        pos: usize,
        invalid: InvalidChar,
    },

    #[error(
        "Path separators can't be escaped, invalid character at position {pos} in glob: `{glob}`"
    )]
    InvalidEscapee { glob: String, pos: usize },

    #[error("Invalid character `{invalid}` in range at position {pos} in glob: `{glob}`")]
    InvalidCharacterRange {
        glob: String,
        pos: usize,
        invalid: InvalidChar,
    },

    #[error("Too many stars at position {pos} in glob: `{glob}`")]
    TooManyStars { glob: String, pos: usize },

    #[error("Trailing backslash at position {pos} in glob: `{glob}`")]
    TrailingEscape { glob: String, pos: usize },
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct InvalidChar(pub char);

impl std::fmt::Display for InvalidChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            '\'' => f.write_char('\''),
            c => c.escape_debug().fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::glob::PortableGlobPattern;
    use insta::assert_snapshot;
    use ruff_db::system::SystemPath;

    #[test]
    fn test_error() {
        #[track_caller]
        fn parse_err(glob: &str) -> String {
            let error = PortableGlobPattern::parse(glob, true).unwrap_err();
            error.to_string()
        }

        assert_snapshot!(
            parse_err(".."),
            @"The parent directory operator (`..`) at position 1 is not allowed in glob: `..`"
        );
        assert_snapshot!(
            parse_err("licenses/.."),
            @"The parent directory operator (`..`) at position 10 is not allowed in glob: `licenses/..`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN!E.txt"),
            @"Invalid character `!` at position 15 in glob: `licenses/LICEN!E.txt`. hint: Characters can be escaped with a backslash"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN[!C]E.txt"),
            @"Invalid character `!` in range at position 15 in glob: `licenses/LICEN[!C]E.txt`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN[C?]E.txt"),
            @"Invalid character `?` in range at position 16 in glob: `licenses/LICEN[C?]E.txt`"
        );
        assert_snapshot!(
            parse_err("******"),
            @"Too many stars at position 1 in glob: `******`"
        );
        assert_snapshot!(
            parse_err("licenses/**license"),
            @"Too many stars at position 10 in glob: `licenses/**license`"
        );
        assert_snapshot!(
            parse_err("licenses/***/licenses.csv"),
            @"Too many stars at position 10 in glob: `licenses/***/licenses.csv`"
        );
        assert_snapshot!(
            parse_err(r"**/@test"),
            @"Invalid character `@` at position 4 in glob: `**/@test`. hint: Characters can be escaped with a backslash"
        );
        // Escapes are not allowed in strict PEP 639 mode
        assert_snapshot!(
            parse_err(r"public domain/Gulliver\\’s Travels.txt"),
            @r"Invalid character ` ` at position 7 in glob: `public domain/Gulliver\\’s Travels.txt`. hint: Characters can be escaped with a backslash"
        );
        assert_snapshot!(
            parse_err(r"**/@test"),
            @"Invalid character `@` at position 4 in glob: `**/@test`. hint: Characters can be escaped with a backslash"
        );
        // Escaping slashes is not allowed.
        assert_snapshot!(
            parse_err(r"licenses\\MIT.txt"),
            @r"Path separators can't be escaped, invalid character at position 9 in glob: `licenses\\MIT.txt`"
        );
        assert_snapshot!(
            parse_err(r"licenses\/MIT.txt"),
            @r"Path separators can't be escaped, invalid character at position 9 in glob: `licenses\/MIT.txt`"
        );
    }

    #[test]
    fn test_valid() {
        let cases = [
            r"licenses/*.txt",
            r"licenses/**/*.txt",
            r"LICEN[CS]E.txt",
            r"LICEN?E.txt",
            r"[a-z].txt",
            r"[a-z._-].txt",
            r"*/**",
            r"LICENSE..txt",
            r"LICENSE_file-1.txt",
            // (google translate)
            r"licenses/라이센스*.txt",
            r"licenses/ライセンス*.txt",
            r"licenses/执照*.txt",
            r"src/**",
        ];
        let cases_uv = [
            r"public-domain/Gulliver\’s\ Travels.txt",
            // https://github.com/astral-sh/uv/issues/13280
            r"**/\@test",
        ];
        for case in cases.iter().chain(cases_uv.iter()) {
            PortableGlobPattern::parse(case, true).unwrap();
        }
    }

    #[track_caller]
    fn assert_absolute_path(pattern: &str, relative_to: impl AsRef<SystemPath>, expected: &str) {
        let pattern = PortableGlobPattern::parse(pattern, true).unwrap();
        let absolute = pattern.into_absolute(relative_to);
        assert_eq!(&*absolute, expected);
    }

    #[test]
    fn absolute_pattern() {
        assert_absolute_path("/src", "/root", "/src");
        assert_absolute_path("./src", "/root", "/root/src");
    }

    #[test]
    #[cfg(windows)]
    fn absolute_pattern_windows() {
        assert_absolute_path("./src", r"C:\root", "C:/root/src");
        assert_absolute_path("./src", r"\\server\test", "//server/test/src");
    }
}

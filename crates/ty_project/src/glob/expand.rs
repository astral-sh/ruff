use crate::glob::portable::{PortableGlobKind, PortableGlobPattern};
use globset::GlobBuilder;
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use std::sync::Mutex;

/// Expands a PEP 639 portable glob pattern to all matching directories.
///
/// Returns all absolute directory paths that match `pattern_str` anchored at `anchor`.
/// Emits `tracing::warn!` and returns an empty `Vec` on invalid patterns or no matches.
pub(crate) fn expand_glob_to_directories(
    pattern_str: &str,
    anchor: &SystemPath,
    system: &dyn System,
) -> Vec<SystemPathBuf> {
    let portable = match PortableGlobPattern::parse(pattern_str, PortableGlobKind::Include) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                "Invalid glob pattern `{pattern_str}` in `environment.extra-paths`: {err}"
            );
            return Vec::new();
        }
    };

    let abs_pattern = portable.into_absolute(anchor);

    // Same settings as IncludeFilterBuilder.
    let glob = match GlobBuilder::new(abs_pattern.absolute())
        .literal_separator(true)
        .backslash_escape(true)
        .build()
    {
        Ok(g) => g,
        Err(err) => {
            tracing::warn!(
                "Failed to compile glob pattern `{pattern_str}` in `environment.extra-paths`: {err}"
            );
            return Vec::new();
        }
    };

    let glob_matcher = glob.compile_matcher();

    // Walk from the literal prefix -- the portion of the pattern before the first metachar.
    // This avoids scanning the entire project root when the pattern has a long literal prefix.
    let walk_root = literal_prefix(abs_pattern.absolute(), anchor);

    // walk_directory().run() may dispatch visitors from multiple threads, so we collect
    // results through a Mutex (same pattern as walk.rs::collect_vec).
    // Path matching uses entry.path().as_str() -- always '/' separated on all platforms --
    // to match the portable pattern built with '/' separators.
    let results: Mutex<Vec<SystemPathBuf>> = Mutex::new(Vec::new());

    system
        .walk_directory(&walk_root)
        .standard_filters(false)
        .ignore_hidden(false)
        .run(|| {
            let results = &results;
            let glob_matcher = &glob_matcher;
            Box::new(move |entry| match entry {
                Ok(entry) if entry.file_type().is_directory() => {
                    if glob_matcher.is_match(entry.path().as_str()) {
                        results.lock().unwrap().push(entry.into_path());
                    }
                    WalkState::Continue
                }
                Ok(_) => WalkState::Continue,
                Err(err) => {
                    tracing::warn!(
                        "I/O error expanding glob `{pattern_str}` in `environment.extra-paths`: {err}"
                    );
                    WalkState::Continue
                }
            })
        });

    let results = results.into_inner().unwrap();

    if results.is_empty() {
        tracing::warn!(
            "Glob pattern `{pattern_str}` in `environment.extra-paths` matched no directories"
        );
    }

    results
}

/// Returns `true` if `s` contains any glob metacharacter (`*`, `?`, `[`).
pub(crate) fn has_glob_metachar(s: &str) -> bool {
    s.contains(is_glob_metachar)
}

fn is_glob_metachar(c: char) -> bool {
    matches!(c, '*' | '?' | '[')
}

/// Returns the longest literal (metachar-free) directory prefix of `abs_pattern`.
///
/// Used to find the deepest directory we can start walking from, avoiding a
/// full scan from the project root.
///
/// # Examples
/// - `/project/packages/*/src`   -> `/project/packages`
/// - `/project/*/src`            -> `/project`
/// - `/project/packages/**/src`  -> `/project/packages`
fn literal_prefix(abs_pattern: &str, fallback: &SystemPath) -> SystemPathBuf {
    // Scan character by character, skipping backslash-escaped pairs (\x).
    // Stop at the first unescaped glob metacharacter.
    let mut chars = abs_pattern.char_indices().peekable();
    let mut prefix_end = abs_pattern.len();

    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            // Skip the escaped character so we don't misidentify \* as a metachar
            chars.next();
            continue;
        }
        if is_glob_metachar(c) {
            prefix_end = i;
            break;
        }
    }

    // Take everything up to (but not including) the last '/' before the metachar.
    // This gives us a valid directory path we can pass to walk_directory.
    let literal = &abs_pattern[..prefix_end];
    match literal.rfind('/') {
        Some(pos) => SystemPathBuf::from(&abs_pattern[..pos]),
        None => fallback.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::system::{SystemPath, SystemPathBuf, TestSystem};

    /// Helper: create a TestSystem with the given directories pre-populated.
    fn make_system_with_dirs(dirs: &[&str]) -> TestSystem {
        let system = TestSystem::default();
        for dir in dirs {
            system
                .memory_file_system()
                .create_directory_all(SystemPath::new(dir))
                .unwrap();
        }
        system
    }

    #[test]
    fn single_level_glob_expands_matching_directories() {
        let system = make_system_with_dirs(&[
            "/project/packages/a/src",
            "/project/packages/b/src",
            "/project/packages/a/tests", // should NOT match
        ]);
        let anchor = SystemPath::new("/project");

        let mut result =
            expand_glob_to_directories("./packages/*/src", anchor, &system);
        result.sort();

        assert_eq!(
            result,
            vec![
                SystemPathBuf::from("/project/packages/a/src"),
                SystemPathBuf::from("/project/packages/b/src"),
            ]
        );
    }

    #[test]
    fn recursive_glob_expands_across_directory_levels() {
        let system = make_system_with_dirs(&[
            "/project/libs/util/src",
            "/project/libs/core/src",
            "/project/libs/core/nested/src",
        ]);
        let anchor = SystemPath::new("/project");

        let mut result =
            expand_glob_to_directories("./libs/**/src", anchor, &system);
        result.sort();

        assert_eq!(
            result,
            vec![
                SystemPathBuf::from("/project/libs/core/nested/src"),
                SystemPathBuf::from("/project/libs/core/src"),
                SystemPathBuf::from("/project/libs/util/src"),
            ]
        );
    }

    #[test]
    fn glob_with_no_matches_returns_empty() {
        let system = make_system_with_dirs(&["/project/other"]);
        let anchor = SystemPath::new("/project");

        let result =
            expand_glob_to_directories("./packages/*/src", anchor, &system);

        assert!(result.is_empty());
    }

    #[test]
    fn invalid_pep639_pattern_returns_empty() {
        // [!abc] is forbidden by PEP 639 -- PortableGlobPattern::parse returns an error
        let system = TestSystem::default();
        let anchor = SystemPath::new("/project");

        let result = expand_glob_to_directories("./[!invalid", anchor, &system);

        assert!(result.is_empty());
    }

    #[test]
    fn tilde_in_pattern_returns_empty_with_warning() {
        // ~ is not a valid PEP 639 character -- fails at parse time
        let system = TestSystem::default();
        let anchor = SystemPath::new("/project");

        let result = expand_glob_to_directories("~/packages/*/src", anchor, &system);

        assert!(result.is_empty());
    }

    #[test]
    fn literal_prefix_strips_glob_suffix() {
        let fallback = SystemPath::new("/fallback");

        // Standard case
        assert_eq!(
            literal_prefix("/project/packages/*/src", fallback),
            SystemPathBuf::from("/project/packages")
        );

        // Pattern starts immediately with glob
        assert_eq!(
            literal_prefix("/project/*/src", fallback),
            SystemPathBuf::from("/project")
        );

        // Recursive glob
        assert_eq!(
            literal_prefix("/project/packages/**/src", fallback),
            SystemPathBuf::from("/project/packages")
        );
    }
}

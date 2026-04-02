use crate::glob::portable::{PortableGlobKind, PortableGlobPattern};
use globset::{Glob, GlobBuilder, GlobSetBuilder};
use regex_automata::dfa;
use regex_automata::dfa::Automaton;
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR};
use std::sync::Mutex;
use tracing::warn;

const DFA_SIZE_LIMIT: usize = 1_000_000;

/// Expands a list of PEP 639 portable glob patterns to all matching directories.
///
/// All patterns are batched into a single directory walk with DFA-based directory
/// pruning to skip subtrees that cannot match any pattern.
///
/// Each entry in `patterns` is a `(pattern_str, anchor)` pair where `anchor` is the
/// directory against which relative patterns are resolved (`project_root` for file-sourced
/// patterns, `cwd` for CLI-sourced ones).
///
/// Literal prefixes (the path segment before any glob metacharacter) are collected as
/// separate walk roots so the traversal starts as deep as possible rather than at the
/// common ancestor. All roots share a single parallel walker.
///
/// Returns an unordered list of absolute directory paths. Invalid patterns emit a
/// [`tracing::warn!`] and are skipped. If no patterns match any directory, a warning is
/// also emitted.
pub(crate) fn expand_globs_to_directories(
    patterns: &[(&str, &SystemPath)],
    system: &dyn System,
) -> Vec<SystemPathBuf> {
    if patterns.is_empty() {
        return Vec::new();
    }

    struct Resolved {
        pattern_str: String,
        absolute: String,
        literal_prefix: SystemPathBuf,
    }

    // Parse and resolve every pattern to an absolute glob string.
    let mut resolved: Vec<Resolved> = Vec::with_capacity(patterns.len());
    for &(pattern_str, anchor) in patterns {
        let portable = match PortableGlobPattern::parse(pattern_str, PortableGlobKind::Include) {
            Ok(p) => p,
            Err(err) => {
                warn!("Invalid glob pattern `{pattern_str}` in `environment.extra-paths`: {err}");
                continue;
            }
        };

        let abs_pattern = portable.into_absolute(anchor);
        let abs_str = abs_pattern.absolute().to_string();
        let prefix = literal_prefix(&abs_str, anchor);

        resolved.push(Resolved {
            pattern_str: pattern_str.to_string(),
            absolute: abs_str,
            literal_prefix: prefix,
        });
    }

    if resolved.is_empty() {
        return Vec::new();
    }

    // Build a single GlobSet and a DFA from all resolved absolute patterns.
    let mut glob_set_builder = GlobSetBuilder::new();
    let mut prefix_regexes: Vec<String> = Vec::with_capacity(resolved.len());
    let main_sep = regex::escape(MAIN_SEPARATOR_STR);

    for r in &resolved {
        let glob = match GlobBuilder::new(&r.absolute)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
        {
            Ok(g) => g,
            Err(err) => {
                warn!(
                    "Failed to compile glob pattern `{}` in `environment.extra-paths`: {err}",
                    r.pattern_str
                );
                continue;
            }
        };

        // For DFA directory pruning we need a regex that can match prefixes of potential
        // matches. If the pattern doesn't already end with `**`, we append `/**` so the
        // DFA can match intermediate directories (same expansion IncludeFilterBuilder does).
        let prefix_glob = if r.absolute.ends_with("**") {
            None
        } else {
            GlobBuilder::new(&format!("{}/**", r.absolute))
                .literal_separator(true)
                .backslash_escape(true)
                .build()
                .ok()
        };
        let prefix_regex = glob_to_regex(prefix_glob.as_ref().unwrap_or(&glob), &main_sep);

        prefix_regexes.push(prefix_regex);
        glob_set_builder.add(glob);
    }

    let glob_set = match glob_set_builder.build() {
        Ok(gs) => gs,
        Err(err) => {
            warn!("Failed to build glob set for `environment.extra-paths`: {err}");
            return Vec::new();
        }
    };

    let dfa = build_dfa(&prefix_regexes);

    // Collect distinct literal prefixes as walk roots, sorted so we start from the
    // shallowest directory.  Using WalkDirectoryBuilder::add() for additional roots
    // puts them all in a single parallel walker.
    let mut walk_roots: Vec<SystemPathBuf> =
        resolved.iter().map(|r| r.literal_prefix.clone()).collect();
    walk_roots.sort();
    walk_roots.dedup();

    let Some((first_root, rest_roots)) = walk_roots.split_first() else {
        return Vec::new();
    };

    let mut walker = system
        .walk_directory(first_root)
        .standard_filters(false)
        .ignore_hidden(false);

    for root in rest_roots {
        walker = walker.add(root);
    }

    let results: Mutex<Vec<SystemPathBuf>> = Mutex::new(Vec::new());

    walker.run(|| {
        let results = &results;
        let glob_set = &glob_set;
        let dfa = &dfa;

        Box::new(move |entry| match entry {
            Ok(entry) if entry.file_type().is_directory() => {
                if !match_directory(entry.path(), dfa) {
                    return WalkState::Skip;
                }
                if glob_set.is_match(entry.path()) {
                    results.lock().unwrap().push(entry.into_path());
                }
                WalkState::Continue
            }
            Ok(_) => WalkState::Continue,
            Err(err) => {
                warn!("I/O error expanding glob in `environment.extra-paths`: {err}");
                WalkState::Continue
            }
        })
    });

    let results = results.into_inner().unwrap();

    if results.is_empty() {
        warn!("No glob pattern in `environment.extra-paths` matched any directories");
    }

    results
}

/// Extracts the byte regex from a compiled glob, normalized for the current platform's
/// path separator. This is the same transformation `IncludeFilterBuilder::push_prefix_regex` does.
fn glob_to_regex(glob: &Glob, main_separator: &str) -> String {
    glob.regex()
        .strip_prefix("(?-u)")
        .expect("a glob is a non-unicode byte regex")
        .replace('/', main_separator)
}

/// Returns `true` if the directory or any of its descendants can match any glob in the DFA.
///
/// This never returns `false` if any child matches, but may return `true` even if no
/// child ends up matching (false positives are safe -- they just cause extra traversal).
fn match_directory(path: &SystemPath, dfa: &Option<dfa::dense::DFA<Vec<u32>>>) -> bool {
    let Some(dfa) = dfa else {
        // No DFA means we fell back to full traversal.
        return true;
    };

    if path.as_str().is_empty() {
        return true;
    }

    let config_anchored =
        regex_automata::util::start::Config::new().anchored(regex_automata::Anchored::Yes);
    let mut state = dfa.start_state(&config_anchored).unwrap();

    for b in path.as_str().as_bytes() {
        state = dfa.next_state(state, *b);
    }

    // Check whether this directory itself matches OR whether any child path could match.
    // `eoi_state`: the state after processing end-of-input (would `path` match exactly?).
    // `slash_state`: the state after appending `/`; if dead, no child can ever match.
    let eoi_state = dfa.next_eoi_state(state);
    let slash_state = dfa.next_state(state, u8::try_from(MAIN_SEPARATOR).unwrap());

    debug_assert!(
        !dfa.is_quit_state(eoi_state) && !dfa.is_quit_state(slash_state),
        "DFA is in quit state"
    );

    dfa.is_match_state(eoi_state) || !dfa.is_dead_state(slash_state)
}

/// Builds a dense DFA from the given prefix-match regexes.
///
/// Returns `None` and emits a warning if the DFA exceeds the size limit, which
/// causes [`match_directory`] to fall back to full traversal.
fn build_dfa(regexes: &[String]) -> Option<dfa::dense::DFA<Vec<u32>>> {
    if regexes.is_empty() {
        return None;
    }

    let result = dfa::dense::Builder::new()
        .syntax(
            regex_automata::util::syntax::Config::new()
                .unicode(false)
                .utf8(false),
        )
        .configure(
            dfa::dense::Config::new()
                .start_kind(dfa::StartKind::Anchored)
                .dfa_size_limit(Some(DFA_SIZE_LIMIT))
                .determinize_size_limit(Some(DFA_SIZE_LIMIT)),
        )
        .build_many(regexes);

    match result {
        Ok(dfa) => Some(dfa),
        Err(_) => {
            warn!(
                "Glob expressions regex is larger than {DFA_SIZE_LIMIT} bytes, \
                    falling back to full directory traversal!"
            );
            None
        }
    }
}

/// Returns the longest literal (metachar-free) directory prefix of an absolute glob pattern.
///
/// This is the deepest directory we can start the walk from, since we know no
/// glob metacharacters appear before it.
///
/// # Examples
/// - `/project/packages/*/src`   → `/project/packages`
/// - `/project/*/src`            → `/project`
/// - `/project/packages/**/src`  → `/project/packages`
fn literal_prefix(abs_pattern: &str, fallback: &SystemPath) -> SystemPathBuf {
    let mut chars = abs_pattern.char_indices();
    let mut prefix_end = abs_pattern.len();

    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            // Skip the escaped character so we don't misidentify `\*` as a metachar.
            chars.next();
            continue;
        }
        if matches!(c, '*' | '?' | '[') {
            prefix_end = i;
            break;
        }
    }

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

    fn make_system(dirs: &[&str]) -> TestSystem {
        let system = TestSystem::default();
        let fs = system.memory_file_system();
        for dir in dirs {
            fs.create_directory_all(SystemPath::new(dir)).unwrap();
        }
        system
    }

    // ── literal_prefix ───────────────────────────────────────────────────────

    #[test]
    fn literal_prefix_single_star() {
        let fallback = SystemPath::new("/fallback");
        assert_eq!(
            literal_prefix("/project/packages/*/src", fallback),
            SystemPathBuf::from("/project/packages")
        );
    }

    #[test]
    fn literal_prefix_star_at_second_segment() {
        let fallback = SystemPath::new("/fallback");
        assert_eq!(
            literal_prefix("/project/*/src", fallback),
            SystemPathBuf::from("/project")
        );
    }

    #[test]
    fn literal_prefix_double_star() {
        let fallback = SystemPath::new("/fallback");
        assert_eq!(
            literal_prefix("/project/packages/**/src", fallback),
            SystemPathBuf::from("/project/packages")
        );
    }

    // ── expand_globs_to_directories ──────────────────────────────────────────

    #[test]
    fn single_level_glob_expands_matching_directories() {
        let system = make_system(&[
            "/project/packages/a/src",
            "/project/packages/b/src",
            "/project/packages/a/tests",
        ]);
        let anchor = SystemPath::new("/project");

        let mut result = expand_globs_to_directories(&[("./packages/*/src", anchor)], &system);
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
        let system = make_system(&[
            "/project/libs/util/src",
            "/project/libs/core/src",
            "/project/libs/core/nested/src",
        ]);
        let anchor = SystemPath::new("/project");

        let mut result = expand_globs_to_directories(&[("./libs/**/src", anchor)], &system);
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
        let system = make_system(&["/project/other"]);
        let anchor = SystemPath::new("/project");

        let result = expand_globs_to_directories(&[("./packages/*/src", anchor)], &system);
        assert!(result.is_empty());
    }

    #[test]
    fn invalid_pep639_pattern_returns_empty() {
        // [!abc] is forbidden by PEP 639
        let system = TestSystem::default();
        let anchor = SystemPath::new("/project");

        let result = expand_globs_to_directories(&[("./[!invalid", anchor)], &system);
        assert!(result.is_empty());
    }

    #[test]
    fn tilde_in_pattern_returns_empty() {
        // `~` is not a valid PEP 639 character
        let system = TestSystem::default();
        let anchor = SystemPath::new("/project");

        let result = expand_globs_to_directories(&[("~/packages/*/src", anchor)], &system);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_patterns_returns_empty() {
        let system = TestSystem::default();
        let result = expand_globs_to_directories(&[], &system);
        assert!(result.is_empty());
    }

    #[test]
    fn multiple_patterns_batched_into_single_result() {
        let system = make_system(&[
            "/project/packages/core/src",
            "/project/packages/utils/src",
            "/project/libs/extra/src",
            "/project/libs/other",
        ]);
        let anchor = SystemPath::new("/project");

        let mut result = expand_globs_to_directories(
            &[("./packages/*/src", anchor), ("./libs/*/src", anchor)],
            &system,
        );
        result.sort();

        assert_eq!(
            result,
            vec![
                SystemPathBuf::from("/project/libs/extra/src"),
                SystemPathBuf::from("/project/packages/core/src"),
                SystemPathBuf::from("/project/packages/utils/src"),
            ]
        );
    }

    #[test]
    fn dfa_prunes_non_matching_subtrees() {
        // Set up a tree with many directories that should NOT be visited.
        // We verify pruning by checking the results are correct — if the DFA
        // were not working, the test would still pass but all directories would
        // be traversed (no correctness impact, but the DFA is also tested via
        // match_directory below).
        let system = make_system(&[
            "/project/packages/a/src",
            "/project/packages/b/src",
            // These should be pruned by the DFA:
            "/project/node_modules/lodash",
            "/project/.git/objects",
            "/project/dist/assets",
        ]);
        let anchor = SystemPath::new("/project");

        let mut result = expand_globs_to_directories(&[("./packages/*/src", anchor)], &system);
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
    fn match_directory_prunes_dead_subtree() {
        // Unit-test the DFA pruning logic directly.
        // Pattern: `/project/packages/*/src`
        // Expected: `match_directory("/project/node_modules")` → false
        let anchor = SystemPath::new("/project");
        let portable =
            PortableGlobPattern::parse("./packages/*/src", PortableGlobKind::Include).unwrap();
        let abs = portable.into_absolute(anchor);
        let abs_str = abs.absolute();

        let prefix_glob = GlobBuilder::new(&format!("{abs_str}/**"))
            .literal_separator(true)
            .backslash_escape(true)
            .build()
            .unwrap();

        let main_sep = regex::escape(MAIN_SEPARATOR_STR);
        let regex = glob_to_regex(&prefix_glob, &main_sep);

        let dfa = build_dfa(&[regex]);

        // The walk root itself should be allowed (so we can descend into packages/).
        assert!(match_directory(SystemPath::new("/project/packages"), &dfa));
        // A matching intermediate dir should be allowed.
        assert!(match_directory(
            SystemPath::new("/project/packages/core"),
            &dfa
        ));
        // A non-matching dir at the same level as packages should be pruned.
        assert!(!match_directory(
            SystemPath::new("/project/node_modules"),
            &dfa
        ));
    }
}

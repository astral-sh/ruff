use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use regex_automata::dfa;
use regex_automata::dfa::Automaton;
use ruff_db::system::SystemPath;
use std::fmt::Formatter;
use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR};
use tracing::warn;

use crate::glob::portable::AbsolutePortableGlobPattern;

/// Chosen at a whim -Konsti
const DFA_SIZE_LIMIT: usize = 1_000_000;

/// Path filter based on a set of include globs.
///
/// The patterns are similar to gitignore, but reversed:
///
/// * `/src`: matches a file or directory with its content named `src`
/// * `/src/`: matches a directory with its content named `src`
/// * `/src/**` or `/src/*`: matches the content of `src`, but not a file named `src`
///
/// Negated patterns are not supported.
///
/// Internally, the globs are converted to a regex and then to a DFA, which unlike the globs and the
/// regex allows to check for prefix matches.
///
/// ## Equality
/// Equality is based on the patterns from which a filter was constructed.
///
/// Because of that, two filters that include the exact same files but were
/// constructed from different patterns (or even just order) compare unequal.
#[derive(Clone, get_size2::GetSize)]
pub(crate) struct IncludeFilter {
    #[get_size(ignore)]
    glob_set: GlobSet,
    original_patterns: Box<[String]>,
    #[get_size(size_fn = dfa_memory_usage)]
    dfa: Option<dfa::dense::DFA<Vec<u32>>>,
}

#[allow(clippy::ref_option)]
fn dfa_memory_usage(dfa: &Option<dfa::dense::DFA<Vec<u32>>>) -> usize {
    dfa.as_ref().map(dfa::dense::DFA::memory_usage).unwrap_or(0)
}

impl IncludeFilter {
    /// Whether the file matches any of the globs.
    pub(crate) fn match_file(&self, path: impl AsRef<SystemPath>) -> bool {
        let path = path.as_ref();

        self.glob_set.is_match(path)
    }

    /// Check whether a directory or any of its children can be matched by any of the globs.
    ///
    /// This never returns `false` if any child matches, but it may return `true` even if we
    /// don't end up including any child.
    pub(crate) fn match_directory(&self, path: impl AsRef<SystemPath>) -> bool {
        self.match_directory_impl(path.as_ref())
    }

    fn match_directory_impl(&self, path: &SystemPath) -> bool {
        let Some(dfa) = &self.dfa else {
            return true;
        };

        // Allow the root path
        if path == SystemPath::new("") {
            return true;
        }

        let config_anchored =
            regex_automata::util::start::Config::new().anchored(regex_automata::Anchored::Yes);
        let mut state = dfa.start_state(&config_anchored).unwrap();

        let byte_path = path
            .as_str()
            .strip_suffix('/')
            .unwrap_or(path.as_str())
            .as_bytes();
        for b in byte_path {
            state = dfa.next_state(state, *b);
        }
        // Say we're looking at a directory `foo/bar`. We want to continue if either `foo/bar` is
        // a match, e.g., from `foo/*`, or a path below it can match, e.g., from `foo/bar/*`.
        let eoi_state = dfa.next_eoi_state(state);
        // We must not call `next_eoi_state` on the slash state, we want to only check if more
        // characters (path components) are allowed, not if we're matching the `$` anchor at the
        // end.
        let slash_state = dfa.next_state(state, u8::try_from(MAIN_SEPARATOR).unwrap());

        debug_assert!(
            !dfa.is_quit_state(eoi_state) && !dfa.is_quit_state(slash_state),
            "matcher is in quit state"
        );

        dfa.is_match_state(eoi_state) || !dfa.is_dead_state(slash_state)
    }
}

impl std::fmt::Debug for IncludeFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("IncludeFilter")
            .field(&self.original_patterns)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for IncludeFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.original_patterns).finish()
    }
}

impl PartialEq for IncludeFilter {
    fn eq(&self, other: &Self) -> bool {
        self.original_patterns == other.original_patterns
    }
}

impl Eq for IncludeFilter {}

#[derive(Debug)]
pub(crate) struct IncludeFilterBuilder {
    set: GlobSetBuilder,
    original_pattern: Vec<String>,
    regexes: Vec<String>,
}

impl IncludeFilterBuilder {
    pub(crate) fn new() -> Self {
        Self {
            set: GlobSetBuilder::new(),
            original_pattern: Vec::new(),
            regexes: Vec::new(),
        }
    }

    /// Adds an include pattern to the filter.
    pub(crate) fn add(
        &mut self,
        input: &AbsolutePortableGlobPattern,
    ) -> Result<&mut Self, globset::Error> {
        let mut glob_pattern = input.absolute();

        let mut only_directory = false;

        // A pattern ending with a `/` should only match directories. E.g. `src/` only matches directories
        // whereas `src` matches both files and directories.
        // We need to remove the `/` to ensure that a path missing the trailing `/` matches.
        if let Some(after) = glob_pattern.strip_suffix('/') {
            // Escaped `/` or `\` aren't allowed. `portable_glob::parse` will error
            only_directory = true;
            glob_pattern = after;
        }

        // If regex ends with `/**`, only push that one glob and regex
        // Otherwise, push two regex, one for `/**` and one for without
        let glob = GlobBuilder::new(glob_pattern)
            .literal_separator(true)
            // No need to support Windows-style paths, so the backslash can be used a escape.
            .backslash_escape(true)
            .build()?;
        self.original_pattern.push(input.relative().to_string());

        // `lib` is the same as `lib/**`
        // Add a glob that matches `lib` exactly, change the glob to `lib/**`.
        if glob_pattern.ends_with("**") {
            self.push_prefix_regex(&glob);
            self.set.add(glob);
        } else {
            let prefix_glob = GlobBuilder::new(&format!("{glob_pattern}/**"))
                .literal_separator(true)
                // No need to support Windows-style paths, so the backslash can be used a escape.
                .backslash_escape(true)
                .build()?;

            self.push_prefix_regex(&prefix_glob);
            self.set.add(prefix_glob);

            // The reason we add the exact glob, e.g. `src` when the original pattern was `src/` is
            // so that `match_file` returns true when matching against a file. However, we don't
            // need to do this if this is a pattern that should only match a directory (specifically, its contents).
            if !only_directory {
                self.set.add(glob);
            }
        }

        Ok(self)
    }

    fn push_prefix_regex(&mut self, glob: &Glob) {
        let main_separator = regex::escape(MAIN_SEPARATOR_STR);

        let regex = glob
            .regex()
            // We are using a custom DFA builder
            .strip_prefix("(?-u)")
            .expect("a glob is a non-unicode byte regex")
            // Match windows paths if applicable
            .replace('/', &main_separator);

        self.regexes.push(regex);
    }

    /// The filter matches if any of the globs matches.
    ///
    /// See <https://github.com/BurntSushi/ripgrep/discussions/2927> for the error returned.
    pub(crate) fn build(self) -> Result<IncludeFilter, globset::Error> {
        let glob_set = self.set.build()?;

        let dfa_builder = dfa::dense::Builder::new()
            .syntax(
                // The glob regex is a byte matcher
                regex_automata::util::syntax::Config::new()
                    .unicode(false)
                    .utf8(false),
            )
            .configure(
                dfa::dense::Config::new()
                    .start_kind(dfa::StartKind::Anchored)
                    // DFA can grow exponentially, in which case we bail out
                    .dfa_size_limit(Some(DFA_SIZE_LIMIT))
                    .determinize_size_limit(Some(DFA_SIZE_LIMIT)),
            )
            .build_many(&self.regexes);
        let dfa = if let Ok(dfa) = dfa_builder {
            Some(dfa)
        } else {
            // TODO(konsti): `regex_automata::dfa::dense::BuildError` should allow asking whether
            // is a size error
            warn!(
                "Glob expressions regex is larger than {DFA_SIZE_LIMIT} bytes, \
                    falling back to full directory traversal!"
            );
            None
        };

        Ok(IncludeFilter {
            glob_set,
            dfa,
            original_patterns: self.original_pattern.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR};

    use crate::glob::include::{IncludeFilter, IncludeFilterBuilder};
    use crate::glob::{PortableGlobKind, PortableGlobPattern};
    use ruff_db::system::{MemoryFileSystem, walk_directory::WalkState};

    fn create_filter(patterns: impl IntoIterator<Item = &'static str>) -> IncludeFilter {
        let mut builder = IncludeFilterBuilder::new();
        for pattern in patterns {
            builder
                .add(
                    &PortableGlobPattern::parse(pattern, PortableGlobKind::Include)
                        .unwrap()
                        .into_absolute(""),
                )
                .unwrap();
        }

        builder.build().unwrap()
    }

    fn setup_files(files: impl IntoIterator<Item = &'static str>) -> MemoryFileSystem {
        let fs = MemoryFileSystem::new();

        fs.write_files_all(files.into_iter().map(|name| (name, "")))
            .unwrap();
        fs
    }

    #[track_caller]
    fn assert_match_directory(filter: &IncludeFilter, path: &str) {
        assert!(filter.match_directory(path.replace('/', MAIN_SEPARATOR_STR)));
    }

    #[track_caller]
    fn assert_not_match_directory(filter: &IncludeFilter, path: &str) {
        assert!(!filter.match_directory(path.replace('/', MAIN_SEPARATOR_STR)));
    }

    #[test]
    fn match_directory() {
        // `lib` is the same as `src/**`. It includes a file or directory (including its contents)
        // `src/*`: The same as `src/**`
        let filter = create_filter(["lib", "src/*", "tests/**", "a/test-*/b", "files/*.py"]);

        assert_match_directory(&filter, "lib");
        assert_match_directory(&filter, "lib/more/test");

        assert_match_directory(&filter, "src");
        assert_match_directory(&filter, "src/more/test");

        assert_match_directory(&filter, "tests");
        assert_match_directory(&filter, "tests/more/test");

        assert_match_directory(&filter, "a");
        assert_match_directory(&filter, "a/test-b");

        assert_not_match_directory(&filter, "a/test-b/x");
        assert_not_match_directory(&filter, "a/test");

        assert_match_directory(&filter, "files/a.py");
        assert_match_directory(&filter, "files/a.py/bcd");

        assert_not_match_directory(&filter, "not_included");
        assert_not_match_directory(&filter, "files/a.pi");
    }

    #[test]
    fn match_file() {
        // `lib` is the same as `src/**`. It includes a file or directory (including its contents)
        // `src/*`: The same as `src/**`
        let filter = create_filter([
            "lib",
            "src/*",
            "directory/",
            "tests/**",
            "a/test-*/b",
            "files/*.py",
        ]);

        assert!(filter.match_file("lib"));
        assert!(filter.match_file("lib/more/test"));

        // Unlike `directory`, `directory/` only includes a directory with the given name and its contents
        assert!(!filter.match_file("directory"));
        assert!(filter.match_file("directory/more/test"));

        // Unlike `src`, `src/*` only includes a directory with the given name.
        assert!(!filter.match_file("src"));
        assert!(filter.match_file("src/more/test"));

        // Unlike `tests`, `tests/**` only includes files under `tests`, but not a file named tests
        assert!(!filter.match_file("tests"));
        assert!(filter.match_file("tests/more/test"));

        // Unlike `match_directory`, prefixes should not be included.
        assert!(!filter.match_file("a"));
        assert!(!filter.match_file("a/test-b"));

        assert!(!filter.match_file("a/test-b/x"));
        assert!(!filter.match_file("a/test"));

        assert!(filter.match_file("files/a.py"));
        assert!(filter.match_file("files/a.py/bcd"));

        assert!(!filter.match_file("not_included"));
        assert!(!filter.match_file("files/a.pi"));
    }

    /// Check that we skip directories that can never match.
    #[test]
    fn prefilter() {
        let filter = create_filter(["/a/b/test-*/d", "/a/b/c/e", "/b/c"]);
        let fs = setup_files([
            // Should visit
            "/a/b/test-a/d",
            "/a/b/c/e",
            "/b/c",
            // Can skip
            "/d/e",
            "/a/b/x/f",
        ]);

        let visited = std::sync::Mutex::new(Vec::new());

        // Test the prefix filtering
        fs.walk_directory("/").run(|| {
            Box::new(|entry| {
                let entry = entry.unwrap();

                if entry.file_type().is_directory() {
                    if !filter.match_directory(entry.path()) {
                        return WalkState::Skip;
                    }
                }

                visited
                    .lock()
                    .unwrap()
                    .push(entry.path().as_str().replace(MAIN_SEPARATOR, "/"));

                WalkState::Continue
            })
        });

        let mut visited = visited.into_inner().unwrap();
        visited.sort();

        // Assert that it didn't traverse into `/d` or `/a/b/x`
        assert_eq!(
            visited,
            [
                "/",
                "/a",
                "/a/b",
                "/a/b/c",
                "/a/b/c/e",
                "/a/b/test-a",
                "/a/b/test-a/d",
                "/b",
                "/b/c"
            ]
        );
    }
}

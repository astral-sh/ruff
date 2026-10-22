//! Loads, matches, and writes diagnostic baselines.
//!
//! Baselines let you adopt ty or enable a new rule without first fixing every existing error or
//! adding individual suppression comments. A baseline records those errors in a project-wide JSON
//! file, so you can focus on new diagnostics while working through the old ones over time.
//! Only lint diagnostics can go in the baseline. Other diagnostics, such as syntax errors and
//! messages from `reveal_type`, still appear.
//!
//! When a diagnostic matches a baseline entry, ty lowers its severity to [`Severity::Hint`].
//! The CLI hides these hints, but the language server still shows them in your editor so you can
//! fix the code as you work. If you suppress a baselined diagnostic with an inline comment,
//! the comment takes precedence and the next baseline update drops the entry.
//!
//! # Recognizing an existing diagnostic
//!
//! As code changes, an existing error can move. For example, adding this import shifts the
//! assignment down by two lines but does nothing to fix the error:
//!
//! ```diff
//! +import logging
//! +
//!  def get_name() -> str: ...
//!
//!  name: int = get_name()
//! ```
//!
//! To recognize an error after an edit, we use two anchors from the surrounding source. One captures
//! the code leading up to the error, and the other starts at the error itself. In each direction,
//! we count up to 100 characters, keeping line breaks and skipping other whitespace, including
//! indentation. A diagnostic matches an entry for the same file and rule if either anchor matches,
//! so the code on the other side can change. This heuristic is inspired by the
//! [two-hash approach][paper] described in section III-C.
//!
//! Within a file, this can preserve matches after:
//!
//! - Adding, removing, or changing code before or after the error.
//! - Changing the spacing around an operator or reindenting a block.
//! - Moving or reordering blocks within the same file.
//! - Editing code outside the hashed context.
//! - Switching between LF, CRLF, and CR line endings.
//!
//! Following an error across files would mean distinguishing moved code from similar code in
//! another file. To keep matching simple, ty only considers entries for the original file path.
//! File renames and moves between files are therefore unsupported.
//!
//! ## Whitespace normalization
//!
//! We skip whitespace other than line breaks when building each anchor so that a match can survive
//! changes in spacing and indentation. For example, removing the spaces around `=` in
//! `name: int = get_name()` leaves both anchors unchanged.
//!
//! We keep line breaks because they can separate Python statements. LF, CRLF, and CR each count
//! as one `\n`, so converting line endings preserves the anchors. We don't lex the source, so even
//! a line break inside parentheses counts. Rewrapping a call can therefore change an anchor.
//!
//! ## What goes into each anchor
//!
//! Both anchors include the word containing the start of the diagnostic's primary range.
//! Whitespace and ASCII punctuation other than `_` separate words, so `get_name` and `123_456`
//! each form one word. The diagram shows the anchor boundaries before whitespace normalization.
//!
//! ```text
//!    name: int = get_name()
//!                ^^^^^^^^^^     primary range
//! ...name: int = get_name        preceding window, read backward
//!                get_name()...  following window, read forward
//! ```
//!
//! Including `get_name` in both means that replacing it with `get_count` changes both anchors. This
//! prevents unchanged preceding code from hiding an error on a different call, but also means a
//! harmless rename can make an existing diagnostic reappear.
//!
//! Each anchor extends up to 100 normalized characters (Unicode scalar values), stopping at file
//! boundaries. We store a hash of each anchor to keep the baseline compact.
//!
//! The 100-character limit keeps enough context to distinguish similar errors without depending on
//! too much nearby code. In Home Assistant trials, shorter windows confused similar diagnostics;
//! larger token windows resurfaced more existing diagnostics in Home Assistant and Airflow.
//!
//! # Reordering and duplicates
//!
//! ty visits diagnostics in source order. For each one, it takes the first unused matching entry
//! anywhere in the file's baseline. If two diagnostics have identical fingerprints and only one
//! entry matches, ty suppresses the first and reports the second.
//!
//! Suppose the baseline records an error in `parse` as entry 1 and an error in `validate` as
//! entry 2. You move `validate` above `parse`, and each error still has a matching anchor.
//!
//! | Diagnostic after the move | Unordered (ty)               | Ordered (forward scan)         |
//! |---------------------------|------------------------------|--------------------------------|
//! | 1. `validate`             | Match entry 2, keep entry 1  | Skip entry 1, match entry 2    |
//! | 2. `parse`                | Match entry 1                | Report the error               |
//!
//! An ordered scan loses the entry for `parse` as soon as it matches `validate`. ty allows
//! out-of-order matches so that moving a function within a file doesn't make an existing error
//! look new.
//!
//! # Format versions
//!
//! Baseline support is experimental and uses format version 0, which may change between ty releases.
//! We will bump the format version to 1 when promoting the feature to stable. From then on, ty must
//! be able to read and match baselines written by the previous ty version. Updating a baseline
//! may always write the current format.
//!
//! [paper]: https://codeql.github.com/publications/tracking-analysis-violations.pdf

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::Hasher;
use std::str::FromStr;

use anyhow::Context;
use compact_str::CompactString;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
use ruff_db::files::{File, FilePath, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;
use rustc_stable_hash::{FromStableHash, SipHasher128Hash, StableSipHasher128};
use serde::{Deserialize, Serialize};

use crate::Db;

const BASELINE_VERSION: u32 = 0;
const CONTEXT_SIZE: usize = 100;

/// Returns a diagnostic if the configured baseline cannot be loaded.
pub(crate) fn settings_diagnostic(db: &dyn Db) -> Option<Diagnostic> {
    db.project().settings(db).baseline()?;
    baselines(db).err().map(Error::to_diagnostic)
}

/// Demotes matched diagnostics to hints and adds a note identifying the baseline.
pub(crate) fn demote_diagnostics(db: &dyn Db, file: File, diagnostics: &mut [Diagnostic]) {
    let Some(baseline) = baseline(db, file) else {
        return;
    };

    let source = source_text(db, file);
    let mut entries: Vec<_> = diagnostics
        .iter()
        .enumerate()
        .filter_map(|(index, diagnostic)| {
            let diagnostic = BaselineDiagnostic::from_diagnostic(diagnostic)?;
            Some((index, diagnostic.entry(source.as_str())))
        })
        .collect();
    entries.sort_by(|(_, left), (_, right)| left.cmp(right));

    // Match the current diagnostics against the entries recorded for this file in the baseline.
    let mut matcher = BaselineMatcher::new(baseline.entries);
    for (index, entry) in entries {
        if matcher.matches(&entry) {
            let diagnostic = &mut diagnostics[index];
            diagnostic.set_severity(Severity::Hint);
            diagnostic.info(format_args!(
                "Suppressed by baseline `{}`",
                baseline.display_path
            ));
        }
    }
}

#[derive(Debug)]
pub struct WriteResult {
    /// The number of diagnostics written to the baseline.
    pub count: usize,
    /// Diagnostics that still need to be reported because the baseline cannot suppress them,
    /// such as syntax errors. A successful write clears any error from loading the old baseline.
    pub diagnostics: Vec<Diagnostic>,
}

/// Writes a new baseline containing lint diagnostics with a primary source range in the project.
///
/// Returns the number of entries written and the diagnostics still to report.
pub fn write(
    db: &dyn Db,
    path: &SystemPath,
    diagnostics: Vec<Diagnostic>,
) -> anyhow::Result<WriteResult> {
    let (baseline, mut remaining) = BaselineFile::from_diagnostics(db, diagnostics);
    let count = baseline.write(db, path)?;
    // Errors from loading the old baseline no longer apply after a successful write.
    remaining.retain(|diagnostic| diagnostic.id() != DiagnosticId::InvalidBaseline);
    Ok(WriteResult {
        count,
        diagnostics: remaining,
    })
}

/// A borrowed view of the entries in a project-wide [`BaselineFile`] for one source file.
#[derive(Debug)]
struct FileBaseline<'a> {
    /// Path to the baseline file shown in diagnostic notes.
    display_path: &'a SystemPath,
    entries: &'a [BaselineEntry],
}

/// Returns the baseline for `file` without depending on the query when baselines are disabled.
fn baseline(db: &dyn Db, file: File) -> Option<FileBaseline<'_>> {
    let project = db.project();
    let path = project.settings(db).baseline()?;

    let Ok(Some(baseline)) = baselines(db) else {
        return None;
    };
    let entries = baseline.for_file(db, file)?;
    Some(FileBaseline {
        display_path: path.strip_prefix(project.root(db)).unwrap_or(path),
        entries,
    })
}

/// Loads and validates the configured baseline.
#[salsa::tracked(returns(as_ref), heap_size = ruff_memory_usage::heap_size)]
fn baselines(db: &dyn Db) -> Result<Option<BaselineFile>, Error> {
    let project = db.project();
    let Some(path) = project.settings(db).baseline() else {
        return Ok(None);
    };

    let file = system_path_to_file(db, path).map_err(|error| Error::Read {
        path: path.to_path_buf(),
        file: None,
        detail: error.to_string(),
    })?;

    let source = source_text(db, file);
    if let Some(error) = source.read_error() {
        return Err(Error::Read {
            path: path.to_path_buf(),
            file: Some(file),
            detail: error.to_string(),
        });
    }

    let baseline: BaselineFile =
        serde_json::from_str(source.as_str()).map_err(|error| Error::Parse {
            path: path.to_path_buf(),
            file,
            detail: error.to_string(),
        })?;

    if baseline.version != BASELINE_VERSION {
        return Err(Error::UnsupportedVersion {
            file,
            version: baseline.version,
        });
    }

    Ok(Some(baseline))
}

/// The project-wide diagnostic baseline as stored in JSON.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, get_size2::GetSize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineFile {
    version: u32,
    /// Map from source file path to its baseline entries. Paths are relative to the project root.
    files: BTreeMap<SystemPathBuf, Box<[BaselineEntry]>>,
}

impl BaselineFile {
    fn for_file(&self, db: &dyn Db, file: File) -> Option<&[BaselineEntry]> {
        let path = project_relative_path(db, file)?;
        self.files.get(path).map(Box::as_ref)
    }

    /// Builds a baseline and returns the diagnostics it cannot record.
    fn from_diagnostics(db: &dyn Db, mut diagnostics: Vec<Diagnostic>) -> (Self, Vec<Diagnostic>) {
        let mut by_file = FxHashMap::default();
        diagnostics.retain(|diagnostic| {
            let Some(diagnostic) = BaselineDiagnostic::from_diagnostic(diagnostic) else {
                return true;
            };
            let Some(path) = project_relative_path(db, diagnostic.file) else {
                return true;
            };
            let (_, file_diagnostics) = by_file
                .entry(diagnostic.file)
                .or_insert_with(|| (path, Vec::new()));
            file_diagnostics.push(diagnostic);
            false
        });

        let mut files = BTreeMap::new();
        #[expect(
            clippy::iter_over_hash_type,
            reason = "each file is processed independently and its entries are sorted"
        )]
        for (file, (path, diagnostics)) in by_file {
            let source = source_text(db, file);
            let mut entries: Vec<_> = diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.entry(source.as_str()))
                .collect();
            entries.sort();
            let mut portable_path = String::with_capacity(path.as_str().len());
            for component in path.components() {
                if !portable_path.is_empty() {
                    portable_path.push('/');
                }
                portable_path.push_str(component.as_str());
            }
            files.insert(
                SystemPathBuf::from(portable_path),
                entries.into_boxed_slice(),
            );
        }

        let baseline = Self {
            version: BASELINE_VERSION,
            files,
        };
        (baseline, diagnostics)
    }

    fn write(self, db: &dyn Db, path: &SystemPath) -> anyhow::Result<usize> {
        let count = self.files.values().map(|entries| entries.len()).sum();
        let mut serialized = serde_json::to_string_pretty(&self)?;
        serialized.push('\n');

        let writable = db
            .system()
            .as_writable()
            .context("The active file system is read-only")?;
        if let Some(parent) = path.parent() {
            writable
                .create_directory_all(parent)
                .with_context(|| format!("Failed to create baseline directory `{parent}`"))?;
        }
        writable
            .write_file(path, &serialized)
            .with_context(|| format!("Failed to write baseline `{path}`"))?;
        Ok(count)
    }
}

/// The rule and context hashes for one diagnostic in a [`BaselineFile`].
#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, get_size2::GetSize,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineEntry {
    /// Used to sort generated entries; baseline files preserve their serialized array order.
    #[serde(skip)]
    offset: TextSize,
    rule: CompactString,
    preceding_hash: BaselineHash,
    following_hash: BaselineHash,
}

impl BaselineEntry {
    fn matches(&self, other: &Self) -> bool {
        self.rule == other.rule
            && (self.preceding_hash == other.preceding_hash
                || self.following_hash == other.following_hash)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error, get_size2::GetSize)]
enum Error {
    #[error("Failed to read baseline `{path}`")]
    Read {
        path: SystemPathBuf,
        file: Option<File>,
        detail: String,
    },

    #[error("Failed to parse baseline `{path}`")]
    Parse {
        path: SystemPathBuf,
        file: File,
        detail: String,
    },

    #[error("Unsupported baseline version `{version}`")]
    UnsupportedVersion { file: File, version: u32 },
}

impl Error {
    fn to_diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidBaseline,
            Severity::Error,
            self.to_string(),
        );
        let (file, detail) = match self {
            Self::Read { file, detail, .. } => (*file, detail.clone()),
            Self::Parse { file, detail, .. } => (Some(*file), detail.clone()),
            Self::UnsupportedVersion { file, .. } => (
                Some(*file),
                format!("ty supports baseline version `{BASELINE_VERSION}`"),
            ),
        };

        if let Some(file) = file {
            diagnostic.annotate(Annotation::primary(
                Span::from(file).with_range(TextRange::default()),
            ));
        }
        diagnostic.info(detail);
        diagnostic
    }
}

fn project_relative_path(db: &dyn Db, file: File) -> Option<&SystemPath> {
    let FilePath::System(path) = file.path(db) else {
        return None;
    };
    path.strip_prefix(db.project().root(db)).ok()
}

struct BaselineDiagnostic {
    rule: LintName,
    file: File,
    range: TextRange,
}

impl BaselineDiagnostic {
    fn from_diagnostic(diagnostic: &Diagnostic) -> Option<Self> {
        let rule = diagnostic.id().as_lint()?;
        let span = diagnostic.primary_span()?;
        Some(Self {
            rule,
            file: span.expect_ty_file(),
            range: span.range()?,
        })
    }

    fn entry(&self, source: &str) -> BaselineEntry {
        let (preceding_hash, following_hash) = context_hashes(source, self.range.start());
        BaselineEntry {
            offset: self.range.start(),
            rule: CompactString::const_new(self.rule.as_str()),
            preceding_hash,
            following_hash,
        }
    }
}

/// Matches each diagnostic to the first unused baseline entry for its source file.
///
/// `next` is the index of the first unused entry, or `entries.len()` when all entries are used.
/// We write baseline entries and visit current diagnostics in source order. When the entries
/// are unchanged since the baseline was written, each diagnostic matches at `next`.
///
/// If a diagnostic matches a later entry, we leave `next` in place so the skipped entries can
/// still match other diagnostics. We allocate `consumed` on the first such match. A set bit
/// marks an entry already matched out of order, so we skip it when searching or advancing `next`.
struct BaselineMatcher<'a> {
    entries: &'a [BaselineEntry],
    next: usize,
    consumed: Box<[bool]>,
}

impl<'a> BaselineMatcher<'a> {
    fn new(entries: &'a [BaselineEntry]) -> Self {
        Self {
            entries,
            next: 0,
            consumed: Box::new([]),
        }
    }

    /// Returns true on the first unused match and marks that baseline entry as used.
    fn matches(&mut self, current: &BaselineEntry) -> bool {
        let remaining = &self.entries[self.next..];
        if let Some(entry) = remaining.first()
            && entry.matches(current)
        {
            self.next += 1;
            while self.consumed.get(self.next) == Some(&true) {
                self.next += 1;
            }
            return true;
        }

        let Some((offset, _)) = remaining
            .iter()
            .enumerate()
            .skip(1)
            .find(|(offset, entry)| {
                self.consumed.get(self.next + offset) != Some(&true) && entry.matches(current)
            })
        else {
            return false;
        };

        if self.consumed.is_empty() {
            self.consumed = vec![false; self.entries.len()].into_boxed_slice();
        }
        self.consumed[self.next + offset] = true;
        true
    }
}

/// Anchors both windows at the word containing `offset`; the diagnostic's end is not used.
/// The character limit includes the word itself (truncating long words) and normalized newlines.
fn context_hashes(source: &str, offset: TextSize) -> (BaselineHash, BaselineHash) {
    let offset = usize::from(offset);
    let (word_start, word_end) = word_bounds(source, offset);

    (
        BaselineHash::backward(&source[..word_end]),
        BaselineHash::forward(&source[word_start..]),
    )
}

/// Returns the bounds of the word containing `offset`.
/// An offset on a separator selects just that character.
fn word_bounds(source: &str, offset: usize) -> (usize, usize) {
    let Some(character) = source[offset..].chars().next() else {
        return (offset, offset);
    };

    let is_separator = |c: char| c.is_whitespace() || (c.is_ascii_punctuation() && c != '_');
    if is_separator(character) {
        return (offset, offset + character.len_utf8());
    }

    let start = source[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, character)| !is_separator(*character))
        .last()
        .map_or(offset, |(index, _)| index);
    let end = source[offset..]
        .char_indices()
        .find(|(_, character)| is_separator(*character))
        .map_or(source.len(), |(index, _)| offset + index);

    (start, end)
}

#[derive(Clone, Copy)]
enum HashDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, get_size2::GetSize)]
struct BaselineHash(u128);

impl BaselineHash {
    fn forward(context: &str) -> Self {
        Self::from_context(context, HashDirection::Forward)
    }

    fn backward(context: &str) -> Self {
        Self::from_context(context, HashDirection::Backward)
    }

    fn from_context(context: &str, direction: HashDirection) -> Self {
        let mut hasher = StableSipHasher128::new();
        let mut characters = context.char_indices();
        let mut length = 0_u8;

        while usize::from(length) < CONTEXT_SIZE {
            let character = match direction {
                HashDirection::Forward => characters.next(),
                HashDirection::Backward => characters.next_back(),
            };
            let Some((index, character)) = character else {
                break;
            };
            let character = match character {
                '\r' if context.as_bytes().get(index + 1) == Some(&b'\n') => continue,
                '\r' | '\n' => '\n',
                character if character.is_whitespace() => continue,
                character => character,
            };

            hasher.write_u32(u32::from(character));
            length += 1;
        }

        hasher.write_u8(length);
        hasher.finish()
    }
}

impl FromStableHash for BaselineHash {
    type Hash = SipHasher128Hash;

    fn from(SipHasher128Hash([first, second]): SipHasher128Hash) -> Self {
        Self((u128::from(first) << 64) | u128::from(second))
    }
}

impl fmt::Display for BaselineHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

impl FromStr for BaselineHash {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() == 32
            && !value.starts_with('+')
            && let Ok(hash) = u128::from_str_radix(value, 16)
        {
            Ok(Self(hash))
        } else {
            Err("expected 32 hexadecimal digits")
        }
    }
}

impl Serialize for BaselineHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BaselineHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(transparent)]
        struct HashText<'a>(#[serde(borrow)] Cow<'a, str>);

        let HashText(text) = HashText::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::diagnostic::Severity;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_db::testing::assert_const_function_query_was_not_run;
    use ruff_text_size::TextSize;

    use crate::ProjectMetadata;
    use crate::db::testing::TestDb;
    use ty_python_semantic::Db as _;

    use super::{
        BaselineEntry, BaselineHash, BaselineMatcher, baselines, context_hashes, word_bounds,
    };

    fn entry(rule: &str, preceding: &str, following: &str) -> BaselineEntry {
        BaselineEntry {
            offset: 0.into(),
            rule: rule.into(),
            preceding_hash: BaselineHash::backward(preceding),
            following_hash: BaselineHash::forward(following),
        }
    }

    #[test]
    fn baseline_hash_serializes_as_hexadecimal() -> anyhow::Result<()> {
        let hash = BaselineHash(42);
        let serialized = r#""0000000000000000000000000000002a""#;

        assert_eq!(serde_json::to_string(&hash)?, serialized);
        assert_eq!(serde_json::from_str::<BaselineHash>(serialized)?, hash);
        Ok(())
    }

    #[test]
    fn baseline_hash_rejects_invalid_hexadecimal() {
        assert!("2a".parse::<BaselineHash>().is_err());
        assert!("g".repeat(32).parse::<BaselineHash>().is_err());
        let signed = format!("+{}", "0".repeat(31));
        assert!(signed.parse::<BaselineHash>().is_err());
    }

    #[test]
    fn entries_match_by_rule_and_either_hash() {
        assert!(entry("rule", "a", "b").matches(&entry("rule", "a", "c")));
        assert!(entry("rule", "a", "b").matches(&entry("rule", "c", "b")));

        assert!(!entry("rule", "a", "b").matches(&entry("rule", "c", "d")));
        assert!(!entry("rule", "a", "b").matches(&entry("other-rule", "a", "b")));
    }

    #[test]
    fn baseline_hash_ignores_non_newline_whitespace() {
        let spaced = "a + \tç . d";
        let compact = "a+ç.d";
        assert_eq!(
            BaselineHash::forward(spaced),
            BaselineHash::forward(compact)
        );
        assert_eq!(
            BaselineHash::backward(spaced),
            BaselineHash::backward(compact)
        );
    }

    #[test]
    fn baseline_hash_preserves_newlines() {
        assert_ne!(BaselineHash::forward("a\nb"), BaselineHash::forward("a b"));
        assert_ne!(
            BaselineHash::backward("a\nb"),
            BaselineHash::backward("a b")
        );
    }

    #[test]
    fn baseline_hash_normalizes_line_endings() {
        let forward = BaselineHash::forward("a\nb");
        assert_eq!(BaselineHash::forward("a\rb"), forward);
        assert_eq!(BaselineHash::forward("a\r\nb"), forward);

        let backward = BaselineHash::backward("a\nb");
        assert_eq!(BaselineHash::backward("a\rb"), backward);
        assert_eq!(BaselineHash::backward("a\r\nb"), backward);
    }

    #[test]
    fn baseline_hash_counts_100_normalized_characters() {
        let window = "ç ".repeat(100);
        let shorter = "ç ".repeat(99);

        let forward = BaselineHash::forward(&window);
        assert_eq!(BaselineHash::forward(&format!("{window}x")), forward);
        assert_ne!(BaselineHash::forward(&shorter), forward);

        let backward = BaselineHash::backward(&window);
        assert_eq!(BaselineHash::backward(&format!("x{window}")), backward);
        assert_ne!(BaselineHash::backward(&shorter), backward);
    }

    #[test]
    fn context_hashes_overlap_the_whole_word() {
        let source = "before café_word2.after";
        let expected = (
            BaselineHash::backward("before café_word2"),
            BaselineHash::forward("café_word2.after"),
        );
        let word_start = TextSize::of("before ");
        let inside_word = TextSize::of("before café_");
        assert_eq!(context_hashes(source, word_start), expected);
        assert_eq!(context_hashes(source, inside_word), expected);
    }

    #[test]
    fn words_end_at_ascii_punctuation_or_whitespace() {
        fn word_at(source: &str, offset: usize) -> &str {
            let (start, end) = word_bounds(source, offset);
            &source[start..end]
        }

        assert_eq!(
            word_at("x+cafe\u{301}_2\u{a0}y", "x+cafe".len()),
            "cafe\u{301}_2"
        );
        assert_eq!(word_at("x$123_456?y", 5), "123_456");
        assert_eq!(word_at("x`a—b.y", 3), "a—b");
    }

    #[test]
    fn context_hashes_overlap_punctuation() {
        let (preceding, following) = context_hashes("before + after", 7.into());

        assert_eq!(preceding, BaselineHash::backward("before+"));
        assert_eq!(following, BaselineHash::forward("+after"));
    }

    #[test]
    fn matching_preserves_duplicate_cardinality() {
        let duplicate = entry("rule", "before", "after");
        let baseline = [duplicate.clone(), duplicate.clone()];
        let mut matcher = BaselineMatcher::new(&baseline);

        assert!(matcher.matches(&duplicate));
        assert!(matcher.matches(&duplicate));
        assert!(!matcher.matches(&duplicate));
    }

    #[test]
    fn matching_allows_reordered_diagnostics() {
        let first = entry("rule", "first-before", "first-after");
        let second = entry("rule", "second-before", "second-after");
        let baseline = [first.clone(), second.clone()];
        let mut matcher = BaselineMatcher::new(&baseline);

        assert!(matcher.matches(&second));
        assert!(matcher.matches(&first));
    }

    #[test]
    fn matching_retains_skipped_entries_after_matching_prefix() {
        let prefix = entry("prefix-rule", "prefix-before", "prefix-after");
        let first = entry("rule", "first-before", "first-after");
        let second = entry("other-rule", "second-before", "second-after");
        let new = entry("rule", "new-before", "new-after");
        let baseline = [prefix.clone(), first.clone(), second.clone(), first.clone()];
        let mut matcher = BaselineMatcher::new(&baseline);

        assert!(matcher.matches(&prefix));
        assert!(!matcher.matches(&new));
        assert!(matcher.matches(&second));
        assert!(!matcher.matches(&second));
        assert!(matcher.matches(&first));
        assert!(matcher.matches(&first));
        assert!(!matcher.matches(&first));
    }

    #[test]
    fn disabled_baseline_does_not_run_baseline_query() -> anyhow::Result<()> {
        let mut db = TestDb::new(ProjectMetadata::new("test", "/project".into()));
        db.write_file("/project/test.py", "x: int = 'wrong'\n")?;
        let file = system_path_to_file(&db, "/project/test.py")?;
        db.take_salsa_events();

        assert_eq!(db.check_file(file)[0].severity(), Severity::Error);
        let events = db.take_salsa_events();
        assert_const_function_query_was_not_run(&db, baselines, &events);
        Ok(())
    }
}

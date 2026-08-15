//! Loads, matches, and writes diagnostic baselines.
//!
//! A diagnostic can move when code is added or removed. Its source offset alone cannot identify
//! the same diagnostic across revisions. ty instead uses nearby source text, based on section III-C
//! of [*Tracking Static Analysis Violations Over Time to Capture Developer Characteristics*][paper].
//!
//! Each baseline entry contains a lint rule and two hashes. Each hash includes up to 100 characters.
//! Line breaks are kept and normalized to `\n`. Other whitespace is ignored. The first hash reads
//! backward from the end of the first word in the diagnostic. The second hash reads forward from
//! the beginning of the same word. Including this word in both hashes ties both hashes to the
//! diagnostic itself. Either hash can still match when the other changes. Ignoring other whitespace
//! keeps the hashes stable when formatting or indentation changes.
//!
//! A diagnostic matches an entry when the file and rule are the same and either hash matches.
//! Diagnostics and entries are matched in source order. Each entry matches at most one diagnostic.
//! A match changes the diagnostic's severity to [`Severity::Hint`]. The caller decides whether to
//! show hints.
//!
//! # Format versions
//!
//! The current baseline format is version 0. Version 0 is unstable and does not need to be
//! compatible with other ty releases.
//!
//! Version 1 will be the first stable format. After that, a ty release must be able to read and
//! compare baseline files written by the previous major ty release. This can include more than one
//! format version. Updating a baseline may always write the current format version.
//!
//! [paper]: https://codeql.github.com/publications/tracking-analysis-violations.pdf

use std::collections::BTreeMap;
use std::fmt;
use std::hash::Hasher;
use std::sync::Arc;

use anyhow::Context;
use compact_str::CompactString;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
use ruff_db::files::{File, FilePath, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;
use rustc_stable_hash::{FromStableHash, SipHasher128Hash, StableSipHasher128};
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};

use crate::Db;

const BASELINE_VERSION: u32 = 0;
const CONTEXT_SIZE: usize = 100;

/// Returns a diagnostic if the configured baseline cannot be loaded.
pub(crate) fn settings_diagnostic(db: &dyn Db) -> Option<Diagnostic> {
    db.project().settings(db).baseline()?;
    baselines(db).err().map(Error::to_diagnostic)
}

/// Demotes diagnostics matched by the configured baseline to hint severity.
pub(crate) fn demote_diagnostics(db: &dyn Db, file: File, diagnostics: &mut [Diagnostic]) {
    let Some(baseline_entries) = baseline(db, file) else {
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

    let mut remaining = baseline_entries;
    for (index, entry) in entries {
        if consume_matching_entry(&mut remaining, &entry) {
            diagnostics[index].set_severity(Severity::Hint);
        }
    }
}

/// Returns `true` for lint diagnostics that can be written to a baseline.
pub fn diagnostic_is_eligible(db: &dyn Db, diagnostic: &Diagnostic) -> bool {
    BaselineDiagnostic::from_diagnostic(diagnostic)
        .and_then(|diagnostic| project_relative_path(db, diagnostic.file))
        .is_some()
}

/// Writes a new baseline containing all eligible diagnostics.
pub fn write(db: &dyn Db, path: &SystemPath, diagnostics: &[Diagnostic]) -> anyhow::Result<usize> {
    BaselineFile::from_diagnostics(db, diagnostics).write(db, path)
}

/// Returns the baseline for `file` without depending on the query when baselines are disabled.
#[inline]
fn baseline(db: &dyn Db, file: File) -> Option<&[BaselineEntry]> {
    #[salsa::tracked(returns(as_deref), heap_size = ruff_memory_usage::heap_size)]
    fn baseline_impl(db: &dyn Db, file: File) -> Option<Arc<[BaselineEntry]>> {
        let path = project_relative_path(db, file)?;
        let baseline = baselines(db).ok()?.as_ref()?;
        baseline.0.get(path).cloned()
    }

    db.project().settings(db).baseline()?;
    baseline_impl(db, file)
}

/// Loads and validates the configured baseline.
///
/// Reading through `source_text` makes this query dependent on the baseline file's contents.
#[salsa::tracked(returns(as_ref), heap_size = ruff_memory_usage::heap_size)]
fn baselines(db: &dyn Db) -> Result<Option<Baseline>, Error> {
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

    Ok(Some(Baseline(baseline.files)))
}

#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize)]
struct Baseline(BTreeMap<SystemPathBuf, Arc<[BaselineEntry]>>);

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineFile {
    version: u32,
    /// Source-file paths relative to the project root.
    files: BTreeMap<SystemPathBuf, Arc<[BaselineEntry]>>,
}

impl BaselineFile {
    fn from_diagnostics(db: &dyn Db, diagnostics: &[Diagnostic]) -> Self {
        let mut grouped: FxHashMap<File, Vec<BaselineDiagnostic>> = FxHashMap::default();
        for diagnostic in diagnostics {
            let Some(diagnostic) = BaselineDiagnostic::from_diagnostic(diagnostic) else {
                continue;
            };
            grouped.entry(diagnostic.file).or_default().push(diagnostic);
        }

        let mut files = BTreeMap::new();
        #[expect(
            clippy::iter_over_hash_type,
            reason = "each file is processed independently and its entries are sorted"
        )]
        for (file, diagnostics) in grouped {
            let Some(path) = project_relative_path(db, file) else {
                continue;
            };
            let source = source_text(db, file);
            let mut entries: Vec<_> = diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.entry(source.as_str()))
                .collect();
            entries.sort();
            files.insert(
                SystemPathBuf::from(path.as_str().replace('\\', "/")),
                Arc::from(entries),
            );
        }

        Self {
            version: BASELINE_VERSION,
            files,
        }
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
        let span = diagnostic.expect_primary_span();
        Some(Self {
            rule,
            file: span.expect_ty_file(),
            range: span.range().expect("lint diagnostics always have a range"),
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

fn consume_matching_entry(baseline: &mut &[BaselineEntry], current: &BaselineEntry) -> bool {
    let Some(index) = baseline
        .iter()
        .position(|entry| two_hash_matches(entry, current))
    else {
        return false;
    };

    *baseline = &baseline[index + 1..];
    true
}

fn two_hash_matches(baseline: &BaselineEntry, current: &BaselineEntry) -> bool {
    baseline.rule == current.rule
        && (baseline.preceding_hash == current.preceding_hash
            || baseline.following_hash == current.following_hash)
}

fn context_hashes(source: &str, offset: TextSize) -> (BaselineHash, BaselineHash) {
    let offset = usize::from(offset);
    let (word_start, word_end) = word_bounds(source, offset);

    (
        stable_hash(&source[..word_end], HashDirection::Backward),
        stable_hash(&source[word_start..], HashDirection::Forward),
    )
}

fn word_bounds(source: &str, offset: usize) -> (usize, usize) {
    let Some(character) = source[offset..].chars().next() else {
        return (offset, offset);
    };

    let is_word_character = |character: char| character == '_' || character.is_alphanumeric();
    if !is_word_character(character) {
        return (offset, offset + character.len_utf8());
    }

    let start = source[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, character)| is_word_character(*character))
        .last()
        .map_or(offset, |(index, _)| index);
    let end = source[offset..]
        .char_indices()
        .find(|(_, character)| !is_word_character(*character))
        .map_or(source.len(), |(index, _)| offset + index);

    (start, end)
}

#[derive(Clone, Copy)]
enum HashDirection {
    Forward,
    Backward,
}

fn stable_hash(context: &str, direction: HashDirection) -> BaselineHash {
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, get_size2::GetSize)]
struct BaselineHash(u128);

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
        struct BaselineHashVisitor;

        impl Visitor<'_> for BaselineHashVisitor {
            type Value = BaselineHash;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a 32-character hexadecimal hash")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.len() != 32 {
                    return Err(E::invalid_length(value.len(), &self));
                }

                u128::from_str_radix(value, 16)
                    .map(BaselineHash)
                    .map_err(E::custom)
            }
        }

        deserializer.deserialize_str(BaselineHashVisitor)
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_db::testing::{
        assert_const_function_query_was_not_run, assert_function_query_was_not_run,
        assert_function_query_was_not_run_by_name, find_will_execute_event_by_name,
    };
    use ruff_text_size::TextRange;

    use crate::db::Db as _;
    use crate::db::testing::TestDb;
    use crate::metadata::options::Options;
    use crate::metadata::value::RelativePathBuf;
    use crate::{ProjectMetadata, check_file_impl};
    use ty_python_semantic::Db as _;

    use super::{
        BaselineEntry, BaselineFile, BaselineHash, Error, HashDirection, baseline, baselines,
        consume_matching_entry, context_hashes, demote_diagnostics, stable_hash, two_hash_matches,
        write,
    };

    fn entry(rule: &str, preceding: &str, following: &str) -> BaselineEntry {
        BaselineEntry {
            offset: 0.into(),
            rule: rule.into(),
            preceding_hash: stable_hash(preceding, HashDirection::Backward),
            following_hash: stable_hash(following, HashDirection::Forward),
        }
    }

    #[test]
    fn baseline_hash_is_inline_and_serializes_as_hexadecimal() -> anyhow::Result<()> {
        let hash = BaselineHash(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let serialized = r#""0123456789abcdef0123456789abcdef""#;

        assert_eq!(std::mem::size_of::<BaselineHash>(), 16);
        assert_eq!(serde_json::to_string(&hash)?, serialized);
        assert_eq!(serde_json::from_str::<BaselineHash>(serialized)?, hash);
        assert_eq!(
            serde_json::from_str::<BaselineHash>(r#""\u0030123456789abcdef0123456789abcdef""#,)?,
            hash
        );
        assert!(serde_json::from_str::<BaselineHash>(r#""0123456789abcdef""#).is_err());
        assert!(
            serde_json::from_str::<BaselineHash>(r#""0123456789abcdef0123456789abcdeg""#).is_err()
        );
        Ok(())
    }

    #[test]
    fn two_hash_matches_either_context() {
        assert!(two_hash_matches(
            &entry("rule", "a", "b"),
            &entry("rule", "a", "c")
        ));
        assert!(two_hash_matches(
            &entry("rule", "a", "b"),
            &entry("rule", "c", "b")
        ));
        assert!(!two_hash_matches(
            &entry("rule", "a", "b"),
            &entry("rule", "c", "d")
        ));

        assert!(!two_hash_matches(
            &entry("rule", "a", "b"),
            &entry("other-rule", "a", "b")
        ));
    }

    #[test]
    fn context_hash_ignores_non_newline_whitespace() {
        assert_eq!(
            context_hashes("a + \tç . d", 5.into()),
            context_hashes("a+ç.d", 2.into())
        );
    }

    #[test]
    fn context_hashes_preserve_newlines() {
        let (preceding, following) = context_hashes("before\nword\nafter", 7.into());

        assert_eq!(
            preceding,
            stable_hash("before\nword", HashDirection::Backward)
        );
        assert_eq!(
            following,
            stable_hash("word\nafter", HashDirection::Forward)
        );
        assert_ne!(
            (preceding, following),
            context_hashes("before word after", 7.into())
        );
    }

    #[test]
    fn context_hashes_normalize_line_endings() {
        let expected = context_hashes("before\nword\nafter", 7.into());

        assert_eq!(expected, context_hashes("before\rword\rafter", 7.into()));
        assert_eq!(
            expected,
            context_hashes("before\r\nword\r\nafter", 8.into())
        );
    }

    #[test]
    fn context_hashes_limit_each_window() {
        let source = format!("{}target{}", "ç ".repeat(100), " β".repeat(100));
        let (preceding, following) = context_hashes(&source, 300.into());

        assert_eq!(
            preceding,
            stable_hash(
                &format!("{}target", "ç".repeat(94)),
                HashDirection::Backward
            )
        );
        assert_eq!(
            following,
            stable_hash(&format!("target{}", "β".repeat(94)), HashDirection::Forward)
        );
    }

    #[test]
    fn context_hashes_overlap_complete_word() {
        let (preceding, following) = context_hashes("before diagnostic_word.after", 7.into());

        assert_eq!(
            preceding,
            stable_hash("beforediagnostic_word", HashDirection::Backward)
        );
        assert_eq!(
            following,
            stable_hash("diagnostic_word.after", HashDirection::Forward)
        );
    }

    #[test]
    fn context_hashes_overlap_word_containing_diagnostic_start() {
        let (preceding, following) = context_hashes("before café_word2.after", 9.into());

        assert_eq!(
            preceding,
            stable_hash("beforecafé_word2", HashDirection::Backward)
        );
        assert_eq!(
            following,
            stable_hash("café_word2.after", HashDirection::Forward)
        );
    }

    #[test]
    fn context_hashes_overlap_punctuation() {
        let (preceding, following) = context_hashes("before + after", 7.into());

        assert_eq!(preceding, stable_hash("before+", HashDirection::Backward));
        assert_eq!(following, stable_hash("+after", HashDirection::Forward));
    }

    #[test]
    fn schema_validation() {
        let unknown_top_level = r#"{"version":0,"files":{},"unknown":true}"#;
        assert!(serde_json::from_str::<BaselineFile>(unknown_top_level).is_err());

        let valid = r#"{
            "version": 0,
            "files": {
                "test.py": [{
                    "rule": "invalid-assignment",
                    "precedingHash": "0123456789abcdef0123456789abcdef",
                    "followingHash": "fedcba9876543210fedcba9876543210"
                }]
            }
        }"#;
        assert!(serde_json::from_str::<BaselineFile>(valid).is_ok());

        let message = r#"{
            "version": 0,
            "files": {
                "test.py": [{
                    "rule": "invalid-assignment",
                    "precedingHash": "0123456789abcdef0123456789abcdef",
                    "followingHash": "fedcba9876543210fedcba9876543210",
                    "message": "review only"
                }]
            }
        }"#;
        assert!(serde_json::from_str::<BaselineFile>(message).is_err());
    }

    #[test]
    fn ordered_matching_preserves_duplicate_cardinality() {
        let duplicate = entry("rule", "before", "after");
        let baseline = [duplicate.clone(), duplicate.clone()];
        let mut remaining = baseline.as_slice();

        assert!(consume_matching_entry(&mut remaining, &duplicate));
        assert!(consume_matching_entry(&mut remaining, &duplicate));
        assert!(!consume_matching_entry(&mut remaining, &duplicate));
    }

    #[test]
    fn matching_preserves_diagnostic_order() {
        let first = entry("rule", "first-before", "first-after");
        let second = entry("rule", "second-before", "second-after");
        let baseline = [first.clone(), second.clone()];
        let mut remaining = baseline.as_slice();

        assert!(consume_matching_entry(&mut remaining, &second));
        assert!(!consume_matching_entry(&mut remaining, &first));
    }

    fn test_db_with_baseline() -> TestDb {
        let root = SystemPathBuf::from("/project");
        let mut metadata = ProjectMetadata::new("test", root);
        metadata.apply_override_options(Options {
            baseline: Some(RelativePathBuf::cli("/project/baseline.json")),
            ..Options::default()
        });
        TestDb::new(metadata)
    }

    #[test]
    fn serialization_order_ignores_input_order_and_severity() -> ruff_db::system::Result<()> {
        let mut db = test_db_with_baseline();
        let source_path = SystemPath::new("/project/test.py");
        db.write_file(source_path, "x\n")?;
        let source_file = system_path_to_file(&db, source_path).unwrap();
        let span = Span::from(source_file).with_range(TextRange::new(0.into(), 1.into()));

        let mut warning = Diagnostic::new(
            DiagnosticId::lint("z-warning-name-longer-than-twenty-four-characters"),
            Severity::Warning,
            "warning",
        );
        warning.annotate(Annotation::primary(span.clone()));
        let mut error = Diagnostic::new(DiagnosticId::lint("a-error"), Severity::Error, "error");
        error.annotate(Annotation::primary(span));

        let initial = BaselineFile::from_diagnostics(&db, &[warning.clone(), error.clone()]);
        assert!(
            initial.files[SystemPath::new("test.py")]
                .iter()
                .all(|entry| !entry.rule.is_heap_allocated())
        );

        let baseline_path = SystemPath::new("/project/baseline.json");
        db.write_file(baseline_path, serde_json::to_string(&initial)?)?;
        let mut diagnostics = vec![warning.clone(), error.clone()];
        demote_diagnostics(&db, source_file, &mut diagnostics);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity() == Severity::Hint)
        );

        warning.set_severity(Severity::Hint);
        error.set_severity(Severity::Hint);
        assert_eq!(
            BaselineFile::from_diagnostics(&db, &[error, warning]),
            initial
        );
        Ok(())
    }

    #[test]
    fn disabled_baseline_returns_none() {
        let db = TestDb::new(ProjectMetadata::new(
            "test",
            SystemPathBuf::from("/project"),
        ));

        assert!(baselines(&db).unwrap().is_none());
    }

    #[test]
    fn disabled_baseline_does_not_run_baseline_queries() -> ruff_db::system::Result<()> {
        let mut db = TestDb::new(ProjectMetadata::new(
            "test",
            SystemPathBuf::from("/project"),
        ));
        let source_path = SystemPath::new("/project/test.py");
        db.write_file(source_path, "x: int = 'wrong'\n")?;
        let source_file = system_path_to_file(&db, source_path).unwrap();
        db.take_salsa_events();

        assert!(baseline(&db, source_file).is_none());
        let diagnostics = check_file_impl(&db, db.program_file(source_file));
        assert_eq!(diagnostics.unwrap()[0].severity(), Severity::Error);

        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "baseline_impl", None, &events);
        assert_const_function_query_was_not_run(&db, baselines, &events);
        Ok(())
    }

    #[test]
    fn per_file_baseline_shares_cached_entries() -> ruff_db::system::Result<()> {
        let mut db = test_db_with_baseline();
        let source_path = SystemPath::new("/project/nested/test.py");
        db.write_file(source_path, "x\n")?;
        db.write_file(
            SystemPath::new("/project/baseline.json"),
            r#"{"version":0,"files":{"nested/test.py":[{"rule":"rule","precedingHash":"0123456789abcdef0123456789abcdef","followingHash":"fedcba9876543210fedcba9876543210"}]}}"#,
        )?;
        let source_file = system_path_to_file(&db, source_path).unwrap();

        let entries =
            &baselines(&db).unwrap().as_ref().unwrap().0[SystemPath::new("nested/test.py")];
        let file_entries = baseline(&db, source_file).unwrap();

        assert!(std::ptr::eq(entries.as_ref(), file_entries));
        Ok(())
    }

    #[test]
    fn baseline_file_changes_invalidate_matching() -> anyhow::Result<()> {
        let mut db = test_db_with_baseline();
        let source_path = SystemPath::new("/project/test.py");
        let baseline_path = SystemPath::new("/project/baseline.json");
        db.write_file(source_path, "x: int = 'wrong'\n")?;
        let source_file = system_path_to_file(&db, source_path).unwrap();

        let initial_result = check_file_impl(&db, db.program_file(source_file));
        let initial = initial_result.as_ref().unwrap();
        assert!(initial.iter().any(|diagnostic| {
            diagnostic.id().is_lint() && diagnostic.severity() != Severity::Hint
        }));

        assert_eq!(write(&db, baseline_path, initial)?, 1);
        ruff_db::files::File::sync_path(&mut db, baseline_path);

        let baselined_result = check_file_impl(&db, db.program_file(source_file));
        let baselined = baselined_result.as_ref().unwrap();
        assert!(baselined.iter().any(|diagnostic| {
            diagnostic.id().is_lint() && diagnostic.severity() == Severity::Hint
        }));

        db.write_file(baseline_path, "{\"version\": 1, \"files\": {}}")?;
        assert!(matches!(
            baselines(&db),
            Err(Error::UnsupportedVersion { version: 1, .. })
        ));
        assert!(
            db.project()
                .check_settings(&db)
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::InvalidBaseline)
        );
        Ok(())
    }

    #[test]
    fn unrelated_baseline_changes_preserve_cached_file_checks() -> anyhow::Result<()> {
        let mut db = test_db_with_baseline();
        let first_path = SystemPath::new("/project/first.py");
        let second_path = SystemPath::new("/project/second.py");
        let baseline_path = SystemPath::new("/project/baseline.json");
        db.write_file(first_path, "x: int = 'wrong'\n")?;
        db.write_file(second_path, "y: int = 'wrong'\n")?;
        let first_file = system_path_to_file(&db, first_path).unwrap();
        let second_file = system_path_to_file(&db, second_path).unwrap();

        let mut diagnostics = db.check_file(first_file);
        diagnostics.extend(db.check_file(second_file));
        assert_eq!(write(&db, baseline_path, &diagnostics)?, 2);
        File::sync_path(&mut db, baseline_path);

        let initial = check_file_impl(&db, db.program_file(second_file));
        assert_eq!(initial.unwrap()[0].severity(), Severity::Hint);

        let mut updated = BaselineFile::from_diagnostics(&db, &diagnostics);
        updated.files.remove(SystemPath::new("first.py"));
        db.write_file(baseline_path, serde_json::to_string(&updated)?)?;
        db.take_salsa_events();

        let unchanged = check_file_impl(&db, db.program_file(second_file));
        assert_eq!(unchanged.unwrap()[0].severity(), Severity::Hint);

        let events = db.take_salsa_events();
        assert!(find_will_execute_event_by_name(&db, "baseline_impl", None, &events).is_some());
        assert_function_query_was_not_run(
            &db,
            check_file_impl,
            db.program_file(second_file),
            &events,
        );
        Ok(())
    }

    #[test]
    fn malformed_baseline_is_a_settings_diagnostic() -> ruff_db::system::Result<()> {
        let mut db = test_db_with_baseline();
        db.write_file(SystemPath::new("/project/baseline.json"), "not json")?;
        assert!(matches!(baselines(&db), Err(Error::Parse { .. })));
        let diagnostics = db.project().check_settings(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::InvalidBaseline);
        Ok(())
    }

    #[test]
    fn missing_baseline_is_a_settings_diagnostic() {
        let db = test_db_with_baseline();
        assert!(matches!(baselines(&db), Err(Error::Read { .. })));
        let diagnostics = db.project().check_settings(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::InvalidBaseline);
    }
}

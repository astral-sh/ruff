//! Diagnostic baseline loading, matching, and serialization.

use std::collections::BTreeMap;
use std::hash::Hasher;

use anyhow::Context;
use ruff_cache::CacheKeyHasher;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
use ruff_db::files::{File, FilePath, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::SystemPath;
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::Db;

const BASELINE_VERSION: u32 = 1;
const CONTEXT_SIZE: usize = 100;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, get_size2::GetSize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Baseline {
    version: u32,
    files: BTreeMap<String, Vec<BaselineEntry>>,
}

#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, get_size2::GetSize,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineEntry {
    #[serde(skip)]
    offset: TextSize,
    rule: String,
    preceding_hash: String,
    following_hash: String,
}

#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize)]
struct BaselineError {
    file: Option<File>,
    message: String,
    detail: String,
}

impl BaselineError {
    fn to_diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidBaseline,
            Severity::Error,
            self.message.clone(),
        );
        if let Some(file) = self.file {
            diagnostic.annotate(Annotation::primary(
                Span::from(file).with_range(TextRange::default()),
            ));
        }
        diagnostic.info(self.detail.clone());
        diagnostic
    }
}

/// Loads and validates the configured baseline.
///
/// Reading through `source_text` makes this query dependent on the baseline file's contents.
#[salsa::tracked(returns(ref), heap_size = ruff_memory_usage::heap_size)]
fn load(db: &dyn Db) -> Result<FxHashMap<String, Box<[BaselineEntry]>>, BaselineError> {
    let project = db.project();
    let Some(path) = project.settings(db).baseline() else {
        return Ok(FxHashMap::default());
    };

    let file = match system_path_to_file(db, path) {
        Ok(file) => file,
        Err(error) => {
            return Err(BaselineError {
                file: None,
                message: format!("Failed to read baseline `{path}`"),
                detail: error.to_string(),
            });
        }
    };

    let source = source_text(db, file);
    if let Some(error) = source.read_error() {
        return Err(BaselineError {
            file: Some(file),
            message: format!("Failed to read baseline `{path}`"),
            detail: error.to_string(),
        });
    }

    let baseline: Baseline = match serde_json::from_str(source.as_str()) {
        Ok(baseline) => baseline,
        Err(error) => {
            return Err(BaselineError {
                file: Some(file),
                message: format!("Failed to parse baseline `{path}`"),
                detail: error.to_string(),
            });
        }
    };

    if baseline.version != BASELINE_VERSION {
        return Err(BaselineError {
            file: Some(file),
            message: format!("Unsupported baseline version `{}`", baseline.version),
            detail: format!("ty supports baseline version `{BASELINE_VERSION}`"),
        });
    }

    Ok(baseline
        .files
        .into_iter()
        .map(|(path, entries)| (path, entries.into_boxed_slice()))
        .collect())
}

pub(crate) fn settings_diagnostic(db: &dyn Db) -> Option<Diagnostic> {
    db.project().settings(db).baseline()?;
    load(db).as_ref().err().map(BaselineError::to_diagnostic)
}

/// Demotes diagnostics matched by the configured baseline to hint severity.
pub(crate) fn demote_diagnostics(db: &dyn Db, file: File, diagnostics: &mut [Diagnostic]) {
    if db.project().settings(db).baseline().is_none() {
        return;
    }
    let Ok(files) = load(db) else {
        return;
    };
    let Some(path) = project_relative_path(db, file) else {
        return;
    };
    let Some(baseline_entries) = files.get(&path) else {
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
    let (diagnostic_indices, entries): (Vec<_>, Vec<_>) = entries.into_iter().unzip();

    for matched_index in matched_indices(baseline_entries, &entries) {
        diagnostics[diagnostic_indices[matched_index]].set_severity(Severity::Hint);
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
    let baseline = Baseline::from_diagnostics(db, diagnostics);
    let count = baseline.files.values().map(Vec::len).sum();
    let mut serialized = serde_json::to_string_pretty(&baseline)?;
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

impl Baseline {
    fn from_diagnostics(db: &dyn Db, diagnostics: &[Diagnostic]) -> Self {
        let mut grouped: BTreeMap<String, Vec<BaselineDiagnostic>> = BTreeMap::new();
        for diagnostic in diagnostics {
            let Some(diagnostic) = BaselineDiagnostic::from_diagnostic(diagnostic) else {
                continue;
            };
            let Some(path) = project_relative_path(db, diagnostic.file) else {
                continue;
            };
            grouped.entry(path).or_default().push(diagnostic);
        }

        let mut files = BTreeMap::new();
        for (path, diagnostics) in grouped {
            let file = diagnostics[0].file;
            let source = source_text(db, file);
            let mut entries: Vec<_> = diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.entry(source.as_str()))
                .collect();
            entries.sort();
            files.insert(path, entries);
        }

        Self {
            version: BASELINE_VERSION,
            files,
        }
    }
}

fn project_relative_path(db: &dyn Db, file: File) -> Option<String> {
    let FilePath::System(path) = file.path(db) else {
        return None;
    };
    let relative = path.strip_prefix(db.project().root(db)).ok()?;
    Some(relative.as_str().replace('\\', "/"))
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
            rule: self.rule.as_str().to_owned(),
            preceding_hash,
            following_hash,
        }
    }
}

fn context_hashes(source: &str, offset: TextSize) -> (String, String) {
    let offset = usize::from(offset);
    let mut preceding: Vec<_> = source[..offset]
        .chars()
        .rev()
        .filter(|character| !character.is_whitespace())
        .take(CONTEXT_SIZE)
        .collect();
    preceding.reverse();
    let preceding: String = preceding.into_iter().collect();
    let following: String = source[offset..]
        .chars()
        .filter(|character| !character.is_whitespace())
        .take(CONTEXT_SIZE)
        .collect();
    (stable_hash(&preceding), stable_hash(&following))
}

fn stable_hash(context: &str) -> String {
    let mut hasher = CacheKeyHasher::new();
    hasher.write(context.as_bytes());
    format!("{:x}", hasher.finish())
}

fn matched_indices(baseline: &[BaselineEntry], current: &[BaselineEntry]) -> Vec<usize> {
    let mut matched = Vec::new();
    let mut baseline_start = 0;
    for (current_index, current_entry) in current.iter().enumerate() {
        let Some(relative_index) = baseline[baseline_start..]
            .iter()
            .position(|baseline_entry| two_hash_matches(baseline_entry, current_entry))
        else {
            continue;
        };
        matched.push(current_index);
        baseline_start += relative_index + 1;
    }
    matched
}

fn two_hash_matches(baseline: &BaselineEntry, current: &BaselineEntry) -> bool {
    baseline.rule == current.rule
        && (baseline.preceding_hash == current.preceding_hash
            || baseline.following_hash == current.following_hash)
}

#[cfg(test)]
mod tests {
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_text_size::TextRange;

    use crate::db::Db as _;
    use crate::db::testing::TestDb;
    use crate::metadata::options::Options;
    use crate::metadata::value::RelativePathBuf;
    use crate::{ProjectMetadata, check_file_impl};
    use ty_python_semantic::Db as _;

    use super::{
        Baseline, BaselineEntry, context_hashes, demote_diagnostics, load, matched_indices,
        two_hash_matches, write,
    };

    fn entry(rule: &str, preceding: &str, following: &str) -> BaselineEntry {
        BaselineEntry {
            offset: 0.into(),
            rule: rule.to_owned(),
            preceding_hash: preceding.to_owned(),
            following_hash: following.to_owned(),
        }
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
    fn context_hash_ignores_whitespace() {
        assert_eq!(
            context_hashes("a \nç d", 3.into()),
            context_hashes("açd", 1.into())
        );
    }

    #[test]
    fn schema_validation() {
        let unknown_top_level = r#"{"version":1,"files":{},"unknown":true}"#;
        assert!(serde_json::from_str::<Baseline>(unknown_top_level).is_err());

        let valid = r#"{
            "version": 1,
            "files": {
                "test.py": [{
                    "rule": "invalid-assignment",
                    "precedingHash": "before",
                    "followingHash": "after"
                }]
            }
        }"#;
        assert!(serde_json::from_str::<Baseline>(valid).is_ok());

        let message = r#"{
            "version": 1,
            "files": {
                "test.py": [{
                    "rule": "invalid-assignment",
                    "precedingHash": "before",
                    "followingHash": "after",
                    "message": "review only"
                }]
            }
        }"#;
        assert!(serde_json::from_str::<Baseline>(message).is_err());
    }

    #[test]
    fn ordered_matching_preserves_duplicate_cardinality() {
        let duplicate = entry("rule", "before", "after");
        let baseline = vec![duplicate.clone(), duplicate.clone()];
        let current = vec![duplicate.clone(), duplicate.clone(), duplicate];

        assert_eq!(matched_indices(&baseline, &current).len(), 2);
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
            DiagnosticId::lint("z-warning"),
            Severity::Warning,
            "warning",
        );
        warning.annotate(Annotation::primary(span.clone()));
        let mut error = Diagnostic::new(DiagnosticId::lint("a-error"), Severity::Error, "error");
        error.annotate(Annotation::primary(span));

        let initial = Baseline::from_diagnostics(&db, &[warning.clone(), error.clone()]);

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
        assert_eq!(Baseline::from_diagnostics(&db, &[error, warning]), initial);
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

        db.write_file(baseline_path, "{\"version\": 2, \"files\": {}}")?;
        assert!(
            db.project()
                .check_settings(&db)
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::InvalidBaseline)
        );
        Ok(())
    }

    #[test]
    fn malformed_baseline_is_a_settings_diagnostic() -> ruff_db::system::Result<()> {
        let mut db = test_db_with_baseline();
        db.write_file(SystemPath::new("/project/baseline.json"), "not json")?;
        assert!(load(&db).is_err());
        let diagnostics = db.project().check_settings(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::InvalidBaseline);
        Ok(())
    }

    #[test]
    fn missing_baseline_is_a_settings_diagnostic() {
        let db = test_db_with_baseline();
        assert!(load(&db).is_err());
        let diagnostics = db.project().check_settings(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::InvalidBaseline);
    }
}

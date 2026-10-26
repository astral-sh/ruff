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
//! ty records the rule, the start and end columns of the primary diagnostic range, and the number
//! of lines it spans. Adding the import changes the line number of `get_name()`, but preserves all
//! four values. Reindenting the assignment changes the columns and can make the error reappear.
//! Each entry can suppress one diagnostic in its original file. Renaming the file makes its errors
//! reappear.
//!
//! # Why match in source order?
//!
//! Suppose the baseline already covers an unresolved reference to `missing` and the assignment to
//! `name`. Then someone adds a second assignment with the same error above both of them:
//!
//! ```diff
//! +size: int = get_name()
//!  print(missing)
//!  name: int = get_name()
//! ```
//!
//! Both calls to `get_name()` have the same rule and range shape. Matching the first one would hide
//! the new error on `size` and report the old error on `name`. Like [basedpyright], ty uses a Myers
//! diff to find the longest sequence of matching entries that still appear in the same order.
//! Since `missing` still precedes the call on `name`, both old errors match and ty reports only the
//! assignment to `size`.
//!
//! Order can't distinguish a new error that replaces an old one with the same rule and range shape.
//! Suppressing that new error is a false negative. Conversely, swapping two functions can break the
//! relative order of their diagnostics and cause an existing error to reappear (a false positive).
//!
//! Including surrounding code in each entry could distinguish some otherwise identical errors, but
//! it would also increase baseline churn. An edit near an unchanged error could then rewrite its
//! entry on the next baseline update. Recording just the rule and range shape keeps those edits out
//! of the baseline and makes updates on different branches easier to merge.
//!
//! # Format versions
//!
//! Baseline support is experimental and uses format version 0, which may change between ty releases.
//! We will bump the format version to 1 when promoting the feature to stable. From then on, ty must
//! be able to read and match baselines written by the previous ty version. Updating a baseline
//! may always write the current format.
//!
//! [basedpyright]: https://github.com/DetachHead/basedpyright/blob/fa4b52d9428c0f355c95af2056cfd4edd4e6e5da/packages/pyright-internal/src/baseline.ts#L212-L225

use std::collections::BTreeMap;
use std::ops::Range;

use anyhow::Context;
use compact_str::CompactString;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
use ruff_db::files::{File, FilePath, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_source_file::LineRanges;
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffTag, capture_diff_slices};

use crate::Db;

const BASELINE_VERSION: u32 = 0;

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

    let mut current: Vec<_> = diagnostics
        .iter_mut()
        .filter_map(|diagnostic| {
            Some((BaselineDiagnostic::from_diagnostic(diagnostic)?, diagnostic))
        })
        .collect();
    current.sort_by_key(|(location, _)| location.sort_key());

    let source = source_text(db, file);
    let entries: Vec<_> = current
        .iter()
        .map(|(location, _)| location.entry(&source))
        .collect();

    let mut demote = |range: Range<usize>| {
        for (_, diagnostic) in &mut current[range] {
            diagnostic.set_severity(Severity::Hint);
            diagnostic.info(format_args!(
                "Suppressed by baseline `{}`",
                baseline.display_path
            ));
        }
    };

    if entries == baseline.entries {
        demote(0..entries.len());
        return;
    }
    for op in capture_diff_slices(Algorithm::Myers, baseline.entries, &entries) {
        if op.tag() == DiffTag::Equal {
            demote(op.new_range());
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
        for (file, (path, mut diagnostics)) in by_file {
            diagnostics.sort_by_key(BaselineDiagnostic::sort_key);
            let source = source_text(db, file);
            let entries = diagnostics
                .iter()
                .map(|diagnostic| diagnostic.entry(&source))
                .collect();
            let mut portable_path = String::with_capacity(path.as_str().len());
            for component in path.components() {
                if !portable_path.is_empty() {
                    portable_path.push('/');
                }
                portable_path.push_str(component.as_str());
            }
            files.insert(SystemPathBuf::from(portable_path), entries);
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

/// The rule and shape of a diagnostic's primary range, independent of its line number.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, get_size2::GetSize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineEntry {
    rule: CompactString,
    /// Zero-based columns counted in Unicode scalar values. The end is exclusive.
    start_column: usize,
    end_column: usize,
    /// Includes both the start and end lines.
    line_count: usize,
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

    /// Break ties at the same position consistently when writing and matching the baseline.
    fn sort_key(&self) -> (TextSize, TextSize, LintName) {
        (self.range.start(), self.range.end(), self.rule)
    }

    fn entry(&self, source: &str) -> BaselineEntry {
        let through_end = &source[TextRange::up_to(self.range.end())];
        let column = |offset| {
            let before = &through_end[TextRange::up_to(offset)];
            let line_start = before.line_start(offset).to_usize();
            before[line_start..].chars().count()
        };
        BaselineEntry {
            rule: CompactString::const_new(self.rule.as_str()),
            start_column: column(self.range.start()),
            end_column: column(self.range.end()),
            line_count: 1 + through_end.count_lines(self.range) as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::diagnostic::Severity;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_db::testing::assert_const_function_query_was_not_run;

    use crate::ProjectMetadata;
    use crate::db::testing::TestDb;
    use ty_python_semantic::Db as _;

    use super::baselines;

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

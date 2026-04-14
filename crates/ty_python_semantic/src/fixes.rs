use crate::{SuppressFix, is_unused_ignore_comment_lint, suppress_all};
use ruff_db::cancellation::{Canceled, CancellationToken};
use ruff_db::diagnostic::{DisplayDiagnosticConfig, DisplayDiagnostics};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::SourceText;
use ruff_db::system::{SystemPath, SystemPathBuf, WritableSystem};
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span},
    files::File,
    source::source_text,
};
use ruff_diagnostics::{Edit, Fix, IsolationLevel, SourceMap};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};
use salsa::Setter as _;
use std::collections::BTreeMap;
use std::sync::Mutex;
use thiserror::Error;

use crate::Db;

pub struct FixAllResults {
    /// The non-lint diagnostics that can't be fixed or the diagnostics of files
    /// that couldn't be fixed because ty failed to write the result back to disk,
    /// or the file contains a syntax errors after fixing.
    pub diagnostics: Vec<Diagnostic>,

    /// The number of diagnostics that were fixed across all files.
    pub count: usize,
}

/// Adds suppressions to all lint diagnostics and writes the changed files back to disk.
///
/// Returns how many diagnostics were suppressed along the remaining, non-suppressed diagnostics.
///
/// ## Panics
/// If the `db`'s system isn't [writable](WritableSystem).
pub fn suppress_all_diagnostics(
    db: &mut dyn Db,
    diagnostics: Vec<Diagnostic>,
    cancellation_token: &CancellationToken,
) -> Result<FixAllResults, Canceled> {
    fix_all(
        db,
        diagnostics,
        FixMode::Suppress,
        cancellation_token,
        Db::check_file,
    )
}

/// Applies the safe fixes for all diagnostics and writes the changed files back to disk.
///
/// Returns how many diagnostics were fixed along the remaining, non-fixed diagnostics.
///
/// ## Panics
/// If the `db`'s system isn't [writable](WritableSystem).
pub fn fix_all_diagnostics(
    db: &mut dyn Db,
    diagnostics: Vec<Diagnostic>,
    cancellation_token: &CancellationToken,
) -> Result<FixAllResults, Canceled> {
    fix_all(
        db,
        diagnostics,
        FixMode::ApplyFixes,
        cancellation_token,
        Db::check_file,
    )
}

const MAX_ITERATIONS: usize = 10;

/// Applies all fixes for the given fix mode.
///
/// `check_file` is a separate parameter so that tests can easily mock out a file's diagnostics.
fn fix_all<F>(
    db: &mut dyn Db,
    mut diagnostics: Vec<Diagnostic>,
    fix_mode: FixMode,
    cancellation_token: &CancellationToken,
    check_file: F,
) -> Result<FixAllResults, Canceled>
where
    F: Fn(&dyn Db, File) -> Vec<Diagnostic> + Sync,
{
    let system = WritableSystem::dyn_clone(
        db.system()
            .as_writable()
            .expect("System should be writable"),
    );

    let has_fixable = diagnostics
        .iter()
        .any(|diagnostic| fix_mode.is_fixable(diagnostic));

    // Early return if there are no diagnostics that can be suppressed to avoid all the heavy work below.
    if !has_fixable {
        return Ok(FixAllResults {
            diagnostics,
            count: 0,
        });
    }

    let mut by_file: BTreeMap<File, Vec<_>> = BTreeMap::new();

    // Group the diagnostics by file, leave the file-agnostic diagnostics in `diagnostics`.
    for diagnostic in diagnostics.extract_if(.., |diagnostic| diagnostic.primary_span().is_some()) {
        let span = diagnostic
            .primary_span()
            .expect("should be set because `extract_if` only yields elements with a primary_span");

        by_file
            .entry(span.expect_ty_file())
            .or_default()
            .push(diagnostic);
    }

    // Identify all files with fixes and queue them for fixing.
    let mut queue: Vec<(QueuedFile, Vec<ApplicableFix>)> = Vec::new();
    let mut source_texts = SourceTexts::default();

    for (&file, diagnostics) in &by_file {
        let path = file.path(db);
        let Some(path) = path.as_system_path() else {
            tracing::debug!("Skipping read-only file `{path}`");
            continue;
        };

        let parsed = parsed_module(db, file);
        if parsed.load(db).has_syntax_errors() {
            tracing::warn!("Skipping file `{path}` with syntax errors");
            continue;
        }

        let fixes = fix_mode.fixes(db, file, diagnostics);

        if fixes.is_empty() {
            tracing::warn!("Skipping file `{path}` without applicable fixes.");
            continue;
        }

        queue.push((QueuedFile::new(file, path, diagnostics), fixes));
    }

    // Try applying the fixes. Iterate at most `MAX_ITERATIONS` times.
    let mut remaining_iterations = MAX_ITERATIONS;
    let mut completed: Vec<FixedFile> = Vec::with_capacity(queue.len());

    while !queue.is_empty() {
        let is_last_iteration = remaining_iterations == 1;
        let mut unstaged_fixes = Vec::with_capacity(queue.len());

        for (file, fixes) in queue.drain(..) {
            if cancellation_token.is_cancelled() {
                source_texts.revert_all(db);
                return Err(Canceled);
            }

            let staged_source = source_texts.staged(db, file.file);

            let FixedCode {
                source,
                source_map,
                applied_fixes,
            } = apply_fixes(&staged_source, fixes);

            let fixed_source = staged_source.with_text(source, &source_map);
            source_texts.set_unstaged(db, file.file, fixed_source.clone());

            unstaged_fixes.push((file, applied_fixes));
        }

        // Check if applying the files introduced any syntax errors, compute the remaining diagnostics and any new fixes.
        // This is done outside the above loop so that it can run in parallel.
        let check_results = recheck_files(
            &*db,
            unstaged_fixes,
            fix_mode,
            cancellation_token,
            &check_file,
        );

        if cancellation_token.is_cancelled() {
            source_texts.revert_all(db);
            return Err(Canceled);
        }

        for result in check_results {
            if cancellation_token.is_cancelled() {
                source_texts.revert_all(db);
                return Err(Canceled);
            }

            match result {
                CheckResult::Checked { mut file, fixes } => {
                    source_texts.stage(file.file);

                    if fixes.is_empty() {
                        completed.push(file.into_fixed());
                        continue;
                    }

                    if is_last_iteration {
                        file.push_diagnostic(create_too_many_iterations_diagnostics(
                            file.file,
                            fix_mode,
                            &diagnostics,
                        ));
                        completed.push(file.into_fixed());
                        continue;
                    }

                    // Requeue the file for another round of fixes.
                    queue.push((file, fixes));
                }

                CheckResult::SyntaxError {
                    mut file,
                    diagnostic,
                } => {
                    // Reset the file's state to the last staged changes (or the original source text if this is the first iteration)
                    source_texts.reset_unstaged(db, file.file);
                    file.push_diagnostic(diagnostic);

                    completed.push(file.into_fixed());
                }
            }
        }

        if is_last_iteration {
            break;
        }

        remaining_iterations -= 1;
    }

    // commit the changes: Write the changes to disk
    let mut fix_count = 0;

    for file in completed {
        if cancellation_token.is_cancelled() {
            source_texts.revert_all(db);
            return Err(Canceled);
        }

        if let Some(fixed) = source_texts.uncommitted(file.file) {
            if let Err(error) = write_changes(db, &*system, file.file, &file.path, fixed) {
                // revert the source text back to its original content.
                source_texts.revert(db, file.file);

                // Writing failed, revert the source text override back to the file's original source.
                let mut diagnostics = by_file.remove(&file.file).unwrap_or_default();
                let mut diag = Diagnostic::new(
                    DiagnosticId::Io,
                    Severity::Error,
                    format_args!("Failed to write fixes to file: {error}"),
                );

                diag.annotate(Annotation::primary(Span::from(file.file)));
                diagnostics.push(diag);
                by_file.insert(file.file, diagnostics);

                continue;
            }

            source_texts.commit(file.file);
        }

        fix_count += file.applied_fixes;
        by_file.insert(file.file, file.remaining_diagnostics);
    }

    // Stitch the remaining diagnostics back together.
    diagnostics.extend(by_file.into_values().flatten());
    diagnostics.sort_by(|left, right| {
        left.rendering_sort_key(db)
            .cmp(&right.rendering_sort_key(db))
    });

    Ok(FixAllResults {
        diagnostics,
        count: fix_count,
    })
}

fn create_fix_introduced_syntax_error_diagnostic(
    db: &dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
) -> Diagnostic {
    let mut diag = Diagnostic::new(
        DiagnosticId::InternalError,
        Severity::Fatal,
        format_args!("Applying fixes introduced a syntax error. Reverting changes."),
    );

    let mut file_annotation = Annotation::primary(Span::from(file));
    file_annotation.hide_snippet(true);
    diag.annotate(file_annotation);

    let parse_diagnostics: Vec<_> = parsed
        .errors()
        .iter()
        .map(|error| Diagnostic::invalid_syntax(Span::from(file), &error.error, error.location))
        .collect();

    diag.add_bug_sub_diagnostics("%5BFix%20error%5D");

    let file_db: &dyn ruff_db::Db = db;

    diag.info(format_args!(
        "Introduced syntax errors:\n\n{}",
        DisplayDiagnostics::new(
            &file_db,
            &DisplayDiagnosticConfig::new("ty"),
            &parse_diagnostics
        )
    ));

    diag
}

fn create_too_many_iterations_diagnostics(
    file: File,
    fix_mode: FixMode,
    diagnostics: &[Diagnostic],
) -> Diagnostic {
    let fixable_ids = diagnostics
        .iter()
        .filter(|diagnostic| fix_mode.is_fixable(diagnostic))
        .map(|diagnostic| diagnostic.id().as_str())
        .collect::<Vec<_>>();

    let codes = fixable_ids.join(", ");

    let mut diag = Diagnostic::new(
        DiagnosticId::InternalError,
        Severity::Fatal,
        format_args!("Fixes failed to converge after {MAX_ITERATIONS} iterations."),
    );

    let mut file_annotation = Annotation::primary(Span::from(file));
    file_annotation.hide_snippet(true);
    diag.annotate(file_annotation);

    diag.add_bug_sub_diagnostics("%5BInfinite%20loop%5D");
    diag.info(format_args!("Fixable diagnostics: {codes}"));
    diag
}

#[derive(Copy, Clone, Debug)]
enum FixMode {
    /// Adds suppression comments for every suppressable diagnostic.
    Suppress,
    /// Applies the diagnostic's safe fixes.
    ApplyFixes,
}

impl FixMode {
    fn is_fixable(self, diagnostic: &Diagnostic) -> bool {
        let Some(primary_span) = diagnostic.primary_span() else {
            return false;
        };

        match self {
            FixMode::Suppress => {
                primary_span.range().is_some()
                    && diagnostic
                        .id()
                        .as_lint()
                        .is_some_and(|name| !is_unused_ignore_comment_lint(name))
            }
            FixMode::ApplyFixes => {
                diagnostic.has_applicable_fix(ruff_diagnostics::Applicability::Safe)
            }
        }
    }

    fn fixes(self, db: &dyn Db, file: File, file_diagnostics: &[Diagnostic]) -> Vec<ApplicableFix> {
        match self {
            FixMode::Suppress => {
                let suppressable_diagnostics: Vec<_> = file_diagnostics
                    .iter()
                    .filter_map(|diagnostic| {
                        let lint_id = diagnostic.id().as_lint()?;

                        // Don't suppress unused ignore comments.
                        if is_unused_ignore_comment_lint(lint_id) {
                            return None;
                        }

                        // We can't suppress diagnostics without a corresponding file or range.
                        let span = diagnostic.primary_span()?;
                        let range = span.range()?;

                        Some((lint_id, range))
                    })
                    .collect();

                suppress_all(db, file, &suppressable_diagnostics)
                    .into_iter()
                    .map(
                        |SuppressFix {
                             fix,
                             suppressed_diagnostics,
                         }| ApplicableFix {
                            fix,
                            fixed_diagnostics: suppressed_diagnostics,
                        },
                    )
                    .collect()
            }
            FixMode::ApplyFixes => file_diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.has_applicable_fix(ruff_diagnostics::Applicability::Safe)
                })
                .filter_map(|diagnostic| {
                    diagnostic.fix().cloned().map(|fix| ApplicableFix {
                        fix,
                        fixed_diagnostics: 1,
                    })
                })
                .collect(),
        }
    }
}

struct ApplicableFix {
    fix: Fix,

    /// The number of diagnostics this fix resolves.
    ///
    /// This is always 1 for `--fix`, but there are instances where `--add-ignore` groups
    /// multiple suppressions into a single fix. We need to track the count here to know
    /// how many diagnostics were fixed in the presence of overlapping fixes (which `--add-ignore` should
    /// never generate but better be safe than sorry).
    fixed_diagnostics: usize,
}

fn write_changes(
    db: &dyn Db,
    system: &dyn WritableSystem,
    file: File,
    path: &SystemPath,
    new_text: &SourceText,
) -> Result<(), WriteChangesError> {
    let metadata = system.path_metadata(path)?;

    if metadata.revision() != file.revision(db) {
        return Err(WriteChangesError::FileWasModified);
    }

    system.write_file_bytes(path, &new_text.to_bytes())?;

    Ok(())
}

#[derive(Debug, Error)]
enum WriteChangesError {
    #[error("failed to write changes to disk: {0}")]
    Io(#[from] std::io::Error),

    #[error("the file has been modified")]
    FileWasModified,
}

/// Apply a series of fixes to `File` and returns the updated source code along with the source map.
///
/// Returns an error if not all fixes were applied because some fixes are overlapping.
fn apply_fixes(source: &str, mut fixes: Vec<ApplicableFix>) -> FixedCode {
    let mut output = String::with_capacity(source.len());
    let mut last_pos: Option<TextSize> = None;
    let mut isolated: FxHashSet<u32> = FxHashSet::default();
    let mut applied_edits: FxHashSet<&Edit> = FxHashSet::default();

    let mut source_map = SourceMap::default();

    fixes.sort_unstable_by_key(|fix| fix.fix.min_start());
    let mut applied_fixes = 0usize;

    for fix in &fixes {
        let ApplicableFix {
            fix,
            fixed_diagnostics,
        } = fix;
        let mut edits = fix
            .edits()
            .iter()
            .filter(|edit| !applied_edits.contains(edit))
            .peekable();

        // If the fix contains at least one new edit, enforce isolation and positional requirements.
        if let Some(first) = edits.peek() {
            // If this fix requires isolation, and we've already applied another fix in the
            // same isolation group, skip it.
            if let IsolationLevel::Group(id) = fix.isolation() {
                if !isolated.insert(id) {
                    continue;
                }
            }

            // If this fix overlaps with a fix we've already applied, skip it.
            if last_pos.is_some_and(|last_pos| last_pos >= first.start()) {
                continue;
            }
        }

        for edit in edits {
            // Add all contents from `last_pos` to `fix.location`.
            let slice = &source[TextRange::new(last_pos.unwrap_or_default(), edit.start())];
            output.push_str(slice);

            // Add the start source marker for the patch.
            source_map.push_start_marker(edit, output.text_len());

            // Add the patch itself.
            output.push_str(edit.content().unwrap_or_default());

            // Add the end source marker for the added patch.
            source_map.push_end_marker(edit, output.text_len());

            // Track that the edit was applied.
            last_pos = Some(edit.end());
        }

        applied_edits.extend(fix.edits());
        applied_fixes += fixed_diagnostics;
    }

    // Add the remaining content.
    let slice = &source[last_pos.unwrap_or_default().to_usize()..];
    output.push_str(slice);

    FixedCode {
        source: output,
        source_map,
        applied_fixes,
    }
}

struct FixedCode {
    /// Source map that allows mapping positions in the fixed code back to positions in the original
    /// source code (useful for mapping fixed lines back to their original notebook cells).
    source_map: SourceMap,

    /// The fixed source code
    source: String,

    /// The number of fixes that were applied.
    applied_fixes: usize,
}

/// Tracks the source text overrides per file.
#[derive(Default)]
struct SourceTexts {
    /// Stores the original source text for each file that has outstanding writes.
    originals: FxHashMap<File, SourceText>,
    unstaged_changes: FxHashMap<File, SourceText>,
    staged_changes: FxHashMap<File, SourceText>,
}

impl SourceTexts {
    /// Returns the staged source text of `file`, ignoring any unstaged changes.
    fn staged(&self, db: &dyn Db, file: File) -> SourceText {
        if let Some(staged) = self.staged_changes.get(&file) {
            staged.clone()
        } else {
            source_text(db, file)
        }
    }

    /// Returns any uncommitted changes (staged or unstaged).
    ///
    /// Returns `None` if there are no uncommitted changes.
    fn uncommitted(&self, file: File) -> Option<&SourceText> {
        self.unstaged_changes
            .get(&file)
            .or_else(|| self.staged_changes.get(&file))
    }

    /// Promotes any unstaged changes of `file` to staged.
    ///
    /// This is a no-op if there are no unstaged changes.
    fn stage(&mut self, file: File) {
        let Some(changes) = self.unstaged_changes.remove(&file) else {
            return;
        };

        self.staged_changes.insert(file, changes);
    }

    /// Sets unstaged changes for `file`.
    fn set_unstaged(&mut self, db: &mut dyn Db, file: File, new_text: SourceText) {
        self.originals
            .entry(file)
            .or_insert_with(|| source_text(db, file));

        file.set_source_text_override(db).to(Some(new_text.clone()));
        self.unstaged_changes.insert(file, new_text);
    }

    /// Reverts any unstaged changes and reverts the source text of `file` to
    /// the last staged changed or its original content.
    fn reset_unstaged(&mut self, db: &mut dyn Db, file: File) {
        if self.unstaged_changes.remove(&file).is_none() {
            return;
        }

        // Try to reset to the last staged changes
        let source = if let Some(staged) = self.staged_changes.get(&file) {
            staged
        } else if let Some(original) = self.originals.get(&file) {
            original
        } else {
            // File was never overridden, nothing to do
            return;
        };

        file.set_source_text_override(db).to(Some(source.clone()));
    }

    /// Revert all files with a tracked override back to their original source text.
    fn revert_all(self, db: &mut dyn Db) {
        for (file, original) in self.originals {
            file.set_source_text_override(db).to(Some(original));
        }
    }

    /// Revert `file` back to its original source text.
    ///
    /// ## Panics
    /// If `file` has no override.
    fn revert(&mut self, db: &mut dyn Db, file: File) {
        let Some(original) = self.originals.remove(&file) else {
            return;
        };

        file.set_source_text_override(db).to(Some(original));
    }

    /// Commits any staged changes.
    fn commit(&mut self, file: File) {
        self.staged_changes.remove(&file);

        if !self.unstaged_changes.contains_key(&file) {
            self.originals.remove(&file);
        }
    }
}

/// A file that's queued for fixing
struct QueuedFile<'a> {
    file: File,

    path: SystemPathBuf,

    /// The original diagnostics of the source text as on disk.
    original_diagnostics: &'a [Diagnostic],

    /// The new diagnostics for this after fixes were applied or `None` if it's still the original diagnostics.
    diagnostics: Option<Vec<Diagnostic>>,

    applied_fixes: usize,
}

impl<'a> QueuedFile<'a> {
    fn new(file: File, path: &SystemPath, original_diagnostics: &'a [Diagnostic]) -> Self {
        Self {
            file,
            path: path.to_path_buf(),
            original_diagnostics,
            diagnostics: None,
            applied_fixes: 0,
        }
    }

    fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        let diagnostics = self
            .diagnostics
            .get_or_insert_with(|| self.original_diagnostics.to_vec());

        diagnostics.push(diagnostic);
    }

    fn into_fixed(self) -> FixedFile {
        FixedFile {
            file: self.file,
            path: self.path,
            remaining_diagnostics: self.diagnostics.unwrap_or_default(),
            applied_fixes: self.applied_fixes,
        }
    }
}

struct FixedFile {
    file: File,
    path: SystemPathBuf,
    remaining_diagnostics: Vec<Diagnostic>,
    applied_fixes: usize,
}

enum CheckResult<'a> {
    /// The unstaged fixes introduced a syntax error.
    SyntaxError {
        diagnostic: Diagnostic,
        file: QueuedFile<'a>,
    },
    /// The fixes were successfully applied without introducing any syntax errors.
    Checked {
        file: QueuedFile<'a>,
        /// The fixes for the next round (may be empty)
        fixes: Vec<ApplicableFix>,
    },
}

fn recheck_files<'a, F>(
    db: &dyn Db,
    changes: Vec<(QueuedFile<'a>, usize)>,
    fix_mode: FixMode,
    cancellation_token: &CancellationToken,
    check_file: &F,
) -> Vec<CheckResult<'a>>
where
    F: Fn(&dyn Db, File) -> Vec<Diagnostic> + Sync,
{
    let results = Mutex::new(Vec::with_capacity(changes.len()));

    {
        let outcomes = &results;
        let db = db.dyn_clone();

        rayon::scope(move |scope| {
            for (mut file, applied_fixes) in changes {
                let db = db.dyn_clone();

                scope.spawn(move |_| {
                    if cancellation_token.is_cancelled() {
                        return;
                    }

                    let db = &*db;

                    let parsed = parsed_module(db, file.file);
                    let parsed = parsed.load(db);

                    let result = if parsed.has_syntax_errors() {
                        let diagnostic =
                            create_fix_introduced_syntax_error_diagnostic(db, file.file, &parsed);

                        CheckResult::SyntaxError { diagnostic, file }
                    } else {
                        let diagnostics = check_file(db, file.file);
                        let fixes = fix_mode.fixes(db, file.file, &diagnostics);

                        file.applied_fixes += applied_fixes;
                        file.diagnostics = Some(diagnostics);

                        CheckResult::Checked { file, fixes }
                    };

                    outcomes.lock().unwrap().push(result);
                });
            }
        });
    }

    results.into_inner().unwrap()
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::Entry;
    use std::hash::{DefaultHasher, Hash, Hasher};

    use insta::assert_snapshot;
    use ruff_db::cancellation::CancellationTokenSource;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, DisplayDiagnostics,
        Severity, Span,
    };
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::parsed::parsed_module;
    use ruff_db::source::source_text;
    use ruff_db::system::SystemPath;
    use ruff_diagnostics::{Edit, Fix};
    use ruff_text_size::{TextRange, TextSize};
    use rustc_hash::FxHashMap;

    use super::suppress_all_diagnostics;
    use crate::Db;
    use crate::db::tests::TestDbBuilder;
    use crate::fixes::{FixMode, fix_all};

    #[test]
    fn simple_suppression() {
        assert_snapshot!(
            suppress_all_in(r#"
                a = b + 10"#
        ),
         @"
        Added 1 suppressions

        ## Fixed source

        ```py
        a = b + 10  # ty:ignore[unresolved-reference]
        ```
        ");
    }

    #[test]
    fn multiple_suppressions_same_code() {
        assert_snapshot!(
            suppress_all_in(r#"
                a = b + 10 + c"#
        ),
         @"
        Added 2 suppressions

        ## Fixed source

        ```py
        a = b + 10 + c  # ty:ignore[unresolved-reference]
        ```
        ");
    }

    #[test]
    fn multiple_suppressions_different_codes() {
        assert_snapshot!(
            suppress_all_in(r#"
                import sys
                a = b + 10 + sys.veeersion"#
        ),
         @"
        Added 2 suppressions

        ## Fixed source

        ```py
        import sys
        a = b + 10 + sys.veeersion  # ty:ignore[unresolved-attribute, unresolved-reference]
        ```
        ");
    }

    #[test]
    fn dont_fix_unused_ignore() {
        assert_snapshot!(
            suppress_all_in(r#"
                import sys
                a = 5 + 10  # ty: ignore[unresolved-reference]"#
        ),
         @"
        Added 0 suppressions

        ## Fixed source

        ```py
        import sys
        a = 5 + 10  # ty: ignore[unresolved-reference]
        ```

        ## Diagnostics after applying fixes

        warning[unused-ignore-comment]: Unused `ty: ignore` directive
         --> test.py:2:13
          |
        1 | import sys
        2 | a = 5 + 10  # ty: ignore[unresolved-reference]
          |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          |
        help: Remove the unused suppression comment
        ");
    }

    #[test]
    fn dont_fix_files_containing_syntax_errors() {
        assert_snapshot!(
            suppress_all_in(r#"
                import sys
                a = x +
                "#
        ),
         @"
        Added 0 suppressions

        ## Fixed source

        ```py
        import sys
        a = x +
        ```

        ## Diagnostics after applying fixes

        error[unresolved-reference]: Name `x` used when not defined
         --> test.py:2:5
          |
        1 | import sys
        2 | a = x +
          |     ^
          |

        error[invalid-syntax]: Expected an expression
         --> test.py:2:8
          |
        1 | import sys
        2 | a = x +
          |        ^
          |
        ");
    }

    #[test]
    fn arguments() {
        assert_snapshot!(
            suppress_all_in(r#"
                def test(a, b):
                    pass


                test(
                    a = 10,
                    c = "unknown"
                )
                "#
        ),
         @r#"
        Added 2 suppressions

        ## Fixed source

        ```py
        def test(a, b):
            pass


        test(
            a = 10,
            c = "unknown"  # ty:ignore[unknown-argument]
        )  # ty:ignore[missing-argument]
        ```
        "#);
    }

    // A same-code suppression inserted at the end of a narrower multiline range can land on the
    // start line of a wider multiline range, which makes the wider range's own suppression
    // redundant.
    #[test]
    fn same_code_multiline_suppressions_with_different_ranges_can_become_redundant() {
        assert_snapshot!(
            suppress_all_in(r#"
                from typing import TypeAlias

                JsonValue: TypeAlias = dict[str, "JsonValue"] | list["JsonValue"] | int


                def get_data() -> dict[str, JsonValue]:
                    return {"home_assistant": {"entities": [{"entity_id": "sensor.test"}]}}


                def f() -> None:
                    diag = get_data()
                    diag["home_assistant"]["entities"] = sorted(
                        diag["home_assistant"]["entities"], key=lambda ent: ent["entity_id"]
                    )
                "#
        ),
         @r#"
        Added 4 suppressions

        ## Fixed source

        ```py
        from typing import TypeAlias

        JsonValue: TypeAlias = dict[str, "JsonValue"] | list["JsonValue"] | int


        def get_data() -> dict[str, JsonValue]:
            return {"home_assistant": {"entities": [{"entity_id": "sensor.test"}]}}


        def f() -> None:
            diag = get_data()
            diag["home_assistant"]["entities"] = sorted(  # ty:ignore[invalid-assignment]
                diag["home_assistant"]["entities"], key=lambda ent: ent["entity_id"]  # ty:ignore[invalid-argument-type, not-subscriptable]
            )
        ```
        "#);
    }

    #[test]
    fn return_type() {
        assert_snapshot!(
            suppress_all_in(r#"class A:
    def test(self, b: int) -> str:
        return "test"


class B(A):
    def test(
        self,
        b: str
    ) -> A.b:
        pass"#
        ),
         @r#"
        Added 2 suppressions

        ## Fixed source

        ```py
        class A:
            def test(self, b: int) -> str:
                return "test"


        class B(A):
            def test(
                self,
                b: str
            ) -> A.b:  # ty:ignore[invalid-method-override, unresolved-attribute]
                pass
        ```
        "#);
    }

    #[test]
    fn existing_ty_ignore() {
        assert_snapshot!(
            suppress_all_in(r#"class A:
    def test(self, b: int) -> str:
        return "test"


class B(A):
    def test(  # ty:ignore[unresolved-reference]
        self,
        b: str
    ) -> A.b:
        pass"#
        ),
         @r#"
        Added 2 suppressions

        ## Fixed source

        ```py
        class A:
            def test(self, b: int) -> str:
                return "test"


        class B(A):
            def test(  # ty:ignore[unresolved-reference, invalid-method-override]
                self,
                b: str
            ) -> A.b:  # ty:ignore[unresolved-attribute]
                pass
        ```

        ## Diagnostics after applying fixes

        warning[unused-ignore-comment]: Unused `ty: ignore` directive: 'unresolved-reference'
         --> test.py:7:28
          |
        6 | class B(A):
        7 |     def test(  # ty:ignore[unresolved-reference, invalid-method-override]
          |                            ^^^^^^^^^^^^^^^^^^^^
        8 |         self,
        9 |         b: str
          |
        help: Remove the unused suppression code
        "#);
    }

    /// Tests that the `fix_all` doesn't end up in an infinite loop
    /// if the fixes never converge and that it emits a diagnostic in that case.
    #[test]
    fn fix_non_convergence() {
        let mut db = TestDbBuilder::new()
            .with_file("test.py", "a = 10")
            .build()
            .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();
        const LINT_ID: DiagnosticId = DiagnosticId::lint("unsatisfiable-lint");

        // For this test, we intentionally keep renaming a variable form `a` -> `b` and
        // from `b` -> `a`. This ensures that the fix never converges.
        let check_file = |db: &dyn Db, file: File| {
            let text = source_text(db, file);

            let message = if text.contains("a") {
                "Variable `a` should be named `b`."
            } else {
                "Variable `b` should be named `a`."
            };

            let mut diag = Diagnostic::new(LINT_ID, Severity::Warning, message);

            let variable_range = TextRange::new(TextSize::new(0), TextSize::new(1));

            diag.annotate(Annotation::primary(
                Span::from(file).with_range(variable_range),
            ));

            let new_name = if text.contains("a") { "b" } else { "a" };

            diag.set_fix(Fix::safe_edit(Edit::range_replacement(
                new_name.to_string(),
                variable_range,
            )));

            vec![diag]
        };

        let initial_diagnostics = check_file(&db, file);

        let cancellation_token_source = CancellationTokenSource::new();
        let fixes = fix_all(
            &mut db,
            initial_diagnostics,
            FixMode::ApplyFixes,
            &cancellation_token_source.token(),
            check_file,
        )
        .expect("operation never gets cancelled");

        // Returns two diagnostic: One is the not fixed diagnostic, the other a fatal diagnostic
        // making the user aware of the non convergence.
        let [convergence_diagnostic, diagnostic] = &*fixes.diagnostics else {
            panic!(
                "Expected `fix_all` to return two diagnostics but it returned  {}",
                fixes.diagnostics.len()
            );
        };

        assert_eq!(diagnostic.id(), LINT_ID);
        assert_eq!(
            diagnostic.primary_message(),
            "Variable `a` should be named `b`."
        );

        assert_eq!(convergence_diagnostic.id(), DiagnosticId::InternalError);
        assert_snapshot!(convergence_diagnostic.primary_message(), @"Fixes failed to converge after 10 iterations.");

        // It should keep the source text from the last allowed fix iteration.
        assert_eq!(&*source_text(&db, file), "a = 10");
    }

    /// Tests that `fix_all` reverts fixes that introduce a syntax error.
    #[test]
    fn fix_syntax_error() {
        let mut db = TestDbBuilder::new()
            .with_file("test.py", "a = 10")
            .build()
            .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();
        const LINT_ID: DiagnosticId = DiagnosticId::lint("with-faulty-fix");

        // For this test, we intentionally keep renaming a variable form `a` -> `b` and
        // from `b` -> `a`. This ensures that the fix never converges.
        let check_file = |db: &dyn Db, file: File| {
            let text = source_text(db, file);

            let message = if text.contains("a") {
                "Variable `a` should be named `b`."
            } else {
                "Variable `b` should be named `c`."
            };

            let mut diag = Diagnostic::new(LINT_ID, Severity::Warning, message);

            let variable_range = TextRange::new(TextSize::new(0), TextSize::new(1));

            diag.annotate(Annotation::primary(
                Span::from(file).with_range(variable_range),
            ));

            let edit = if text.contains("a") {
                Edit::range_replacement("b".to_string(), variable_range)
            } else {
                // Insert an extra `=`, resulting in a syntax error
                Edit::range_replacement("c =".to_string(), variable_range)
            };

            diag.set_fix(Fix::safe_edit(edit));

            vec![diag]
        };

        let initial_diagnostics = check_file(&db, file);

        let cancellation_token_source = CancellationTokenSource::new();
        let fixes = fix_all(
            &mut db,
            initial_diagnostics,
            FixMode::ApplyFixes,
            &cancellation_token_source.token(),
            check_file,
        )
        .expect("operation never gets cancelled");

        // Returns two diagnostic: One is the not fixed diagnostic, the other a fatal diagnostic
        // making the user aware of the non convergence.
        let [syntax_error, diagnostic] = &*fixes.diagnostics else {
            panic!(
                "Expected `fix_all` to return two diagnostics but it returned  {}",
                fixes.diagnostics.len()
            );
        };

        assert_eq!(diagnostic.id(), LINT_ID);
        assert_eq!(
            diagnostic.primary_message(),
            "Variable `b` should be named `c`."
        );

        assert_eq!(syntax_error.id(), DiagnosticId::InternalError);
        assert_snapshot!(syntax_error.primary_message(), @"Applying fixes introduced a syntax error. Reverting changes.");

        // It should revert the source to the last known error free version.
        assert_eq!(&*source_text(&db, file), "b = 10");
    }

    #[test]
    fn fix_cancellation_reverts_changes() {
        let mut db = TestDbBuilder::new()
            .with_file("test.py", "a = 10")
            .build()
            .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();
        const LINT_ID: DiagnosticId = DiagnosticId::lint("rename-a-to-b");

        let cancellation_token_source = CancellationTokenSource::new();

        let create_diagnostics = |file: File| {
            let mut diag = Diagnostic::new(
                LINT_ID,
                Severity::Warning,
                "Variable `a` should be named `b`.",
            );

            let variable_range = TextRange::new(TextSize::new(0), TextSize::new(1));

            diag.annotate(Annotation::primary(
                Span::from(file).with_range(variable_range),
            ));
            diag.set_fix(Fix::safe_edit(Edit::range_replacement(
                "b".to_string(),
                variable_range,
            )));

            vec![diag]
        };

        let initial_diagnostics = create_diagnostics(file);

        let check_file = |_: &dyn Db, file: File| {
            // Normally, this would happen on another thread but we do it here for simplicity.
            cancellation_token_source.cancel();

            create_diagnostics(file)
        };

        let result = fix_all(
            &mut db,
            initial_diagnostics,
            FixMode::ApplyFixes,
            &cancellation_token_source.token(),
            check_file,
        );

        assert!(matches!(result, Err(ruff_db::cancellation::Canceled)));

        // Cancellation should revert any staged or unstaged source text overrides.
        assert_eq!(&*source_text(&db, file), "a = 10");
    }

    #[test]
    fn fix_overlapping_diagnostics_requires_multiple_iterations() {
        let mut db = TestDbBuilder::new()
            .with_file(
                "test.py",
                "from typing import List, Optional\n\
                 value: Optional[List[int]] = None\n\
                ",
            )
            .build()
            .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();

        // Simulates two overlapping typing-modernization diagnostics for
        // `Optional[List[int]]`: one rewrites the outer `Optional[...]` to `... | None`, while
        // the other rewrites the nested `List[int]` to `list[int]` and, on the next pass,
        // removes the now-unused `List` import. Because `List[int]` is nested inside
        // `Optional[List[int]]`, only the outer rewrite can apply in the first iteration.
        let check_file = |db: &dyn Db, file: File| {
            let text = source_text(db, file);

            let range_of = |needle: &str| {
                let start = text.find(needle).unwrap_or_else(|| {
                    panic!("Expected `{needle}` in source:\n{}", text.as_str());
                }) as u32;
                let end = start + needle.len() as u32;
                TextRange::new(TextSize::new(start), TextSize::new(end))
            };

            let use_builtin_list_diagnostic = |file: File| {
                let mut list = Diagnostic::new(
                    DiagnosticId::lint("use-builtin-list"),
                    Severity::Warning,
                    "Use `list` instead of `List`.",
                );
                let list_range = range_of("List[int]");
                list.annotate(Annotation::primary(Span::from(file).with_range(list_range)));
                (list, list_range)
            };

            // Iteration 0: Replace `Optional[List[int]]` with `List[int] | None`
            if text.contains("Optional[List[int]]") {
                let mut optional = Diagnostic::new(
                    DiagnosticId::lint("use-pep604-optional"),
                    Severity::Warning,
                    "Use PEP 604 syntax for `Optional`.",
                );
                let optional_range = range_of("Optional[List[int]]");
                optional.annotate(Annotation::primary(
                    Span::from(file).with_range(optional_range),
                ));
                optional.set_fix(Fix::safe_edit(Edit::range_replacement(
                    "List[int] | None".to_string(),
                    optional_range,
                )));

                // This fix overlaps with `Optional[List[int]]` but the `Optional` fix applies
                // first because its `range.start` sorts before `List[int]`.
                let (mut list, list_range) = use_builtin_list_diagnostic(file);
                list.set_fix(Fix::safe_edit(Edit::range_replacement(
                    "list[int]".to_string(),
                    list_range,
                )));

                vec![optional, list]
            }
            // Iteration 2, replace `List[int] | None` with `list[int] | None`
            else if text.contains("List[int] | None") {
                let (mut list, list_range) = use_builtin_list_diagnostic(file);
                list.set_fix(Fix::safe_edits(
                    Edit::range_replacement("list[int]".to_string(), list_range),
                    [Edit::range_replacement(
                        "from typing import Optional".to_string(),
                        range_of("from typing import List, Optional"),
                    )],
                ));

                vec![list]
            } else {
                Vec::new()
            }
        };

        let initial_diagnostics = check_file(&db, file);

        let cancellation_token_source = CancellationTokenSource::new();
        let fixes = fix_all(
            &mut db,
            initial_diagnostics,
            FixMode::ApplyFixes,
            &cancellation_token_source.token(),
            check_file,
        )
        .expect("operation never gets cancelled");

        assert!(fixes.diagnostics.is_empty());
        assert_eq!(fixes.count, 2);
        assert_eq!(
            &*source_text(&db, file),
            "from typing import Optional\n\
             value: list[int] | None = None\n\
            "
        );
    }

    #[track_caller]
    fn suppress_all_in(source: &str) -> String {
        use std::fmt::Write as _;

        let mut db = TestDbBuilder::new()
            .with_file(
                "test.py",
                ruff_python_trivia::textwrap::dedent(source).trim(),
            )
            .build()
            .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();

        let parsed_before = parsed_module(&db, file);
        let had_syntax_errors = parsed_before.load(&db).has_syntax_errors();

        let diagnostics = db.check_file(file);
        let total_diagnostics = diagnostics.len();
        let cancellation_token_source = CancellationTokenSource::new();
        let fixes =
            suppress_all_diagnostics(&mut db, diagnostics, &cancellation_token_source.token())
                .expect("operation never gets cancelled");

        assert_eq!(fixes.count, total_diagnostics - fixes.diagnostics.len());

        File::sync_path(&mut db, SystemPath::new("test.py"));

        let fixed = source_text(&db, file);

        let parsed = parsed_module(&db, file);
        let parsed = parsed.load(&db);

        let diagnostics_after_applying_fixes = db.check_file(file);

        let mut output = String::new();

        writeln!(
            output,
            "Added {} suppressions\n\n## Fixed source\n\n```py\n{}\n```\n",
            fixes.count,
            fixed.as_str()
        )
        .unwrap();

        if !fixes.diagnostics.is_empty() {
            writeln!(
                output,
                "## Diagnostics after applying fixes\n\n{diagnostics}\n",
                diagnostics = DisplayDiagnostics::new(
                    &db,
                    &DisplayDiagnosticConfig::new("ty"),
                    &fixes.diagnostics
                )
            )
            .unwrap();
        }

        assert!(
            !parsed.has_syntax_errors() || had_syntax_errors,
            "Fixed introduced syntax errors\n\n{output}"
        );

        let new_diagnostics =
            diff_diagnostics(&fixes.diagnostics, &diagnostics_after_applying_fixes);

        if !new_diagnostics.is_empty() {
            writeln!(
                &mut output,
                "## New diagnostics after re-checking file\n\n{diagnostics}\n",
                diagnostics = DisplayDiagnostics::new(
                    &db,
                    &DisplayDiagnosticConfig::new("ty"),
                    &new_diagnostics
                )
            )
            .unwrap();
        }

        output
    }

    fn diff_diagnostics<'a>(before: &'a [Diagnostic], after: &'a [Diagnostic]) -> Vec<Diagnostic> {
        let before = DiagnosticFingerprint::group_diagnostics(before);
        let after = DiagnosticFingerprint::group_diagnostics(after);

        after
            .into_iter()
            .filter(|(key, _)| !before.contains_key(key))
            .map(|(_, diagnostic)| diagnostic.clone())
            .collect()
    }

    #[derive(Copy, Clone, Eq, PartialEq, Hash)]
    struct DiagnosticFingerprint(u64);

    impl DiagnosticFingerprint {
        fn group_diagnostics(diagnostics: &[Diagnostic]) -> FxHashMap<Self, &Diagnostic> {
            let mut result = FxHashMap::default();

            for diagnostic in diagnostics {
                Self::from_diagnostic(diagnostic, &mut result);
            }

            result
        }

        fn from_diagnostic<'a>(
            diagnostic: &'a Diagnostic,
            seen: &mut FxHashMap<DiagnosticFingerprint, &'a Diagnostic>,
        ) -> DiagnosticFingerprint {
            let mut disambiguator = 0u64;

            loop {
                let mut h = DefaultHasher::default();
                disambiguator.hash(&mut h);

                diagnostic.id().hash(&mut h);

                let key = DiagnosticFingerprint(h.finish());
                match seen.entry(key) {
                    Entry::Occupied(_) => {
                        disambiguator += 1;
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(diagnostic);
                        return key;
                    }
                }
            }
        }
    }
}

use ruff_db::cancellation::{Canceled, CancellationToken};
use ruff_db::diagnostic::{DisplayDiagnosticConfig, DisplayDiagnostics};
use ruff_db::parsed::parsed_module;
use ruff_db::source::SourceText;
use ruff_db::system::{SystemPath, WritableSystem};
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span},
    files::File,
    source::source_text,
};
use ruff_diagnostics::{Fix, IsolationLevel, SourceMap};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use rustc_hash::FxHashSet;
use salsa::Setter as _;
use std::collections::BTreeMap;
use thiserror::Error;
use ty_python_semantic::{UNUSED_IGNORE_COMMENT, suppress_all};

use crate::Db;

pub struct SuppressAllResult {
    /// The non-lint diagnostics that can't be suppressed or the diagnostics of files
    /// that couldn't be suppressed (because ty failed to write the result back to disk,
    /// or the file contains syntax errors).
    pub diagnostics: Vec<Diagnostic>,

    /// The number of diagnostics that were suppressed.
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
    mut diagnostics: Vec<Diagnostic>,
    cancellation_token: &CancellationToken,
) -> Result<SuppressAllResult, Canceled> {
    let system = WritableSystem::dyn_clone(
        db.system()
            .as_writable()
            .expect("System should be writable"),
    );

    let has_fixable = diagnostics.iter().any(|diagnostic| {
        diagnostic
            .primary_span()
            .and_then(|span| span.range())
            .is_some()
            && diagnostic.id().is_lint()
            && diagnostic.id() != DiagnosticId::Lint(UNUSED_IGNORE_COMMENT.name())
    });

    // Early return if there are no diagnostics that can be suppressed to avoid all the heavy work below.
    if !has_fixable {
        return Ok(SuppressAllResult {
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

    let mut fixed_count = 0usize;
    let project = db.project();

    // Try to suppress all lint-diagnostics in the given file.
    for (&file, file_diagnostics) in &mut by_file {
        if cancellation_token.is_cancelled() {
            return Err(Canceled);
        }

        let Some(path) = file.path(db).as_system_path() else {
            tracing::debug!(
                "Skipping file `{}` with non-system path because vendored and system virtual file paths are read-only",
                file.path(db)
            );

            continue;
        };

        let parsed = parsed_module(db, file);
        if parsed.load(db).has_syntax_errors() {
            tracing::warn!("Skipping file `{path}` with syntax errors",);
            continue;
        }

        let fixable_diagnostics: Vec<_> = file_diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let lint_id = diagnostic.id().as_lint()?;

                // Don't suppress unused ignore comments.
                if lint_id == UNUSED_IGNORE_COMMENT.name() {
                    return None;
                }

                // We can't suppress diagnostics without a corresponding file or range.
                let span = diagnostic.primary_span()?;
                let range = span.range()?;

                Some((lint_id, range))
            })
            .collect();

        if fixable_diagnostics.is_empty() {
            tracing::debug!(
                "Skipping file `{path}` because it contains no suppressable diagnostics"
            );
            continue;
        }

        tracing::debug!(
            "Suppressing {} diagnostics in `{path}`.",
            fixable_diagnostics.len()
        );

        // Required to work around borrow checker issues.
        let path = path.to_path_buf();
        let fixes = suppress_all(db, file, &fixable_diagnostics);
        let source = source_text(db, file);

        // TODO: Handle overlapping fixes when adding support for `--fix` by iterating until all fixes
        // were successfully applied. We don't need to do that for suppressions because suppression fixes
        // should never overlap (and, if they were, the worst outcome is that some suppressions are missing).
        let FixedCode {
            source: new_source,
            source_map,
        } = apply_fixes(&source, fixes).unwrap_or_else(|fixed| fixed);

        let new_source = source.with_text(new_source, &source_map);

        // Verify that the fix didn't introduce any syntax errors by overriding
        // the source text for `file`.
        let mut source_guard = WithUpdatedSourceGuard::new(db, file, &source, new_source.clone());
        let db = source_guard.db();
        let new_parsed = parsed_module(db, file);
        let new_parsed = new_parsed.load(db);

        if new_parsed.has_syntax_errors() {
            let mut diag = Diagnostic::new(
                DiagnosticId::InternalError,
                Severity::Fatal,
                format_args!(
                    "Adding suppressions introduced a syntax error. Reverting all changes."
                ),
            );

            let mut file_annotation = Annotation::primary(Span::from(file));
            file_annotation.hide_snippet(true);
            diag.annotate(file_annotation);

            let parse_diagnostics: Vec<_> = new_parsed
                .errors()
                .iter()
                .map(|error| {
                    Diagnostic::invalid_syntax(Span::from(file), &error.error, error.location)
                })
                .collect();

            diag.add_bug_sub_diagnostics("%5BFix%20error%5D");

            let file_db: &dyn ruff_db::Db = db;

            diag.info(format_args!(
                "Introduced syntax errors:\n\n{}",
                DisplayDiagnostics::new(
                    &file_db,
                    &DisplayDiagnosticConfig::default(),
                    &parse_diagnostics
                )
            ));

            file_diagnostics.push(diag);

            continue;
        }

        // Write the changes back to disk.
        if let Err(err) = write_changes(db, &*system, file, &path, &new_source) {
            let mut diag = Diagnostic::new(
                DiagnosticId::Io,
                Severity::Error,
                format_args!("Failed to write fixes to file: {err}"),
            );

            diag.annotate(Annotation::primary(Span::from(file)));
            diagnostics.push(diag);

            continue;
        }

        // If we got here then we've been successful. Re-check to get the diagnostics with the
        // update source, update the fix count.

        if fixable_diagnostics.len() == file_diagnostics.len() {
            file_diagnostics.clear();
        } else {
            // If there are any other file level diagnostics, call `check_file` to re-compute them
            // with updated ranges.
            let diagnostics = project.check_file(db, file);
            *file_diagnostics = diagnostics;
        }

        fixed_count += fixable_diagnostics.len();
        // Don't restore the source text or we risk a panic when rendering the diagnostics
        // if reading any of the fixed files fails (for whatever reason).
        // The override will get removed on the next `File::sync_path` call.
        source_guard.defuse();
    }

    // Stitch the remaining diagnostics back together.
    diagnostics.extend(by_file.into_values().flatten());
    diagnostics.sort_by(|left, right| {
        left.rendering_sort_key(db)
            .cmp(&right.rendering_sort_key(db))
    });

    Ok(SuppressAllResult {
        diagnostics,
        count: fixed_count,
    })
}

fn write_changes(
    db: &dyn Db,
    system: &dyn WritableSystem,
    file: File,
    path: &SystemPath,
    new_source: &SourceText,
) -> Result<(), WriteChangesError> {
    let metadata = system.path_metadata(path)?;

    if metadata.revision() != file.revision(db) {
        return Err(WriteChangesError::FileWasModified);
    }

    system.write_file_bytes(path, &new_source.to_bytes())?;

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
fn apply_fixes(source: &str, mut fixes: Vec<Fix>) -> Result<FixedCode, FixedCode> {
    let mut output = String::with_capacity(source.len());
    let mut last_pos: Option<TextSize> = None;
    let mut has_overlapping_fixes = false;
    let mut isolated: FxHashSet<u32> = FxHashSet::default();

    let mut source_map = SourceMap::default();

    fixes.sort_unstable_by_key(Fix::min_start);

    for fix in fixes {
        let mut edits = fix.edits().iter().peekable();

        // If the fix contains at least one new edit, enforce isolation and positional requirements.
        if let Some(first) = edits.peek() {
            // If this fix requires isolation, and we've already applied another fix in the
            // same isolation group, skip it.
            if let IsolationLevel::Group(id) = fix.isolation() {
                if !isolated.insert(id) {
                    has_overlapping_fixes = true;
                    continue;
                }
            }

            // If this fix overlaps with a fix we've already applied, skip it.
            if last_pos.is_some_and(|last_pos| last_pos >= first.start()) {
                has_overlapping_fixes = true;
                continue;
            }
        }

        let mut applied_edits = Vec::with_capacity(fix.edits().len());
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
            applied_edits.push(edit);
        }
    }

    // Add the remaining content.
    let slice = &source[last_pos.unwrap_or_default().to_usize()..];
    output.push_str(slice);

    let fixed = FixedCode {
        source: output,
        source_map,
    };

    if has_overlapping_fixes {
        Err(fixed)
    } else {
        Ok(fixed)
    }
}

struct FixedCode {
    /// Source map that allows mapping positions in the fixed code back to positions in the original
    /// source code (useful for mapping fixed lines back to their original notebook cells).
    source_map: SourceMap,

    /// The fixed source code
    source: String,
}

/// Guard that sets [`File::set_source_text_override`] and guarantees to restore the original source
/// text unless the guard is explicitly defused.
struct WithUpdatedSourceGuard<'db> {
    db: &'db mut dyn Db,
    file: File,
    old_source: Option<SourceText>,
}

impl<'db> WithUpdatedSourceGuard<'db> {
    fn new(
        db: &'db mut dyn Db,
        file: File,
        old_source: &SourceText,
        new_source: SourceText,
    ) -> Self {
        file.set_source_text_override(db).to(Some(new_source));
        Self {
            db,
            file,
            old_source: Some(old_source.clone()),
        }
    }

    fn defuse(&mut self) {
        self.old_source = None;
    }

    fn db(&mut self) -> &mut dyn Db {
        self.db
    }
}

impl Drop for WithUpdatedSourceGuard<'_> {
    fn drop(&mut self) {
        if let Some(old_source) = self.old_source.take() {
            // We don't set `source_text_override` to `None` here because setting the value
            // invalidates the `source_text` query and there's the chance that reading the file's content
            // will fail this time (e.g. because the file was deleted), resulting in ty panicking
            // when trying to render any diagnostic for that file (because all offsets now point nowhere).
            // The override will be cleared by `File::sync_path`, the next time the revision changes.
            self.file
                .set_source_text_override(self.db)
                .to(Some(old_source));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::Entry;
    use std::hash::{DefaultHasher, Hash, Hasher};

    use insta::assert_snapshot;
    use ruff_db::cancellation::CancellationTokenSource;
    use ruff_db::diagnostic::{Diagnostic, DisplayDiagnosticConfig, DisplayDiagnostics};
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::parsed::parsed_module;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_python_ast::name::Name;
    use rustc_hash::FxHashMap;
    use ty_python_semantic::UNUSED_IGNORE_COMMENT;
    use ty_python_semantic::lint::Level;

    use crate::db::tests::TestDb;
    use crate::metadata::options::Rules;
    use crate::metadata::value::RangedValue;
    use crate::{Db, ProjectMetadata, suppress_all_diagnostics};

    #[test]
    fn simple_suppression() {
        assert_snapshot!(
            suppress_all_in(r#"
                a = b + 10"#
        ),
         @r"
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
         @r"
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
         @r"
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
         @r"
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
         @r"
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
        info: rule `unresolved-reference` is enabled by default

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

    #[track_caller]
    fn suppress_all_in(source: &str) -> String {
        use std::fmt::Write as _;

        let mut metadata = ProjectMetadata::new(Name::new_static("test"), SystemPathBuf::from("."));
        metadata.options.rules = Some(Rules::from_iter([(
            RangedValue::cli(UNUSED_IGNORE_COMMENT.name.to_string()),
            RangedValue::cli(Level::Warn),
        )]));

        let mut db = TestDb::new(metadata);
        db.init_program().unwrap();

        db.write_file(
            "test.py",
            ruff_python_trivia::textwrap::dedent(source).trim(),
        )
        .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();

        let parsed_before = parsed_module(&db, file);
        let had_syntax_errors = parsed_before.load(&db).has_syntax_errors();

        let diagnostics = db.project().check_file(&db, file);
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

        let diagnostics_after_applying_fixes = db.project().check_file(&db, file);

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
                    &DisplayDiagnosticConfig::default(),
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
                    &DisplayDiagnosticConfig::default(),
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

use std::collections::BTreeMap;

use ruff_db::cancellation::{CancellationToken, Cancelled};
use ruff_db::parsed::parsed_module;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span},
    files::File,
    source::source_text,
};
use ruff_diagnostics::{Fix, SourceMap};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use ty_python_semantic::{UNUSED_IGNORE_COMMENT, suppress_all};

use crate::Db;

pub struct SuppressAllResult {
    /// The non-lint diagnostics that can't be suppressed.
    pub diagnostics: Vec<Diagnostic>,

    /// The number of diagnostics that were suppressed.
    pub count: usize,
}

/// Adds suppressions to all lint diagnostics and writes the changed files back to disk.
///
/// Returns how many diagnostics were suppressed
pub fn suppress_all_diagnostics(
    db: &dyn Db,
    mut diagnostics: Vec<Diagnostic>,
    cancellation_token: &CancellationToken,
) -> Result<SuppressAllResult, Cancelled> {
    let system = db
        .system()
        .as_writable()
        .expect("System should be writable");

    let mut by_file: BTreeMap<File, (Vec<_>, bool)> = BTreeMap::new();

    // Group the diagnostics by file
    for diagnostic in &diagnostics {
        if cancellation_token.is_cancelled() {
            return Err(Cancelled);
        }

        let DiagnosticId::Lint(lint_id) = diagnostic.id() else {
            continue;
        };

        // Don't suppress unused ignore comments.
        if lint_id == UNUSED_IGNORE_COMMENT.name() {
            continue;
        }

        // We can't suppress diagnostics without a corresponding file or range.
        let Some(span) = diagnostic.primary_span() else {
            continue;
        };

        let Some(range) = span.range() else {
            continue;
        };

        by_file
            .entry(span.expect_ty_file())
            .or_default()
            .0
            .push((lint_id, range));
    }

    let mut count = 0usize;

    for (&file, (to_suppress, fixed)) in &mut by_file {
        if cancellation_token.is_cancelled() {
            return Err(Cancelled);
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
            tracing::debug!(
                "Skipping file `{}` because it contains syntax errors",
                file.path(db)
            );
            continue;
        }

        let mut source = source_text(db, file);

        let count_current_file = to_suppress.len();

        let fixes = suppress_all(db, file, to_suppress);
        let (new_source, source_map) = apply_fixes(db, file, fixes);

        source.updated(new_source, &source_map);

        let Ok(metadata) = system.path_metadata(path) else {
            // TODO, handle error
            continue;
        };

        // Don't write back the changes if the file has changed in the meantime.
        if metadata.revision() != file.revision(db) {
            // TODO
            continue;
        }

        // TODO: Assert that there are no new syntax errors, somehow :)

        // TODO: Assert that the file hasn't changed
        // Create new source from applying fixes
        if let Err(err) = system.write_file(path, &source.to_raw_content()) {
            let mut diag = Diagnostic::new(
                DiagnosticId::Io,
                Severity::Error,
                format_args!("Failed to write fixes: {err}"),
            );
            diag.annotate(Annotation::primary(Span::from(file)));
            diagnostics.push(diag);
            continue;
        }

        *fixed = true;
        count += count_current_file;
    }

    // Remove the now suppressed diagnostics
    diagnostics.retain(|diagnostic| {
        let Some(span) = diagnostic.primary_span() else {
            return true;
        };

        if let Some((_, fixed)) = by_file.get(&span.expect_ty_file()) {
            !fixed
        } else {
            true
        }
    });

    Ok(SuppressAllResult { diagnostics, count })
}

/// Apply a series of fixes to `File` and returns the updated source code along with the source map.
fn apply_fixes(db: &dyn Db, file: File, mut fixes: Vec<Fix>) -> (String, SourceMap) {
    let source = source_text(db, file);
    let source = source.as_str();

    let mut output = String::with_capacity(source.len());
    let mut last_pos: Option<TextSize> = None;

    let mut source_map = SourceMap::default();

    fixes.sort_unstable_by_key(ruff_diagnostics::Fix::min_start);

    for fix in fixes {
        let mut edits = fix.edits().iter().peekable();

        // If the fix contains at least one new edit, enforce isolation and positional requirements.
        if let Some(first) = edits.peek() {
            // If this fix overlaps with a fix we've already applied, skip it.
            if last_pos.is_some_and(|last_pos| last_pos >= first.start()) {
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

    (output, source_map)
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
        let fixes = suppress_all_diagnostics(&db, diagnostics, &cancellation_token_source.token())
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

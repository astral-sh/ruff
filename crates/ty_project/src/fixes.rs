use std::collections::BTreeMap;

use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span},
    files::{File, FilePath},
    source::source_text,
};
use ruff_diagnostics::{Fix, SourceMap};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use ty_python_semantic::suppress_all;

use crate::Db;

pub struct SuppressAllResult {
    /// The non-lint diagnostics that can't be suppressed.
    pub diagnostics: Vec<Diagnostic>,

    /// The number of diagnostics that were suppressed.
    pub count: usize,
}

/// Suppress all
pub fn suppress_all_diagnostics(db: &dyn Db, diagnostics: Vec<Diagnostic>) -> SuppressAllResult {
    let system = db
        .system()
        .as_writable()
        .expect("System should be writable");

    let mut non_lint_diagnostics = diagnostics;
    let mut by_file: BTreeMap<File, Vec<_>> = BTreeMap::new();

    non_lint_diagnostics.retain(|diagnostic| {
        let DiagnosticId::Lint(lint_id) = diagnostic.id() else {
            return true;
        };

        let Some(span) = diagnostic.primary_span() else {
            return true;
        };

        let Some(range) = span.range() else {
            return true;
        };

        by_file
            .entry(span.expect_ty_file())
            .or_default()
            .push((lint_id, range));

        false
    });

    let mut count = 0usize;
    for (file, to_suppress) in by_file {
        let FilePath::System(path) = file.path(db) else {
            tracing::debug!(
                "Skipping file `{}` with non-system path because vendored and system virtual file paths are read-only",
                file.path(db)
            );
            continue;
        };

        let mut source = source_text(db, file);

        let count_current_file = to_suppress.len();

        let fixes = suppress_all(db, file, to_suppress);
        let (new_source, source_map) = apply_fixes(db, file, fixes);

        source.updated(new_source, &source_map);

        // Create new source from applying fixes
        if let Err(err) = system.write_file(path, &*source.to_raw_content()) {
            let mut diag = Diagnostic::new(
                DiagnosticId::Io,
                Severity::Error,
                format_args!("Failed to write fixes: {err}"),
            );
            diag.annotate(Annotation::primary(Span::from(file)));
            non_lint_diagnostics.push(diag);
            continue;
        }

        count += count_current_file;
    }

    SuppressAllResult {
        diagnostics: non_lint_diagnostics,
        count,
    }
}

/// Apply a series of fixes to `File` and returns the updated source code along with the source map.
fn apply_fixes(db: &dyn Db, file: File, mut fixes: Vec<Fix>) -> (String, SourceMap) {
    let source = source_text(db, file);
    let source = source.as_str();

    let mut output = String::with_capacity(source.len());
    let mut last_pos: Option<TextSize> = None;

    let mut source_map = SourceMap::default();

    fixes.sort_unstable_by_key(|fix| fix.min_start());

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

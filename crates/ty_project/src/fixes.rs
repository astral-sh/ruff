//! Applying fixes to ty configuration and Python source files.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use ruff_db::Db as _;
use ruff_db::cancellation::{Canceled, CancellationToken};
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, Severity, Span, SubDiagnostic, UnifiedFile,
};
use ruff_db::files::File;
use ruff_db::system::{DbWithWritableSystem as _, SystemPath};
use ruff_diagnostics::Applicability;
use ruff_text_size::Ranged;
use ty_python_semantic::FixAllResults;

use crate::metadata::options::RuleRename;
use crate::{CheckDiagnostics, Db as _, ProjectDatabase, ProjectFiles};

impl ProjectDatabase {
    /// Applies safe fixes to configuration and Python files and returns the remaining diagnostics.
    pub fn fix_all_diagnostics(
        &mut self,
        mut diagnostics: CheckDiagnostics,
        cancellation_token: &CancellationToken,
    ) -> Result<FixAllResults, Canceled> {
        let mut by_file: BTreeMap<File, Vec<RuleRename>> = BTreeMap::new();
        let project = self.project();
        for (file, rename) in project
            .settings_diagnostics(self)
            .iter()
            .filter_map(|diagnostic| diagnostic.rule_rename())
        {
            by_file.entry(file).or_default().push(rename.clone());
        }

        let mut count = 0;
        let mut errors = Vec::new();
        let mut unfixed_configs = Vec::new();
        let mut edited_configs = BTreeSet::new();
        for (file, renames) in by_file {
            if cancellation_token.is_cancelled() {
                return Err(Canceled);
            }

            let Some(path) = file.path(self).as_system_path() else {
                continue;
            };
            match write_rule_renames(self, file, path, renames) {
                Ok(0) => unfixed_configs.push(file),
                Ok(applied) => {
                    count += applied;
                    edited_configs.insert(file);
                    let path = path.to_path_buf();
                    File::sync_path_only(self, &path);
                }
                Err(error) => {
                    unfixed_configs.push(file);
                    let mut diagnostic = Diagnostic::new(
                        DiagnosticId::Io,
                        Severity::Error,
                        format_args!("Failed to write fixes to file: {error}"),
                    );
                    diagnostic.annotate(Annotation::primary(Span::from(file)));
                    errors.push(diagnostic);
                }
            }
        }

        if count > 0 {
            if cancellation_token.is_cancelled() {
                return Err(Canceled);
            }
            let root = project.root(self).to_path_buf();
            let workspace = project.metadata(self).uv_workspace().clone();
            match project.rediscover(self, &root, workspace) {
                Ok(_) => diagnostics.refresh_after_config_edits(
                    self,
                    &edited_configs,
                    cancellation_token,
                )?,
                Err(error) => {
                    errors.push(Diagnostic::new(
                        DiagnosticId::Io,
                        Severity::Error,
                        format_args!(
                            "Failed to reload configuration after applying fixes: {error}"
                        ),
                    ));
                    return Ok(FixAllResults {
                        diagnostics: errors,
                        count,
                    });
                }
            }
        }

        // Clear skipped TOML edits before passing the warnings to the Python fixer.
        for diagnostic in &mut diagnostics.workspace {
            if diagnostic.id() == DiagnosticId::RenamedRule
                && diagnostic
                    .primary_span()
                    .is_some_and(|span| unfixed_configs.contains(&span.expect_ty_file()))
            {
                diagnostic.remove_fix();
            }
        }

        let diagnostics = diagnostics.into_sorted(self);
        let mut result = ty_python_semantic::fix_all_diagnostics(
            self,
            diagnostics,
            Applicability::Safe,
            cancellation_token,
        )?;
        result.count += count;
        if !errors.is_empty() {
            result.diagnostics.extend(errors);
            result.diagnostics.sort_by(|left, right| {
                left.rendering_sort_key(self)
                    .cmp(&right.rendering_sort_key(self))
            });
        }
        Ok(result)
    }
}

impl CheckDiagnostics {
    /// Refreshes workspace diagnostics and rechecks files whose diagnostics point to edited TOML.
    fn refresh_after_config_edits(
        &mut self,
        db: &ProjectDatabase,
        edited_configs: &BTreeSet<File>,
        cancellation_token: &CancellationToken,
    ) -> Result<(), Canceled> {
        let project = db.project();
        self.workspace = project.workspace_diagnostics(db, &ProjectFiles::new(db, project));

        // A rule rename preserves rule selection but can shift byte offsets in the TOML file.
        let files_to_recheck: BTreeSet<_> = self
            .file_diagnostics
            .iter()
            .filter_map(|(file, diagnostic)| {
                references_config(diagnostic, edited_configs).then_some(*file)
            })
            .collect();

        self.file_diagnostics
            .retain(|(file, _)| !files_to_recheck.contains(file));
        for file in files_to_recheck {
            if cancellation_token.is_cancelled() {
                return Err(Canceled);
            }
            self.file_diagnostics.extend(
                db.check_file(file)
                    .into_iter()
                    .map(|diagnostic| (file, diagnostic)),
            );
        }
        Ok(())
    }
}

/// Returns whether the diagnostic or any of its subdiagnostics annotates an edited config file.
fn references_config(diagnostic: &Diagnostic, edited_configs: &BTreeSet<File>) -> bool {
    diagnostic
        .annotations()
        .iter()
        .chain(diagnostic.sub_diagnostics().iter().flat_map(SubDiagnostic::annotations))
        .any(|annotation| {
            matches!(annotation.get_span().file(), UnifiedFile::Ty(file) if edited_configs.contains(file))
        })
}

/// Applies a batch of rule renames to one TOML file and returns the number of edits.
/// Skips the whole batch if any edit was computed from different text.
fn write_rule_renames(
    db: &ProjectDatabase,
    file: File,
    path: &SystemPath,
    renames: Vec<RuleRename>,
) -> io::Result<usize> {
    let mut renames = renames.into_iter();
    let Some(RuleRename { edit, source }) = renames.next() else {
        return Ok(0);
    };
    let mut edits = vec![edit];
    for rename in renames {
        if rename.source != source {
            return Ok(0);
        }
        edits.push(rename.edit);
    }

    edits.sort_unstable();
    edits.dedup();
    let mut output = source.to_string();
    let mut next_start = source.len();
    for edit in edits.iter().rev() {
        let range = edit.range().to_std_range();
        if range.end > next_start || source.get(range.clone()).is_none() {
            return Err(io::Error::other("invalid rule key replacement range"));
        }
        next_start = range.start;
        output.replace_range(range, edit.content().unwrap_or_default());
    }
    toml::from_str::<toml::Table>(&output).map_err(io::Error::other)?;

    let system = db.writable_system()?;
    if db.system().read_to_string(path)? != *source
        || system.path_metadata(path)?.revision() != file.revision(db)
    {
        return Ok(0);
    }
    system.write_file(path, &output)?;
    Ok(edits.len())
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::cancellation::CancellationTokenSource;
    use ruff_db::diagnostic::DisplayDiagnosticConfig;
    use ruff_db::system::TestSystem;

    use crate::metadata::options::OutputFormat;
    use crate::watch::ChangeEvent;
    use crate::{CollectReporter, ProjectDatabase, ProjectMetadata};

    #[test]
    fn skips_rule_rename_after_failed_config_reload() -> anyhow::Result<()> {
        // The unknown `[bogus]` table makes reload fail, so ty keeps the old rule key's byte range.
        // Padding puts the same quoted name at that range in `src.exclude`. The file must stay intact.
        let system = TestSystem::default();
        let fs = system.memory_file_system();
        let root = fs.current_directory();
        let path = root.join("ty.toml");
        let original = r#"#12345678901
[rules]
"unresolved-import" = "ignore"
"#;
        let changed = r#"[src]
exclude = [    "unresolved-import"]
[bogus]
x = 1
"#;
        fs.write_file_all(&path, original)?;
        let metadata = ProjectMetadata::discover(root, &system)?;
        let mut db = ProjectDatabase::fallible(metadata, system.clone())?;

        fs.write_file_all(&path, changed)?;
        db.apply_changes(&[ChangeEvent::file_content_changed(path.clone())]);
        let mut reporter = CollectReporter::default();
        db.check_with_reporter(&mut reporter);
        let result = db.fix_all_diagnostics(
            reporter.into_diagnostics(),
            &CancellationTokenSource::new().token(),
        )?;

        let display_config = DisplayDiagnosticConfig::new("ty").with_show_fix_status(true);
        let diagnostics = OutputFormat::Full.display(&db, &display_config, &result.diagnostics);
        assert_eq!(result.count, 0);
        assert_eq!(fs.read_to_string(&path)?, changed);
        assert_snapshot!(diagnostics, @"
        warning[renamed-rule]: Rule `unresolved-import` has been renamed to `import-not-found`
        --> ty.toml:1:1
        ");
        Ok(())
    }
}

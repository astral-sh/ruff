use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity, UnifiedFile};
use ruff_db::file_revision::FileRevision;
use ruff_db::source::line_index;
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    DirectoryEntry, FileType, Metadata, OsSystem, System, SystemPath, SystemPathBuf,
    SystemVirtualPath, WhichResult, WritableSystem,
};
use ruff_notebook::{Notebook, NotebookError};
use similar::{DiffOp, TextDiff};
use ty_project::watch::{ChangeEvent, CreatedKind, DeletedKind};
use ty_project::{Db, ProjectDatabase};

use crate::IndicatifReporter;
use crate::printer::Printer;

/// A change between the merge base and the working tree, including untracked files.
#[derive(Debug)]
struct ChangedFile {
    baseline_path: Option<SystemPathBuf>,
    current_path: Option<SystemPathBuf>,
    baseline_contents: Option<String>,
    current_contents: Option<String>,
}

#[derive(Debug)]
pub(crate) struct GitDiff {
    files: Vec<ChangedFile>,
    baseline_files: BTreeMap<SystemPathBuf, Option<String>>,
}

impl GitDiff {
    pub(crate) fn discover(system: &OsSystem, cwd: &SystemPath, revision: &str) -> Result<Self> {
        let root_output = run_git(system, cwd, &["rev-parse", "--show-toplevel"])?;
        let root = SystemPathBuf::from(git_text(&root_output, "repository root")?.trim());

        let reference = if revision.is_empty() {
            default_reference(system, &root)
        } else {
            revision.to_owned()
        };

        let merge_base_output = run_git(system, &root, &["merge-base", &reference, "HEAD"])
            .with_context(|| format!("Failed to find the Git merge base for `{reference}`"))?;
        let merge_base = git_text(&merge_base_output, "merge base")?
            .trim()
            .to_owned();

        let changes = run_git(
            system,
            &root,
            &[
                "diff",
                "--name-status",
                "--find-renames",
                "-z",
                &merge_base,
                "--",
            ],
        )?;

        let mut files = Vec::new();
        let mut fields = nul_fields(&changes.stdout)?;

        while let Some(status) = fields.next() {
            let old = fields
                .next()
                .ok_or_else(|| anyhow!("Git returned a change without a file path"))?;

            let (baseline_relative, current_relative) = if status.starts_with('R') {
                let new = fields
                    .next()
                    .ok_or_else(|| anyhow!("Git returned a rename without its destination"))?;
                (Some(old), Some(new))
            } else if status.starts_with('C') {
                let new = fields
                    .next()
                    .ok_or_else(|| anyhow!("Git returned a copy without its destination"))?;
                (None, Some(new))
            } else {
                match status {
                    "A" => (None, Some(old)),
                    "D" => (Some(old), None),
                    _ => (Some(old), Some(old)),
                }
            };

            let baseline_contents = baseline_relative
                .map(|path| git_file(system, &root, &merge_base, path))
                .transpose()?;
            let current_path = current_relative.map(|path| root.join(path));
            let current_contents = current_path
                .as_deref()
                .and_then(|path| system.read_to_string(path).ok());

            // Binary files cannot affect Python source or TOML configuration and must not make a
            // Python-only check fail merely because the same commit also updates an image.
            if baseline_contents.as_ref().is_some_and(Option::is_none) {
                continue;
            }

            files.push(ChangedFile {
                baseline_path: baseline_relative.map(|path| root.join(path)),
                current_path,
                baseline_contents: baseline_contents.flatten(),
                current_contents,
            });
        }

        let untracked = run_git(
            system,
            &root,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )?;
        for path in nul_fields(&untracked.stdout)? {
            let path = root.join(path);
            files.push(ChangedFile {
                baseline_path: None,
                current_contents: system.read_to_string(&path).ok(),
                current_path: Some(path),
                baseline_contents: None,
            });
        }

        let mut baseline_files = BTreeMap::new();
        for file in &files {
            if let Some(path) = &file.baseline_path {
                baseline_files.insert(path.clone(), file.baseline_contents.clone());
            }
            if let Some(path) = &file.current_path
                && file.baseline_path.as_ref() != Some(path)
            {
                baseline_files.insert(path.clone(), None);
            }
        }

        tracing::debug!(
            "Comparing diagnostics against Git revision `{merge_base}` ({} changed paths)",
            files.len()
        );

        Ok(Self {
            files,
            baseline_files,
        })
    }

    pub(crate) fn check_baseline(
        self,
        db: &mut ProjectDatabase,
        printer: Printer,
    ) -> Result<DiagnosticBaseline> {
        // The OS walker cannot discover files deleted from the working tree, even though the Git
        // overlay can still read them. Seed those paths through the same creation events used by
        // the language server after forcing the initial project index.
        let _ = db.project().files(db);
        let baseline_creations = self
            .files
            .iter()
            .filter_map(|file| {
                let path = file.baseline_path.as_ref()?;
                (file.current_path.as_ref() != Some(path)
                    && is_python_path(path)
                    && db.project().is_file_included(db, path).is_included())
                .then(|| ChangeEvent::Created {
                    path: path.clone(),
                    kind: CreatedKind::File,
                })
            })
            .collect::<Vec<_>>();

        if !baseline_creations.is_empty() {
            db.apply_changes(&baseline_creations);
        }

        let mut reporter = IndicatifReporter::from(printer);
        db.check_with_reporter(&mut reporter);
        reporter.bar.finish_and_clear();
        let diagnostics = reporter.collector.into_sorted(db);
        let baseline = DiagnosticBaseline::capture(db, diagnostics, &self.files);

        let system = db.system_mut();
        let Some(system) = system.as_any_mut().downcast_mut::<GitSystem>() else {
            bail!("The Git baseline requires ty's Git-backed file system");
        };
        system.activate_current();

        let mut changes = Vec::new();
        for file in &self.files {
            match (&file.baseline_path, &file.current_path) {
                (Some(old), Some(new)) if old == new => {
                    changes.push(ChangeEvent::file_content_changed(new.clone()));
                }
                (Some(old), Some(new)) => {
                    changes.push(ChangeEvent::Deleted {
                        path: old.clone(),
                        kind: DeletedKind::File,
                    });
                    changes.push(ChangeEvent::Created {
                        path: new.clone(),
                        kind: CreatedKind::File,
                    });
                }
                (Some(old), None) => changes.push(ChangeEvent::Deleted {
                    path: old.clone(),
                    kind: DeletedKind::File,
                }),
                (None, Some(new)) => changes.push(ChangeEvent::Created {
                    path: new.clone(),
                    kind: CreatedKind::File,
                }),
                (None, None) => {}
            }
        }

        if !changes.is_empty() {
            db.apply_changes(&changes);
        }

        Ok(baseline)
    }
}

fn is_python_path(path: &SystemPath) -> bool {
    matches!(path.extension(), Some("py" | "pyi" | "ipynb"))
}

fn run_git(system: &OsSystem, cwd: &SystemPath, args: &[&str]) -> Result<Output> {
    let output = system
        .run_command("git", args, cwd)
        .with_context(|| format!("Failed to run `git {}`", args.join(" ")))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("`git {}` failed: {}", args.join(" "), error.trim());
    }

    Ok(output)
}

fn git_text<'a>(output: &'a Output, description: &str) -> Result<&'a str> {
    std::str::from_utf8(&output.stdout)
        .with_context(|| format!("Git returned a non-UTF-8 {description}"))
}

fn nul_fields(bytes: &[u8]) -> Result<impl Iterator<Item = &str>> {
    let text = std::str::from_utf8(bytes).context("Git returned a non-UTF-8 file path")?;
    Ok(text.split_terminator('\0'))
}

fn default_reference(system: &OsSystem, root: &SystemPath) -> String {
    if let Ok(output) = run_git(
        system,
        root,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) && let Ok(reference) = git_text(&output, "default branch")
    {
        return reference.trim().to_owned();
    }

    for candidate in ["main", "master"] {
        if run_git(system, root, &["rev-parse", "--verify", candidate]).is_ok() {
            return candidate.to_owned();
        }
    }

    "HEAD".to_owned()
}

fn git_file(
    system: &OsSystem,
    root: &SystemPath,
    revision: &str,
    path: &str,
) -> Result<Option<String>> {
    let object = format!("{revision}:{path}");
    let output = run_git(system, root, &["show", &object])?;
    Ok(String::from_utf8(output.stdout).ok())
}

/// A normal OS filesystem whose changed files initially expose their merge-base contents.
#[derive(Clone, Debug)]
pub(crate) struct GitSystem {
    native: OsSystem,
    baseline_files: Arc<BTreeMap<SystemPathBuf, Option<String>>>,
    baseline_active: Arc<AtomicBool>,
}

impl GitSystem {
    pub(crate) fn new(native: OsSystem, diff: &GitDiff) -> Self {
        Self {
            native,
            baseline_files: Arc::new(diff.baseline_files.clone()),
            baseline_active: Arc::new(AtomicBool::new(true)),
        }
    }

    fn activate_current(&mut self) {
        self.baseline_active.store(false, Ordering::Release);
    }

    fn baseline_file(&self, path: &SystemPath) -> Option<&Option<String>> {
        self.baseline_active
            .load(Ordering::Acquire)
            .then(|| self.baseline_files.get(path))
            .flatten()
    }

    fn has_baseline_descendant(&self, path: &SystemPath) -> bool {
        self.baseline_active.load(Ordering::Acquire)
            && self.baseline_files.iter().any(|(candidate, contents)| {
                contents.is_some() && candidate.as_path() != path && candidate.starts_with(path)
            })
    }
}

impl System for GitSystem {
    fn path_metadata(&self, path: &SystemPath) -> std::io::Result<Metadata> {
        match self.baseline_file(path) {
            Some(Some(contents)) => {
                let mut hasher = DefaultHasher::new();
                contents.hash(&mut hasher);
                let revision = FileRevision::new((1_u128 << 127) | u128::from(hasher.finish()));
                let permissions = self
                    .native
                    .path_metadata(path)
                    .ok()
                    .and_then(|metadata| metadata.permissions());
                Ok(Metadata::new(revision, permissions, FileType::File))
            }
            Some(None) => Err(not_found(path)),
            None => match self.native.path_metadata(path) {
                Ok(metadata) => Ok(metadata),
                Err(_) if self.has_baseline_descendant(path) => Ok(Metadata::new(
                    FileRevision::new(1_u128 << 127),
                    None,
                    FileType::Directory,
                )),
                Err(error) => Err(error),
            },
        }
    }

    fn canonicalize_path(&self, path: &SystemPath) -> std::io::Result<SystemPathBuf> {
        match self.native.canonicalize_path(path) {
            Ok(path) => Ok(path),
            Err(_) if self.baseline_file(path).is_some_and(Option::is_some) => {
                Ok(SystemPath::absolute(path, self.current_directory()))
            }
            Err(error) => Err(error),
        }
    }

    fn is_same_file(&self, first: &SystemPath, second: &SystemPath) -> std::io::Result<bool> {
        if self.baseline_file(first).is_some_and(Option::is_some)
            || self.baseline_file(second).is_some_and(Option::is_some)
        {
            Ok(SystemPath::absolute(first, self.current_directory())
                == SystemPath::absolute(second, self.current_directory()))
        } else {
            self.native.is_same_file(first, second)
        }
    }

    fn which(&self, binary_name: &str) -> WhichResult {
        self.native.which(binary_name)
    }

    fn run_command(
        &self,
        program: &str,
        args: &[&str],
        current_directory: &SystemPath,
    ) -> std::io::Result<Output> {
        self.native.run_command(program, args, current_directory)
    }

    fn read_to_string(&self, path: &SystemPath) -> std::io::Result<String> {
        match self.baseline_file(path) {
            Some(Some(contents)) => Ok(contents.clone()),
            Some(None) => Err(not_found(path)),
            None => self.native.read_to_string(path),
        }
    }

    fn read_to_notebook(&self, path: &SystemPath) -> Result<Notebook, NotebookError> {
        match self.baseline_file(path) {
            Some(Some(contents)) => Notebook::from_source_code(contents),
            Some(None) => Err(NotebookError::Io(not_found(path))),
            None => self.native.read_to_notebook(path),
        }
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> std::io::Result<String> {
        self.native.read_virtual_path_to_string(path)
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> Result<Notebook, NotebookError> {
        self.native.read_virtual_path_to_notebook(path)
    }

    fn current_directory(&self) -> &SystemPath {
        self.native.current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.native.user_config_directory()
    }

    fn cache_dir(&self) -> Option<SystemPathBuf> {
        self.native.cache_dir()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> std::io::Result<Box<dyn Iterator<Item = std::io::Result<DirectoryEntry>> + 'a>> {
        if !self.baseline_active.load(Ordering::Acquire) {
            return self.native.read_directory(path);
        }

        let mut entries = BTreeMap::new();
        match self.native.read_directory(path) {
            Ok(native_entries) => {
                for entry in native_entries {
                    let entry = entry?;
                    if self.path_metadata(entry.path()).is_ok() {
                        entries.insert(entry.path().to_path_buf(), entry);
                    }
                }
            }
            Err(_) if self.has_baseline_descendant(path) => {}
            Err(error) => return Err(error),
        }

        for (candidate, contents) in self.baseline_files.iter() {
            if contents.is_none() {
                continue;
            }

            let Ok(relative) = candidate.strip_prefix(path) else {
                continue;
            };
            let Some(component) = relative.as_str().split('/').next() else {
                continue;
            };
            if component.is_empty() {
                continue;
            }

            let child = path.join(component);
            let file_type = if child == *candidate {
                FileType::File
            } else {
                FileType::Directory
            };
            entries
                .entry(child.clone())
                .or_insert_with(|| DirectoryEntry::new(child, file_type));
        }

        Ok(Box::new(entries.into_values().map(Ok)))
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.native.walk_directory(path)
    }

    fn env_var(&self, name: &str) -> Result<String, std::env::VarError> {
        self.native.env_var(name)
    }

    fn as_writable(&self) -> Option<&dyn WritableSystem> {
        self.native.as_writable()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn dyn_clone(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }
}

fn not_found(path: &SystemPath) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("File does not exist in the Git baseline: {path}"),
    )
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct DiagnosticKey {
    path: SystemPathBuf,
    line: usize,
    column: usize,
    id: DiagnosticId,
    severity: Severity,
    message: String,
}

#[derive(Debug)]
struct LineMapping {
    current_path: SystemPathBuf,
    lines: Vec<Option<usize>>,
}

#[derive(Debug, Default)]
pub(crate) struct DiagnosticBaseline {
    diagnostics: HashMap<DiagnosticKey, usize>,
}

impl DiagnosticBaseline {
    fn capture(db: &ProjectDatabase, diagnostics: Vec<Diagnostic>, files: &[ChangedFile]) -> Self {
        let mappings = files
            .iter()
            .filter_map(|file| {
                let old_path = file.baseline_path.as_ref()?;
                let new_path = file.current_path.as_ref()?;
                let old_contents = file.baseline_contents.as_ref()?;
                let new_contents = file.current_contents.as_ref()?;

                let diff = TextDiff::from_lines(old_contents, new_contents);
                let mut lines = vec![None; diff.old_len()];

                for op in diff.ops() {
                    if let DiffOp::Equal {
                        old_index,
                        new_index,
                        len,
                    } = *op
                    {
                        for offset in 0..len {
                            lines[old_index + offset] = Some(new_index + offset + 1);
                        }
                    }
                }

                Some((
                    old_path.clone(),
                    LineMapping {
                        current_path: new_path.clone(),
                        lines,
                    },
                ))
            })
            .collect::<HashMap<_, _>>();

        let mut counts = HashMap::new();
        for diagnostic in diagnostics {
            if diagnostic.severity().is_fatal() {
                continue;
            }

            let Some(mut key) = diagnostic_key(db, &diagnostic) else {
                continue;
            };

            if let Some(mapping) = mappings.get(&key.path) {
                let Some(line) = key
                    .line
                    .checked_sub(1)
                    .and_then(|line| mapping.lines.get(line))
                    .and_then(|line| *line)
                else {
                    continue;
                };

                key.path.clone_from(&mapping.current_path);
                key.line = line;
            } else if files.iter().any(|file| {
                file.baseline_path.as_ref() == Some(&key.path) && file.current_path.is_none()
            }) {
                continue;
            }

            *counts.entry(key).or_insert(0) += 1;
        }

        Self {
            diagnostics: counts,
        }
    }

    pub(crate) fn filter(
        &mut self,
        db: &ProjectDatabase,
        diagnostics: Vec<Diagnostic>,
    ) -> Vec<Diagnostic> {
        diagnostics
            .into_iter()
            .filter(|diagnostic| {
                if diagnostic.severity().is_fatal() {
                    return true;
                }

                let Some(key) = diagnostic_key(db, diagnostic) else {
                    return true;
                };

                let Some(count) = self.diagnostics.get_mut(&key) else {
                    return true;
                };

                if *count == 0 {
                    return true;
                }

                *count -= 1;
                false
            })
            .collect()
    }
}

fn diagnostic_key(db: &ProjectDatabase, diagnostic: &Diagnostic) -> Option<DiagnosticKey> {
    let span = diagnostic.primary_span()?;
    let UnifiedFile::Ty(file) = span.file() else {
        return None;
    };
    let path = file.path(db).as_system_path()?.to_path_buf();
    let range = span.range()?;
    let source = ruff_db::source::source_text(db, *file);
    let location = line_index(db, *file).line_column(range.start(), source.as_str());

    Some(DiagnosticKey {
        path,
        line: location.line.get(),
        column: location.column.get(),
        id: diagnostic.id(),
        severity: diagnostic.severity(),
        message: diagnostic.concise_message().to_string(),
    })
}

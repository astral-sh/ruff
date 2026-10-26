//! Checks which declared dependencies are imported by a workspace member.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use char_str::CharStr;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use ty_python_core::ProgramFile;
use ty_python_semantic::dependency::{
    DependencyMetadata, DependencyProject, DependencyProjectKind, UNUSED_DEPENDENCY,
    imported_modules,
};
use ty_python_semantic::lint::LintId;

use crate::{Db, Project};

#[cfg(test)]
mod tests;

impl Project {
    /// Whether any selected manifest has a dependency we can check. This does not index source files.
    fn has_dependency_checks(self, db: &dyn Db) -> bool {
        self.metadata(db).use_uv().workspace_discovery_enabled()
            && self
                .settings(db)
                .enables_lint(LintId::of(&UNUSED_DEPENDENCY))
            && !manifests_to_check(db, self).is_empty()
    }
}

pub(crate) fn project_dependency_diagnostics(db: &dyn Db) -> &[Diagnostic] {
    let project = db.project();
    if project.has_dependency_checks(db) {
        check_project(db, project)
    } else {
        &[]
    }
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct ManifestCheck {
    file: File,
    severity: Severity,
    candidates: Candidates,
}

#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn manifests_to_check(db: &dyn Db, project: Project) -> Box<[ManifestCheck]> {
    let lint = LintId::of(&UNUSED_DEPENDENCY);
    if project.metadata(db).uv_workspace().error.is_some() {
        return Box::default();
    }
    let Ok(Some(metadata)) = project.dependency_metadata(db) else {
        return Box::default();
    };
    let mut importable = None;
    let mut manifests = Vec::new();
    for member in &metadata.projects {
        if member.kind != DependencyProjectKind::Project || member.dependencies.is_empty() {
            continue;
        }
        // An explicit check of a file or subdirectory can't establish what's unused elsewhere
        // in its member. The configured source filters still apply when checking a whole member.
        if !project
            .included_paths_or_root(db)
            .iter()
            .any(|included| member.path.starts_with(included))
        {
            continue;
        }
        let Ok(file) = system_path_to_file(db, member.path.join("pyproject.toml")) else {
            continue;
        };
        let Some(severity) = db.rule_selection(file).severity(lint) else {
            continue;
        };
        let importable = importable.get_or_insert_with(|| importable_ids(metadata));
        let candidates = candidate_declarations(metadata, member, importable);
        if !candidates.is_empty() {
            manifests.push(ManifestCheck {
                file,
                severity,
                candidates,
            });
        }
    }
    manifests.into_boxed_slice()
}

#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn check_project(db: &dyn Db, project: Project) -> Box<[Diagnostic]> {
    let mut diagnostics = Vec::new();
    for manifest in manifests_to_check(db, project) {
        if let Some(used) = member_dependency_usage(db, manifest.file) {
            diagnostics.extend(unused_diagnostics(
                &manifest.candidates,
                used,
                Span::from(manifest.file),
                manifest.severity,
            ));
        }
    }
    diagnostics.into_boxed_slice()
}

/// Partition the filtered project index once, so each member only analyzes its own files.
#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn member_files(db: &dyn Db, project: Project) -> Option<BTreeMap<SystemPathBuf, Arc<[File]>>> {
    let metadata = project.dependency_metadata(db).as_ref().ok()?.as_deref()?;
    let index = project.files(db);
    if !index.diagnostics().is_empty() {
        // A failed directory walk may have omitted a file that imports a dependency.
        return None;
    }

    let mut groups: BTreeMap<&SystemPath, Vec<File>> = BTreeMap::new();
    for file in &index {
        db.unwind_if_revision_cancelled();
        if !index.scripts().contains(&file)
            && let Some(path) = file.path(db).as_system_path()
            && let Some(member) = metadata.project_for_path(path)
            && member.kind == DependencyProjectKind::Project
        {
            groups.entry(&member.path).or_default().push(file);
        }
    }
    Some(
        groups
            .into_iter()
            .map(|(path, mut files)| {
                files.sort_unstable();
                (path.to_path_buf(), files.into())
            })
            .collect(),
    )
}

// When a sibling member gains or loses a file, its list changes but this member's imports don't.
#[salsa::tracked(returns(as_deref))]
fn files_for_manifest(db: &dyn Db, manifest: File) -> Option<Arc<[File]>> {
    let root = manifest.path(db).as_system_path()?.parent()?;
    let groups = member_files(db, db.project())?;
    Some(groups.get(root).cloned().unwrap_or_default())
}

#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
fn member_dependency_usage(db: &dyn Db, manifest: File) -> Option<Box<[CharStr]>> {
    let project = db.project();
    let metadata = project.dependency_metadata(db).as_ref().ok()?.as_deref()?;
    let program = project.program(db);
    let mut used = BTreeSet::new();
    for &file in files_for_manifest(db, manifest)? {
        db.unwind_if_revision_cancelled();
        record_imports(db, program.program_file(db, file), metadata, &mut used)?;
    }
    Some(used.into_iter().collect())
}

fn record_imports<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    metadata: &DependencyMetadata,
    used: &mut BTreeSet<CharStr>,
) -> Option<()> {
    let imports = imported_modules(db, file);
    if imports.incomplete {
        return None;
    }
    for import in &imports.modules {
        used.extend(
            metadata
                .import_owners(db, file, import.target)
                .iter()
                .cloned(),
        );
    }
    for name in &imports.unresolved {
        used.extend(metadata.module_owners_for_name(name).iter().cloned());
    }
    Some(())
}

fn importable_ids(metadata: &DependencyMetadata) -> FxHashSet<&CharStr> {
    metadata
        .module_owners
        .values()
        .flat_map(|owners| owners.iter())
        .collect()
}

type Candidates = BTreeMap<CharStr, Box<[CharStr]>>;

/// A name may refer to several declared versions or sources. An uninstalled alternative may
/// provide different modules from the installed one, so we need ownership information for each.
fn candidate_declarations(
    metadata: &DependencyMetadata,
    member: &DependencyProject,
    importable: &FxHashSet<&CharStr>,
) -> Candidates {
    let mut by_name: BTreeMap<&CharStr, Vec<CharStr>> = BTreeMap::new();
    for id in &member.dependencies {
        if let Some(distribution) = metadata.distributions.get(id) {
            by_name
                .entry(&distribution.name)
                .or_default()
                .push(id.clone());
        }
    }
    by_name
        .into_iter()
        .filter(|(_, ids)| ids.iter().all(|id| importable.contains(id)))
        .map(|(name, ids)| (name.clone(), ids.into_boxed_slice()))
        .collect()
}

fn unused_diagnostics<'a>(
    candidates: &'a Candidates,
    used: &'a [CharStr],
    span: Span,
    severity: Severity,
) -> impl Iterator<Item = Diagnostic> + 'a {
    unused_names(candidates, used).map(move |name| {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(UNUSED_DEPENDENCY.name()),
            severity,
            format_args!("Dependency `{name}` is declared but never imported"),
        );
        diagnostic.annotate(Annotation::primary(span.clone()));
        diagnostic.set_documentation_url(Some(UNUSED_DEPENDENCY.documentation_url()));
        diagnostic
    })
}

fn unused_names<'a>(
    candidates: &'a Candidates,
    used: &'a [CharStr],
) -> impl Iterator<Item = &'a CharStr> {
    candidates.iter().filter_map(move |(name, ids)| {
        ids.iter()
            .all(|id| used.binary_search(id).is_err())
            .then_some(name)
    })
}

//! Checks dependency declarations against imports in their project or standalone script.

use std::collections::BTreeSet;

use char_str::CharStr;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::SystemPath;
use ruff_text_size::Ranged;
use rustc_hash::FxHashSet;
use ty_module_resolver::{ImportingFile, resolve_real_shadowable_module};
use ty_python_semantic::dependency::{
    DependencyMetadata, DependencyProject, DependencyProjectKind, UNUSED_DEPENDENCY,
    imported_modules,
};
use ty_python_semantic::lint::LintId;

use crate::script::{Script, script_tag};
use crate::{Db, Project};

#[cfg(test)]
mod tests;

/// Checks declarations in workspace members selected for checking, counting imports in their
/// indexed files, including unopened files.
///
/// These diagnostics belong to `pyproject.toml`. Both the command line and language server report
/// them separately from diagnostics on individual Python files.
pub fn project_dependency_diagnostics(db: &dyn Db) -> &[Diagnostic] {
    let project = db.project();
    if project.metadata(db).uv_workspace().metadata.is_none() {
        return &[];
    }
    check_project_dependencies(db, project)
}

#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn check_project_dependencies(db: &dyn Db, project: Project) -> Box<[Diagnostic]> {
    let Ok(Some(metadata)) = project.dependency_metadata(db) else {
        return Box::default();
    };
    let mut indexed = None;
    let mut diagnostics = Vec::new();
    for member in &metadata.projects {
        if member.kind != DependencyProjectKind::Project || member.dependencies.is_empty() {
            continue;
        }
        if !project
            .included_paths_or_root(db)
            .iter()
            .any(|path| member.path.starts_with(path))
        {
            // A check of `src/` or a single file cannot establish that a dependency is unused by
            // the containing project. Selecting a complete workspace member is sufficient.
            continue;
        }
        let Ok(manifest) = system_path_to_file(db, member.path.join("pyproject.toml")) else {
            continue;
        };
        let Some(severity) = db
            .rule_selection(manifest)
            .severity(LintId::of(&UNUSED_DEPENDENCY))
        else {
            continue;
        };
        // Build the lazy index here so each member's usage query depends on the resulting
        // file set, without also depending on every setting read by the directory walker.
        let indexed = indexed.get_or_insert_with(|| project.files(db));
        if !indexed.diagnostics().is_empty() {
            // A failed directory walk can hide an import that uses any declared dependency.
            return Box::default();
        }
        let Some(used) = member_dependency_usage(db, manifest) else {
            continue;
        };
        diagnostics.extend(unused_declarations(
            metadata,
            member,
            used,
            Span::from(manifest),
            severity,
        ));
    }
    diagnostics.into_boxed_slice()
}

/// Returns the IDs of used distributions in sorted order, or `None` if usage could not be determined.
#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
fn member_dependency_usage(db: &dyn Db, manifest: File) -> Option<Box<[CharStr]>> {
    let project = db.project();
    let metadata = project.dependency_metadata(db).as_ref().ok()?.as_deref()?;
    let root = manifest.path(db).as_system_path()?.parent()?;
    let member = metadata.projects.iter().find(|member| {
        member.kind == DependencyProjectKind::Project && member.path.as_path() == root
    })?;
    let indexed = project.files(db);
    let mut used = BTreeSet::new();
    for file in indexed.iter().filter(|file| {
        file.path(db).as_system_path().is_some_and(|path| {
            containing_project(metadata, path).is_some_and(|owner| owner == member)
        }) && script_tag(db, *file).is_none()
    }) {
        let file = project.program(db).program_file(db, file);
        let imports = imported_modules(db, file);
        if imports.incomplete {
            return None;
        }
        metadata.record_used_dependencies(db, file, imports, &mut used);
    }
    Some(used.into_iter().collect())
}

/// Checks script dependencies before suppression comments are evaluated.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn script_dependency_diagnostics(db: &dyn Db, script: Script<'_>) -> Box<[Diagnostic]> {
    let file = script.file(db);
    if !script.has_valid_settings(db) {
        return Box::default();
    }
    let Some(severity) = db
        .rule_selection(file)
        .severity(LintId::of(&UNUSED_DEPENDENCY))
    else {
        return Box::default();
    };
    let Ok(Some(metadata)) = script.dependency_metadata(db) else {
        return Box::default();
    };
    let Some(tag) = script_tag(db, file) else {
        return Box::default();
    };
    let Some(project) = metadata.projects.iter().find(|project| {
        project.kind == DependencyProjectKind::Script
            && file.path(db).as_system_path() == Some(project.path.as_path())
    }) else {
        return Box::default();
    };
    let Some(used) = script_dependency_usage(db, script) else {
        return Box::default();
    };

    unused_declarations(
        metadata,
        project,
        used,
        Span::from(file).with_range(tag.range()),
        severity,
    )
    .collect::<Vec<_>>()
    .into_boxed_slice()
}

/// Counts imports in the script and its local modules using the script's environment.
/// A helper's own script metadata does not apply when it is imported.
#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
fn script_dependency_usage(db: &dyn Db, script: Script<'_>) -> Option<Box<[CharStr]>> {
    let metadata = script.dependency_metadata(db).as_ref().ok()?.as_deref()?;
    let mut pending = vec![script.program(db).program_file(db, script.file(db))];
    let mut visited = FxHashSet::default();
    let mut used = BTreeSet::new();

    while let Some(file) = pending.pop() {
        if !visited.insert(file.file(db)) {
            continue;
        }
        let imports = imported_modules(db, file);
        if imports.incomplete {
            return None;
        }
        metadata.record_used_dependencies(db, file, imports, &mut used);

        let local_modules = imports.modules.iter().flat_map(|module| {
            [
                Some(*module),
                resolve_real_shadowable_module(
                    db,
                    ImportingFile::File(file.file(db), file.resolver_environment(db)),
                    module.name(db),
                ),
            ]
            .into_iter()
            .flatten()
        });
        for module in local_modules {
            if !module.search_path(db).is_some_and(|path| {
                !path.is_standard_library() && !path.is_site_packages() && !path.is_editable()
            }) {
                continue;
            }
            let Some(imported_file) = module.file(db) else {
                continue;
            };
            let Some(path) = imported_file.path(db).as_system_path() else {
                continue;
            };
            if metadata.distributions.values().any(|distribution| {
                distribution
                    .editable_path
                    .as_ref()
                    .is_some_and(|root| path.starts_with(root))
            }) {
                // Workspace members and other editable dependencies supply their own imports.
                // Looking inside them would make their transitive dependencies appear used here.
                continue;
            }
            pending.push(file.program(db).program_file(db, imported_file));
        }
    }

    Some(used.into_iter().collect())
}

fn containing_project<'a>(
    metadata: &'a DependencyMetadata,
    path: &SystemPath,
) -> Option<&'a DependencyProject> {
    metadata
        .projects
        .iter()
        .filter(|project| {
            project.kind == DependencyProjectKind::Project && path.starts_with(&project.path)
        })
        .max_by_key(|project| project.path.as_str().len())
}

fn unused_declarations<'a>(
    metadata: &'a DependencyMetadata,
    project: &'a DependencyProject,
    used: &'a [CharStr],
    span: Span,
    severity: Severity,
) -> impl Iterator<Item = Diagnostic> + 'a {
    // uv can resolve multiple versions of a package. Count an import of any version as use,
    // and report the package only once.
    let names: BTreeSet<_> = project
        .dependencies
        .iter()
        .filter_map(|id| {
            metadata
                .distributions
                .get(id)
                .map(|distribution| &distribution.name)
        })
        .collect();
    names.into_iter().filter_map(move |name| {
        let mut has_known_modules = false;
        for id in &project.dependencies {
            let Some(distribution) = metadata.distributions.get(id) else {
                continue;
            };
            if &distribution.name != name {
                continue;
            }
            if used.binary_search(id).is_ok() {
                return None;
            }
            has_known_modules |= metadata
                .module_owners
                .values()
                .any(|owners| owners.contains(id));
        }
        if !has_known_modules {
            // Packages absent from the environment, or exposing only commands or resources,
            // cannot be checked from Python imports.
            return None;
        }
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(UNUSED_DEPENDENCY.name()),
            severity,
            format_args!("Dependency `{name}` is declared but never imported"),
        );
        diagnostic.annotate(Annotation::primary(span.clone()));
        diagnostic.set_documentation_url(Some(format!(
            "https://ty.dev/rules#{}",
            UNUSED_DEPENDENCY.name()
        )));
        Some(diagnostic)
    })
}

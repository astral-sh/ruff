//! Checks dependency declarations against imports in their project or standalone script.

use std::collections::BTreeSet;

use compact_str::CompactString;
use pep508_rs::PackageName;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::SystemPath;
use rustc_hash::FxHashSet;
use ty_module_resolver::{ImportingFile, resolve_real_module};
use ty_python_core::ProgramFile;
use ty_python_semantic::dependency::{
    DependencyMetadata, DependencyProject, DependencyProjectKind, UNUSED_DEPENDENCY,
    imported_modules,
};
use ty_python_semantic::lint::LintId;

use crate::script::{Script, script_tag};
use crate::{Db, Project};

mod declarations;

#[cfg(test)]
mod tests;

use declarations::{DependencyDeclaration, declarations};

/// Checks declarations in fully included workspace members, including imports in unopened files.
///
/// These diagnostics belong to `pyproject.toml`, so document diagnostics for individual Python
/// files cannot deliver them. Both the command line and language server report them separately.
pub fn project_dependency_diagnostics(db: &dyn Db) -> &[Diagnostic] {
    let project = db.project();
    if project.metadata(db).uv_workspace().is_none() {
        return &[];
    }
    check_project_dependencies(db, project)
}

#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn check_project_dependencies(db: &dyn Db, project: Project) -> Box<[Diagnostic]> {
    let Ok(Some(metadata)) = project.dependency_metadata(db) else {
        return Box::default();
    };
    project_diagnostics(db, metadata)
}

fn project_diagnostics(db: &dyn Db, metadata: &DependencyMetadata) -> Box<[Diagnostic]> {
    let project = db.project();
    if project
        .metadata(db)
        .to_merged_options()
        .options()
        .src
        .as_ref()
        .is_some_and(|src| {
            src.include.is_some()
                || src
                    .exclude
                    .as_ref()
                    .is_some_and(|patterns| !patterns.is_empty())
        })
    {
        // Source filters describe the files to type-check, not every file that can use a
        // project's dependencies. Imports outside that selection can keep a dependency in use.
        return Box::default();
    }
    let mut indexed = None;
    let mut diagnostics = Vec::new();
    for member in &metadata.projects {
        if member.kind != DependencyProjectKind::Project
            || !project
                .included_paths_or_root(db)
                .iter()
                .any(|path| member.path.starts_with(path))
        {
            // A check of `src/` or a single file cannot establish that a dependency is unused by
            // the containing project. Selecting a complete workspace member is sufficient.
            continue;
        }

        let Ok(file) = system_path_to_file(db, member.path.join("pyproject.toml")) else {
            continue;
        };
        let Some(severity) = db
            .rule_selection(file)
            .severity(LintId::of(&UNUSED_DEPENDENCY))
        else {
            continue;
        };
        let Some(declarations) = declarations(db, file, DependencyProjectKind::Project) else {
            continue;
        };
        if declarations.is_empty() {
            continue;
        }

        let indexed = indexed.get_or_insert_with(|| project.files(db));
        if !indexed.diagnostics().is_empty() {
            // A failed directory walk can hide an import that uses any declared dependency.
            return Box::default();
        }

        let files = indexed
            .iter()
            .chain(project.open_files(db).iter().copied())
            .filter(|file| {
                file.path(db).as_system_path().is_some_and(|path| {
                    containing_project(metadata, path).is_some_and(|owner| owner == member)
                }) && script_tag(db, *file).is_none()
            })
            .map(|file| project.program(db).program_file(db, file));
        let Some(used) = used_dependencies(db, metadata, files, Some(member)) else {
            continue;
        };
        diagnostics.extend(unused_declarations(
            metadata,
            member,
            declarations,
            &used,
            file,
            severity,
        ));
    }

    diagnostics.into_boxed_slice()
}

/// Checks one script before semantic diagnostics apply the script's suppression comments.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn script_dependency_diagnostics(db: &dyn Db, file: File) -> Box<[Diagnostic]> {
    let Some(script) = Script::for_file(db, file) else {
        return Box::default();
    };
    if !script.has_valid_settings(db)
        || !db
            .rule_selection(file)
            .is_enabled(LintId::of(&UNUSED_DEPENDENCY))
    {
        return Box::default();
    }
    let Ok(Some(metadata)) = script.dependency_metadata(db) else {
        return Box::default();
    };
    script_diagnostics(db, script, metadata)
}

fn script_diagnostics(
    db: &dyn Db,
    script: Script<'_>,
    metadata: &DependencyMetadata,
) -> Box<[Diagnostic]> {
    let file = script.file(db);
    let Some(severity) = db
        .rule_selection(file)
        .severity(LintId::of(&UNUSED_DEPENDENCY))
    else {
        return Box::default();
    };
    let Some(declarations) = declarations(db, file, DependencyProjectKind::Script) else {
        return Box::default();
    };
    let Some(project) = metadata.projects.iter().find(|project| {
        project.kind == DependencyProjectKind::Script
            && file.path(db).as_system_path() == Some(project.path.as_path())
    }) else {
        return Box::default();
    };
    let Some(used) = used_dependencies(
        db,
        metadata,
        [script.program(db).program_file(db, file)],
        None,
    ) else {
        return Box::default();
    };

    unused_declarations(metadata, project, declarations, &used, file, severity)
        .collect::<Vec<_>>()
        .into_boxed_slice()
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

/// Includes local helpers in the caller's environment. Their own script metadata, if present,
/// does not describe the environment in which an importing script executes them.
fn used_dependencies<'db>(
    db: &'db dyn Db,
    metadata: &DependencyMetadata,
    files: impl IntoIterator<Item = ProgramFile<'db>>,
    member: Option<&DependencyProject>,
) -> Option<BTreeSet<CompactString>> {
    let mut pending: Vec<_> = files.into_iter().collect();
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
                resolve_real_module(
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
            if let Some(member) = member
                && containing_project(metadata, path).is_some_and(|owner| owner != member)
            {
                continue;
            }
            if metadata.distributions.iter().any(|(id, distribution)| {
                member.and_then(|member| member.distribution.as_ref()) != Some(id)
                    && distribution
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

    Some(used)
}

fn unused_declarations<'a>(
    metadata: &'a DependencyMetadata,
    project: &'a DependencyProject,
    declarations: &'a [DependencyDeclaration],
    used: &'a BTreeSet<CompactString>,
    file: File,
    severity: Severity,
) -> impl Iterator<Item = Diagnostic> + 'a {
    declarations.iter().filter_map(move |declaration| {
        let mut has_known_modules = false;
        for id in &project.dependencies {
            let Some(distribution) = metadata.distributions.get(id) else {
                continue;
            };
            if !distribution
                .name
                .parse::<PackageName>()
                .is_ok_and(|name| name.as_ref() == declaration.name.as_str())
            {
                continue;
            }
            if used.contains(id) {
                return None;
            }
            has_known_modules |= metadata
                .module_owners
                .values()
                .any(|owners| owners.contains(id));
        }
        if !has_known_modules {
            // Import analysis says nothing about packages that only provide commands, plugins,
            // or resources, or about dependencies absent from the synchronized environment.
            return None;
        }

        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(UNUSED_DEPENDENCY.name()),
            severity,
            format_args!(
                "Dependency `{}` is declared but never imported",
                declaration.name
            ),
        );
        diagnostic.annotate(Annotation::primary(
            Span::from(file).with_range(declaration.range),
        ));
        diagnostic.set_documentation_url(Some(format!(
            "https://ty.dev/rules#{}",
            UNUSED_DEPENDENCY.name()
        )));
        Some(diagnostic)
    })
}

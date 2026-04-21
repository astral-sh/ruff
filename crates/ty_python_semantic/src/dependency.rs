use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use std::fmt;
use ty_module_resolver::{
    Module, ModuleName, ModuleNameResolutionError, SearchPath, resolve_module,
};
use ty_python_core::scope::{FileScopeId, NodeWithScopeRef};
use ty_python_core::{SemanticIndex, semantic_index};

use crate::lint::{Level, LintId, LintSource, LintStatus};
use crate::reachability::is_range_reachable;
use crate::suppression::suppressions;
use crate::types::TypeCheckDiagnostics;
use crate::{Db, declare_lint};

declare_lint! {
    /// ## What it does
    /// Checks for third-party imports that are used without a matching direct dependency
    /// declaration.
    ///
    /// ## Why is this bad?
    /// Importing a package that is only available transitively can make the project break when
    /// dependency resolution changes.
    ///
    /// ## Examples
    /// ```python
    /// import requests  # requests is not declared as a direct dependency
    /// ```
    pub(crate) static MISSING_DIRECT_DEPENDENCY = {
        summary: "detects third-party imports without direct dependency declarations",
        status: LintStatus::preview("0.0.0"),
        default_level: Level::Warn,
    }
}

/// Dependency metadata supplied by the package manager.
///
/// This is intentionally smaller than the full `uv workspace metadata` schema. `uv` can keep
/// exporting its normalized graph, while ty normalizes only the fields it needs for import-driven
/// dependency linting.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyMetadata {
    projects: Vec<DependencyProject>,
    module_owners: Vec<ModuleOwner>,
}

impl DependencyMetadata {
    pub fn new(projects: Vec<DependencyProject>, module_owners: Vec<ModuleOwner>) -> Self {
        Self {
            projects,
            module_owners,
        }
    }

    fn project_for_file(&self, db: &dyn Db, file: File) -> Option<&DependencyProject> {
        let path = file.path(db).as_system_path()?;

        self.projects
            .iter()
            .filter(|project| path.starts_with(project.path()))
            .max_by_key(|project| project.path().as_str().len())
    }

    fn ownership_for_module(&self, module: &ModuleName) -> ModuleOwnership<'_> {
        let mut best_component_count = 0;
        let mut best_owners: Option<&[DistributionName]> = None;

        for owner in &self.module_owners {
            if !module.starts_with(owner.module()) {
                continue;
            }

            let component_count = owner.module().components().count();
            if component_count > best_component_count {
                best_component_count = component_count;
                best_owners = Some(owner.owners());
            }
        }

        match best_owners {
            None | Some([]) => ModuleOwnership::Unknown,
            Some([owner]) => ModuleOwnership::Unique(owner),
            Some(_) => ModuleOwnership::Ambiguous,
        }
    }
}

/// A workspace member or project whose imports are checked against direct dependencies.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyProject {
    name: DistributionName,
    path: SystemPathBuf,
    direct_dependencies: FxHashSet<DistributionName>,
}

impl DependencyProject {
    pub fn new(
        name: impl Into<String>,
        path: SystemPathBuf,
        direct_dependencies: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: DistributionName::new(name),
            path,
            direct_dependencies: direct_dependencies
                .into_iter()
                .map(DistributionName::new)
                .collect(),
        }
    }

    fn path(&self) -> &SystemPath {
        &self.path
    }

    fn declares_dependency(&self, dependency: &DistributionName) -> bool {
        self.name == *dependency || self.direct_dependencies.contains(dependency)
    }
}

/// A mapping from an importable module prefix to the distributions that provide it.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct ModuleOwner {
    module: ModuleName,
    owners: Vec<DistributionName>,
}

impl ModuleOwner {
    pub fn new(module: ModuleName, owners: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut unique_owners = Vec::new();
        for owner in owners {
            let owner = DistributionName::new(owner);
            if !unique_owners.contains(&owner) {
                unique_owners.push(owner);
            }
        }

        Self {
            module,
            owners: unique_owners,
        }
    }

    pub fn from_module(
        module: &str,
        owners: impl IntoIterator<Item = impl Into<String>>,
    ) -> Option<Self> {
        Some(Self::new(ModuleName::new(module)?, owners))
    }

    fn module(&self) -> &ModuleName {
        &self.module
    }

    fn owners(&self) -> &[DistributionName] {
        &self.owners
    }
}

#[derive(Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct DistributionName {
    normalized: String,
    display: String,
}

impl DistributionName {
    pub fn new(name: impl Into<String>) -> Self {
        let display = name.into();
        Self {
            normalized: normalize_distribution_name(&display),
            display,
        }
    }
}

impl fmt::Debug for DistributionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.display.fmt(f)
    }
}

impl fmt::Display for DistributionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.display.fmt(f)
    }
}

fn normalize_distribution_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut previous_was_separator = false;

    for char in name.chars() {
        if matches!(char, '-' | '_' | '.') {
            if !previous_was_separator {
                normalized.push('-');
                previous_was_separator = true;
            }
        } else {
            normalized.push(char.to_ascii_lowercase());
            previous_was_separator = false;
        }
    }

    normalized
}

enum ModuleOwnership<'a> {
    Unique(&'a DistributionName),
    Ambiguous,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct ImportFact {
    range: TextRange,
    highlight_range: TextRange,
    requested_module: Option<ModuleName>,
    imported_module: Option<ModuleName>,
    resolution: ImportResolution,
    context: ImportContext,
    kind: ImportFactKind,
    scope: FileScopeId,
}

impl ImportFact {
    fn resolved_third_party_module(&self) -> Option<&ModuleName> {
        match &self.resolution {
            ImportResolution::Resolved {
                source: ImportSource::ThirdParty,
            } => self.imported_module.as_ref(),
            ImportResolution::Resolved { .. }
            | ImportResolution::Unresolved
            | ImportResolution::InvalidSyntax => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum ImportFactKind {
    Import,
    ImportFrom { is_star: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum ImportContext {
    Runtime,
    TypeChecking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum ImportResolution {
    Resolved { source: ImportSource },
    Unresolved,
    InvalidSyntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum ImportSource {
    StandardLibrary,
    FirstParty,
    ThirdParty,
    Other,
}

#[salsa::tracked(returns(ref), no_eq, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn import_facts(db: &dyn Db, file: File) -> Vec<ImportFact> {
    let module = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);
    let mut collector = ImportFactsCollector::new(db, file, index);

    collector.visit_body(module.suite());
    collector.finish()
}

pub(crate) fn register_lints(registry: &mut crate::lint::LintRegistryBuilder) {
    registry.register_lint(&MISSING_DIRECT_DEPENDENCY);
}

pub(crate) fn check_dependency_lints(db: &dyn Db, file: File) -> TypeCheckDiagnostics {
    let mut diagnostics = TypeCheckDiagnostics::default();
    let Some(metadata) = db.analysis_settings(file).dependency_metadata.as_deref() else {
        return diagnostics;
    };

    let Some(project) = metadata.project_for_file(db, file) else {
        return diagnostics;
    };

    let lint_id = LintId::of(&MISSING_DIRECT_DEPENDENCY);
    let Some((severity, source)) = db.rule_selection(file).get(lint_id) else {
        return diagnostics;
    };

    let suppressions = suppressions(db, file);
    let index = semantic_index(db, file);
    let mut reported = FxHashSet::default();

    for fact in import_facts(db, file).iter() {
        if fact.context == ImportContext::TypeChecking {
            continue;
        }

        let Some(imported_module) = fact.resolved_third_party_module() else {
            continue;
        };

        if !is_range_reachable(db, index, fact.scope, fact.range) {
            continue;
        }

        let ModuleOwnership::Unique(owner) = metadata.ownership_for_module(imported_module) else {
            continue;
        };

        if project.declares_dependency(owner) {
            continue;
        }

        if let Some(suppression) = suppressions.find_suppression(fact.range, lint_id) {
            diagnostics.mark_used(suppression.id());
            continue;
        }

        if !reported.insert(owner.clone()) {
            continue;
        }

        diagnostics.push(missing_direct_dependency_diagnostic(
            file,
            fact,
            owner,
            severity,
            source,
            db.verbose(),
        ));
    }

    diagnostics
}

fn missing_direct_dependency_diagnostic(
    file: File,
    fact: &ImportFact,
    owner: &DistributionName,
    severity: Severity,
    source: LintSource,
    verbose: bool,
) -> Diagnostic {
    let module = fact
        .imported_module
        .as_ref()
        .or(fact.requested_module.as_ref())
        .expect("missing-direct-dependency diagnostics require an imported module");

    let mut diagnostic = Diagnostic::new(
        DiagnosticId::Lint(MISSING_DIRECT_DEPENDENCY.name()),
        severity,
        format_args!(
            "Third-party import `{module}` is used but no direct dependency on `{owner}` is declared"
        ),
    );

    diagnostic.set_documentation_url(Some(MISSING_DIRECT_DEPENDENCY.documentation_url()));
    diagnostic.annotate(Annotation::primary(
        Span::from(file).with_range(fact.highlight_range),
    ));

    if matches!(fact.kind, ImportFactKind::ImportFrom { is_star: true }) {
        diagnostic.info("The import is a star import; dependency ownership was inferred from the imported module.");
    }

    if verbose {
        diagnostic.info(match source {
            LintSource::Default => "rule `missing-direct-dependency` is enabled by default",
            LintSource::Cli => "rule `missing-direct-dependency` was selected on the command line",
            LintSource::File => {
                "rule `missing-direct-dependency` was selected in the configuration file"
            }
            LintSource::Editor => {
                "rule `missing-direct-dependency` was selected in the editor settings"
            }
        });
    }

    diagnostic
}

struct ImportFactsCollector<'db> {
    db: &'db dyn Db,
    file: File,
    index: &'db SemanticIndex<'db>,
    scope: FileScopeId,
    facts: Vec<ImportFact>,
}

impl<'db> ImportFactsCollector<'db> {
    fn new(db: &'db dyn Db, file: File, index: &'db SemanticIndex<'db>) -> Self {
        Self {
            db,
            file,
            index,
            scope: FileScopeId::global(),
            facts: Vec::new(),
        }
    }

    fn finish(self) -> Vec<ImportFact> {
        self.facts
    }

    fn with_scope(&mut self, scope: FileScopeId, f: impl FnOnce(&mut Self)) {
        let previous = self.scope;
        self.scope = scope;
        f(self);
        self.scope = previous;
    }

    fn import_context(&self, range: TextRange) -> ImportContext {
        if self.index.is_in_type_checking_block(self.scope, range) {
            ImportContext::TypeChecking
        } else {
            ImportContext::Runtime
        }
    }

    fn push_import_fact(&mut self, alias: &ast::Alias) {
        let imported_module = ModuleName::new(&alias.name);
        let resolution = imported_module
            .as_ref()
            .map(|module_name| self.resolve_import(module_name))
            .unwrap_or(ImportResolution::InvalidSyntax);

        self.facts.push(ImportFact {
            range: alias.range(),
            highlight_range: alias.range(),
            requested_module: imported_module.clone(),
            imported_module,
            resolution,
            context: self.import_context(alias.range()),
            kind: ImportFactKind::Import,
            scope: self.scope,
        });
    }

    fn push_import_from_facts(&mut self, import_from: &ast::StmtImportFrom) {
        let requested_module =
            match ModuleName::from_import_statement(self.db, self.file, import_from) {
                Ok(module_name) => Some(module_name),
                Err(ModuleNameResolutionError::InvalidSyntax) => None,
                Err(
                    ModuleNameResolutionError::TooManyDots
                    | ModuleNameResolutionError::UnknownCurrentModule,
                ) => {
                    for alias in &import_from.names {
                        self.facts.push(ImportFact {
                            range: alias.range(),
                            highlight_range: import_from_highlight_range(import_from, alias),
                            requested_module: None,
                            imported_module: None,
                            resolution: ImportResolution::Unresolved,
                            context: self.import_context(alias.range()),
                            kind: ImportFactKind::ImportFrom {
                                is_star: alias.name.as_str() == "*",
                            },
                            scope: self.scope,
                        });
                    }
                    return;
                }
            };

        let Some(requested_module) = requested_module else {
            for alias in &import_from.names {
                self.facts.push(ImportFact {
                    range: alias.range(),
                    highlight_range: import_from_highlight_range(import_from, alias),
                    requested_module: None,
                    imported_module: None,
                    resolution: ImportResolution::InvalidSyntax,
                    context: self.import_context(alias.range()),
                    kind: ImportFactKind::ImportFrom {
                        is_star: alias.name.as_str() == "*",
                    },
                    scope: self.scope,
                });
            }
            return;
        };

        let base_resolution = self.resolve_import(&requested_module);

        for alias in &import_from.names {
            let is_star = alias.name.as_str() == "*";
            let imported_module = if is_star {
                requested_module.clone()
            } else {
                self.resolve_imported_member(&requested_module, &alias.name)
            };

            let resolution = if imported_module == requested_module {
                base_resolution
            } else {
                self.resolve_import(&imported_module)
            };

            self.facts.push(ImportFact {
                range: alias.range(),
                highlight_range: import_from_highlight_range(import_from, alias),
                requested_module: Some(requested_module.clone()),
                imported_module: Some(imported_module),
                resolution,
                context: self.import_context(alias.range()),
                kind: ImportFactKind::ImportFrom { is_star },
                scope: self.scope,
            });
        }
    }

    fn resolve_imported_member(&self, module_name: &ModuleName, member: &str) -> ModuleName {
        let Some(member_name) = ModuleName::new(member) else {
            return module_name.clone();
        };

        let mut submodule_name = module_name.clone();
        submodule_name.extend(&member_name);

        if resolve_module(self.db, self.file, &submodule_name).is_some() {
            submodule_name
        } else {
            module_name.clone()
        }
    }

    fn resolve_import(&self, module_name: &ModuleName) -> ImportResolution {
        resolve_module(self.db, self.file, module_name)
            .map(|module| ImportResolution::Resolved {
                source: import_source(self.db, module),
            })
            .unwrap_or(ImportResolution::Unresolved)
    }
}

impl<'db, 'ast> StatementVisitor<'ast> for ImportFactsCollector<'db> {
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match stmt {
            ast::Stmt::Import(import) => {
                for alias in &import.names {
                    self.push_import_fact(alias);
                }
            }
            ast::Stmt::ImportFrom(import_from) => {
                self.push_import_from_facts(import_from);
            }
            ast::Stmt::FunctionDef(function) => {
                let scope = self.index.node_scope(NodeWithScopeRef::Function(function));
                self.with_scope(scope, |collector| collector.visit_body(&function.body));
            }
            ast::Stmt::ClassDef(class) => {
                let scope = self.index.node_scope(NodeWithScopeRef::Class(class));
                self.with_scope(scope, |collector| collector.visit_body(&class.body));
            }
            _ => walk_stmt(self, stmt),
        }
    }
}

fn import_from_highlight_range(import_from: &ast::StmtImportFrom, alias: &ast::Alias) -> TextRange {
    import_from
        .module
        .as_ref()
        .map_or(alias.range(), |module| module.range)
}

fn import_source(db: &dyn Db, module: Module) -> ImportSource {
    let Some(search_path) = module.search_path(db) else {
        return ImportSource::Other;
    };

    search_path_source(search_path)
}

fn search_path_source(search_path: &SearchPath) -> ImportSource {
    if search_path.is_standard_library() {
        ImportSource::StandardLibrary
    } else if search_path.is_first_party() {
        ImportSource::FirstParty
    } else if search_path.is_site_packages() {
        ImportSource::ThirdParty
    } else {
        ImportSource::Other
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ruff_db::Db as _;
    use ruff_db::diagnostic::Diagnostic;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem, SystemPathBuf};
    use ty_module_resolver::SearchPathSettings;
    use ty_python_core::platform::PythonPlatform;
    use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    use super::*;
    use crate::AnalysisSettings;
    use crate::db::tests::TestDb;
    use crate::types::check_types;

    fn setup_db(
        source: &str,
        direct_dependencies: &[&str],
        module_owners: Vec<ModuleOwner>,
    ) -> anyhow::Result<(TestDb, File)> {
        let mut db = TestDb::new();

        let src_root = SystemPathBuf::from("/src");
        let site_packages = SystemPathBuf::from("/site-packages");

        db.memory_file_system().create_directory_all(&src_root)?;
        db.memory_file_system()
            .create_directory_all(&site_packages)?;
        db.write_file("/src/app.py", source)?;

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: ruff_python_ast::PythonVersion::default(),
                    source: PythonVersionSource::default(),
                },
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings {
                    src_roots: vec![src_root.clone()],
                    site_packages_paths: vec![site_packages],
                    ..SearchPathSettings::empty()
                }
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)?,
            },
        );

        db.set_analysis_settings(AnalysisSettings {
            dependency_metadata: Some(Arc::new(DependencyMetadata::new(
                vec![DependencyProject::new(
                    "app",
                    src_root,
                    direct_dependencies.iter().copied(),
                )],
                module_owners,
            ))),
            ..AnalysisSettings::default()
        });

        let file = system_path_to_file(&db, "/src/app.py")?;
        Ok((db, file))
    }

    fn messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics
            .iter()
            .map(Diagnostic::primary_message)
            .collect()
    }

    fn primary_highlight<'a>(source: &'a str, diagnostic: &Diagnostic) -> &'a str {
        source[diagnostic.expect_primary_span().range().unwrap()].as_ref()
    }

    #[test]
    fn missing_direct_dependency_for_third_party_import() -> anyhow::Result<()> {
        let (mut db, file) = setup_db(
            "import requests\n",
            &[],
            vec![ModuleOwner::new(
                ModuleName::new_static("requests").unwrap(),
                ["requests"],
            )],
        )?;
        db.write_file("/site-packages/requests/__init__.py", "")?;

        let diagnostics = check_types(&db, file);

        assert_eq!(
            messages(&diagnostics),
            [
                "Third-party import `requests` is used but no direct dependency on `requests` is declared"
            ]
        );

        Ok(())
    }

    #[test]
    fn declared_direct_dependency_is_not_reported() -> anyhow::Result<()> {
        let (mut db, file) = setup_db(
            "import requests\n",
            &["requests"],
            vec![ModuleOwner::new(
                ModuleName::new_static("requests").unwrap(),
                ["requests"],
            )],
        )?;
        db.write_file("/site-packages/requests/__init__.py", "")?;

        let diagnostics = check_types(&db, file);

        assert_eq!(messages(&diagnostics), Vec::<&str>::new());

        Ok(())
    }

    #[test]
    fn type_checking_import_is_not_reported_as_runtime_dependency() -> anyhow::Result<()> {
        let (mut db, file) = setup_db(
            r#"
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import rich
"#,
            &[],
            vec![ModuleOwner::new(
                ModuleName::new_static("rich").unwrap(),
                ["rich"],
            )],
        )?;
        db.write_file("/site-packages/rich/__init__.py", "")?;

        let diagnostics = check_types(&db, file);

        assert_eq!(messages(&diagnostics), Vec::<&str>::new());

        Ok(())
    }

    #[test]
    fn from_import_uses_resolved_submodule_owner() -> anyhow::Result<()> {
        let source = "from google.cloud import storage\n";
        let (mut db, file) = setup_db(
            source,
            &[],
            vec![
                ModuleOwner::new(
                    ModuleName::new_static("google.cloud").unwrap(),
                    ["google-cloud-core"],
                ),
                ModuleOwner::new(
                    ModuleName::new_static("google.cloud.storage").unwrap(),
                    ["google-cloud-storage"],
                ),
            ],
        )?;
        db.write_file("/site-packages/google/__init__.py", "")?;
        db.write_file("/site-packages/google/cloud/__init__.py", "")?;
        db.write_file("/site-packages/google/cloud/storage/__init__.py", "")?;

        let diagnostics = check_types(&db, file);

        assert_eq!(
            messages(&diagnostics),
            [
                "Third-party import `google.cloud.storage` is used but no direct dependency on `google-cloud-storage` is declared"
            ]
        );
        assert_eq!(primary_highlight(source, &diagnostics[0]), "google.cloud");

        Ok(())
    }
}

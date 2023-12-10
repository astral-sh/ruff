use anyhow::Result;
use log::{error, warn};
use rustc_hash::FxHashMap;

use ruff_linter::source_kind::SourceKind;
use ruff_linter::warn_user_once;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{populate_module_imports, ImportMap, ModuleImport};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::Stmt;
use ruff_python_ast::{SourceType, Suite};
use ruff_python_parser::parse_suite;
use ruff_workspace::resolver::{python_files_in_path, ResolvedFile};

use crate::args::{CliOverrides, ImportMapCommand};
use crate::resolve::resolve;
use crate::{resolve_default_files, ExitStatus};

/// Export imports for the given files.
pub(crate) fn import_map(cli: ImportMapCommand) -> Result<ExitStatus> {
    let overrides = CliOverrides::default(); // TODO
    let pyproject_config = resolve(cli.isolated, cli.config.as_deref(), &overrides, None)?;
    let files = resolve_default_files(cli.files, false);
    let (paths, resolver) = python_files_in_path(&files, &pyproject_config, &overrides)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(ExitStatus::Success);
    }

    // Discover the package root for each Python file.
    let package_roots = resolver.package_roots(
        &paths
            .iter()
            .flatten()
            .map(ResolvedFile::path)
            .collect::<Vec<_>>(),
        &pyproject_config,
    );

    let mut errors = 0;
    let mut import_map = ImportMap::new();
    let mut path_to_module_path = FxHashMap::default();

    // TODO: par_iter() where possible
    for ent in paths.iter() {
        match ent {
            Ok(resolved_file) => {
                let path = resolved_file.path();
                let SourceType::Python(source_type) = SourceType::from(&path) else {
                    continue;
                };
                let source_kind = match SourceKind::from_path(path, source_type) {
                    Ok(Some(source_kind)) => source_kind,
                    _ => panic!("Failed to read source file"),
                };

                let package = path
                    .parent()
                    .and_then(|parent| package_roots.get(parent).copied())
                    .flatten();

                let Some(package) = package else {
                    continue; // TODO: report?
                };
                let Some(module_path) = to_module_path(package, path) else {
                    continue; // TODO: report?
                };

                let dotted_module_path = module_path.join(".");
                path_to_module_path.insert(
                    dotted_module_path.clone(),
                    path.to_string_lossy().to_string(),
                );

                match parse_suite(source_kind.source_code(), &path.to_string_lossy()) {
                    Ok(python_ast) => {
                        let module_imports = extract_imports(&python_ast, &module_path);
                        import_map.insert(dotted_module_path, module_imports);
                    }
                    Err(parse_error) => {
                        errors += 1;
                        error!(
                            "Failed to parse {path}: {error}",
                            path = path.display(),
                            error = parse_error
                        );
                    }
                };
            }
            Err(error) => {
                error!("{error}");
                errors += 1;
            }
        }
    }
    // TODO: come up with an actual JSON format
    println!("{}", serde_json::to_string(&import_map)?);
    println!("{}", serde_json::to_string(&path_to_module_path)?);

    if errors > 0 {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Error)
    }
}

#[derive(Debug, Default)]
struct ImportStmtFinder<'a> {
    imports: Vec<&'a Stmt>,
}

impl<'a, 'b> StatementVisitor<'b> for ImportStmtFinder<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::Import(_) | Stmt::ImportFrom(_) => self.imports.push(stmt),
            _ => walk_stmt(self, stmt),
        }
    }
}

fn extract_imports(python_ast: &Suite, module_path: &Vec<String>) -> Vec<ModuleImport> {
    let mut visitor = ImportStmtFinder::default();
    visitor.visit_body(python_ast);
    let mut module_imports = Vec::with_capacity(visitor.imports.len());
    for stmt in visitor.imports {
        populate_module_imports(&mut module_imports, &module_path, stmt);
    }
    module_imports
}

//! Resolves an Import, `ImportFrom` or `StarImport` definition to one or more
//! "resolved definitions". This is done recursively to find the original
//! definition targeted by the import.

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use rustc_hash::FxHashSet;

use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::semantic_index::place::ScopeId;
use crate::semantic_index::{global_scope, place_table, use_def_map};
use crate::{Db, ModuleName, resolve_module};

/// Represents the result of resolving an import to either a specific definition or a module file.
/// This enum helps distinguish between cases where an import resolves to:
/// - A specific definition within a module (e.g., `from os import path` -> definition of `path`)
/// - An entire module file (e.g., `import os` -> the `os` module file itself)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedDefinition<'db> {
    /// The import resolved to a specific definition within a module
    Definition(Definition<'db>),
    /// The import resolved to an entire module file
    ModuleFile(File),
}

/// Resolve import definitions to their targets.
/// Returns resolved definitions which can be either specific definitions or module files.
/// For non-import definitions, returns the definition wrapped in `ResolvedDefinition::Definition`.
/// Always returns at least the original definition as a fallback if resolution fails.
pub(crate) fn resolve_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    symbol_name: Option<&str>,
) -> Vec<ResolvedDefinition<'db>> {
    let mut visited = FxHashSet::default();
    let resolved = resolve_definition_recursive(db, definition, &mut visited, symbol_name);

    // If resolution failed, return the original definition as fallback
    if resolved.is_empty() {
        vec![ResolvedDefinition::Definition(definition)]
    } else {
        resolved
    }
}

/// Helper function to resolve import definitions recursively.
fn resolve_definition_recursive<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    visited: &mut FxHashSet<Definition<'db>>,
    symbol_name: Option<&str>,
) -> Vec<ResolvedDefinition<'db>> {
    // Prevent infinite recursion if there are circular imports
    if visited.contains(&definition) {
        return Vec::new(); // Return empty list for circular imports
    }
    visited.insert(definition);

    let kind = definition.kind(db);

    match kind {
        DefinitionKind::Import(import_def) => {
            let file = definition.file(db);
            let module = parsed_module(db, file).load(db);
            let alias = import_def.alias(&module);

            // Get the full module name being imported
            let Some(module_name) = ModuleName::new(&alias.name) else {
                return Vec::new(); // Invalid module name, return empty list
            };

            // Resolve the module to its file
            let Some(resolved_module) = resolve_module(db, &module_name) else {
                return Vec::new(); // Module not found, return empty list
            };

            let Some(module_file) = resolved_module.file() else {
                return Vec::new(); // No file for module, return empty list
            };

            // For simple imports like "import os", we want to navigate to the module itself.
            // Return the module file directly instead of trying to find definitions within it.
            vec![ResolvedDefinition::ModuleFile(module_file)]
        }

        DefinitionKind::ImportFrom(import_from_def) => {
            let file = definition.file(db);
            let module = parsed_module(db, file).load(db);
            let import_node = import_from_def.import(&module);
            let alias = import_from_def.alias(&module);

            // For `ImportFrom`, we need to resolve the original imported symbol name
            // (alias.name), not the local alias (symbol_name)
            resolve_from_import_definitions(db, file, import_node, &alias.name, visited)
        }

        // For star imports, try to resolve to the specific symbol being accessed
        DefinitionKind::StarImport(star_import_def) => {
            let file = definition.file(db);
            let module = parsed_module(db, file).load(db);
            let import_node = star_import_def.import(&module);

            // If we have a symbol name, use the helper to resolve it in the target module
            if let Some(symbol_name) = symbol_name {
                resolve_from_import_definitions(db, file, import_node, symbol_name, visited)
            } else {
                // No symbol context provided, can't resolve star import
                Vec::new()
            }
        }

        // For non-import definitions, return the definition as is
        _ => vec![ResolvedDefinition::Definition(definition)],
    }
}

/// Helper function to resolve import definitions for `ImportFrom` and `StarImport` cases.
fn resolve_from_import_definitions<'db>(
    db: &'db dyn Db,
    file: File,
    import_node: &ast::StmtImportFrom,
    symbol_name: &str,
    visited: &mut FxHashSet<Definition<'db>>,
) -> Vec<ResolvedDefinition<'db>> {
    // Resolve the target module file
    let module_file = {
        // Resolve the module being imported from (handles both relative and absolute imports)
        let Some(module_name) = ModuleName::from_import_statement(db, file, import_node).ok()
        else {
            return Vec::new();
        };
        let Some(resolved_module) = resolve_module(db, &module_name) else {
            return Vec::new();
        };
        resolved_module.file()
    };

    let Some(module_file) = module_file else {
        return Vec::new(); // Module resolution failed
    };

    // Find the definition of this symbol in the imported module's global scope
    let global_scope = global_scope(db, module_file);
    let definitions_in_module = find_symbol_in_scope(db, global_scope, symbol_name);

    // Recursively resolve any import definitions found in the target module
    if definitions_in_module.is_empty() {
        // If we can't find the specific symbol, return empty list
        Vec::new()
    } else {
        let mut resolved_definitions = Vec::new();
        for def in definitions_in_module {
            let resolved = resolve_definition_recursive(db, def, visited, Some(symbol_name));
            resolved_definitions.extend(resolved);
        }
        resolved_definitions
    }
}

/// Find definitions for a symbol name in a specific scope.
pub(crate) fn find_symbol_in_scope<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol_name: &str,
) -> Vec<Definition<'db>> {
    let place_table = place_table(db, scope);
    let Some(place_id) = place_table.place_id_by_name(symbol_name) else {
        return Vec::new();
    };

    let use_def_map = use_def_map(db, scope);
    let mut definitions = Vec::new();

    // Get all definitions (both bindings and declarations) for this place
    let bindings = use_def_map.all_reachable_bindings(place_id);
    let declarations = use_def_map.all_reachable_declarations(place_id);

    for binding in bindings {
        if let Some(def) = binding.binding.definition() {
            definitions.push(def);
        }
    }

    for declaration in declarations {
        if let Some(def) = declaration.declaration.definition() {
            definitions.push(def);
        }
    }

    definitions
}

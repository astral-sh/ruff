use ruff_db::vfs::{vfs_path_to_file, VfsFile, VfsPath};

use crate::module::{Module, ModuleName};
use crate::Db;

/// Resolves a module name to a module.
#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_module(db: &dyn Db, module_name: ModuleName) -> Option<Module> {
    let interned_name = ModuleNameIngredient::new(db, module_name);

    resolve_module_query(db, interned_name)
}

/// Salsa query that resolves an interned [`ModuleNameIngredient`] to a module.
///
/// This query should not be called directly. Instead, use [`resolve_module`]. It only exists
/// because Salsa requires the module name to be an ingredient.
#[salsa::tracked]
pub(crate) fn resolve_module_query(
    db: &dyn Db,
    module_name: ModuleNameIngredient,
) -> Option<Module> {
    todo!()
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn path_to_module(db: &dyn Db, path: &VfsPath) -> Option<Module> {
    let file = vfs_path_to_file(db.upcast(), path)?;
    file_to_module(db, file)
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via `sys.path`.
#[salsa::tracked]
#[tracing::instrument(level = "debug", skip(db))]
pub fn file_to_module(db: &dyn Db, file: VfsFile) -> Option<Module> {
    todo!()
}

/// A thin wrapper around `ModuleName` to make it a Salsa ingredient.
///
/// This is needed because Salsa requires that all query arguments are salsa ingredients.
#[salsa::interned]
pub(crate) struct ModuleNameIngredient {
    name: ModuleName,
}

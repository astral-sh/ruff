use crate::module_name::ModuleName;
use crate::module_resolver::resolve_module;
use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::Db;

/// Salsa query to get the builtins scope.
///
/// Can return None if a custom typeshed is used that is missing `builtins.pyi`.
#[salsa::tracked]
pub(crate) fn builtins_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    let builtins_name =
        ModuleName::new_static("builtins").expect("Expected 'builtins' to be a valid module name");
    let builtins_file = resolve_module(db, builtins_name)?.file();
    Some(global_scope(db, builtins_file))
}

/// Salsa query to get the scope for the `types` module.
///
/// Can return None if a custom typeshed is used that is missing `types.pyi`.
#[salsa::tracked]
pub(crate) fn types_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    let types_module_name =
        ModuleName::new_static("types").expect("Expected 'types' to be a valid module name");
    let types_file = resolve_module(db, types_module_name)?.file();
    Some(global_scope(db, types_file))
}

/// Salsa query to get the scope for the `_typeshed` module.
///
/// Can return None if a custom typeshed is used that is missing a `_typeshed` directory.
#[salsa::tracked]
pub(crate) fn typeshed_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    let typeshed_module_name = ModuleName::new_static("_typeshed")
        .expect("Expected '_typeshed' to be a valid module name");
    let typeshed_file = resolve_module(db, typeshed_module_name)?.file();
    Some(global_scope(db, typeshed_file))
}

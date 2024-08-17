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

use red_knot_module_resolver::builtins_file;

use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::Db;

/// Salsa query to get the builtins scope.
///
/// Can return None if a custom typeshed is used that is missing `builtins.pyi`.
#[salsa::tracked]
pub(crate) fn builtins_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    Some(global_scope(db, builtins_file(db.upcast())?))
}

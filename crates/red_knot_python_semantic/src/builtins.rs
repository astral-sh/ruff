use red_knot_module_resolver::builtins_module;

use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::Db;

#[salsa::tracked]
pub(crate) fn builtins_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    Some(global_scope(db, builtins_module(db.upcast())?.file()))
}

use ruff_db::files::File;
use ruff_python_ast::{self as ast};

use crate::ast_node_ref::AstNodeRef;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};
use crate::Db;

#[salsa::tracked]
pub(crate) struct Unpack<'db> {
    #[id]
    pub(crate) file: File,

    #[id]
    pub(crate) file_scope: FileScopeId,

    #[no_eq]
    #[return_ref]
    pub(crate) target: AstNodeRef<ast::Expr>,

    #[no_eq]
    pub(crate) value: Expression<'db>,

    #[no_eq]
    count: countme::Count<Unpack<'static>>,
}

impl<'db> Unpack<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

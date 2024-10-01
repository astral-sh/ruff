use ruff_db::files::File;
use ruff_python_ast as ast;

use crate::ast_node_ref::AstNodeRef;
use crate::db::Db;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Constraint<'db> {
    Expression(Expression<'db>),
    Pattern(PatternConstraint<'db>),
}

#[salsa::tracked]
pub(crate) struct PatternConstraint<'db> {
    #[id]
    pub(crate) file: File,

    #[id]
    pub(crate) file_scope: FileScopeId,

    #[no_eq]
    #[return_ref]
    pub(crate) subject: AstNodeRef<ast::Expr>,

    #[no_eq]
    #[return_ref]
    pub(crate) pattern: AstNodeRef<ast::Pattern>,

    #[no_eq]
    count: countme::Count<PatternConstraint<'static>>,
}

impl<'db> PatternConstraint<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

use crate::ast_node_ref::AstNodeRef;
use crate::db::Db;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};
use ruff_db::files::File;
use ruff_python_ast as ast;
use salsa;

/// An independently type-inferable expression.
///
/// Includes constraint expressions (e.g. if tests) and the RHS of an unpacking assignment.
///
/// ## Module-local type
/// This type should not be used as part of any cross-module API because
/// it holds a reference to the AST node. Range-offset changes
/// then propagate through all usages, and deserialization requires
/// reparsing the entire module.
///
/// E.g. don't use this type in:
///
/// * a return type of a cross-module query
/// * a field of a type that is a return type of a cross-module query
/// * an argument of a cross-module query
#[salsa::tracked]
pub(crate) struct Expression<'db> {
    /// The file in which the expression occurs.
    #[id]
    pub(crate) file: File,

    /// The scope in which the expression occurs.
    #[id]
    pub(crate) file_scope: FileScopeId,

    /// The expression node.
    #[no_eq]
    #[return_ref]
    pub(crate) node_ref: AstNodeRef<ast::Expr>,

    #[no_eq]
    count: countme::Count<Expression<'static>>,
}

impl<'db> Expression<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

use ruff_db::files::File;
use ruff_python_ast::{self as ast};

use crate::ast_node_ref::AstNodeRef;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};
use crate::Db;

/// This ingredient represents a single unpacking.
///
/// This is required to make use of salsa to cache the complete unpacking of multiple variables
/// involved. It allows us to:
/// 1. Avoid doing structural match multiple times for each definition
/// 2. Avoid highlighting the same error multiple times
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
pub(crate) struct Unpack<'db> {
    #[id]
    pub(crate) file: File,

    #[id]
    pub(crate) file_scope: FileScopeId,

    /// The target expression that is being unpacked. For example, in `(a, b) = (1, 2)`, the target
    /// expression is `(a, b)`.
    #[no_eq]
    #[return_ref]
    pub(crate) target: AstNodeRef<ast::Expr>,

    /// The ingredient representing the value expression of the unpacking. For example, in
    /// `(a, b) = (1, 2)`, the value expression is `(1, 2)`.
    #[no_eq]
    pub(crate) value: Expression<'db>,

    #[no_eq]
    count: countme::Count<Unpack<'static>>,
}

impl<'db> Unpack<'db> {
    /// Returns the scope where the unpacking is happening.
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

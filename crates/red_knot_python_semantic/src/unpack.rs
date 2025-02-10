use ruff_db::files::File;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};

use crate::ast_node_ref::AstNodeRef;
use crate::semantic_index::ast_ids::{HasScopedExpressionId, ScopedExpressionId};
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
    pub(crate) value: UnpackValue<'db>,

    #[no_eq]
    count: countme::Count<Unpack<'static>>,
}

impl<'db> Unpack<'db> {
    /// Returns the scope where the unpacking is happening.
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }

    /// Returns the range of the unpack target expression.
    pub(crate) fn range(self, db: &'db dyn Db) -> TextRange {
        self.target(db).range()
    }
}

/// The expression that is being unpacked.
#[derive(Clone, Copy, Debug)]
pub(crate) enum UnpackValue<'db> {
    /// An iterable expression like the one in a `for` loop or a comprehension.
    Iterable(Expression<'db>),
    /// An expression that is being assigned to a target.
    Assign(Expression<'db>),
}

impl<'db> UnpackValue<'db> {
    /// Returns `true` if the value is an iterable expression.
    pub(crate) const fn is_iterable(self) -> bool {
        matches!(self, UnpackValue::Iterable(_))
    }

    /// Returns `true` if the value is being assigned to a target.
    pub(crate) const fn is_assign(self) -> bool {
        matches!(self, UnpackValue::Assign(_))
    }

    /// Returns the underlying [`Expression`] that is being unpacked.
    pub(crate) const fn expression(self) -> Expression<'db> {
        match self {
            UnpackValue::Assign(expr) | UnpackValue::Iterable(expr) => expr,
        }
    }

    /// Returns the [`ScopedExpressionId`] of the underlying expression.
    pub(crate) fn scoped_expression_id(
        self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> ScopedExpressionId {
        self.expression()
            .node_ref(db)
            .scoped_expression_id(db, scope)
    }

    /// Returns the expression as an [`AnyNodeRef`].
    pub(crate) fn as_any_node_ref(self, db: &'db dyn Db) -> AnyNodeRef<'db> {
        self.expression().node_ref(db).node().into()
    }
}

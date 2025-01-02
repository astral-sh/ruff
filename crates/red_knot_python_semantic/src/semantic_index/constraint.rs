use ruff_db::files::File;
use ruff_python_ast::Singleton;

use crate::db::Db;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Constraint<'db> {
    pub(crate) node: ConstraintNode<'db>,
    pub(crate) is_positive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConstraintNode<'db> {
    Expression(Expression<'db>),
    Pattern(PatternConstraint<'db>),
}

/// Pattern kinds for which we support type narrowing and/or static visibility analysis.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PatternConstraintKind<'db> {
    Singleton(Singleton, Option<Expression<'db>>),
    Value(Expression<'db>, Option<Expression<'db>>),
    Class(Expression<'db>, Option<Expression<'db>>),
    Unsupported,
}

#[salsa::tracked]
pub(crate) struct PatternConstraint<'db> {
    #[id]
    pub(crate) file: File,

    #[id]
    pub(crate) file_scope: FileScopeId,

    #[no_eq]
    #[return_ref]
    pub(crate) subject: Expression<'db>,

    #[no_eq]
    #[return_ref]
    pub(crate) kind: PatternConstraintKind<'db>,

    #[no_eq]
    count: countme::Count<PatternConstraint<'static>>,
}

impl<'db> PatternConstraint<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

use ruff_db::files::File;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::Singleton;

use crate::db::Db;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};

// A scoped identifier for each `Constraint` in a scope.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub(crate) struct ScopedConstraintId;

// A collection of constraints. This is currently stored in `UseDefMap`, which means we maintain a
// separate set of constraints for each scope in a file.
pub(crate) type Constraints<'db> = IndexVec<ScopedConstraintId, Constraint<'db>>;

#[derive(Debug, Default)]
pub(crate) struct ConstraintsBuilder<'db> {
    constraints: IndexVec<ScopedConstraintId, Constraint<'db>>,
}

impl<'db> ConstraintsBuilder<'db> {
    /// Adds a constraint, ensuring that we only store any particular constraint once.
    pub(crate) fn add_constraint(&mut self, constraint: Constraint<'db>) -> ScopedConstraintId {
        self.constraints.push(constraint)
    }

    pub(crate) fn build(mut self) -> Constraints<'db> {
        self.constraints.shrink_to_fit();
        self.constraints
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub(crate) struct Constraint<'db> {
    pub(crate) node: ConstraintNode<'db>,
    pub(crate) is_positive: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub(crate) enum ConstraintNode<'db> {
    Expression(Expression<'db>),
    Pattern(PatternConstraint<'db>),
}

/// Pattern kinds for which we support type narrowing and/or static visibility analysis.
#[derive(Debug, Clone, Hash, PartialEq, salsa::Update)]
pub(crate) enum PatternConstraintKind<'db> {
    Singleton(Singleton, Option<Expression<'db>>),
    Value(Expression<'db>, Option<Expression<'db>>),
    Class(Expression<'db>, Option<Expression<'db>>),
    Unsupported,
}

#[salsa::tracked]
pub(crate) struct PatternConstraint<'db> {
    pub(crate) file: File,

    pub(crate) file_scope: FileScopeId,

    pub(crate) subject: Expression<'db>,

    #[return_ref]
    pub(crate) kind: PatternConstraintKind<'db>,

    count: countme::Count<PatternConstraint<'static>>,
}

impl<'db> PatternConstraint<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

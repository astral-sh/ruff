//! _Predicates_ are Python expressions whose runtime values can affect type inference.
//!
//! We currently use predicates in two places:
//!
//! - [_Narrowing constraints_][crate::semantic_index::narrowing_constraints] constrain the type of
//!   a binding that is visible at a particular use.
//! - [_Visibility constraints_][crate::semantic_index::visibility_constraints] determine the
//!   static visibility of a binding, and the reachability of a statement.

use ruff_db::files::File;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::Singleton;

use crate::db::Db;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};

// A scoped identifier for each `Predicate` in a scope.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub(crate) struct ScopedPredicateId;

// A collection of predicates for a given scope.
pub(crate) type Predicates<'db> = IndexVec<ScopedPredicateId, Predicate<'db>>;

#[derive(Debug, Default)]
pub(crate) struct PredicatesBuilder<'db> {
    predicates: IndexVec<ScopedPredicateId, Predicate<'db>>,
}

impl<'db> PredicatesBuilder<'db> {
    /// Adds a predicate. Note that we do not deduplicate predicates. If you add a `Predicate`
    /// more than once, you will get distinct `ScopedPredicateId`s for each one. (This lets you
    /// model predicates that might evaluate to different values at different points of execution.)
    pub(crate) fn add_predicate(&mut self, predicate: Predicate<'db>) -> ScopedPredicateId {
        self.predicates.push(predicate)
    }

    pub(crate) fn build(mut self) -> Predicates<'db> {
        self.predicates.shrink_to_fit();
        self.predicates
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub(crate) struct Predicate<'db> {
    pub(crate) node: PredicateNode<'db>,
    pub(crate) is_positive: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub(crate) enum PredicateNode<'db> {
    Expression(Expression<'db>),
    Pattern(PatternPredicate<'db>),
}

/// Pattern kinds for which we support type narrowing and/or static visibility analysis.
#[derive(Debug, Clone, Hash, PartialEq, salsa::Update)]
pub(crate) enum PatternPredicateKind<'db> {
    Singleton(Singleton),
    Value(Expression<'db>),
    Or(Vec<PatternPredicateKind<'db>>),
    Class(Expression<'db>),
    Unsupported,
}

#[salsa::tracked(debug)]
pub(crate) struct PatternPredicate<'db> {
    pub(crate) file: File,

    pub(crate) file_scope: FileScopeId,

    pub(crate) subject: Expression<'db>,

    #[return_ref]
    pub(crate) kind: PatternPredicateKind<'db>,

    pub(crate) guard: Option<Expression<'db>>,

    count: countme::Count<PatternPredicate<'static>>,
}

impl<'db> PatternPredicate<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

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
use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::{FileScopeId, ScopeId, ScopedSymbolId};

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

impl Predicate<'_> {
    pub(crate) fn negated(self) -> Self {
        Self {
            node: self.node,
            is_positive: !self.is_positive,
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub(crate) enum PredicateNode<'db> {
    Expression(Expression<'db>),
    Pattern(PatternPredicate<'db>),
    StarImportPlaceholder(StarImportPlaceholderPredicate<'db>),
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

/// A "placeholder predicate" that is used to model the fact that the boundness of a
/// (possible) definition or declaration caused by a `*` import cannot be fully determined
/// until type-inference time. This is essentially the same as a standard visibility constraint,
/// so we reuse the [`Predicate`] infrastructure to model it.
///
/// To illustrate, say we have a module `exporter.py` like so:
///
/// ```py
/// if <condition>:
///     class A: ...
/// ```
///
/// and we have a module `importer.py` like so:
///
/// ```py
/// A = 1
///
/// from importer import *
/// ```
///
/// Since we cannot know whether or not <condition> is true at semantic-index time,
/// we record a definition for `A` in `b.py` as a result of the `from a import *`
/// statement, but place a predicate on it to record the fact that we don't yet
/// know whether this definition will be visible from all control-flow paths or not.
/// Essentially, we model `b.py` as something similar to this:
///
/// ```py
/// A = 1
///
/// if <star_import_placeholder_predicate>:
///     from a import A
/// ```
///
/// At type-check time, the placeholder predicate for the `A` definition is evaluated by
/// attempting to resolve the `A` symbol in `a.py`'s global namespace:
/// - If it resolves to a definitely bound symbol, then the predicate resolves to [`Truthiness::AlwaysTrue`]
/// - If it resolves to an unbound symbol, then the predicate resolves to [`Truthiness::AlwaysFalse`]
/// - If it resolves to a possibly bound symbol, then the predicate resolves to [`Truthiness::Ambiguous`]
///
/// [Truthiness]: [crate::types::Truthiness]
#[salsa::tracked(debug)]
pub(crate) struct StarImportPlaceholderPredicate<'db> {
    pub(crate) importing_file: File,

    /// Each symbol imported by a `*` import has a separate predicate associated with it:
    /// this field identifies which symbol that is.
    ///
    /// Note that a [`ScopedSymbolId`] is only meaningful if you also know the scope
    /// it is relative to. For this specific struct, however, there's no need to store a
    /// separate field to hold the ID of the scope. `StarImportPredicate`s are only created
    /// for valid `*`-import definitions, and valid `*`-import definitions can only ever
    /// exist in the global scope; thus, we know that the `symbol_id` here will be relative
    /// to the global scope of the importing file.
    pub(crate) symbol_id: ScopedSymbolId,

    pub(crate) referenced_file: File,
}

impl<'db> StarImportPlaceholderPredicate<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        // See doc-comment above [`StarImportPlaceholderPredicate::symbol_id`]:
        // valid `*`-import definitions can only take place in the global scope.
        global_scope(db, self.importing_file(db))
    }
}

impl<'db> From<StarImportPlaceholderPredicate<'db>> for Predicate<'db> {
    fn from(predicate: StarImportPlaceholderPredicate<'db>) -> Self {
        Predicate {
            node: PredicateNode::StarImportPlaceholder(predicate),
            is_positive: true,
        }
    }
}

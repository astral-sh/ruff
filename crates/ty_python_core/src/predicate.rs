//! _Predicates_ are Python expressions whose runtime values can affect type inference.
//!
//! We currently use predicates in two places:
//!
//! - [_Narrowing constraints_][crate::narrowing_constraints] constrain the type of
//!   a binding that is visible at a particular use.
//! - [_Reachability constraints_][crate::reachability_constraints] determine the
//!   static reachability of a binding, and the reachability of a statement or expression.

use ruff_db::files::File;
use ruff_index::{FrozenIndexVec, Idx, IndexVec};
use ruff_python_ast::{Singleton, name::Name};

use crate::ast_ids::ExpressionNodeKey;
use crate::db::Db;
use crate::expression::Expression;
use crate::global_scope;
use crate::scope::{FileScopeId, ScopeId};
use crate::symbol::ScopedSymbolId;

// A scoped identifier for each `Predicate` in a scope.
#[derive(Clone, Debug, Copy, PartialOrd, Ord, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct ScopedPredicateId(u32);

impl ScopedPredicateId {
    /// A special ID that is used for an "always true" predicate.
    pub(crate) const ALWAYS_TRUE: ScopedPredicateId = ScopedPredicateId(0xffff_ffff);

    /// A special ID that is used for an "always false" predicate.
    pub(crate) const ALWAYS_FALSE: ScopedPredicateId = ScopedPredicateId(0xffff_fffe);

    const SMALLEST_TERMINAL: ScopedPredicateId = Self::ALWAYS_FALSE;

    fn is_terminal(self) -> bool {
        self >= Self::SMALLEST_TERMINAL
    }
}

impl Idx for ScopedPredicateId {
    #[inline]
    fn new(value: usize) -> Self {
        assert!(value <= (Self::SMALLEST_TERMINAL.0 as usize));
        #[expect(clippy::cast_possible_truncation)]
        Self(value as u32)
    }

    #[inline]
    fn index(self) -> usize {
        debug_assert!(!self.is_terminal());
        self.0 as usize
    }
}

// A collection of predicates for a given scope.
pub type Predicates<'db> = FrozenIndexVec<ScopedPredicateId, Predicate<'db>>;

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

    pub(crate) fn build(self) -> Predicates<'db> {
        self.predicates.into()
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub struct Predicate<'db> {
    pub node: PredicateNode<'db>,
    pub is_positive: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) enum PredicateOrLiteral<'db> {
    Literal(bool),
    Predicate(Predicate<'db>),
}

impl PredicateOrLiteral<'_> {
    pub(crate) fn negated(self) -> Self {
        match self {
            PredicateOrLiteral::Literal(value) => PredicateOrLiteral::Literal(!value),
            PredicateOrLiteral::Predicate(Predicate { node, is_positive }) => {
                PredicateOrLiteral::Predicate(Predicate {
                    node,
                    is_positive: !is_positive,
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub struct CallableAndCallExpr<'db> {
    pub callable: Expression<'db>,
    pub call_expr: Expression<'db>,
    /// Whether the call is wrapped in an `await` expression. If `true`, `call_expr` refers to the
    /// `await` expression rather than the call itself. This is used to detect terminal `await`s of
    /// async functions that return `Never`.
    pub is_await: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum PredicateNode<'db> {
    Expression(Expression<'db>),
    /// These predicates are recorded for statements with call expressions. As part of
    /// reachability constraints, they are used to determine whether control flow can
    /// continue past this statement or not.
    ///
    /// The predicate evaluates to
    /// [`crate::Truthiness::AlwaysTrue`] in the common case where a call
    /// is inferred as returning an inhabited type: in these situations, we will
    /// infer control flow as flowing through the call expression without
    /// terminating. If it can be statically guaranteed that a call always
    /// returns `Never`/`NoReturn`, however, the predicate evaluates to
    /// [`crate::Truthiness::AlwaysFalse`], signaling that control flow
    /// ends as a result of the call: these call expressions are terminal.
    ///
    /// These predicates never evaluate to
    /// [`crate::Truthiness::Ambiguous`], even if the return type of the
    /// call is `Unknown`/`Any`, because that would result in too many false
    /// positives.
    IsNonTerminalCall(CallableAndCallExpr<'db>),
    /// Whether an iterable is statically known to yield at least one item.
    ///
    /// Currently, this predicate is only emitted for direct `range(...)` calls. It is resolved
    /// semantically during type checking, so calls to a shadowed `range` remain ambiguous.
    IsNonEmptyIterable(Expression<'db>),
    Pattern(PatternPredicate<'db>),
    SubjectElementPattern(SubjectElementPatternPredicate<'db>),
    StarImportPlaceholder(StarImportPlaceholderPredicate<'db>),
}

/// A pattern predicate applied to one expression in a sequence-display subject.
///
/// The full pattern determines the predicate's truth value, while `target` selects the subject
/// occurrence whose aligned pattern constraint should be applied to a binding.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub struct SubjectElementPatternPredicate<'db> {
    pub pattern: PatternPredicate<'db>,
    pub target: ExpressionNodeKey,
}

/// Structural details for sequence patterns that affect narrowing and reachability.
#[derive(Debug, Clone, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct SequencePatternPredicateKind<'db> {
    pub patterns: Box<[PatternPredicateKind<'db>]>,
}

impl<'db> SequencePatternPredicateKind<'db> {
    /// Return `true` for `case [*rest]`, the only sequence pattern with no
    /// length or element constraints.
    pub fn is_irrefutable(&self) -> bool {
        matches!(self.patterns.as_ref(), [PatternPredicateKind::Star(_)])
    }

    /// Return the patterns before and after the starred element.
    pub fn split_around_star(
        &self,
    ) -> Option<(&[PatternPredicateKind<'db>], &[PatternPredicateKind<'db>])> {
        let star_index = self
            .patterns
            .iter()
            .position(|pattern| matches!(pattern, PatternPredicateKind::Star(_)))?;
        let (prefix, star_and_suffix) = self.patterns.split_at(star_index);
        Some((prefix, &star_and_suffix[1..]))
    }
}

/// Structural details for a class pattern.
#[derive(Debug, Clone, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct ClassPatternPredicateKind<'db> {
    pub class: Expression<'db>,
    pub positional: Box<[PatternPredicateKind<'db>]>,
    pub keywords: Box<[ClassPatternKeywordPredicateKind<'db>]>,
}

impl ClassPatternPredicateKind<'_> {
    pub fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.keywords.is_empty()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct ClassPatternKeywordPredicateKind<'db> {
    pub attr: Name,
    pub pattern: PatternPredicateKind<'db>,
}

/// Structural details for a mapping pattern.
#[derive(Debug, Clone, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct MappingPatternPredicateKind<'db> {
    pub entries: Box<[MappingPatternEntryPredicateKind<'db>]>,
    pub rest: Option<Name>,
}

impl MappingPatternPredicateKind<'_> {
    pub fn is_irrefutable(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct MappingPatternEntryPredicateKind<'db> {
    pub key: Expression<'db>,
    pub pattern: PatternPredicateKind<'db>,
}

/// Pattern structure used for type narrowing, static reachability, and inferring the types of
/// names bound by a successful match.
#[derive(Debug, Clone, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub enum PatternPredicateKind<'db> {
    Singleton(Singleton),
    Value(Expression<'db>),
    Or(Box<[PatternPredicateKind<'db>]>),
    Class(ClassPatternPredicateKind<'db>),
    Mapping(MappingPatternPredicateKind<'db>),
    Sequence(SequencePatternPredicateKind<'db>),
    As(Option<Box<PatternPredicateKind<'db>>>, Option<Name>),
    Star(Option<Name>),
}

#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct PatternPredicate<'db> {
    pub file: File,

    pub file_scope: FileScopeId,

    pub subject: Expression<'db>,

    #[returns(ref)]
    pub kind: PatternPredicateKind<'db>,

    pub guard: Option<Expression<'db>>,

    /// A reference to the pattern of the previous match case
    pub previous_predicate: Option<Box<PatternPredicate<'db>>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PatternPredicate<'_> {}

impl<'db> PatternPredicate<'db> {
    pub fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

/// A "placeholder predicate" that is used to model the fact that the boundness of a (possible)
/// definition or declaration caused by a `*` import cannot be fully determined until type-
/// inference time. This is essentially the same as a standard reachability constraint, so we reuse
/// the [`Predicate`] infrastructure to model it.
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
/// from exporter import *
/// ```
///
/// Since we cannot know whether or not <condition> is true at semantic-index time, we record
/// a definition for `A` in `importer.py` as a result of the `from exporter import *` statement,
/// but place a predicate on it to record the fact that we don't yet know whether this definition
/// will be visible from all control-flow paths or not. Essentially, we model `importer.py` as
/// something similar to this:
///
/// ```py
/// A = 1
///
/// if <star_import_placeholder_predicate>:
///     from a import A
/// ```
///
/// At type-check time, the placeholder predicate for the `A` definition is evaluated by attempting
/// to resolve the `A` symbol in `exporter.py`'s global namespace:
/// - If it resolves to a definitely bound symbol, then the predicate resolves to [`Truthiness::AlwaysTrue`]
/// - If it resolves to an unbound symbol, then the predicate resolves to [`Truthiness::AlwaysFalse`]
/// - If it resolves to a possibly bound symbol, then the predicate resolves to [`Truthiness::Ambiguous`]
///
/// [Truthiness]: [crate::types::Truthiness]
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct StarImportPlaceholderPredicate<'db> {
    pub importing_file: File,

    /// Each symbol imported by a `*` import has a separate predicate associated with it:
    /// this field identifies which symbol that is.
    ///
    /// Note that a [`ScopedPlaceId`] is only meaningful if you also know the scope
    /// it is relative to. For this specific struct, however, there's no need to store a
    /// separate field to hold the ID of the scope. `StarImportPredicate`s are only created
    /// for valid `*`-import definitions, and valid `*`-import definitions can only ever
    /// exist in the global scope; thus, we know that the `symbol_id` here will be relative
    /// to the global scope of the importing file.
    pub symbol_id: ScopedSymbolId,

    pub referenced_file: File,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StarImportPlaceholderPredicate<'_> {}

impl<'db> StarImportPlaceholderPredicate<'db> {
    pub fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        // See doc-comment above [`StarImportPlaceholderPredicate::symbol_id`]:
        // valid `*`-import definitions can only take place in the global scope.
        global_scope(db, self.importing_file(db))
    }
}

impl<'db> From<StarImportPlaceholderPredicate<'db>> for PredicateOrLiteral<'db> {
    fn from(predicate: StarImportPlaceholderPredicate<'db>) -> Self {
        PredicateOrLiteral::Predicate(Predicate {
            node: PredicateNode::StarImportPlaceholder(predicate),
            is_positive: true,
        })
    }
}

//! Structural recursive types and their binding and substitution operations.
//!
//! A structural recursive type describes recursion within the type itself.
//! We write `μa. B` for a recursive type with body `B`, where occurrences of the recursive
//! variable `a` refer to the whole type. The `μa` is called a binder: it determines
//! what `a` refers to within `B`. For example:
//!
//! ```text
//! R = μa. tuple[int, a | None]
//! ```
//!
//! Such a recursive type is constructed, for example,
//! from the implicit type alias `R = tuple[int, "R | None"]`.
//!
//! A type is *closed* if every recursive variable is bound within that type;
//! otherwise, it is *open* (there is a dangling reference).
//! All types exposed to ordinary type operations must be closed, including during
//! intermediate normalization steps. The binding and substitution operations
//! in this module must maintain that invariant: an open body cannot be passed
//! directly to operations such as assignability checking.
//!
//! Substitution replaces occurrences of a variable with a type. It is
//! *capture-avoiding* when it preserves which binder every remaining variable refers
//! to, including variables in the inserted expression. We write `B[a := R]` for this
//! substitution of `R` for `a` in `B`. Unfolding exposes one layer of a recursive type
//! by substituting the whole type for its bound references:
//!
//! ```text
//! unfold(μa. B) = B[a := μa. B]
//! unfold(R) = tuple[int, R | None]
//! ```
//!
//! The result is closed, so ordinary type operations can inspect it. Binding works in
//! the other direction: it replaces applications of a recursive type with variables
//! and encloses the resulting open body in a `RecursiveType`. Both operations respect
//! nested binders. A variable refers to the nearest enclosing binder with the same
//! `RecursiveCycle`; substitution does not enter that binder's body when it binds the
//! target variable. The binder's arguments are outside its scope and are still substituted.
//!
//! A *type constructor* takes type arguments and produces a type. Recursive types can
//! also be parameterized this way. With type variable `T`, the alias
//! `Tree = tuple[T, "Tree[list[T]] | None"]` has the recursive type constructor:
//!
//! ```text
//! μF. λT. tuple[T, F[list[T]] | None]
//! ```
//!
//! Here `μF` binds the recursive constructor, and `λT` binds its type parameter.
//! The occurrence `F[list[T]]` is stored as `RecursiveVar` with the constructor's
//! `RecursiveCycle` and unspecialized arguments `[list[T]]`.
//!
//! To infer `x[1]` for `x: Tree[int]`, first unfold `x`'s type: substitute the constructor
//! for `F`, then apply `T := int`. This order closes the body before specializing it:
//!
//! ```text
//! unfold((μF. λT. tuple[T, F[list[T]] | None])[int])
//! = tuple[int, (μF. λT. tuple[T, F[list[T]] | None])[list[int]] | None]
//! = tuple[int, Tree[list[int]] | None]
//! ```
//!
//! Tuple subscripting then selects the element at index 1: `x[1]: Tree[list[int]] | None`.
//!
//! A constructor can also defer a type derived from a declaration. The value projection of a
//! `TypedDict` is the union of its declared field types and extra-item types. Storing that
//! projection without expanding it lets a mapping comparison establish the class relationship
//! before inspecting the values. These projections have no source alias name; ordinary union
//! construction and type display expand their completed bodies.

use std::cell::{Cell, OnceCell, RefCell};
use std::hash::Hash;
use std::marker::PhantomData;

use rustc_hash::FxHashSet;
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::cyclic::{
    ActiveRecursionDetector, CycleDetector, CycleDetectorCache, CycleKind, HasIdentity,
    TypeTransformer,
};
use super::generics::{ApplySpecialization, Specialization};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::AliasCycleSummary;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, ClassLiteral, ClassType, GenericContext,
    MaterializationKind, Type, TypeContext, TypeMapping, TypeRecursionContext, TypedDictType,
    VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

/// Access to recursive bodies while a caller's recursion guard is active.
///
/// A mapping visitor alone does not establish a recursion guard for the operation that reads the
/// unfolded body. This context is borrowed inside a guarded visit and cannot be constructed by a
/// type operation. The caller's mapping visitor remains shared throughout that visit.
pub(super) struct TypeExpansionContext<'a, 'env, 'db, Mode = SemanticTraversal> {
    mapping: &'a ApplyTypeMappingVisitor<'env, 'db>,
    mode: PhantomData<Mode>,
}

impl<'a, 'env, 'db, Mode> TypeExpansionContext<'a, 'env, 'db, Mode> {
    fn new(mapping: &'a ApplyTypeMappingVisitor<'env, 'db>) -> Self {
        Self {
            mapping,
            mode: PhantomData,
        }
    }
}

impl<'db> TypeExpansionContext<'_, '_, 'db> {
    /// Inspect a recursive body's outer alternatives without revisiting shared union edges.
    pub(super) fn expand_recursive(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> UnfoldResult<'db> {
        recursive
            .unfold(db, self.mapping)
            .map(|body| self.normalize(db, body))
    }

    /// Normalize outer aliases and unions under the caller's active recursion guard.
    pub(super) fn normalize(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        TypeInspector::with_mapping_visitor(self.mapping, |inspector| {
            inspector.inspect(db, ty, |ty, _| ty, |_, ty| ty)
        })
    }
}

/// Normalize the alternatives exposed by a semantic operation, retaining growing constructors.
///
/// Cycle guards stop a path that returns to an ancestor. They do not prevent visiting every path
/// through a shared graph. Expanding the outer union once preserves that sharing for queries,
/// stateful walks, and operations whose guard includes more than one operand.
fn normalize_outer_union<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    mapping: &ApplyTypeMappingVisitor<'_, 'db>,
) -> Type<'db> {
    match ty {
        Type::Union(union) if union.needs_alias_expansion(db, mapping) => {
            union.expand_aliases(db, mapping)
        }
        _ => ty,
    }
}

/// Whether a traversal consumes semantic alternatives or rebuilds the original type structure.
pub(crate) trait TypeTraversalMode: traversal_mode::Sealed {
    const NORMALIZE_UNIONS: bool;
}

pub(crate) struct SemanticTraversal;

mod traversal_mode {
    pub(crate) trait Sealed {}

    impl Sealed for super::SemanticTraversal {}
    impl Sealed for super::StructuralTraversal {}
}

impl TypeTraversalMode for SemanticTraversal {
    const NORMALIZE_UNIONS: bool = true;
}

/// Structural transformations must observe the original edges, including redundant alternatives.
pub(super) struct StructuralTraversal;

impl TypeTraversalMode for StructuralTraversal {
    const NORMALIZE_UNIONS: bool = false;
}

pub(super) type StructuralTypeTraversal<'env, 'db, O> =
    TypeTraversal<'env, 'db, O, StructuralTraversal>;

pub(super) type StructuralExpansionContext<'a, 'env, 'db> =
    TypeExpansionContext<'a, 'env, 'db, StructuralTraversal>;

impl<'db> StructuralExpansionContext<'_, '_, 'db> {
    /// Observe the original body when collecting dependencies or rebuilding type structure.
    /// Normalization can remove redundant alternatives that still contain relevant parameters.
    pub(super) fn recursive_body(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> UnfoldResult<'db> {
        recursive.unfold(db, self.mapping)
    }
}

impl<'db, Tag, T, R: Clone, const N: usize> CycleDetector<'db, Tag, T, R, N>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    /// Inspect recursive bodies under this detector's existing key and cycle policy.
    pub(super) fn visit_with_expansion(
        &self,
        db: &'db dyn Db,
        item: T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        compute: impl FnOnce(&TypeExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(db, item, || compute(&TypeExpansionContext::new(mapping)))
    }

    pub(super) fn visit_structure(
        &self,
        db: &'db dyn Db,
        item: T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        compute: impl FnOnce(&StructuralExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(db, item, || {
            compute(&StructuralExpansionContext::new(mapping))
        })
    }

    pub(super) fn try_visit_with_expansion(
        &self,
        db: &'db dyn Db,
        item: T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        reuse_cached: impl FnOnce(&R) -> bool,
        compute: impl FnOnce(&TypeExpansionContext<'_, '_, 'db>) -> R,
    ) -> Result<R, T> {
        self.try_visit(db, item, reuse_cached, || {
            compute(&TypeExpansionContext::new(mapping))
        })
    }
}

impl<'db, Tag, T, const N: usize> CycleDetector<'db, Tag, T, (), N>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    pub(super) fn visit_active_with_expansion<R>(
        &self,
        db: &'db dyn Db,
        item: T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        compute: impl FnOnce(Result<&TypeExpansionContext<'_, '_, 'db>, CycleKind>) -> R,
    ) -> R {
        self.visit_active(db, item, |cycle| match cycle {
            Some(kind) => compute(Err(kind)),
            None => compute(Ok(&TypeExpansionContext::new(mapping))),
        })
    }
}

impl<T: Hash + Eq + Clone> ActiveRecursionDetector<T> {
    pub(super) fn visit_structure<'db, R>(
        &self,
        item: &T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        on_cycle: impl FnOnce() -> R,
        compute: impl FnOnce(&StructuralExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(item, on_cycle, || {
            compute(&StructuralExpansionContext::new(mapping))
        })
    }

    pub(super) fn visit_with_expansion<'db, R>(
        &self,
        item: &T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        on_cycle: impl FnOnce() -> R,
        compute: impl FnOnce(&TypeExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(item, on_cycle, || {
            compute(&TypeExpansionContext::new(mapping))
        })
    }
}

impl<'db, Tag> TypeTransformer<'db, Tag> {
    pub(super) fn visit_type_with_expansion(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        compute: impl FnOnce(&TypeExpansionContext<'_, '_, 'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.visit_type(db, ty, || compute(&TypeExpansionContext::new(mapping)))
    }
}

/// Recursion and mapping state owned by a complete walk with operation-specific keys.
pub(crate) struct TypeVisitorState<'env, 'db, Tag, T: HasIdentity<'db>, R, const N: usize> {
    cycles: CycleDetector<'db, Tag, T, R, N>,
    mapping: ApplyTypeMappingVisitor<'env, 'db>,
}

impl<'env, 'db, Tag, T: HasIdentity<'db>, R, const N: usize>
    TypeVisitorState<'env, 'db, Tag, T, R, N>
{
    pub(super) fn new(env: &'env ProgramEnvironment<'db>, fallback: R) -> Self {
        Self {
            cycles: CycleDetector::new(fallback),
            mapping: ApplyTypeMappingVisitor::new(env),
        }
    }
}

impl<'db, Tag, T, R: Clone, const N: usize> TypeVisitorState<'_, 'db, Tag, T, R, N>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    pub(super) fn visit(&self, db: &'db dyn Db, item: T, compute: impl FnOnce() -> R) -> R {
        self.cycles.visit(db, item, compute)
    }

    pub(super) fn visit_with_expansion(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(&TypeExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.cycles
            .visit_with_expansion(db, item, &self.mapping, compute)
    }

    pub(super) fn visit_structure(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(&StructuralExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.cycles
            .visit_structure(db, item, &self.mapping, compute)
    }
}

impl<'db, Tag, T, const N: usize> TypeVisitorState<'_, 'db, Tag, T, (), N>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    pub(super) fn visit_active_with_expansion<R>(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(Result<&TypeExpansionContext<'_, '_, 'db>, CycleKind>) -> R,
    ) -> R {
        self.cycles
            .visit_active_with_expansion(db, item, &self.mapping, compute)
    }
}

/// A walk over outer type alternatives that can borrow mutable state.
///
/// [`TypeTraversal`] expands aliases and recursive types, and normalizes outer unions, before
/// calling [`Self::visit`]. Transformations that descend into type arguments use
/// [`StructuralTypeTraversal`] to retain recursive alias edges and the original alternatives.
/// A recursive reference whose body is still unavailable is passed through unchanged.
/// Implementations select which other children to inspect by calling the traversal's `visit`
/// method. They never unfold recursive bodies or manage the active recursion stack themselves.
/// `State` passes per-visit inputs, including mutable builders, without storing them in the traversal.
/// Walk results are not cached; use [`TypeQuery`] when completed results can be reused.
pub(crate) trait TypeWalk<'db, State = (), Mode: TypeTraversalMode = SemanticTraversal>:
    Sized
{
    type Output;

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &TypeTraversal<'_, 'db, Self, Mode>,
        ty: Type<'db>,
        state: State,
    ) -> Self::Output;

    /// Return the result of revisiting an active type, including a growing specialization.
    fn on_cycle(
        &self,
        _traversal: &TypeTraversal<'_, 'db, Self, Mode>,
        ty: Type<'db>,
        kind: CycleKind,
        state: State,
    ) -> Self::Output;

    /// Consume one unfolded body while its binder remains active.
    ///
    /// Most operations continue inspecting its outer shape. Structural visitors can instead
    /// visit the body themselves so their callbacks still observe aliases and other containers.
    fn visit_recursive_body(
        &self,
        db: &'db dyn Db,
        traversal: &TypeTraversal<'_, 'db, Self, Mode>,
        body: Type<'db>,
        state: State,
    ) -> Self::Output {
        traversal.visit_with(db, body, state)
    }

    /// Process an alias or recursive type with its traversal guard active.
    ///
    /// Operations that can start another projection while expanding an alias can retain their
    /// additional context here. Calling `visit_body` unfolds and visits the body under that guard;
    /// an operation can also retain the reference without invoking the callback.
    fn visit_alias_like(
        &self,
        _db: &'db dyn Db,
        _traversal: &TypeTraversal<'_, 'db, Self, Mode>,
        _ty: Type<'db>,
        state: State,
        visit_body: impl FnOnce(State) -> Self::Output,
    ) -> Self::Output {
        visit_body(state)
    }
}

struct TypeTraversalTag;

/// Owns the recursion and materialization state for one type operation.
///
/// A recursive body can expose a union that contains the same recursive type again. The guard
/// therefore covers both unfolding and the operation's subsequent traversal of that body.
/// This is a walk: callbacks can mutate builders or collect occurrences. Use [`TypeQueryContext`]
/// for operations that compute a reusable result from a type and immutable input.
pub(crate) struct TypeTraversal<'env, 'db, O, Mode = SemanticTraversal> {
    operation: O,
    env: &'env ProgramEnvironment<'db>,
    recursion_context: Option<&'env TypeRecursionContext<'db>>,
    mapping: TraversalMapping<'env, 'db>,
    active: CycleDetector<'db, TypeTraversalTag, Type<'db>, (), 3>,
    mode: PhantomData<Mode>,
}

impl<'env, 'db, O, Mode: TypeTraversalMode> TypeTraversal<'env, 'db, O, Mode> {
    fn new(env: &'env ProgramEnvironment<'db>, operation: O) -> Self {
        Self {
            operation,
            env,
            recursion_context: None,
            mapping: TraversalMapping::Owned(OnceCell::new()),
            active: CycleDetector::new(()),
            mode: PhantomData,
        }
    }

    /// Run an operation with one borrowed traversal for its entire recursion.
    pub(crate) fn with_root<R>(
        env: &'env ProgramEnvironment<'db>,
        operation: O,
        visit: impl FnOnce(&Self) -> R,
    ) -> R {
        visit(&Self::new(env, operation))
    }

    pub(crate) fn environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn mapping(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        match &self.mapping {
            TraversalMapping::Owned(mapping) => mapping.get_or_init(|| {
                ApplyTypeMappingVisitor::new(self.env)
                    .with_recursion_context(self.recursion_context)
            }),
            TraversalMapping::Borrowed(mapping) => mapping,
        }
    }

    fn new_with_recursion_context(
        env: &'env ProgramEnvironment<'db>,
        operation: O,
        context: &'env TypeRecursionContext<'db>,
    ) -> Self {
        Self {
            recursion_context: Some(context),
            ..Self::new(env, operation)
        }
    }

    /// Start a traversal inside a transformation without replacing that transformation's guards.
    fn from_mapping_visitor(
        visitor: &'env ApplyTypeMappingVisitor<'env, 'db>,
        operation: O,
    ) -> Self {
        Self {
            operation,
            env: visitor.env,
            recursion_context: visitor.recursion_context,
            mapping: TraversalMapping::Borrowed(visitor),
            active: CycleDetector::new(()),
            mode: PhantomData,
        }
    }

    pub(super) fn with_mapping_visitor<R>(
        visitor: &'env ApplyTypeMappingVisitor<'env, 'db>,
        operation: O,
        visit: impl FnOnce(&Self) -> R,
    ) -> R {
        visit(&Self::from_mapping_visitor(visitor, operation))
    }

    pub(crate) fn visit(&self, db: &'db dyn Db, ty: Type<'db>) -> O::Output
    where
        O: TypeWalk<'db, (), Mode>,
    {
        self.visit_with(db, ty, ())
    }

    /// Visit a type while passing operation-specific state, such as a mutable type builder.
    pub(crate) fn visit_with<State>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        state: State,
    ) -> O::Output
    where
        O: TypeWalk<'db, State, Mode>,
    {
        self.with_active_type(db, ty, |cycle| {
            if let Some(kind) = cycle {
                return self.operation.on_cycle(self, ty, kind, state);
            }
            match ty {
                Type::Recursive(recursive) => {
                    self.operation
                        .visit_alias_like(db, self, ty, state, |state| {
                            match recursive.unfold(db, self.mapping()) {
                                UnfoldResult::Unfolded(body) => {
                                    self.operation.visit_recursive_body(db, self, body, state)
                                }
                                UnfoldResult::Unchanged(_) => {
                                    self.operation.visit(db, self, ty, state)
                                }
                            }
                        })
                }
                Type::TypeAlias(alias) => {
                    self.operation
                        .visit_alias_like(db, self, ty, state, |state| {
                            self.visit_with(
                                db,
                                alias.value_type_with_recursion(db, self.recursion_context),
                                state,
                            )
                        })
                }
                Type::Union(_) if Mode::NORMALIZE_UNIONS => {
                    let normalized = normalize_outer_union(db, ty, self.mapping());
                    if normalized == ty {
                        self.operation.visit(db, self, ty, state)
                    } else {
                        self.visit_with(db, normalized, state)
                    }
                }
                _ => self.operation.visit(db, self, ty, state),
            }
        })
    }

    fn with_active_type<R>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visit: impl FnOnce(Option<CycleKind>) -> R,
    ) -> R {
        // These outer-shape operations recurse through deferred definitions, bounds, and newtype
        // bases. Revisiting a concrete type during normalization is not a recursive unfolding.
        if needs_traversal_guard(ty) {
            self.active.visit_active(db, ty, visit)
        } else {
            visit(None)
        }
    }
}

impl<'env, 'db, O> StructuralTypeTraversal<'env, 'db, O> {
    pub(super) fn mapping_visitor(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        self.mapping()
    }
}

fn needs_traversal_guard(ty: Type<'_>) -> bool {
    matches!(
        ty,
        Type::TypeAlias(_) | Type::Recursive(_) | Type::TypeVar(_) | Type::NewTypeInstance(_)
    )
}

/// A query of a type's outer alternatives, with immutable operation-specific input.
///
/// The context unfolds aliases, expands outer unions, and memoizes recursive queries. A query
/// must return the same result for the same type and input under the same recursion guards.
/// Parameters shared by all recursive calls, such as an attribute name, belong on `Self`.
/// Parameters that change between calls, such as a descriptor's receiver, belong in `Input`.
/// Keep mutable collectors and builders in a separate walk.
pub trait TypeQuery<'db, Input = ()>: Sized
where
    Input: Clone + Eq + Hash,
{
    type Output: Clone;

    fn visit(
        &self,
        db: &'db dyn Db,
        context: &TypeQueryContext<'_, 'db, Self, Input>,
        ty: Type<'db>,
        input: Input,
    ) -> Self::Output;

    fn on_cycle(
        &self,
        context: &TypeQueryContext<'_, 'db, Self, Input>,
        ty: Type<'db>,
        kind: CycleKind,
        input: Input,
    ) -> Self::Output;

    /// Keep guards shared with other operations active while inspecting an alias body.
    fn visit_alias_like(
        &self,
        _db: &'db dyn Db,
        _context: &TypeQueryContext<'_, 'db, Self, Input>,
        _ty: Type<'db>,
        input: Input,
        visit_body: impl FnOnce(Input) -> Self::Output,
    ) -> Self::Output {
        visit_body(input)
    }
}

type TypeQueryCache<'db, Input, R> =
    CycleDetectorCache<(Type<'db>, Input), TypeQueryResult<'db, R>>;

/// Shares completed queries as well as recursion and materialization state.
///
/// A cycle result can depend on the enclosing types. Each cached answer therefore records the
/// guards consulted by its computation. Reuse requires those guards to give the same answers
/// in the current context, including guards reached through another cached query.
/// Changes to guards shared with other operations invalidate the cache as well.
pub struct TypeQueryContext<'env, 'db, Q, Input = ()>
where
    Q: TypeQuery<'db, Input>,
    Input: Clone + Eq + Hash,
{
    traversal: TypeTraversal<'env, 'db, Q>,
    cache: RefCell<TypeQueryCache<'db, Input, Q::Output>>,
    recursion_revision: Cell<Option<[u64; 3]>>,
    dependencies: RefCell<Vec<Type<'db>>>,
    depth: Cell<usize>,
}

impl<'env, 'db, Q, Input> TypeQueryContext<'env, 'db, Q, Input>
where
    Q: TypeQuery<'db, Input>,
    Input: Clone + Eq + Hash,
{
    fn new(traversal: TypeTraversal<'env, 'db, Q>) -> Self {
        Self {
            traversal,
            cache: RefCell::new(CycleDetectorCache::new()),
            recursion_revision: Cell::new(None),
            dependencies: RefCell::default(),
            depth: Cell::new(0),
        }
    }

    /// Run a query with one cache and one set of guards for all recursive calls.
    pub fn with_root<R>(
        env: &'env ProgramEnvironment<'db>,
        query: Q,
        visit: impl FnOnce(&Self) -> R,
    ) -> R {
        visit(&Self::new(TypeTraversal::new(env, query)))
    }

    pub(super) fn with_recursion_context<R>(
        env: &'env ProgramEnvironment<'db>,
        query: Q,
        context: &'env TypeRecursionContext<'db>,
        visit: impl FnOnce(&Self) -> R,
    ) -> R {
        visit(&Self::new(TypeTraversal::new_with_recursion_context(
            env, query, context,
        )))
    }

    pub(crate) fn environment(&self) -> &ProgramEnvironment<'db> {
        self.traversal.environment()
    }

    fn mapping_visitor(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        self.traversal.mapping()
    }

    pub(crate) fn visit_with(&self, db: &'db dyn Db, ty: Type<'db>, input: Input) -> Q::Output {
        self.depth.set(self.depth.get() + 1);
        let _frame = TypeQueryFrame {
            depth: &self.depth,
            dependencies: &self.dependencies,
        };
        self.validate_recursion_context();
        let key = (ty, input.clone());
        if let Some(cached) = self.cache.borrow().get(&key)
            && cached.dependencies.iter().all(|(dependency, expected)| {
                self.traversal.active.cycle_kind(db, dependency) == *expected
            })
        {
            self.dependencies
                .borrow_mut()
                .extend(cached.dependencies.iter().map(|(ty, _)| *ty));
            return cached.output.clone();
        }

        let start = self.dependencies.borrow().len();
        if needs_traversal_guard(ty) {
            self.dependencies.borrow_mut().push(ty);
        }
        let output = self.traversal.with_active_type(db, ty, |cycle| {
            let query = &self.traversal.operation;
            if let Some(kind) = cycle {
                return query.on_cycle(self, ty, kind, input);
            }
            match ty {
                Type::TypeAlias(alias) => query.visit_alias_like(db, self, ty, input, |input| {
                    self.visit_with(
                        db,
                        alias.value_type_with_recursion(db, self.traversal.recursion_context),
                        input,
                    )
                }),
                Type::Recursive(recursive) => {
                    query.visit_alias_like(db, self, ty, input, |input| {
                        match recursive.unfold(db, self.mapping_visitor()) {
                            UnfoldResult::Unfolded(body) => self.visit_with(db, body, input),
                            UnfoldResult::Unchanged(_) => query.visit(db, self, ty, input),
                        }
                    })
                }
                Type::Union(_) => {
                    let expanded = normalize_outer_union(db, ty, self.mapping_visitor());
                    if expanded == ty {
                        query.visit(db, self, ty, input)
                    } else {
                        self.visit_with(db, expanded, input)
                    }
                }
                _ => query.visit(db, self, ty, input),
            }
        });

        // Nested queries contribute their complete dependency sets, even on a cache hit. Check
        // those dependencies after leaving this query's guards, when the enclosing stack is the
        // same as it was on entry. In A -> B -> A, B's result cannot escape its dependency on A.
        let dependencies: FxHashSet<_> = self.dependencies.borrow_mut().drain(start..).collect();
        self.dependencies.borrow_mut().extend(&dependencies);
        let dependencies = dependencies
            .into_iter()
            .map(|ty| (ty, self.traversal.active.cycle_kind(db, &ty)))
            .collect();
        self.validate_recursion_context();
        self.cache.borrow_mut().insert(
            key,
            TypeQueryResult {
                output: output.clone(),
                dependencies,
            },
        );
        output
    }

    fn validate_recursion_context(&self) {
        let revision = self
            .traversal
            .recursion_context
            .map(TypeRecursionContext::revision);
        if self.recursion_revision.replace(revision) != revision {
            // Shared guards can change in a nested projection or specialization without entering
            // this query's local traversal. Discard answers computed under the previous context.
            *self.cache.borrow_mut() = CycleDetectorCache::new();
        }
    }
}

impl<'db, Q: TypeQuery<'db>> TypeQueryContext<'_, 'db, Q> {
    pub fn visit(&self, db: &'db dyn Db, ty: Type<'db>) -> Q::Output {
        self.visit_with(db, ty, ())
    }
}

struct TypeQueryResult<'db, R> {
    output: R,
    dependencies: Box<[(Type<'db>, Option<CycleKind>)]>,
}

/// The dependency log belongs to active queries; completed roots retain only their cache entries.
struct TypeQueryFrame<'a, 'db> {
    depth: &'a Cell<usize>,
    dependencies: &'a RefCell<Vec<Type<'db>>>,
}

impl Drop for TypeQueryFrame<'_, '_> {
    fn drop(&mut self) {
        self.depth.set(self.depth.get() - 1);
        if self.depth.get() == 0 {
            self.dependencies.borrow_mut().clear();
        }
    }
}

enum TraversalMapping<'env, 'db> {
    Owned(OnceCell<ApplyTypeMappingVisitor<'env, 'db>>),
    Borrowed(&'env ApplyTypeMappingVisitor<'env, 'db>),
}

struct InspectType;

/// A normalized type together with the outer structure needed by union transformations.
pub(super) struct NormalizedType<'db> {
    pub(super) ty: Type<'db>,
    /// A union can normalize to a single type. That result is still an alternative to filter or
    /// use as an inference context, unlike an alias whose body was never a union.
    pub(super) was_union: bool,
}

/// A scoped inspection that keeps recursion guards active while normalizing outer alternatives.
pub(super) struct TypeInspector<'env, 'db> {
    traversal: StructuralTypeTraversal<'env, 'db, InspectType>,
}

struct Inspection<'a, 'env, 'db, C, F> {
    inspector: &'a TypeInspector<'env, 'db>,
    on_cycle: C,
    inspect: F,
    was_union: bool,
}

impl<'a, 'env, 'db, R, C, F> TypeWalk<'db, Inspection<'a, 'env, 'db, C, F>, StructuralTraversal>
    for InspectType
where
    C: FnOnce(Type<'db>, CycleKind) -> R,
    F: FnOnce(&TypeInspector<'_, 'db>, NormalizedType<'db>) -> R,
{
    type Output = R;

    fn on_cycle(
        &self,
        _traversal: &StructuralTypeTraversal<'_, 'db, Self>,
        ty: Type<'db>,
        kind: CycleKind,
        state: Inspection<'a, 'env, 'db, C, F>,
    ) -> R {
        (state.on_cycle)(ty, kind)
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &StructuralTypeTraversal<'_, 'db, Self>,
        ty: Type<'db>,
        mut state: Inspection<'a, 'env, 'db, C, F>,
    ) -> R {
        state.was_union |= ty.is_union();
        let normalized = normalize_outer_union(db, ty, traversal.mapping());
        if normalized != ty {
            return traversal.visit_with(db, normalized, state);
        }
        (state.inspect)(
            state.inspector,
            NormalizedType {
                ty,
                was_union: state.was_union,
            },
        )
    }
}

impl<'env, 'db> TypeInspector<'env, 'db> {
    pub(super) fn scope<R>(
        env: &'env ProgramEnvironment<'db>,
        inspect: impl FnOnce(&Self) -> R,
    ) -> R {
        inspect(&Self {
            traversal: StructuralTypeTraversal::new(env, InspectType),
        })
    }

    fn with_mapping_visitor<R>(
        mapping: &'env ApplyTypeMappingVisitor<'env, 'db>,
        inspect: impl FnOnce(&Self) -> R,
    ) -> R {
        inspect(&Self {
            traversal: StructuralTypeTraversal::from_mapping_visitor(mapping, InspectType),
        })
    }

    pub(super) fn environment(&self) -> &ProgramEnvironment<'db> {
        self.traversal.environment()
    }

    /// Start a complete inspection. Recursive helpers borrow the inspector passed to `inspect`.
    pub(super) fn inspect_root<R>(
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
        ty: Type<'db>,
        on_cycle: impl FnOnce(Type<'db>, CycleKind) -> R,
        inspect: impl FnOnce(&TypeInspector<'_, 'db>, Type<'db>) -> R,
    ) -> R {
        Self::scope(env, |inspector| {
            inspector.inspect(db, ty, on_cycle, inspect)
        })
    }

    pub(super) fn inspect<R>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        on_cycle: impl FnOnce(Type<'db>, CycleKind) -> R,
        inspect: impl FnOnce(&TypeInspector<'_, 'db>, Type<'db>) -> R,
    ) -> R {
        self.inspect_normalized(db, ty, on_cycle, |inspector, normalized| {
            inspect(inspector, normalized.ty)
        })
    }

    /// Preserve whether expansion encountered a union, including one that collapsed to one type.
    pub(super) fn inspect_normalized<R>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        on_cycle: impl FnOnce(Type<'db>, CycleKind) -> R,
        inspect: impl FnOnce(&TypeInspector<'_, 'db>, NormalizedType<'db>) -> R,
    ) -> R {
        self.traversal.visit_with(
            db,
            ty,
            Inspection {
                inspector: self,
                on_cycle,
                inspect,
                was_union: false,
            },
        )
    }
}

/// A recursive variable named by its binder's query cycle.
/// An escaping reference has no type semantics; in particular, it is neither a
/// gradual type nor an assignability operand.
/// Only recursive-type binding and substitution operations may construct or operate on recursive variables.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveVar<'db> {
    /// Refers to the nearest enclosing recursive binder with this cycle identity.
    #[returns(copy)]
    cycle: RecursiveCycle,
    /// The unspecialized arguments of this occurrence, in the alias definition's scope.
    /// For `Tree = tuple[T, "Tree[list[T]] | None"]`, these are `[list[T]]`.
    /// Unfolding substitutes the enclosing application's arguments for the type parameters.
    #[returns(copy)]
    arguments: Option<Specialization<'db>>,
}

impl get_size2::GetSize for RecursiveVar<'_> {}

impl<'db> RecursiveVar<'db> {
    /// Unfold references to the target cycle, retaining variables bound by other cycles.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let arguments = self
            .arguments(db)
            .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(recursive),
            )) if self.cycle(db) == recursive.cycle(db) => {
                Type::Recursive(recursive.with_arguments(db, arguments))
            }
            TypeMapping::ApplyRecursiveSubstitution(_) => {
                Type::RecursiveVar(Self::new_internal(db, self.cycle(db), arguments))
            }
            _ => unreachable!("semantic operation on an unbound recursive variable"),
        }
    }
}

/// A structural substitution that only the recursive-type binder can construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct RecursiveMapping<'db>(RecursiveSubstitution<'db>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum RecursiveSubstitution<'db> {
    /// Replace references to a binder with applications of its recursive constructor.
    /// In the module example, this replaces `F[list[T]]` with `Tree[list[T]]`.
    Unfold(RecursiveType<'db>),
    /// Replace applications of the target binder with variables to form an open body.
    /// In the module example, this replaces `Tree[list[T]]` with `F[list[T]]`.
    Bind(RecursiveCycle),
}

impl RecursiveSubstitution<'_> {
    fn cycle(self, db: &dyn Db) -> RecursiveCycle {
        match self {
            Self::Unfold(recursive) => recursive.cycle(db),
            Self::Bind(cycle) => cycle,
        }
    }
}

/// Identifies a recursive binder or a deferred value projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecursiveCycle {
    Alias(salsa::Id),
    TypedDictValues(salsa::Id),
}

impl get_size2::GetSize for RecursiveCycle {}

impl RecursiveCycle {
    fn id(self) -> salsa::Id {
        match self {
            Self::Alias(id) | Self::TypedDictValues(id) => id,
        }
    }
}

/// A stored recursive expression or a projection that can expose another recursive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RecursiveBody<'db> {
    Type(Type<'db>),
    TypedDictValues(ClassLiteral<'db>),
}

/// An application of a structural recursive type constructor.
/// The private body remains unspecialized; `arguments` records this application's
/// substitution for the constructor's type parameters. Unfolding replaces recursive
/// references with closed types, then applies that substitution before exposing the
/// result to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// The declaration that owns this constructor, including for qualified references.
    #[returns(copy)]
    pub(super) definition: Definition<'db>,
    /// Distinguishes binders and projections, including provisional types of alias queries.
    #[returns(copy)]
    cycle: RecursiveCycle,
    #[returns(copy)]
    body: RecursiveBody<'db>,
    /// The actual arguments for this application, which may themselves contain type variables.
    /// They are applied when unfolding; the stored body remains unspecialized.
    #[returns(copy)]
    pub(super) arguments: Option<Specialization<'db>>,
    /// The lazy materialization applied to this recursive type, if any.
    #[returns(copy)]
    pub(super) materialization_kind: Option<MaterializationKind>,
}

impl get_size2::GetSize for RecursiveType<'_> {}

#[salsa::tracked]
impl<'db> RecursiveType<'db> {
    /// Summarize the unspecialized constructor without applying semantic substitutions.
    pub(super) fn cycle_summary(self, db: &'db dyn Db) -> &'db AliasCycleSummary<'db> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|db, id, _, ()| AliasCycleSummary::from_type(db, Type::divergent_alias(id)),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn cycle_summary_impl<'db>(
            db: &'db dyn Db,
            recursive: RecursiveType<'db>,
            (): (),
        ) -> AliasCycleSummary<'db> {
            let body = match recursive.body(db) {
                RecursiveBody::Type(body) => body,
                RecursiveBody::TypedDictValues(class) => {
                    TypedDictType::new(class.identity_specialization(db)).unfold_value_type(db)
                }
            };
            let mut summary = AliasCycleSummary::from_type(db, body);
            // Nested bodies can refer to an enclosing binder. Close only the cycle marker,
            // so recovery never exposes an unbound variable as a standalone type.
            if let Some(Type::RecursiveVar(variable)) = summary.cycle {
                summary.cycle = Some(Type::divergent_alias(variable.cycle(db).id()));
            }
            summary
        }

        cycle_summary_impl(db, self.constructor(db), ())
    }

    /// Seed a query cycle with `μa. a`, using the same identity for binder and variable.
    pub(super) fn initial(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
    ) -> Self {
        let cycle = RecursiveCycle::Alias(cycle);
        let arguments = parameters.map(|parameters| parameters.identity_specialization(db));
        Self::new_internal(
            db,
            definition,
            cycle,
            RecursiveBody::Type(Type::RecursiveVar(RecursiveVar::new_internal(
                db, cycle, arguments,
            ))),
            arguments,
            None,
        )
    }

    /// Defer the possible values of a `TypedDict` until a type operation observes them.
    pub(super) fn typed_dict_values(db: &'db dyn Db, class: ClassType<'db>) -> Option<Self> {
        let definition = class.definition(db)?;
        let (origin, arguments) = class.class_literal_and_specialization(db);
        Some(Self::new_internal(
            db,
            definition,
            RecursiveCycle::TypedDictValues(definition.as_id()),
            RecursiveBody::TypedDictValues(origin),
            arguments,
            None,
        ))
    }

    /// Close recursive occurrences after inferring an alias's constructor expression.
    pub(super) fn recover(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
        result: Type<'db>,
    ) -> Type<'db> {
        // Shared dependencies can still contain older iterations of this alias. They refer
        // to the same definition even when their provisional bodies differ. Derive the binder
        // from the query inputs: an earlier result might not expose recursive references yet.
        Self::initial(db, definition, cycle, parameters).bind(
            db,
            &ProgramEnvironment::from_definition(definition),
            result,
        )
    }

    /// Bind references to this alias's query cycle in a closed inference result.
    /// Each bound occurrence retains its arguments and refers to this constructor's cycle.
    fn bind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Type<'db>,
    ) -> Type<'db> {
        let body = original.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(self.cycle(db)),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db) {
            Type::divergent_alias(self.cycle(db).id())
        } else if body == original {
            // Binding changes a closed type only by introducing references to this binder.
            body
        } else {
            Type::Recursive(Self::new_internal(
                db,
                self.definition(db),
                self.cycle(db),
                RecursiveBody::Type(body),
                self.arguments(db),
                None,
            ))
        }
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new_internal(
            db,
            self.definition(db),
            self.cycle(db),
            self.body(db),
            arguments,
            self.materialization_kind(db),
        )
    }

    /// Normalize markers from inference cycles in this application without changing its bound body.
    /// Recursive arguments are outside the binder and can contain results from value inference.
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let arguments = match self.arguments(db) {
            Some(arguments) => {
                Some(arguments.recursive_type_normalized_impl(db, env, div, nested)?)
            }
            None => None,
        };
        Some(self.with_arguments(db, arguments))
    }

    fn with_materialization(
        self,
        db: &'db dyn Db,
        materialization: Option<MaterializationKind>,
    ) -> Self {
        Self::new_internal(
            db,
            self.definition(db),
            self.cycle(db),
            self.body(db),
            self.arguments(db),
            materialization,
        )
    }

    /// Parameters bound by this recursive type constructor.
    pub(super) fn parameters(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.arguments(db)
            .map(|arguments| arguments.generic_context(db))
    }

    /// The declaration's name, shared by all specializations of this constructor.
    pub(super) fn name(self, db: &'db dyn Db) -> &'db str {
        let definition = self.definition(db);
        // Qualified uses retain the original declaration's symbol, not the access expression.
        place_table(db, definition.scope(db))
            .symbol(definition.place(db).expect_symbol())
            .name()
    }

    /// The source alias's definition and name, if this binder comes from an alias.
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        match self.body(db) {
            RecursiveBody::Type(_) => Some((self.definition(db), self.name(db))),
            RecursiveBody::TypedDictValues(_) => None,
        }
    }

    /// Restore the formal arguments and remove materialization for constructor analysis.
    pub(super) fn constructor(self, db: &'db dyn Db) -> Self {
        // Like an unspecialized PEP 695 alias, parameter-flow analysis must not
        // re-enter materialization while deriving the constructor's identity.
        self.with_materialization(db, None).with_arguments(
            db,
            self.parameters(db)
                .map(|parameters| parameters.identity_specialization(db)),
        )
    }

    /// The program in which the recursive type's body was constructed.
    pub(crate) fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_definition(self.definition(db))
    }

    /// Substitute closed types for references before exposing the body.
    ///
    /// Report whether unfolding returns exactly `Type::Recursive(self)`. An unfolded
    /// type can still contain recursive references, so callers must retain their recursion guards.
    /// General type inspections use [`TypeTraversal`], which owns those guards across the visit.
    ///
    /// Materializing the body can compare an argument with its materialization, which can unfold
    /// another application of this constructor. Keep the caller's equivalence guard so growing
    /// arguments do not restart that comparison with an empty recursion stack.
    fn unfold(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> UnfoldResult<'db> {
        // Materialize the original body so each recursive reference is transformed according
        // to its variance, rather than inheriting the enclosing application's materialization.
        let unfolded = self.with_materialization(db, None).unfolded_body(db);
        let unfolded = match self.materialization_kind(db) {
            Some(kind) => unfolded.materialize(db, kind, visitor),
            None => unfolded,
        };
        if unfolded == Type::Recursive(self) {
            UnfoldResult::Unchanged(self)
        } else {
            UnfoldResult::Unfolded(unfolded)
        }
    }

    /// Share the closed, specialized body across mappings with different visitors.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, recursive: RecursiveType<'db>| Type::Recursive(recursive),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn unfolded_body(self, db: &'db dyn Db) -> Type<'db> {
        let body = match self.body(db) {
            RecursiveBody::Type(body) => body,
            RecursiveBody::TypedDictValues(origin) => {
                let constructor = self.constructor(db);
                if self == constructor {
                    return TypedDictType::new(origin.identity_specialization(db))
                        .unfold_value_type(db);
                }

                // Specialize one shared constructor body. Reading each specialized schema can
                // expose different inference approximations, so growth analysis and unfolding
                // would follow different recursive references.
                let body = constructor.unfolded_body(db);
                let Some(arguments) = self.arguments(db) else {
                    return body;
                };
                let env = self.environment(db);
                return body.apply_type_mapping_impl(
                    db,
                    &TypeMapping::from_specialization(db, arguments, false),
                    TypeContext::default(),
                    &ApplyTypeMappingVisitor::new_without_union_normalization(&env),
                );
            }
        };
        let env = self.environment(db);
        let unfolded = body.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(self),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );
        match self.arguments(db) {
            Some(arguments) => unfolded.apply_type_mapping(
                db,
                &env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments)),
                TypeContext::default(),
            ),
            None => unfolded,
        }
    }

    /// Substitute by cycle identity, respecting the scope of nested recursive binders.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(cycle),
            )) if self.cycle(db) == *cycle && self.materialization_kind(db).is_none() => {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(db, self.cycle(db), arguments))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                // This binder shadows the target in its body, but not in its arguments.
                let body = match self.body(db) {
                    RecursiveBody::Type(body) if self.cycle(db) != substitution.cycle(db) => {
                        RecursiveBody::Type(body.apply_type_mapping_impl(db, mapping, tcx, visitor))
                    }
                    body => body,
                };
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(Self::new_internal(
                    db,
                    self.definition(db),
                    self.cycle(db),
                    body,
                    arguments,
                    self.materialization_kind(db),
                ))
            }
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::ApplySpecializationWithMaterialization { .. }
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(_)
            | TypeMapping::ReplaceSelf { .. } => {
                // These mappings substitute free variables, which are captured by the alias's
                // arguments. Its formal body must remain independent of the calling context.
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(self.with_arguments(db, arguments))
            }
            TypeMapping::Materialize(_) if self.materialization_kind(db).is_some() => {
                Type::Recursive(self)
            }
            TypeMapping::Materialize(kind) => {
                visitor.visit(db, Type::Recursive(self), mapping, || {
                    match self.unfold(db, visitor) {
                        UnfoldResult::Unfolded(unfolded) => {
                            let mapped =
                                unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                            // Preserve static aliases, including recursive references that the
                            // visitor leaves unchanged while materializing their enclosing body.
                            Type::Recursive(if mapped == unfolded {
                                self
                            } else {
                                self.with_materialization(db, Some(*kind))
                            })
                        }
                        // A deferred body cannot establish that materialization is a no-op.
                        // Retain the operation so it applies when the body becomes available.
                        UnfoldResult::Unchanged(_) => {
                            Type::Recursive(self.with_materialization(db, Some(*kind)))
                        }
                    }
                })
            }
            TypeMapping::EagerExpansion => {
                visitor.visit(db, Type::Recursive(self), mapping, || {
                    // Expand arguments only where the body exposes them. Expanding stored arguments
                    // first can feed a recursive alias's previous approximation into its own arguments.
                    self.unfold(db, visitor)
                        .map(|unfolded| {
                            let mapped =
                                unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                            if mapped == unfolded {
                                Type::Recursive(self)
                            } else {
                                mapped
                            }
                        })
                        .into_type()
                })
            }
            _ => visitor.visit(db, Type::Recursive(self), mapping, || {
                // Map arguments before unfolding so recursive backedges retain their mapped
                // arguments. Keep the application's materialization throughout the traversal.
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                let recursive = self.with_arguments(db, arguments);
                recursive
                    .unfold(db, visitor)
                    .map(|unfolded| {
                        let mapped = unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                        if mapped == unfolded {
                            Type::Recursive(recursive)
                        } else {
                            mapped
                        }
                    })
                    .into_type()
            }),
        }
    }

    /// Compose argument variance with the constructor's formal parameters instead of unfolding
    /// successively larger applications such as `Values[list[T]]`.
    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = self.environment(db);
        if let Some(arguments) = self.arguments(db) {
            let parameters = arguments.generic_context(db);
            let constructor = self.with_arguments(
                db,
                Some(
                    parameters
                        .identity_specialization(db)
                        .with_materialization_kind(db, arguments.materialization_kind(db)),
                ),
            );
            if constructor != self {
                let origin = VarianceOrigin::Recursive(constructor);
                let variances = parameters.variables(db).zip(arguments.types(db)).map(
                    |(parameter, argument)| {
                        argument
                            .variance_of(db, &env, typevar)
                            .compose_thunk(db, || {
                                VarianceTerm::variable(db, origin, parameter.identity(db))
                            })
                    },
                );
                // A constructor can also capture a type variable from an enclosing scope.
                let captured = (!parameters
                    .variables(db)
                    .any(|parameter| parameter.identity(db) == typevar))
                .then(|| VarianceTerm::variable(db, origin, typevar));
                return VarianceTerm::join(db, variances.chain(captured));
            }
        }
        self.unfold(db, &ApplyTypeMappingVisitor::new(&env))
            .map(|unfolded| unfolded.variance_of(db, &env, typevar))
            .unwrap_or(VarianceTerm::BIVARIANT)
    }
}

/// The outcome of unfolding one layer of a [`RecursiveType`].
///
/// Mapping transforms the unfolded value while retaining the original recursive type
/// when unfolding made no progress.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) enum UnfoldResult<'db, T = Type<'db>> {
    /// The unfolded type, or a value produced by mapping it.
    Unfolded(T),
    /// Unfolding returns the original recursive type, for example for `μa. a`.
    Unchanged(RecursiveType<'db>),
}

impl<'db> UnfoldResult<'db, Type<'db>> {
    /// Return the unfolded or mapped type, or the original recursive type if unfolding made no progress.
    ///
    /// Callers that recursively process the returned type must use their own recursion guards.
    /// Unfolding one layer does not eliminate cycles, even when it makes progress.
    #[inline]
    pub(super) fn into_type(self) -> Type<'db> {
        match self {
            Self::Unfolded(ty) => ty,
            Self::Unchanged(recursive) => Type::Recursive(recursive),
        }
    }
}

impl<'db, T> UnfoldResult<'db, T> {
    /// Return the contained value if unfolding made progress.
    #[inline]
    pub(crate) fn into_unfolded(self) -> Option<T> {
        match self {
            Self::Unfolded(value) => Some(value),
            Self::Unchanged(_) => None,
        }
    }

    /// Return whether unfolding made progress and the contained value satisfies `predicate`.
    #[inline]
    #[expect(
        clippy::wrong_self_convention,
        reason = "Like Option::is_some_and, the predicate consumes the contained value."
    )]
    pub(crate) fn is_unfolded_and(self, predicate: impl FnOnce(T) -> bool) -> bool {
        match self {
            Self::Unfolded(value) => predicate(value),
            Self::Unchanged(_) => false,
        }
    }

    /// Transform the contained value, preserving the original recursive type if unfolding made no progress.
    #[inline]
    pub(crate) fn map<U>(self, operation: impl FnOnce(T) -> U) -> UnfoldResult<'db, U> {
        match self {
            Self::Unfolded(value) => UnfoldResult::Unfolded(operation(value)),
            Self::Unchanged(recursive) => UnfoldResult::Unchanged(recursive),
        }
    }

    /// Return the contained value, or return `fallback` if unfolding made no progress.
    #[inline]
    pub(crate) fn unwrap_or(self, fallback: T) -> T {
        match self {
            Self::Unfolded(value) => value,
            Self::Unchanged(_) => fallback,
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Prove a relation between applications of the same recursive constructor
    /// (same unspecialized body) based on inclusion or overlap of their type
    /// arguments' materialization families.
    ///
    /// For example, every materialization of `R[int]` is also a materialization of
    /// `R[Any]`: choosing `int` restricts the possibilities for the argument without
    /// changing the constructor's body. Comparing arguments lets us establish
    /// relations between the applications' top and bottom materializations without
    /// unfolding their bodies, even when unfolding would keep growing the arguments.
    ///
    /// Argument comparison is sufficient but not necessary to establish the relation:
    /// different arguments can produce equivalent alias types. If this check cannot
    /// establish the relation, the caller can still unfold and compare the bodies.
    pub(super) fn when_recursive_types_relate_by_arguments(
        &self,
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if !matches!(
            self.relation,
            TypeRelation::Subtyping | TypeRelation::Assignability
        ) || source.constructor(db) != target.constructor(db)
        {
            return self.never();
        }
        let (Some(source_arguments), Some(target_arguments)) =
            (source.arguments(db), target.arguments(db))
        else {
            return self.never();
        };
        // This proof substitutes plain parameter values. Variadic packs and
        // materialized specializations carry additional substitution semantics.
        if source_arguments.generic_context(db) != target_arguments.generic_context(db)
            || source_arguments.materialization_kind(db).is_some()
            || target_arguments.materialization_kind(db).is_some()
            || source_arguments.tuple(db).is_some()
            || target_arguments.tuple(db).is_some()
        {
            return self.never();
        }
        let source_kind =
            source
                .materialization_kind(db)
                .unwrap_or(if self.relation.is_assignability() {
                    MaterializationKind::Bottom
                } else {
                    MaterializationKind::Top
                });
        let target_kind =
            target
                .materialization_kind(db)
                .unwrap_or(if self.relation.is_assignability() {
                    MaterializationKind::Top
                } else {
                    MaterializationKind::Bottom
                });
        // Top-to-bottom also requires a static body: equal arguments do not rule
        // out a fixed `Any`. Unchanged materializations retain the original binder.
        let unmaterialized_target = Type::Recursive(target.with_materialization(db, None));
        if matches!(
            (source_kind, target_kind),
            (MaterializationKind::Top, MaterializationKind::Bottom)
        ) && unmaterialized_target.materialize(
            db,
            MaterializationKind::Top,
            self.materialization_visitor,
        ) != unmaterialized_target.materialize(
            db,
            MaterializationKind::Bottom,
            self.materialization_visitor,
        ) {
            return self.never();
        }
        source_arguments
            .types(db)
            .iter()
            .zip(target_arguments.types(db))
            .when_all(db, self.constraints, |(source, target)| {
                self.check_subtyping_in_invariant_position(
                    db,
                    *source,
                    source_kind,
                    *target,
                    target_kind,
                )
            })
    }
}

impl<'db> VarianceInferable<'db> for RecursiveType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        VarianceTerm::variable(db, VarianceOrigin::Recursive(self), typevar)
    }
}

impl Type<'_> {
    /// Reject a bare recursive variable at a semantic-operation boundary.
    pub(super) const fn assert_not_recursive_var(self) {
        debug_assert!(
            !matches!(self, Self::RecursiveVar(_)),
            "semantic operation on an unbound recursive variable"
        );
    }
}

#[cfg(test)]
mod query_tests {
    use std::cell::{Cell, RefCell};

    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::name::Name;
    use rustc_hash::FxHashMap;
    use ty_python_core::ProgramFile;

    use super::{CycleKind, TypeInspector, TypeQuery, TypeQueryContext, TypeTraversal, TypeWalk};
    use crate::Db;
    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::cyclic::CycleDetector;
    use crate::types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, Type, TypeRecursionContext,
        TypeVarVariance,
    };

    /// Each projection exposes the same union, whose alternatives lead back to every projection.
    fn shared_recursive_values(db: &mut TestDb) -> anyhow::Result<File> {
        db.write_dedented(
            "/src/shared.py",
            r#"
            from typing import cast
            from typing_extensions import TypedDict
            from ty_extensions._internal import TypeOf

            class Edges(TypedDict, closed=True):
                leaf: int
                a: TypeOf[next(iter(cast("A", {}).values()))]
                b: TypeOf[next(iter(cast("B", {}).values()))]
                c: TypeOf[next(iter(cast("C", {}).values()))]

            class A(Edges): ...
            class B(Edges): ...
            class C(Edges): ...

            value = next(iter(cast(A, {}).values()))
            "#,
        )?;
        Ok(system_path_to_file(db, "/src/shared.py")?)
    }

    #[test]
    fn semantic_walks_visit_shared_recursive_alternatives_once() -> anyhow::Result<()> {
        struct CountAlternatives;

        impl<'db> TypeWalk<'db, &Cell<usize>> for CountAlternatives {
            type Output = Vec<Type<'db>>;

            fn visit(
                &self,
                db: &'db dyn Db,
                traversal: &TypeTraversal<'_, 'db, Self>,
                ty: Type<'db>,
                visits: &Cell<usize>,
            ) -> Self::Output {
                visits.set(visits.get() + 1);
                match ty {
                    Type::Union(union) => union
                        .elements(db)
                        .iter()
                        .flat_map(|ty| traversal.visit_with(db, *ty, visits))
                        .collect(),
                    _ => vec![ty],
                }
            }

            fn on_cycle(
                &self,
                _traversal: &TypeTraversal<'_, 'db, Self>,
                _ty: Type<'db>,
                _kind: CycleKind,
                _visits: &Cell<usize>,
            ) -> Self::Output {
                Vec::new()
            }
        }

        let mut db = setup_db();
        let file = shared_recursive_values(&mut db)?;
        let env = db.program_environment();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let ty = global_symbol(&db, file, "value").place.expect_type();
        assert!(
            ty.is_alias_like(),
            "the fixture must retain its recursive projection"
        );
        let visits = Cell::new(0);
        TypeTraversal::with_root(&env, CountAlternatives, |traversal| {
            assert_eq!(
                traversal.visit_with(&db, ty, &visits),
                [KnownClass::Int.to_instance(&db, &env)],
            );
        });
        assert_eq!(visits.get(), 1);
        Ok(())
    }

    #[test]
    fn pair_expansion_normalizes_either_operand() -> anyhow::Result<()> {
        let mut db = setup_db();
        let file = shared_recursive_values(&mut db)?;
        let env = db.program_environment();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let ty = global_symbol(&db, file, "value").place.expect_type();
        let int = KnownClass::Int.to_instance(&db, &env);
        let literal = Type::int_literal(1);
        let mapping = ApplyTypeMappingVisitor::new(&env);
        let pairs: CycleDetector<'_, (), (Type<'_>, Type<'_>), (), 3> = CycleDetector::new(());
        for ((left, right), expected) in [
            ((ty, literal), (int, literal)),
            ((literal, ty), (literal, int)),
        ] {
            let actual =
                pairs.visit_active_with_expansion(&db, (left, right), &mapping, |expansion| {
                    expansion.ok().map(|expansion| {
                        (
                            expansion.normalize(&db, left),
                            expansion.normalize(&db, right),
                        )
                    })
                });
            assert_eq!(actual, Some(expected));
        }
        Ok(())
    }

    #[test]
    fn normalization_distinguishes_collapsed_unions_from_single_aliases() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/aliases.py",
            r#"
            type First = int
            type Second = int

            single: First
            union: First | Second
            "#,
        )?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/aliases.py")?;
        let file = ProgramFile::new(&db, file, env.program(&db));
        for (name, was_union) in [("single", false), ("union", true)] {
            let ty = global_symbol(&db, file, name).place.expect_type();
            let normalized = TypeInspector::scope(&env, |inspector| {
                inspector.inspect_normalized(
                    &db,
                    ty,
                    |_, _| None,
                    |_, normalized| Some((normalized.ty, normalized.was_union)),
                )
            });
            assert_eq!(
                normalized,
                Some((KnownClass::Int.to_instance(&db, &env), was_union))
            );
        }
        Ok(())
    }

    struct SumGraph<'db> {
        edges: FxHashMap<Type<'db>, Vec<Type<'db>>>,
        visits: RefCell<FxHashMap<(Type<'db>, u64), usize>>,
    }

    impl<'db> TypeQuery<'db, u64> for SumGraph<'db> {
        type Output = u64;

        fn visit(
            &self,
            db: &'db dyn Db,
            context: &TypeQueryContext<'_, 'db, Self, u64>,
            ty: Type<'db>,
            value: u64,
        ) -> u64 {
            *self.visits.borrow_mut().entry((ty, value)).or_default() += 1;
            value
                + self
                    .edges
                    .get(&ty)
                    .into_iter()
                    .flatten()
                    .map(|child| context.visit_with(db, *child, value))
                    .sum::<u64>()
        }

        fn on_cycle(
            &self,
            _context: &TypeQueryContext<'_, 'db, Self, u64>,
            _ty: Type<'db>,
            _kind: CycleKind,
            _value: u64,
        ) -> u64 {
            0
        }
    }

    #[test]
    fn shared_queries_are_evaluated_once_per_input() {
        let db = setup_db();
        let env = db.program_environment();
        let mut edges = FxHashMap::default();
        let mut previous = [Type::int_literal(0), Type::int_literal(1)];
        // Each pair shares both nodes from the preceding level. Walking every path would take
        // millions of visits, although this graph contains only fifty distinct queries.
        for level in 1..=24 {
            let pair = [
                Type::int_literal(2 * level),
                Type::int_literal(2 * level + 1),
            ];
            for ty in pair {
                edges.insert(ty, previous.to_vec());
            }
            previous = pair;
        }
        TypeQueryContext::with_root(
            &env,
            SumGraph {
                edges,
                visits: RefCell::default(),
            },
            |context| {
                for value in [1, 2] {
                    for root in previous {
                        assert_eq!(
                            context.visit_with(&db, root, value),
                            ((1 << 25) - 1) * value,
                        );
                    }
                }
                let visits = context.traversal.operation.visits.borrow();
                assert_eq!(visits.len(), 100);
                assert!(visits.values().all(|count| *count == 1));
            },
        );
    }

    #[test]
    fn cycle_dependent_answers_are_recomputed_outside_the_cycle() {
        let db = setup_db();
        let env = db.program_environment();
        let [a, b] = ["A", "B"].map(|name| {
            Type::TypeVar(BoundTypeVarInstance::synthetic(
                &db,
                &env,
                Name::new(name),
                TypeVarVariance::Invariant,
            ))
        });
        TypeQueryContext::with_root(
            &env,
            SumGraph {
                edges: [(a, vec![b, Type::int_literal(0)]), (b, vec![a])]
                    .into_iter()
                    .collect(),
                visits: RefCell::default(),
            },
            |context| {
                // B contributes only itself while A is active. That partial answer cannot be
                // reused when B becomes the root and can reach A and the literal as well.
                assert_eq!(context.visit_with(&db, a, 1), 3);
                assert_eq!(context.visit_with(&db, b, 1), 3);
                assert_eq!(context.visit_with(&db, a, 1), 3);
            },
        );
    }

    #[test]
    fn cached_dependencies_survive_cache_hits_and_new_enclosing_guards() {
        let db = setup_db();
        let env = db.program_environment();
        let [a, b] = ["A", "B"].map(|name| {
            Type::TypeVar(BoundTypeVarInstance::synthetic(
                &db,
                &env,
                Name::new(name),
                TypeVarVariance::Invariant,
            ))
        });
        let c = Type::int_literal(0);
        TypeQueryContext::with_root(
            &env,
            SumGraph {
                edges: [(c, vec![b]), (b, vec![a])].into_iter().collect(),
                visits: RefCell::default(),
            },
            |context| {
                assert_eq!(context.visit_with(&db, b, 1), 2);
                assert_eq!(context.visit_with(&db, c, 1), 3);
                // C reused B's answer, but still depends on being able to enter A. Even though
                // that computation encountered no cycle, it is invalid while A is active.
                context.traversal.with_active_type(&db, a, |cycle| {
                    assert_eq!(cycle, None);
                    assert_eq!(context.visit_with(&db, c, 1), 2);
                });
                assert_eq!(context.visit_with(&db, c, 1), 3);
            },
        );
    }

    #[test]
    fn shared_guard_changes_invalidate_completed_queries() {
        struct HasSharedGuard<'a, 'db>(&'a TypeRecursionContext<'db>);

        impl<'db> TypeQuery<'db> for HasSharedGuard<'_, 'db> {
            type Output = bool;

            fn visit(
                &self,
                _db: &'db dyn Db,
                _context: &TypeQueryContext<'_, 'db, Self>,
                _ty: Type<'db>,
                (): (),
            ) -> bool {
                self.0.meta_type.is_active()
            }

            fn on_cycle(
                &self,
                _context: &TypeQueryContext<'_, 'db, Self>,
                _ty: Type<'db>,
                _kind: CycleKind,
                (): (),
            ) -> bool {
                false
            }
        }

        let db = setup_db();
        let env = db.program_environment();
        let shared = TypeRecursionContext::default();
        let typevar = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let key = (env.program(&db), typevar.identity(&db));
        TypeQueryContext::with_recursion_context(&env, HasSharedGuard(&shared), &shared, |query| {
            let ty = Type::int_literal(0);
            assert!(!query.visit(&db, ty));
            shared
                .meta_type
                .typevars
                .visit(&key, || (), || assert!(query.visit(&db, ty)));
            assert!(!query.visit(&db, ty));
        });
    }
}

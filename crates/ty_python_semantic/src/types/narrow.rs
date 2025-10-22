use crate::Db;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::place::{PlaceExpr, PlaceTable, ScopedPlaceId};
use crate::semantic_index::place_table;
use crate::semantic_index::predicate::{
    CallableAndCallExpr, ClassPatternKind, PatternPredicate, PatternPredicateKind, Predicate,
    PredicateNode,
};
use crate::semantic_index::scope::ScopeId;
use crate::types::enums::{enum_member_literals, enum_metadata};
use crate::types::function::KnownFunction;
use crate::types::infer::infer_same_file_expression_type;
use crate::types::{
    ClassLiteral, ClassType, IntersectionBuilder, KnownClass, SubclassOfInner, SubclassOfType,
    Truthiness, Type, TypeContext, TypeVarBoundOrConstraints, UnionBuilder, infer_expression_types,
};

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_stdlib::identifiers::is_identifier;

use itertools::Itertools;
use ruff_python_ast as ast;
use ruff_python_ast::{BoolOp, ExprBoolOp};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec};
use std::collections::hash_map::Entry;

use super::UnionType;

/// Return the type constraint that `test` (if true) would place on `symbol`, if any.
///
/// For example, if we have this code:
///
/// ```python
/// y = 1 if flag else None
/// x = 1 if flag else None
/// if x is not None:
///     ...
/// ```
///
/// The `test` expression `x is not None` places the constraint "not None" on the definition of
/// `x`, so in that case we'd return `Some(Type::Intersection(negative=[Type::None]))`.
///
/// But if we called this with the same `test` expression, but the `symbol` of `y`, no
/// constraint is applied to that symbol, so we'd just return `None`.
pub(crate) fn infer_narrowing_constraint<'db>(
    db: &'db dyn Db,
    predicate: Predicate<'db>,
    place: ScopedPlaceId,
) -> Option<Type<'db>> {
    let constraints = match predicate.node {
        PredicateNode::Expression(expression) => {
            if predicate.is_positive {
                all_narrowing_constraints_for_expression(db, expression)
            } else {
                all_negative_narrowing_constraints_for_expression(db, expression)
            }
        }
        PredicateNode::Pattern(pattern) => {
            if predicate.is_positive {
                all_narrowing_constraints_for_pattern(db, pattern)
            } else {
                all_negative_narrowing_constraints_for_pattern(db, pattern)
            }
        }
        PredicateNode::ReturnsNever(_) => return None,
        PredicateNode::StarImportPlaceholder(_) => return None,
    };
    if let Some(constraints) = constraints {
        constraints.get(&place).copied()
    } else {
        None
    }
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn all_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternPredicate<'db>,
) -> Option<NarrowingConstraints<'db>> {
    let module = parsed_module(db, pattern.file(db)).load(db);
    NarrowingConstraintsBuilder::new(db, &module, PredicateNode::Pattern(pattern), true).finish()
}

#[salsa::tracked(
    returns(as_ref),
    cycle_fn=constraints_for_expression_cycle_recover,
    cycle_initial=constraints_for_expression_cycle_initial,
    heap_size=ruff_memory_usage::heap_size,
)]
fn all_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    let module = parsed_module(db, expression.file(db)).load(db);
    NarrowingConstraintsBuilder::new(db, &module, PredicateNode::Expression(expression), true)
        .finish()
}

#[salsa::tracked(
    returns(as_ref),
    cycle_fn=negative_constraints_for_expression_cycle_recover,
    cycle_initial=negative_constraints_for_expression_cycle_initial,
    heap_size=ruff_memory_usage::heap_size,
)]
fn all_negative_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    let module = parsed_module(db, expression.file(db)).load(db);
    NarrowingConstraintsBuilder::new(db, &module, PredicateNode::Expression(expression), false)
        .finish()
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn all_negative_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternPredicate<'db>,
) -> Option<NarrowingConstraints<'db>> {
    let module = parsed_module(db, pattern.file(db)).load(db);
    NarrowingConstraintsBuilder::new(db, &module, PredicateNode::Pattern(pattern), false).finish()
}

#[expect(clippy::ref_option)]
fn constraints_for_expression_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Option<NarrowingConstraints<'db>>,
    _count: u32,
    _expression: Expression<'db>,
) -> salsa::CycleRecoveryAction<Option<NarrowingConstraints<'db>>> {
    salsa::CycleRecoveryAction::Iterate
}

fn constraints_for_expression_cycle_initial<'db>(
    _db: &'db dyn Db,
    _expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    None
}

#[expect(clippy::ref_option)]
fn negative_constraints_for_expression_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Option<NarrowingConstraints<'db>>,
    _count: u32,
    _expression: Expression<'db>,
) -> salsa::CycleRecoveryAction<Option<NarrowingConstraints<'db>>> {
    salsa::CycleRecoveryAction::Iterate
}

fn negative_constraints_for_expression_cycle_initial<'db>(
    _db: &'db dyn Db,
    _expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    None
}

/// Functions that can be used to narrow the type of a first argument using a "classinfo" second argument.
///
/// A "classinfo" argument is either a class or a tuple of classes, or a tuple of tuples of classes
/// (etc. for arbitrary levels of recursion)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassInfoConstraintFunction {
    /// `builtins.isinstance`
    IsInstance,
    /// `builtins.issubclass`
    IsSubclass,
}

impl ClassInfoConstraintFunction {
    /// Generate a constraint from the type of a `classinfo` argument to `isinstance` or `issubclass`.
    ///
    /// The `classinfo` argument can be a class literal, a tuple of (tuples of) class literals. PEP 604
    /// union types are not yet supported. Returns `None` if the `classinfo` argument has a wrong type.
    fn generate_constraint<'db>(self, db: &'db dyn Db, classinfo: Type<'db>) -> Option<Type<'db>> {
        let constraint_fn = |class: ClassLiteral<'db>| match self {
            ClassInfoConstraintFunction::IsInstance => {
                Type::instance(db, class.top_materialization(db))
            }
            ClassInfoConstraintFunction::IsSubclass => {
                SubclassOfType::from(db, class.top_materialization(db))
            }
        };

        match classinfo {
            Type::TypeAlias(alias) => self.generate_constraint(db, alias.value_type(db)),
            Type::ClassLiteral(class_literal) => Some(constraint_fn(class_literal)),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(ClassType::NonGeneric(class)) => Some(constraint_fn(class)),
                // It's not valid to use a generic alias as the second argument to `isinstance()` or `issubclass()`,
                // e.g. `isinstance(x, list[int])` fails at runtime.
                SubclassOfInner::Class(ClassType::Generic(_)) => None,
                SubclassOfInner::Dynamic(dynamic) => Some(Type::Dynamic(dynamic)),
            },
            Type::Dynamic(_) => Some(classinfo),
            Type::Intersection(intersection) => {
                if intersection.negative(db).is_empty() {
                    let mut builder = IntersectionBuilder::new(db);
                    for element in intersection.positive(db) {
                        builder = builder.add_positive(self.generate_constraint(db, *element)?);
                    }
                    Some(builder.build())
                } else {
                    // TODO: can we do better here?
                    None
                }
            }
            Type::Union(union) => {
                union.try_map(db, |element| self.generate_constraint(db, *element))
            }
            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db)? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        self.generate_constraint(db, bound)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        self.generate_constraint(db, Type::Union(constraints))
                    }
                }
            }

            // It's not valid to use a generic alias as the second argument to `isinstance()` or `issubclass()`,
            // e.g. `isinstance(x, list[int])` fails at runtime.
            Type::GenericAlias(_) => None,

            Type::NominalInstance(nominal) => nominal.tuple_spec(db).and_then(|tuple| {
                UnionType::try_from_elements(
                    db,
                    tuple
                        .all_elements()
                        .map(|element| self.generate_constraint(db, *element)),
                )
            }),

            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BooleanLiteral(_)
            | Type::BoundMethod(_)
            | Type::BoundSuper(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::Callable(_)
            | Type::DataclassDecorator(_)
            | Type::Never
            | Type::KnownBoundMethod(_)
            | Type::ModuleLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::ProtocolInstance(_)
            | Type::PropertyInstance(_)
            | Type::SpecialForm(_)
            | Type::LiteralString
            | Type::StringLiteral(_)
            | Type::IntLiteral(_)
            | Type::KnownInstance(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassTransformer(_)
            | Type::TypedDict(_) => None,
        }
    }
}

/// Represents a single conjunction (AND) of constraints in Disjunctive Normal
/// Form (DNF).
///
/// A conjunction may contain: - A regular constraint (intersection of types) -
/// An optional `TypeGuard` constraint that "replaces" the type rather than
/// intersecting
///
/// For example, `(Conjunction { constraint: A, typeguard: Some(B) } &
/// Conjunction { constraint: C, typeguard: Some(D)})` evlaluates to
/// `Conjunction { constraint: C, typeguard: Some(D) }` because the type guard
/// in the second clobbers the first.
#[derive(Hash, PartialEq, Debug, Eq, Clone, Copy)]
struct Conjunction<'db> {
    /// The intersected constraints (represented as an intersection type)
    constraint: Type<'db>,
    /// If any constraint in this conjunction is a `TypeGuard`, this is Some and
    /// contains the union of all `TypeGuard` types in this conjunction
    typeguard: Option<Type<'db>>,
}

impl get_size2::GetSize for Conjunction<'_> {}

impl<'db> Conjunction<'db> {
    /// Create a new conjunction with just a regular constraint
    fn regular(constraint: Type<'db>) -> Self {
        Self {
            constraint,
            typeguard: None,
        }
    }

    /// Create a new conjunction with a `TypeGuard` constraint
    fn typeguard(constraint: Type<'db>) -> Self {
        Self {
            constraint: Type::object(),
            typeguard: Some(constraint),
        }
    }

    /// Evaluate this conjunction to a single type.
    /// If there's a `TypeGuard` constraint, it replaces the regular constraint.
    /// Otherwise, returns the regular constraint.
    fn evaluate_type_constraint(self) -> Type<'db> {
        self.typeguard.unwrap_or(self.constraint)
    }
}

/// Represents narrowing constraints in Disjunctive Normal Form (DNF).
///
/// This is a disjunction (OR) of conjunctions (AND) of constraints.
/// The DNF representation allows us to properly track `TypeGuard` constraints
/// through boolean operations.
///
/// For example:
/// - `f(x) and g(x)` where f returns `TypeIs[A]` and g returns `TypeGuard[B]`
///   => `[Conjunction { constraint: A, typeguard: Some(B) }]`
///   => evaluates to `B` (`TypeGuard` clobbers)
///
/// - `f(x) or g(x)` where f returns `TypeIs[A]` and g returns `TypeGuard[B]`
///   => `[Conjunction { constraint: A, typeguard: None }, Conjunction { constraint: object, typeguard: Some(B) }]`
///   => evaluates to `A | B`
#[derive(Hash, PartialEq, Debug, Eq, Clone)]
struct NarrowingConstraint<'db> {
    /// Disjunctions of conjunctions (DNF)
    disjuncts: SmallVec<[Conjunction<'db>; 4]>,
}

impl get_size2::GetSize for NarrowingConstraint<'_> {}

impl<'db> NarrowingConstraint<'db> {
    /// Create a constraint from a regular (non-`TypeGuard`) type
    fn regular(constraint: Type<'db>) -> Self {
        Self {
            disjuncts: smallvec![Conjunction::regular(constraint)],
        }
    }

    /// Create a constraint from a `TypeGuard` type
    fn typeguard(constraint: Type<'db>) -> Self {
        Self {
            disjuncts: smallvec![Conjunction::typeguard(constraint)],
        }
    }

    /// Evaluate the type this effectively constrains to
    ///
    /// Forgets whether each constraint originated from a `TypeGuard` or not
    fn evaluate_type_constraint(self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(
            db,
            self.disjuncts
                .into_iter()
                .map(Conjunction::evaluate_type_constraint),
        )
    }
}

impl<'db> From<Type<'db>> for NarrowingConstraint<'db> {
    fn from(constraint: Type<'db>) -> Self {
        Self::regular(constraint)
    }
}

/// Internal representation of constraints with DNF structure for tracking `TypeGuard` semantics.
///
/// This is a newtype wrapper around `FxHashMap<ScopedPlaceId, NarrowingConstraint<'db>>` that
/// provides methods for working with constraints during boolean operation evaluation.
#[derive(Clone, Debug, Default)]
struct InternalConstraints<'db> {
    constraints: FxHashMap<ScopedPlaceId, NarrowingConstraint<'db>>,
}

impl<'db> InternalConstraints<'db> {
    /// Insert a regular (non-`TypeGuard`) constraint for a place
    fn insert_regular(&mut self, place: ScopedPlaceId, ty: Type<'db>) {
        self.constraints
            .insert(place, NarrowingConstraint::regular(ty));
    }

    /// Convert internal constraints to public constraints by evaluating each DNF constraint to a Type
    fn evaluate_type_constraints(self, db: &'db dyn Db) -> NarrowingConstraints<'db> {
        self.constraints
            .into_iter()
            .map(|(place, constraint)| (place, constraint.evaluate_type_constraint(db)))
            .collect()
    }
}

impl<'db> FromIterator<(ScopedPlaceId, NarrowingConstraint<'db>)> for InternalConstraints<'db> {
    fn from_iter<T: IntoIterator<Item = (ScopedPlaceId, NarrowingConstraint<'db>)>>(
        iter: T,
    ) -> Self {
        Self {
            constraints: FxHashMap::from_iter(iter),
        }
    }
}

/// Public representation of constraints as returned by tracked functions
type NarrowingConstraints<'db> = FxHashMap<ScopedPlaceId, Type<'db>>;

/// Merge constraints with AND semantics (intersection/conjunction).
///
/// When we have `constraint1 & constraint2`, we need to distribute AND over the OR
/// in the DNF representations:
/// `(A | B) & (C | D)` becomes `(A & C) | (A & D) | (B & C) | (B & D)`
///
/// For each conjunction pair, we:
/// - Take the right conjunct if it has a `TypeGuard`
/// - Intersect the constraints normally otherwise
fn merge_constraints_and<'db>(
    into: &mut InternalConstraints<'db>,
    from: &InternalConstraints<'db>,
    db: &'db dyn Db,
) {
    for (key, from_constraint) in &from.constraints {
        match into.constraints.entry(*key) {
            Entry::Occupied(mut entry) => {
                let into_constraint = entry.get();

                // Distribute AND over OR: (A1 | A2 | ...) AND (B1 | B2 | ...)
                // becomes (A1 & B1) | (A1 & B2) | ... | (A2 & B1) | ...
                let mut new_disjuncts = SmallVec::new();

                for left_conj in &into_constraint.disjuncts {
                    for right_conj in &from_constraint.disjuncts {
                        if right_conj.typeguard.is_some() {
                            // If the right conjunct has a TypeGuard, it "wins" the conjunction
                            new_disjuncts.push(*right_conj);
                        } else {
                            // Intersect the regular constraints
                            let new_regular = IntersectionBuilder::new(db)
                                .add_positive(left_conj.constraint)
                                .add_positive(right_conj.constraint)
                                .build();

                            new_disjuncts.push(Conjunction {
                                constraint: new_regular,
                                typeguard: left_conj.typeguard,
                            });
                        }
                    }
                }

                *entry.get_mut() = NarrowingConstraint {
                    disjuncts: new_disjuncts,
                };
            }
            Entry::Vacant(entry) => {
                entry.insert(from_constraint.clone());
            }
        }
    }
}

/// Merge constraints with OR semantics (union/disjunction).
///
/// When we have `constraint1 OR constraint2`, we simply concatenate the disjuncts
/// from both constraints: `(A | B) OR (C | D)` becomes `A | B | C | D`
///
/// However, if a place appears in only one branch of the OR, we need to widen it
/// to `object` in the overall result (because the other branch doesn't constrain it).
fn merge_constraints_or<'db>(
    into: &mut InternalConstraints<'db>,
    from: &InternalConstraints<'db>,
    _db: &'db dyn Db,
) {
    // For places that appear in `into` but not in `from`, widen to object
    for (key, value) in &mut into.constraints {
        if !from.constraints.contains_key(key) {
            *value = NarrowingConstraint::regular(Type::object());
        }
    }

    for (key, from_constraint) in &from.constraints {
        match into.constraints.entry(*key) {
            Entry::Occupied(mut entry) => {
                // Simply concatenate the disjuncts
                entry
                    .get_mut()
                    .disjuncts
                    .extend(from_constraint.disjuncts.clone());
            }
            Entry::Vacant(entry) => {
                // Place only appears in `from`, not in `into`.
                // Widen to object since the other branch doesn't constrain it.
                entry.insert(NarrowingConstraint::regular(Type::object()));
            }
        }
    }
}

fn place_expr(expr: &ast::Expr) -> Option<PlaceExpr> {
    match expr {
        ast::Expr::Named(named) => PlaceExpr::try_from_expr(named.target.as_ref()),
        _ => PlaceExpr::try_from_expr(expr),
    }
}

struct NarrowingConstraintsBuilder<'db, 'ast> {
    db: &'db dyn Db,
    module: &'ast ParsedModuleRef,
    predicate: PredicateNode<'db>,
    is_positive: bool,
}

impl<'db, 'ast> NarrowingConstraintsBuilder<'db, 'ast> {
    fn new(
        db: &'db dyn Db,
        module: &'ast ParsedModuleRef,
        predicate: PredicateNode<'db>,
        is_positive: bool,
    ) -> Self {
        Self {
            db,
            module,
            predicate,
            is_positive,
        }
    }

    fn finish(mut self) -> Option<NarrowingConstraints<'db>> {
        let constraints: Option<InternalConstraints<'db>> = match self.predicate {
            PredicateNode::Expression(expression) => {
                self.evaluate_expression_predicate(expression, self.is_positive)
            }
            PredicateNode::Pattern(pattern) => {
                self.evaluate_pattern_predicate(pattern, self.is_positive)
            }
            PredicateNode::ReturnsNever(_) => return None,
            PredicateNode::StarImportPlaceholder(_) => return None,
        };
        if let Some(mut constraints) = constraints {
            constraints.constraints.shrink_to_fit();
            Some(constraints.evaluate_type_constraints(self.db))
        } else {
            None
        }
    }

    fn evaluate_expression_predicate(
        &mut self,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let expression_node = expression.node_ref(self.db, self.module);
        self.evaluate_expression_node_predicate(expression_node, expression, is_positive)
    }

    fn evaluate_expression_node_predicate(
        &mut self,
        expression_node: &ruff_python_ast::Expr,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        match expression_node {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                self.evaluate_simple_expr(expression_node, is_positive)
            }
            ast::Expr::Compare(expr_compare) => {
                self.evaluate_expr_compare(expr_compare, expression, is_positive)
            }
            ast::Expr::Call(expr_call) => {
                self.evaluate_expr_call(expr_call, expression, is_positive)
            }
            ast::Expr::UnaryOp(unary_op) if unary_op.op == ast::UnaryOp::Not => {
                self.evaluate_expression_node_predicate(&unary_op.operand, expression, !is_positive)
            }
            ast::Expr::BoolOp(bool_op) => self.evaluate_bool_op(bool_op, expression, is_positive),
            ast::Expr::Named(expr_named) => self.evaluate_expr_named(expr_named, is_positive),
            _ => None,
        }
    }

    fn evaluate_pattern_predicate_kind(
        &mut self,
        pattern_predicate_kind: &PatternPredicateKind<'db>,
        subject: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        match pattern_predicate_kind {
            PatternPredicateKind::Singleton(singleton) => {
                self.evaluate_match_pattern_singleton(subject, *singleton, is_positive)
            }
            PatternPredicateKind::Class(cls, kind) => {
                self.evaluate_match_pattern_class(subject, *cls, *kind, is_positive)
            }
            PatternPredicateKind::Value(expr) => {
                self.evaluate_match_pattern_value(subject, *expr, is_positive)
            }
            PatternPredicateKind::Or(predicates) => {
                self.evaluate_match_pattern_or(subject, predicates, is_positive)
            }
            PatternPredicateKind::As(pattern, _) => pattern
                .as_deref()
                .and_then(|p| self.evaluate_pattern_predicate_kind(p, subject, is_positive)),
            PatternPredicateKind::Unsupported => None,
        }
    }

    fn evaluate_pattern_predicate(
        &mut self,
        pattern: PatternPredicate<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        self.evaluate_pattern_predicate_kind(
            pattern.kind(self.db),
            pattern.subject(self.db),
            is_positive,
        )
    }

    fn places(&self) -> &'db PlaceTable {
        place_table(self.db, self.scope())
    }

    fn scope(&self) -> ScopeId<'db> {
        match self.predicate {
            PredicateNode::Expression(expression) => expression.scope(self.db),
            PredicateNode::Pattern(pattern) => pattern.scope(self.db),
            PredicateNode::ReturnsNever(CallableAndCallExpr { callable, .. }) => {
                callable.scope(self.db)
            }
            PredicateNode::StarImportPlaceholder(definition) => definition.scope(self.db),
        }
    }

    #[track_caller]
    fn expect_place(&self, place_expr: &PlaceExpr) -> ScopedPlaceId {
        self.places()
            .place_id(place_expr)
            .expect("We should always have a place for every `PlaceExpr`")
    }

    fn evaluate_simple_expr(
        &mut self,
        expr: &ast::Expr,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let target = place_expr(expr)?;
        let place = self.expect_place(&target);

        let ty = if is_positive {
            Type::AlwaysFalsy.negate(self.db)
        } else {
            Type::AlwaysTruthy.negate(self.db)
        };

        Some(InternalConstraints::from_iter([(
            place,
            NarrowingConstraint::regular(ty),
        )]))
    }

    fn evaluate_expr_named(
        &mut self,
        expr_named: &ast::ExprNamed,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        self.evaluate_simple_expr(&expr_named.target, is_positive)
    }

    fn evaluate_expr_eq(&mut self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        // We can only narrow on equality checks against single-valued types.
        if rhs_ty.is_single_valued(self.db) || rhs_ty.is_union_of_single_valued(self.db) {
            // The fully-general (and more efficient) approach here would be to introduce a
            // `NeverEqualTo` type that can wrap a single-valued type, and then simply return
            // `~NeverEqualTo(rhs_ty)` here and let union/intersection builder sort it out. This is
            // how we handle `AlwaysTruthy` and `AlwaysFalsy`. But this means we have to deal with
            // this type everywhere, and possibly have it show up unsimplified in some cases, and
            // so we instead prefer to just do the simplification here. (Another hybrid option that
            // would be similar to this, but more efficient, would be to allow narrowing to return
            // something that is not a type, and handle this not-a-type in `symbol_from_bindings`,
            // instead of intersecting with a type.)

            // Return `true` if it is possible for any two inhabitants of the given types to
            // compare equal to each other; otherwise return `false`.
            fn could_compare_equal<'db>(
                db: &'db dyn Db,
                left_ty: Type<'db>,
                right_ty: Type<'db>,
            ) -> bool {
                if !left_ty.is_disjoint_from(db, right_ty) {
                    // If types overlap, they have inhabitants in common; it's definitely possible
                    // for an object to compare equal to itself.
                    return true;
                }
                match (left_ty, right_ty) {
                    // In order to be sure a union type cannot compare equal to another type, it
                    // must be true that no element of the union can compare equal to that type.
                    (Type::Union(union), _) => union
                        .elements(db)
                        .iter()
                        .any(|ty| could_compare_equal(db, *ty, right_ty)),
                    (_, Type::Union(union)) => union
                        .elements(db)
                        .iter()
                        .any(|ty| could_compare_equal(db, left_ty, *ty)),
                    // Boolean literals and int literals are disjoint, and single valued, and yet
                    // `True == 1` and `False == 0`.
                    (Type::BooleanLiteral(b), Type::IntLiteral(i))
                    | (Type::IntLiteral(i), Type::BooleanLiteral(b)) => i64::from(b) == i,
                    // Other than the above cases, two single-valued disjoint types cannot compare
                    // equal.
                    _ => !(left_ty.is_single_valued(db) && right_ty.is_single_valued(db)),
                }
            }

            // Return `true` if `lhs_ty` consists only of `LiteralString` and types that cannot
            // compare equal to `rhs_ty`.
            fn can_narrow_to_rhs<'db>(
                db: &'db dyn Db,
                lhs_ty: Type<'db>,
                rhs_ty: Type<'db>,
            ) -> bool {
                match lhs_ty {
                    Type::Union(union) => union
                        .elements(db)
                        .iter()
                        .all(|ty| can_narrow_to_rhs(db, *ty, rhs_ty)),
                    // Either `rhs_ty` is a string literal, in which case we can narrow to it (no
                    // other string literal could compare equal to it), or it is not a string
                    // literal, in which case (given that it is single-valued), LiteralString
                    // cannot compare equal to it.
                    Type::LiteralString => true,
                    _ => !could_compare_equal(db, lhs_ty, rhs_ty),
                }
            }

            // Filter `ty` to just the types that cannot be equal to `rhs_ty`.
            fn filter_to_cannot_be_equal<'db>(
                db: &'db dyn Db,
                ty: Type<'db>,
                rhs_ty: Type<'db>,
            ) -> Type<'db> {
                match ty {
                    Type::Union(union) => {
                        union.map(db, |ty| filter_to_cannot_be_equal(db, *ty, rhs_ty))
                    }
                    // Treat `bool` as `Literal[True, False]`.
                    Type::NominalInstance(instance)
                        if instance.has_known_class(db, KnownClass::Bool) =>
                    {
                        UnionType::from_elements(
                            db,
                            [Type::BooleanLiteral(true), Type::BooleanLiteral(false)]
                                .into_iter()
                                .map(|ty| filter_to_cannot_be_equal(db, ty, rhs_ty)),
                        )
                    }
                    // Treat enums as a union of their members.
                    Type::NominalInstance(instance)
                        if enum_metadata(db, instance.class_literal(db)).is_some() =>
                    {
                        UnionType::from_elements(
                            db,
                            enum_member_literals(db, instance.class_literal(db), None)
                                .expect("Calling `enum_member_literals` on an enum class")
                                .map(|ty| filter_to_cannot_be_equal(db, ty, rhs_ty)),
                        )
                    }
                    _ => {
                        if ty.is_single_valued(db) && !could_compare_equal(db, ty, rhs_ty) {
                            ty
                        } else {
                            Type::Never
                        }
                    }
                }
            }
            Some(if can_narrow_to_rhs(self.db, lhs_ty, rhs_ty) {
                rhs_ty
            } else {
                filter_to_cannot_be_equal(self.db, lhs_ty, rhs_ty).negate(self.db)
            })
        } else {
            None
        }
    }

    fn evaluate_expr_ne(&mut self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        match (lhs_ty, rhs_ty) {
            (Type::NominalInstance(instance), Type::IntLiteral(i))
                if instance.has_known_class(self.db, KnownClass::Bool) =>
            {
                if i == 0 {
                    Some(Type::BooleanLiteral(false).negate(self.db))
                } else if i == 1 {
                    Some(Type::BooleanLiteral(true).negate(self.db))
                } else {
                    None
                }
            }
            (_, Type::BooleanLiteral(b)) => Some(
                UnionType::from_elements(self.db, [rhs_ty, Type::IntLiteral(i64::from(b))])
                    .negate(self.db),
            ),
            _ if rhs_ty.is_single_valued(self.db) => Some(rhs_ty.negate(self.db)),
            _ => None,
        }
    }

    // TODO `expr_in` and `expr_not_in` should perhaps be unified with `expr_eq` and `expr_ne`,
    // since `eq` and `ne` are equivalent to `in` and `not in` with only one element in the RHS.
    fn evaluate_expr_in(&mut self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        if lhs_ty.is_single_valued(self.db) || lhs_ty.is_union_of_single_valued(self.db) {
            rhs_ty
                .try_iterate(self.db)
                .ok()
                .map(|iterable| iterable.homogeneous_element_type(self.db))
        } else if lhs_ty.is_union_with_single_valued(self.db) {
            let rhs_values = rhs_ty
                .try_iterate(self.db)
                .ok()?
                .homogeneous_element_type(self.db);

            let mut builder = UnionBuilder::new(self.db);

            // Add the narrowed values from the RHS first, to keep literals before broader types.
            builder = builder.add(rhs_values);

            if let Some(lhs_union) = lhs_ty.as_union() {
                for element in lhs_union.elements(self.db) {
                    // Keep only the non-single-valued portion of the original type.
                    if !element.is_single_valued(self.db)
                        && !element.is_literal_string()
                        && !element.is_bool(self.db)
                        && (!element.is_enum(self.db) || element.overrides_equality(self.db))
                    {
                        builder = builder.add(*element);
                    }
                }
            }
            Some(builder.build())
        } else {
            None
        }
    }

    fn evaluate_expr_not_in(&mut self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        let rhs_values = rhs_ty
            .try_iterate(self.db)
            .ok()?
            .homogeneous_element_type(self.db);

        if lhs_ty.is_single_valued(self.db) || lhs_ty.is_union_of_single_valued(self.db) {
            // Exclude the RHS values from the entire (single-valued) LHS domain.
            let complement = IntersectionBuilder::new(self.db)
                .add_positive(lhs_ty)
                .add_negative(rhs_values)
                .build();
            Some(complement)
        } else if lhs_ty.is_union_with_single_valued(self.db) {
            // Split LHS into single-valued portion and the rest. Exclude RHS values from the
            // single-valued portion, keep the rest intact.
            let mut single_builder = UnionBuilder::new(self.db);
            let mut rest_builder = UnionBuilder::new(self.db);

            if let Some(lhs_union) = lhs_ty.as_union() {
                for element in lhs_union.elements(self.db) {
                    if element.is_single_valued(self.db)
                        || element.is_literal_string()
                        || element.is_bool(self.db)
                        || (element.is_enum(self.db) && !element.overrides_equality(self.db))
                    {
                        single_builder = single_builder.add(*element);
                    } else {
                        rest_builder = rest_builder.add(*element);
                    }
                }
            }

            let single_union = single_builder.build();
            let rest_union = rest_builder.build();

            let narrowed_single = IntersectionBuilder::new(self.db)
                .add_positive(single_union)
                .add_negative(rhs_values)
                .build();

            // Keep order: first literal complement, then broader arms.
            let result = UnionBuilder::new(self.db)
                .add(narrowed_single)
                .add(rest_union)
                .build();
            Some(result)
        } else {
            None
        }
    }

    fn evaluate_expr_compare_op(
        &mut self,
        lhs_ty: Type<'db>,
        rhs_ty: Type<'db>,
        op: ast::CmpOp,
        is_positive: bool,
    ) -> Option<Type<'db>> {
        let op = if is_positive { op } else { op.negate() };

        match op {
            ast::CmpOp::IsNot => {
                if rhs_ty.is_singleton(self.db) {
                    let ty = IntersectionBuilder::new(self.db)
                        .add_negative(rhs_ty)
                        .build();
                    Some(ty)
                } else {
                    // Non-singletons cannot be safely narrowed using `is not`
                    None
                }
            }
            ast::CmpOp::Is => Some(rhs_ty),
            ast::CmpOp::Eq => self.evaluate_expr_eq(lhs_ty, rhs_ty),
            ast::CmpOp::NotEq => self.evaluate_expr_ne(lhs_ty, rhs_ty),
            ast::CmpOp::In => self.evaluate_expr_in(lhs_ty, rhs_ty),
            ast::CmpOp::NotIn => self.evaluate_expr_not_in(lhs_ty, rhs_ty),
            _ => None,
        }
    }

    fn evaluate_expr_compare(
        &mut self,
        expr_compare: &ast::ExprCompare,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        fn is_narrowing_target_candidate(expr: &ast::Expr) -> bool {
            matches!(
                expr,
                ast::Expr::Name(_)
                    | ast::Expr::Attribute(_)
                    | ast::Expr::Subscript(_)
                    | ast::Expr::Call(_)
                    | ast::Expr::Named(_)
            )
        }

        let ast::ExprCompare {
            range: _,
            node_index: _,
            left,
            ops,
            comparators,
        } = expr_compare;

        // Performance optimization: early return if there are no potential narrowing targets.
        if !is_narrowing_target_candidate(left)
            && comparators
                .iter()
                .all(|c| !is_narrowing_target_candidate(c))
        {
            return None;
        }

        if !is_positive && comparators.len() > 1 {
            // We can't negate a constraint made by a multi-comparator expression, since we can't
            // know which comparison part is the one being negated.
            // For example, the negation of  `x is 1 is y is 2`, would be `(x is not 1) or (y is not 1) or (y is not 2)`
            // and that requires cross-symbol constraints, which we don't support yet.
            return None;
        }

        let inference = infer_expression_types(self.db, expression, TypeContext::default());

        let comparator_tuples = std::iter::once(&**left)
            .chain(comparators)
            .tuple_windows::<(&ruff_python_ast::Expr, &ruff_python_ast::Expr)>();
        let mut constraints = InternalConstraints::default();

        let mut last_rhs_ty: Option<Type> = None;

        for (op, (left, right)) in std::iter::zip(&**ops, comparator_tuples) {
            let lhs_ty = last_rhs_ty.unwrap_or_else(|| inference.expression_type(left));
            let rhs_ty = inference.expression_type(right);
            last_rhs_ty = Some(rhs_ty);

            match left {
                ast::Expr::Name(_)
                | ast::Expr::Attribute(_)
                | ast::Expr::Subscript(_)
                | ast::Expr::Named(_) => {
                    if let Some(left) = place_expr(left)
                        && let Some(ty) =
                            self.evaluate_expr_compare_op(lhs_ty, rhs_ty, *op, is_positive)
                    {
                        let place = self.expect_place(&left);
                        constraints.insert_regular(place, ty);
                    }
                }
                ast::Expr::Call(ast::ExprCall {
                    range: _,
                    node_index: _,
                    func: callable,
                    arguments:
                        ast::Arguments {
                            args,
                            keywords,
                            range: _,
                            node_index: _,
                        },
                }) if keywords.is_empty() => {
                    let Type::ClassLiteral(rhs_class) = rhs_ty else {
                        continue;
                    };

                    let target = match &**args {
                        [first] => match place_expr(first) {
                            Some(target) => target,
                            None => continue,
                        },
                        _ => continue,
                    };

                    let is_positive = if is_positive {
                        op == &ast::CmpOp::Is
                    } else {
                        op == &ast::CmpOp::IsNot
                    };

                    // `else`-branch narrowing for `if type(x) is Y` can only be done
                    // if `Y` is a final class
                    if !rhs_class.is_final(self.db) && !is_positive {
                        continue;
                    }

                    let callable_type = inference.expression_type(&**callable);

                    if callable_type
                        .as_class_literal()
                        .is_some_and(|c| c.is_known(self.db, KnownClass::Type))
                    {
                        let place = self.expect_place(&target);
                        constraints.insert_regular(
                            place,
                            Type::instance(self.db, rhs_class.unknown_specialization(self.db))
                                .negate_if(self.db, !is_positive),
                        );
                    }
                }
                _ => {}
            }
        }
        Some(constraints)
    }

    fn evaluate_expr_call(
        &mut self,
        expr_call: &ast::ExprCall,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let inference = infer_expression_types(self.db, expression, TypeContext::default());

        let callable_ty = inference.expression_type(&*expr_call.func);

        // TODO: add support for PEP 604 union types on the right hand side of `isinstance`
        // and `issubclass`, for example `isinstance(x, str | (int | float))`.
        match callable_ty {
            Type::FunctionLiteral(function_type)
                if matches!(
                    function_type.known(self.db),
                    None | Some(KnownFunction::RevealType)
                ) =>
            {
                let return_ty = inference.expression_type(expr_call);

                let place_and_constraint = match return_ty {
                    Type::TypeIs(type_is) => {
                        let (_, place) = type_is.place_info(self.db)?;
                        Some((
                            place,
                            NarrowingConstraint::regular(
                                type_is
                                    .return_type(self.db)
                                    .negate_if(self.db, !is_positive),
                            ),
                        ))
                    }
                    // TypeGuard only narrows in the positive case
                    Type::TypeGuard(type_guard) if is_positive => {
                        let (_, place) = type_guard.place_info(self.db)?;
                        Some((
                            place,
                            NarrowingConstraint::typeguard(type_guard.return_type(self.db)),
                        ))
                    }
                    _ => None,
                }?;

                Some(InternalConstraints::from_iter([place_and_constraint]))
            }
            Type::FunctionLiteral(function_type) if expr_call.arguments.keywords.is_empty() => {
                let [first_arg, second_arg] = &*expr_call.arguments.args else {
                    return None;
                };
                let first_arg = place_expr(first_arg)?;
                let function = function_type.known(self.db)?;
                let place = self.expect_place(&first_arg);

                if function == KnownFunction::HasAttr {
                    let attr = inference
                        .expression_type(second_arg)
                        .as_string_literal()?
                        .value(self.db);

                    if !is_identifier(attr) {
                        return None;
                    }

                    // Since `hasattr` only checks if an attribute is readable,
                    // the type of the protocol member should be a read-only property that returns `object`.
                    let constraint =
                        Type::protocol_with_readonly_members(self.db, [(attr, Type::object())]);

                    return Some(InternalConstraints::from_iter([(
                        place,
                        NarrowingConstraint::regular(constraint.negate_if(self.db, !is_positive)),
                    )]));
                }

                let function = function.into_classinfo_constraint_function()?;

                let class_info_ty = inference.expression_type(second_arg);

                function
                    .generate_constraint(self.db, class_info_ty)
                    .map(|constraint| {
                        InternalConstraints::from_iter([(
                            place,
                            NarrowingConstraint::regular(
                                constraint.negate_if(self.db, !is_positive),
                            ),
                        )])
                    })
            }
            // for the expression `bool(E)`, we further narrow the type based on `E`
            Type::ClassLiteral(class_type)
                if expr_call.arguments.args.len() == 1
                    && expr_call.arguments.keywords.is_empty()
                    && class_type.is_known(self.db, KnownClass::Bool) =>
            {
                self.evaluate_expression_node_predicate(
                    &expr_call.arguments.args[0],
                    expression,
                    is_positive,
                )
            }
            _ => None,
        }
    }

    fn evaluate_match_pattern_singleton(
        &mut self,
        subject: Expression<'db>,
        singleton: ast::Singleton,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let subject = place_expr(subject.node_ref(self.db, self.module))?;
        let place = self.expect_place(&subject);

        let ty = match singleton {
            ast::Singleton::None => Type::none(self.db),
            ast::Singleton::True => Type::BooleanLiteral(true),
            ast::Singleton::False => Type::BooleanLiteral(false),
        };
        let ty = ty.negate_if(self.db, !is_positive);
        Some(InternalConstraints::from_iter([(
            place,
            NarrowingConstraint::regular(ty),
        )]))
    }

    fn evaluate_match_pattern_class(
        &mut self,
        subject: Expression<'db>,
        cls: Expression<'db>,
        kind: ClassPatternKind,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        if !kind.is_irrefutable() && !is_positive {
            // A class pattern like `case Point(x=0, y=0)` is not irrefutable. In the positive case,
            // we can still narrow the type of the match subject to `Point`. But in the negative case,
            // we cannot exclude `Point` as a possibility.
            return None;
        }

        let subject = place_expr(subject.node_ref(self.db, self.module))?;
        let place = self.expect_place(&subject);

        let ty = infer_same_file_expression_type(self.db, cls, TypeContext::default(), self.module)
            .to_instance(self.db)?;

        let ty = ty.negate_if(self.db, !is_positive);
        Some(InternalConstraints::from_iter([(
            place,
            NarrowingConstraint::regular(ty),
        )]))
    }

    fn evaluate_match_pattern_value(
        &mut self,
        subject: Expression<'db>,
        value: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let place = {
            let subject = place_expr(subject.node_ref(self.db, self.module))?;
            self.expect_place(&subject)
        };
        let subject_ty =
            infer_same_file_expression_type(self.db, subject, TypeContext::default(), self.module);

        let value_ty =
            infer_same_file_expression_type(self.db, value, TypeContext::default(), self.module);

        self.evaluate_expr_compare_op(subject_ty, value_ty, ast::CmpOp::Eq, is_positive)
            .map(|ty| InternalConstraints::from_iter([(place, NarrowingConstraint::regular(ty))]))
    }

    fn evaluate_match_pattern_or(
        &mut self,
        subject: Expression<'db>,
        predicates: &Vec<PatternPredicateKind<'db>>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let db = self.db;

        // DeMorgan's law---if the overall `or` is negated, we need to `and` the negated sub-constraints.
        let merge_constraints = if is_positive {
            merge_constraints_or
        } else {
            merge_constraints_and
        };

        predicates
            .iter()
            .filter_map(|predicate| {
                self.evaluate_pattern_predicate_kind(predicate, subject, is_positive)
            })
            .reduce(|mut constraints, constraints_| {
                merge_constraints(&mut constraints, &constraints_, db);
                constraints
            })
    }

    fn evaluate_bool_op(
        &mut self,
        expr_bool_op: &ExprBoolOp,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<InternalConstraints<'db>> {
        let inference = infer_expression_types(self.db, expression, TypeContext::default());
        let sub_constraints = expr_bool_op
            .values
            .iter()
            // filter our arms with statically known truthiness
            .filter(|expr| {
                inference.expression_type(*expr).bool(self.db)
                    != match expr_bool_op.op {
                        BoolOp::And => Truthiness::AlwaysTrue,
                        BoolOp::Or => Truthiness::AlwaysFalse,
                    }
            })
            .map(|sub_expr| {
                self.evaluate_expression_node_predicate(sub_expr, expression, is_positive)
            })
            .collect::<Vec<_>>();
        match (expr_bool_op.op, is_positive) {
            (BoolOp::And, true) | (BoolOp::Or, false) => {
                let mut aggregation: Option<InternalConstraints> = None;
                for sub_constraint in sub_constraints.into_iter().flatten() {
                    if let Some(ref mut some_aggregation) = aggregation {
                        merge_constraints_and(some_aggregation, &sub_constraint, self.db);
                    } else {
                        aggregation = Some(sub_constraint);
                    }
                }
                aggregation
            }
            (BoolOp::Or, true) | (BoolOp::And, false) => {
                let (mut first, rest) = {
                    let mut it = sub_constraints.into_iter();
                    (it.next()?, it)
                };

                if let Some(ref mut first) = first {
                    for rest_constraint in rest {
                        if let Some(rest_constraint) = rest_constraint {
                            merge_constraints_or(first, &rest_constraint, self.db);
                        } else {
                            return None;
                        }
                    }
                }
                first
            }
        }
    }
}

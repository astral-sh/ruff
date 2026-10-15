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

use super::set_theoretic::generic_gradual_intersections::{
    GenericIntersection, generic_gradual_intersection,
};
use crate::subscript::PyIndex;
use crate::types::{TypeOperations, TypeVarVariance};
use std::cell::{Cell, OnceCell, RefCell};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;
use ty_python_core::place_table;
use ty_python_core::{ExpressionNodeKey, Program, scope::ScopeId};

use super::call::bind::{ClassInfoCheck, ClassInfoValidator};
use super::callable::{CallableTypes, CallableUpcastContext, UpcastPolicy};
use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::cyclic::{
    ActiveRecursionDetector, CycleDetector, CycleDetectorCache, CycleKind, HasIdentity,
    TypeTransformer,
};
use super::generics::{ApplySpecialization, Specialization};
use super::generics::{InferSpecializationVisitor, SpecializationBuilder, SpecializationError};
use super::infer::{BinaryOperand, IdentityUpcast, IdentityUpcastResult};
use super::narrow::ClassInfoConstraint;
use super::relation::{DisjointnessChecker, GuardedRelation, TypeRelation, TypeRelationChecker};
use super::subscript::{SubscriptChecker, SubscriptError};
use super::type_alias::AliasCycleSummary;
use super::typevar::TypeVarDefaultSearch;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, CallableType, ClassLiteral, ClassType,
    GenericContext, KnownClass, LiteralValueType, LiteralValueTypeKind, MaterializationKind,
    Parameters, SubclassOfInner, Type, TypeContext, TypeMapping, TypeVarBoundOrConstraints,
    TypedDictType, UnionType, VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

mod structural;
pub(super) use structural::RecursiveReferenceMapper;

/// Access to recursive bodies while a caller's recursion guard is active.
///
/// A mapping visitor alone does not establish a recursion guard for the operation that reads the
/// unfolded body. This context is borrowed inside a guarded visit and cannot be constructed by a
/// type operation. The caller's mapping visitor remains shared throughout that visit.
pub(super) struct TypeExpansionContext<'a, 'env, 'db, Mode = SemanticTraversal> {
    mapping: &'a ApplyTypeMappingVisitor<'env, 'db>,
    mode: PhantomData<Mode>,
}

/// Permission to compute a deferred body inside a guarded semantic operation.
/// Only the operation implementations in this module can construct this token.
pub(super) struct BodyAccess<'scope, 'db> {
    env: &'scope TypeOperations<'db>,
}

impl<'db> BodyAccess<'_, 'db> {
    pub(super) fn environment(&self) -> &TypeOperations<'db> {
        self.env
    }
}

/// A deferred body borrowed by the operation that opened it.
///
/// This view has no raw-type accessor. Its consuming operations keep the enclosing guard alive.
pub(super) struct ExpandedType<'scope, 'db> {
    ty: Type<'db>,
    mapping: &'scope ApplyTypeMappingVisitor<'scope, 'db>,
}

/// A body opened for comparison, with its union alternatives and their identities intact.
/// Normalized inspection views cannot be converted into comparison operands.
pub(super) struct ComparisonBody<'scope, 'db> {
    ty: Type<'db>,
    _mapping: &'scope ApplyTypeMappingVisitor<'scope, 'db>,
}

/// Whether a context containing `TypeForm` also permits an ordinary value.
pub(super) enum TypeFormContext<'db> {
    OnlyTypeForms,
    WithValueAlternative(Type<'db>),
}

/// The result of unfolding under an operation's guard.
pub(super) struct ExpandedRecursive<'scope, 'db> {
    result: UnfoldResult<'db>,
    mapping: &'scope ApplyTypeMappingVisitor<'scope, 'db>,
}

impl<'scope, 'db> ExpandedRecursive<'scope, 'db> {
    pub(super) fn into_unfolded(self) -> Option<ExpandedType<'scope, 'db>> {
        self.result.into_unfolded().map(|ty| ExpandedType {
            ty,
            mapping: self.mapping,
        })
    }

    pub(super) fn body_or_reference(self) -> ExpandedType<'scope, 'db> {
        ExpandedType {
            ty: self.result.into_type(),
            mapping: self.mapping,
        }
    }

    pub(super) fn map<R>(
        self,
        consume: impl FnOnce(ExpandedType<'scope, 'db>) -> R,
    ) -> UnfoldResult<'db, R> {
        self.result.map(|ty| {
            consume(ExpandedType {
                ty,
                mapping: self.mapping,
            })
        })
    }
}

impl<'db> ExpandedType<'_, 'db> {
    pub(super) fn enum_complement_class(
        self,
        db: &'db dyn Db,
    ) -> Option<super::enums::EnumClassLiteral<'db>> {
        self.ty.as_nominal_instance().and_then(|instance| {
            instance
                .class_literal(db, self.mapping.env)
                .into_enum_class(db)
        })
    }

    pub(super) fn enum_literal(self) -> Option<super::literal::EnumLiteralType<'db>> {
        self.ty.as_enum_literal()
    }

    pub(super) fn known_class(self, db: &'db dyn Db) -> Option<KnownClass> {
        self.ty
            .as_nominal_instance()
            .and_then(|instance| instance.known_class(db))
    }

    pub(super) fn has_nominal_class(self, db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
        self.ty
            .as_nominal_instance()
            .is_some_and(|instance| instance.class_literal(db, self.mapping.env) == class)
    }

    pub(super) fn as_bool_literal(&self) -> Option<bool> {
        self.ty
            .as_literal_value()
            .and_then(LiteralValueType::as_bool)
    }

    pub(super) fn typeform_subclass(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
    ) -> Option<Type<'db>> {
        let Type::TypeForm(typeform) = self.ty else {
            return None;
        };
        inspector.inspect(
            db,
            typeform.type_argument(db),
            |_, _| None,
            |_, ty| ty.subclass_type(db),
        )
    }

    pub(super) fn intersect_generic(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        original: Type<'db>,
    ) -> Option<GenericIntersection<'db>> {
        generic_gradual_intersection(db, self.mapping.env, other, self.ty).map(
            |result| match result {
                GenericIntersection::Simplified(merged) if merged == self.ty => {
                    GenericIntersection::Simplified(original)
                }
                result => result,
            },
        )
    }

    pub(super) fn project_intersection_typevar(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let env = self.mapping.env;
        match self.ty {
            Type::TypeVar(tvar) => Some(match tvar.require_bound_or_constraints(db, env) {
                TypeVarBoundOrConstraints::UpperBound(bound) => bound,
                TypeVarBoundOrConstraints::Constraints(constraints) => constraints.as_type(db, env),
            }),
            Type::NewTypeInstance(newtype) => Some(newtype.concrete_base_type(db, env)),
            _ => None,
        }
    }

    pub(super) fn needs_intersection_typevar_expansion(&self) -> bool {
        matches!(self.ty, Type::TypeVar(_) | Type::NewTypeInstance(_))
    }

    pub(super) fn is_newtype_instance(&self) -> bool {
        matches!(self.ty, Type::NewTypeInstance(_))
    }

    pub(super) fn constrained_typevar_is_subset(
        self,
        db: &'db dyn Db,
        constraints: super::typevar::TypeVarConstraints<'db>,
    ) -> bool {
        let Some(actual) = self.ty.as_typevar() else {
            return false;
        };
        matches!(actual.typevar(db).bound_or_constraints(db, self.mapping.env),
            Some(TypeVarBoundOrConstraints::Constraints(actual_constraints))
            if actual_constraints.is_subset_of(db, self.mapping.env, constraints))
    }

    pub(super) fn merge_ambiguous_return(
        self,
        merger: &super::call::ambiguous::AmbiguousReturnMerger<'_, 'db>,
        types: &[Type<'db>],
        index: usize,
    ) -> Type<'db> {
        merger.merge_expanded_column(types, index, self.ty)
    }

    pub(super) fn divergence(&self) -> Option<super::DivergentType> {
        self.ty.as_divergent()
    }

    pub(super) fn narrow_identity_comparison(
        self,
        builder: &super::narrow::NarrowingConstraintsBuilder<'db, '_>,
        lhs: Type<'db>,
        rhs: Type<'db>,
        identity: Type<'db>,
    ) -> Option<Type<'db>> {
        builder.narrow_identity_resolved(self.ty, lhs, rhs, identity)
    }
    pub(super) fn finish_identity_overlap(
        self,
        overlap: super::narrow::RuntimeOverlap<'_, 'db, '_>,
    ) -> Type<'db> {
        overlap.finish(self.ty)
    }

    pub(super) fn paramspec_prefix_length(
        self,
        checker: &super::call::bind::ArgumentTypeChecker<'_, 'db>,
        argument: Type<'db>,
        paramspec: super::BoundTypeVarInstance<'db>,
    ) -> Option<usize> {
        checker.paramspec_prefix_length_resolved(self.ty, argument, paramspec)
    }
    pub(super) fn narrow_missing_typed_dict_key(
        self,
        db: &'db dyn Db,
        key: &str,
    ) -> Option<Type<'db>> {
        let env = self.mapping.env;
        let resolved_rhs_type = self.ty;
        let requires_key = |td: TypedDictType<'db>| -> bool {
            td.key_membership_truthiness(db, env, key).is_always_true()
        };

        let narrowed = match resolved_rhs_type {
            Type::TypedDict(td) => {
                if requires_key(td) {
                    Type::Never
                } else {
                    resolved_rhs_type
                }
            }
            Type::Intersection(intersection) => {
                if intersection
                    .positive(db)
                    .iter()
                    .copied()
                    .filter_map(Type::as_typed_dict)
                    .any(requires_key)
                {
                    Type::Never
                } else {
                    resolved_rhs_type
                }
            }
            Type::Union(union) => {
                // remove all members of the union that would require the key
                union.filter(db, |ty| match ty {
                    Type::TypedDict(td) => !requires_key(*td),
                    Type::Intersection(intersection) => !intersection
                        .positive(db)
                        .iter()
                        .copied()
                        .filter_map(Type::as_typed_dict)
                        .any(requires_key),
                    _ => true,
                })
            }
            _ => resolved_rhs_type,
        };

        (narrowed != resolved_rhs_type).then_some(narrowed)
    }
    pub(super) fn narrow_tuple_identity(
        self,
        db: &'db dyn Db,
        index: i32,
        rhs_ty: Type<'db>,
        is_positive_check: bool,
    ) -> Option<Type<'db>> {
        let env = self.mapping.env;
        let ty = self.ty;

        let Type::Union(union) = ty else {
            return None;
        };
        let filtered = union.filter(db, |elem| {
            elem.tuple_instance_spec(db, env)
                .and_then(|spec| spec.py_index(db, env, index).ok())
                .is_none_or(|el_ty| {
                    el_ty
                        .identity_comparison_truthiness(db, env, rhs_ty)
                        .negate_if(!is_positive_check)
                        .may_be_true()
                })
        });
        (filtered != Type::Union(union)).then_some(filtered)
    }

    pub(super) fn class_decorator_result(
        self,
        db: &'db dyn Db,
        result: super::infer::ClassBindingUpdate<'_, 'db>,
    ) -> Type<'db> {
        result.apply(db, self.mapping.env, self.ty)
    }
    pub(super) fn check_class_method_variance(
        self,
        check: super::infer::ClassMethodVarianceCheck<'_, 'db, '_>,
    ) {
        check.check(self.ty);
    }

    pub(super) fn matches_string_membership(
        self,
        db: &'db dyn Db,
        haystack: &str,
        is_contained: bool,
    ) -> bool {
        let env = self.mapping.env;
        let element = self.ty;

        if let Some(needle) = element.as_string_literal() {
            haystack.contains(needle.value(db)) == is_contained
        } else {
            !(is_contained
                && element.is_disjoint_from(db, env, KnownClass::Str.to_instance(db, env)))
        }
    }

    pub(super) fn containment_behavior(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
    ) -> super::narrow::ContainmentBehavior<'db> {
        super::narrow::containment_behavior_resolved(db, inspector, self.ty)
    }
    pub(super) fn narrow_string_membership(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        haystack: &str,
        contained: bool,
    ) -> Option<Type<'db>> {
        super::narrow::narrow_string_membership_resolved(
            db, inspector, self.ty, haystack, contained,
        )
    }

    pub(super) fn override_write_domain(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        receiver: Type<'db>,
        storage: Type<'db>,
    ) -> Option<Type<'db>> {
        super::overrides::descriptor_write_domain_resolved(
            db, inspector, self.ty, receiver, storage,
        )
    }

    pub(super) fn unpack_annotation(
        self,
        builder: &mut super::infer::TypeInferenceBuilder<'db, '_>,
        expression: ruff_python_ast::ExprRef<'_>,
        syntax: &str,
    ) -> Type<'db> {
        builder.validate_unpack_annotation(expression, self.ty, syntax)
    }
    pub(super) fn subclass_argument(
        self,
        builder: &super::infer::TypeInferenceBuilder<'db, '_>,
        expression: &ruff_python_ast::Expr,
    ) -> Type<'db> {
        builder.subclass_argument_resolved(expression, self.ty)
    }
    pub(super) fn has_singleton_intersection(self, db: &'db dyn Db) -> bool {
        self.ty.as_intersection().is_some_and(|intersection| {
            intersection
                .positive(db)
                .iter()
                .any(|ty| ty.is_singleton(db, self.mapping.env))
        })
    }

    pub(super) fn ambiguous_truthiness_condition(
        self,
        builder: &super::infer::TypeInferenceBuilder<'db, '_>,
        expression: &ruff_python_ast::Expr,
        original: Type<'db>,
    ) -> Option<super::infer::ConditionKind<'db>> {
        builder.ambiguous_truthiness_condition(expression, self.ty, original)
    }

    pub(super) fn truthiness_none_union(
        self,
        builder: &super::infer::TypeInferenceBuilder<'db, '_>,
        expression: &ruff_python_ast::Expr,
    ) -> Option<UnionType<'db>> {
        builder.truthiness_none_union_resolved(expression, self.ty)
    }

    pub(super) fn requires_generic_instance_storage(
        self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
    ) -> bool {
        let env = self.mapping.env;
        let ty = self.ty;
        let alternatives = match &ty {
            Type::Union(union) => union.elements(db),
            _ => std::slice::from_ref(&ty),
        };
        alternatives.iter().any(|ty| {
            // Synthesized methods bind through `try_call_dunder_get` without necessarily
            // exposing a `__get__` member on their meta-type.
            if let Type::Callable(callable) = ty
                && callable.is_method_like(db)
            {
                return false;
            }

            // Descriptors define their own class-access behavior, but do not exempt other
            // alternatives in a union from the restriction on generic instance storage.
            ty.class_member(db, env, "__get__").is_undefined()
                // Variance accounts for aliases without expanding recursive specializations,
                // and ignores alias arguments that do not affect the resulting type.
                && generic_context.variables(db).any(|typevar| {
                    ty.variance_of(db, env, typevar.identity(db)).evaluate(db)
                        != TypeVarVariance::Bivariant
                })
        })
    }

    pub(super) fn class_pattern_contexts(
        self,
        db: &'db dyn Db,
        analyzer: &super::narrow::PatternSuccessAnalyzer<'db, '_>,
        kind: &ty_python_core::predicate::ClassPatternPredicateKind<'db>,
    ) -> smallvec::SmallVec<[super::narrow::ClassPatternContext<'db>; 2]> {
        match self.ty {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .map(|ty| analyzer.class_pattern_context(kind, *ty))
                .collect(),
            ty => smallvec::smallvec![analyzer.class_pattern_context(kind, ty)],
        }
    }

    pub(super) fn pattern_subject_arms(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
    ) -> smallvec::SmallVec<[(Type<'db>, Type<'db>); 2]> {
        match self.ty {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .flat_map(|original| {
                    inspector.inspect(
                        db,
                        *original,
                        |ty, _| pattern_filtering_arms(db, *original, ty),
                        |inspector, ty| ty.pattern_filtering_arms(db, inspector, *original),
                    )
                })
                .collect(),
            Type::TypeVar(typevar)
                if let Some(bound) = typevar.typevar(db).upper_bound(db, self.mapping.env) =>
            {
                inspector.inspect(
                    db,
                    bound,
                    |ty, _| smallvec::smallvec![(self.ty, ty)],
                    |_, ty| pattern_filtering_arms(db, self.ty, ty.ty),
                )
            }
            ty => smallvec::smallvec![(ty, ty)],
        }
    }

    fn pattern_filtering_arms(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        original: Type<'db>,
    ) -> smallvec::SmallVec<[(Type<'db>, Type<'db>); 2]> {
        if let Type::TypeVar(typevar) = self.ty
            && let Some(bound) = typevar.typevar(db).upper_bound(db, self.mapping.env)
        {
            inspector.inspect(
                db,
                bound,
                |ty, _| pattern_filtering_arms(db, original, ty),
                |_, ty| pattern_filtering_arms(db, original, ty.ty),
            )
        } else {
            pattern_filtering_arms(db, original, self.ty)
        }
    }

    pub(super) fn narrow_membership(
        self,
        builder: &super::narrow::NarrowingConstraintsBuilder<'db, '_>,
        inspector: &TypeInspector<'_, 'db>,
        lhs: Type<'db>,
    ) -> Option<Type<'db>> {
        builder.narrow_membership_resolved(inspector, lhs, self.ty)
    }
    pub(super) fn narrow_non_membership(
        self,
        builder: &super::narrow::NarrowingConstraintsBuilder<'db, '_>,
        lhs: Type<'db>,
    ) -> Option<Type<'db>> {
        builder.narrow_non_membership_resolved(lhs, self.ty)
    }

    pub(super) fn narrow_length(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        original: Type<'db>,
        length: i64,
        comparison: super::narrow::LengthComparison,
    ) -> Type<'db> {
        super::narrow::NarrowingConstraintsBuilder::narrow_type_by_len_comparison_impl(
            db, inspector, original, self.ty, length, comparison,
        )
    }

    pub(super) fn validate_subscript_assignment(
        self,
        inspector: &TypeInspector<'_, 'db>,
        assignment: super::infer::SubscriptAssignment<'_, 'db, '_>,
    ) -> bool {
        assignment.check(inspector, self.ty)
    }

    pub(super) fn retained_function_binding(
        self,
        db: &'db dyn Db,
        exposed_type: &ExpandedType<'_, 'db>,
        exposed: Type<'db>,
        function: super::FunctionType<'db>,
    ) -> Option<(super::FunctionType<'db>, Type<'db>)> {
        let env = self.mapping.env;
        let binding = self.ty;
        let exposed_type = exposed_type.ty;

        let exposed_union = match exposed_type {
            Type::Union(union) => Some(union),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db, env).as_union(),
            _ => None,
        };
        let exposed_alternatives =
            exposed_union.map_or(std::slice::from_ref(&exposed), |union| union.elements(db));
        let binding_union = match binding {
            Type::Union(union) => Some(union),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db, env).as_union(),
            _ => None,
        };
        let binding_alternatives =
            binding_union.map_or(std::slice::from_ref(&binding), |union| union.elements(db));
        let retained = UnionType::from_elements(
            db,
            env,
            binding_alternatives
                .iter()
                .copied()
                .filter(|ty| exposed_alternatives.contains(ty)),
        );
        (!retained.is_never()).then_some((function, retained))
    }

    pub(super) fn known_constructor_preserves_value_type(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        annotation: Type<'db>,
    ) -> bool {
        super::enums::known_constructor_preserves_value_type_resolved(
            db, inspector, self.ty, annotation,
        )
    }

    pub(super) fn value_has_exact_known_class(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        data_type: KnownClass,
    ) -> bool {
        super::enums::value_has_exact_known_class_resolved(db, inspector, self.ty, data_type)
    }

    pub(super) fn collect_implementation_roots(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        seen: &mut FxHashSet<ClassLiteral<'db>>,
        roots: &mut Vec<ClassLiteral<'db>>,
    ) {
        super::ide_support::collect_implementation_roots_resolved(
            db, inspector, self.ty, seen, roots,
        );
    }

    pub(super) fn is_string_keyed_mapping(&self, db: &'db dyn Db) -> bool {
        let env = self.mapping.env;
        let ty = self.ty;

        let Type::NominalInstance(instance) = ty else {
            return false;
        };

        matches!(
                instance.class(db, env).known(db),
                Some(
                    KnownClass::Dict
                        | KnownClass::Mapping
                        | KnownClass::MutableMapping
                        | KnownClass::DefaultDict
                        | KnownClass::ChainMap
                        | KnownClass::OrderedDict
                )
            ) && instance
                .class(db, env)
                .into_generic_alias()
                .is_some_and(|alias| {
                    matches!(
                        alias.specialization(db).types(db),
                        [key, _] if key.is_same_resolved_type(db, env, KnownClass::Str.to_instance(db, env))
                    )
                })
    }

    pub(super) fn type_form_context_fallback(
        self,
        db: &'db dyn Db,
    ) -> Option<TypeFormContext<'db>> {
        let env = self.mapping.env;
        let target = self.ty;
        match target {
            Type::TypeForm(_) => Some(TypeFormContext::OnlyTypeForms),
            Type::Union(union)
                if union.elements(db).iter().any(|element| {
                    element.has_outer_type(db, env, |ty| matches!(ty, Type::TypeForm(_)))
                }) =>
            {
                Some(TypeFormContext::WithValueAlternative(target.filter_union(
                    db,
                    env,
                    |element| {
                        !element.has_outer_type(db, env, |ty| matches!(ty, Type::TypeForm(_)))
                    },
                )))
            }
            _ => None,
        }
    }

    pub(super) fn wrap_descriptor(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        kind: super::callable::CallableTypeKind,
    ) -> Option<Type<'db>> {
        let ty = self.ty;
        match ty {
            Type::Union(union) => union.try_map(db, inspector.environment(), |ty| {
                inspector.inspect(
                    db,
                    *ty,
                    |_, _| None,
                    |inspector, ty| ty.wrap_descriptor(db, inspector, kind),
                )
            }),
            Type::FunctionLiteral(function)
                if function.callable_type_kind(db)
                    == super::callable::CallableTypeKind::FunctionLike
                    || function.callable_type_kind(db) == kind =>
            {
                Some(Type::FunctionLiteral(
                    function.with_descriptor_kind(db, kind),
                ))
            }
            Type::Callable(callable)
                if matches!(
                    callable.kind(db),
                    super::callable::CallableTypeKind::Regular
                        | super::callable::CallableTypeKind::FunctionLike
                ) || callable.kind(db) == kind =>
            {
                Some(Type::Callable(callable.with_kind(db, kind)))
            }
            _ => None,
        }
    }

    pub(super) fn accepts_literal_context(self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        let env = self.mapping.env;
        let tcx = self.ty;

        let literal_tcx = tcx.filter_union(db, env, |ty| ty.as_literal_value().is_some());
        matches!(literal_tcx, Type::Union(_) | Type::LiteralValue(_))
            && ty.is_assignable_to(db, env, literal_tcx)
    }

    pub(super) fn matching_generic_class(
        self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Option<super::GenericAlias<'db>> {
        let target = self.ty;

        let generic_alias = match target {
            Type::SubclassOf(subclass)
                if let SubclassOfInner::Class(ClassType::Generic(alias)) =
                    subclass.subclass_of() =>
            {
                alias
            }
            Type::GenericAlias(alias) => alias,
            _ => return None,
        };

        if class.as_static()? == generic_alias.origin(db) {
            Some(generic_alias)
        } else {
            None
        }
    }

    pub(super) fn specialize_class_callable_context(
        self,
        builder: &super::infer::TypeInferenceBuilder<'db, '_>,
        inspector: &TypeInspector<'_, 'db>,
        class: ClassLiteral<'db>,
    ) -> Option<Type<'db>> {
        builder.specialize_class_callable_context_resolved(inspector, class, self.ty)
    }
    pub(super) fn specialize_class_context(
        self,
        builder: &super::infer::TypeInferenceBuilder<'db, '_>,
        inspector: &TypeInspector<'_, 'db>,
        class: ClassLiteral<'db>,
    ) -> Type<'db> {
        builder.specialize_class_context_resolved(inspector, class, self.ty)
    }

    pub(super) fn infer_dict_context(
        self,
        builder: &mut super::infer::TypeInferenceBuilder<'db, '_>,
        inspector: &TypeInspector<'_, 'db>,
        dict: &ruff_python_ast::ExprDict,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        builder.infer_dict_expression_inner(
            inspector,
            dict,
            tcx.with_annotation(tcx.annotation.map(|_| self.ty)),
        )
    }
    pub(super) fn accepts_dict_literal(
        self,
        builder: &super::infer::TypeInferenceBuilder<'db, '_>,
        inspector: &TypeInspector<'_, 'db>,
        dict: &ruff_python_ast::ExprDict,
        tcx: TypeContext<'db>,
    ) -> bool {
        builder.accepts_dict_literal_resolved(inspector, dict, tcx, self.ty)
    }

    pub(super) fn descriptor_setter_domain(
        self,
        db: &'db dyn Db,
        receiver: Type<'db>,
    ) -> super::attribute_write::DescriptorSetterDomain<'db> {
        super::attribute_write::descriptor_setter_domain_resolved(
            db,
            self.mapping.env,
            self.ty,
            receiver,
        )
    }

    pub(super) fn protocol_descriptor_member(
        self,
        db: &'db dyn Db,
        protocol: ClassType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Option<super::protocol_class::ProtocolMemberData<'db>> {
        super::protocol_class::protocol_descriptor_member_resolved(
            db,
            self.mapping.env,
            self.ty,
            protocol,
            definition,
        )
    }

    pub(super) fn matches_constructor_return_typevar(
        self,
        db: &'db dyn Db,
        return_typevar: super::BoundTypeVarInstance<'db>,
    ) -> bool {
        let cls_parameter_ty = self.ty;

        let Type::SubclassOf(subclass_of) = cls_parameter_ty else {
            return false;
        };
        let Some(cls_typevar) = subclass_of.into_type_var() else {
            return false;
        };

        cls_typevar.typevar(db).identity(db) == return_typevar.typevar(db).identity(db)
    }

    pub(super) fn constructor_returns_instance(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        class: ClassLiteral<'db>,
    ) -> bool {
        super::call::bind::constructor_returns_instance_resolved(db, inspector, class, self.ty)
    }
    pub(super) fn normalize_constructor_return(
        self,
        db: &'db dyn Db,
        binding: &super::call::bind::Binding<'db>,
    ) -> Option<Type<'db>> {
        binding.normalized_constructor_return_resolved(db, self.mapping.env, self.ty)
    }

    pub(super) fn class_pattern_positional_result(
        self,
        db: &'db dyn Db,
    ) -> Option<super::match_pattern::ClassPatternPositionalResult<'db>> {
        let env = self.mapping.env;
        let match_args = self.ty;

        if let Some(limit) = match_args.exact_tuple_instance_spec(db).and_then(|tuple| {
            tuple
                .as_fixed_length()
                .map(super::tuple::FixedLengthTuple::len)
        }) {
            Some(super::match_pattern::ClassPatternPositionalResult::Limit(
                limit,
            ))
        } else {
            match_args
                .is_disjoint_from(db, env, Type::homogeneous_tuple(db, env, Type::unknown()))
                .then_some(
                    super::match_pattern::ClassPatternPositionalResult::InvalidType(match_args),
                )
        }
    }

    pub(super) fn mapping_pattern_value(
        self,
        analyzer: &super::narrow::PatternSuccessAnalyzer<'db, '_>,
        inspector: &TypeInspector<'_, 'db>,
        key: Type<'db>,
    ) -> Option<Type<'db>> {
        analyzer.mapping_pattern_value_resolved(inspector, self.ty, key)
    }

    pub(super) fn preserve_pattern_subject(
        self,
        analyzer: &super::narrow::PatternSuccessAnalyzer<'db, '_>,
        original: Type<'db>,
        filtered: Type<'db>,
        preservation: super::narrow::OriginalSubjectPreservation,
    ) -> Type<'db> {
        analyzer.preserve_pattern_subject_resolved(self.ty, original, filtered, preservation)
    }

    pub(super) fn filter_tuple_subscript(
        self,
        builder: &super::narrow::NarrowingConstraintsBuilder<'db, '_>,
        expression: &ruff_python_ast::Expr,
        index: Type<'db>,
        predicate: impl FnMut(Type<'db>) -> bool,
    ) -> Option<(
        ty_python_core::place::ScopedPlaceId,
        super::narrow::NarrowingConstraint<'db>,
    )> {
        builder.filter_tuple_subscript_resolved(self.ty, expression, index, predicate)
    }

    pub(super) fn narrow_nominal_attribute(
        self,
        builder: &super::narrow::NarrowingConstraintsBuilder<'db, '_>,
        expression: &ruff_python_ast::Expr,
        name: &str,
        rhs: Type<'db>,
        operator: ruff_python_ast::CmpOp,
        positive: bool,
    ) -> Option<(
        ty_python_core::place::ScopedPlaceId,
        super::narrow::NarrowingConstraint<'db>,
    )> {
        builder
            .narrow_nominal_attribute_resolved(self.ty, expression, name, rhs, operator, positive)
    }

    pub(super) fn narrow_nominal_attribute_by_truthiness(
        self,
        builder: &super::narrow::NarrowingConstraintsBuilder<'db, '_>,
        expression: &ruff_python_ast::Expr,
        name: &str,
        positive: bool,
    ) -> Option<(
        ty_python_core::place::ScopedPlaceId,
        super::narrow::NarrowingConstraint<'db>,
    )> {
        builder.narrow_nominal_attribute_by_truthiness_resolved(self.ty, expression, name, positive)
    }

    pub(super) fn nominal_class(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
    ) -> Option<ClassType<'db>> {
        let env = self.mapping.env;
        let ty = self.ty;
        match ty {
            Type::NominalInstance(instance) => Some(instance.class(db, env)),
            Type::ProtocolInstance(instance) => instance.class_origin(db).map(|class| *class),
            Type::NewTypeInstance(newtype) => newtype
                .concrete_base_type(db, env)
                .nominal_class_in(db, inspector),
            Type::TypeVar(typevar) => {
                let TypeVarBoundOrConstraints::UpperBound(bound) =
                    typevar.typevar(db).bound_or_constraints(db, env)?
                else {
                    return None;
                };
                bound.nominal_class_in(db, inspector)
            }
            Type::LiteralValue(literal) => literal
                .fallback_instance(db, env)
                .nominal_class_in(db, inspector),
            Type::PropertyInstance(property) => property
                .instance_fallback(db, env)
                .nominal_class_in(db, inspector),
            Type::SlotDescriptor(_) => KnownClass::MemberDescriptorType
                .to_instance(db, env)
                .nominal_class_in(db, inspector),
            _ => None,
        }
    }

    pub(super) fn is_literal_or_union_of_literals(
        &self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
    ) -> bool {
        let env = self.mapping.env;
        let ty = self.ty;
        match ty {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_literal_or_union_of_literals_in(db, inspector)),
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
                | LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::Enum(_) => true,
                LiteralValueTypeKind::LiteralString => false,
            },
            Type::NominalInstance(_) => ty.is_none(db) || ty.is_bool(db) || ty.is_enum(db, env),
            _ => false,
        }
    }

    pub(super) fn as_callable(&self) -> Option<CallableType<'db>> {
        self.ty.as_callable()
    }
    pub(super) fn as_typevar(&self) -> Option<super::BoundTypeVarInstance<'db>> {
        self.ty.as_typevar()
    }
    pub(super) fn as_typed_dict(&self) -> Option<TypedDictType<'db>> {
        self.ty.as_typed_dict()
    }
    pub(super) fn as_string_literal(&self) -> Option<super::StringLiteralType<'db>> {
        self.ty.as_string_literal()
    }
    pub(super) fn as_int_like_literal(&self) -> Option<i64> {
        self.ty.as_int_like_literal()
    }
    pub(super) fn subclass_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        super::SubclassOfType::try_from_instance(db, self.mapping.env, self.ty).ok()
    }

    pub(super) fn unpacked_keyword_is_gradual(self, db: &'db dyn Db) -> bool {
        let env = self.mapping.env;
        let ty = self.ty;
        match ty {
            ty if ty.is_never() || ty.is_dynamic() => true,
            Type::Union(union) => union
                .elements(db)
                .iter()
                .any(|element| element.has_outer_type(db, env, |ty| ty.is_dynamic())),
            _ => false,
        }
    }

    pub(super) fn build_super(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        pivot: Type<'db>,
    ) -> Result<Type<'db>, super::bound_super::BoundSuperError<'db>> {
        super::bound_super::BoundSuperType::build_impl(db, inspector, pivot, self.ty)
    }

    pub(super) fn equality_exclusion(
        self,
        db: &'db dyn Db,
        policy: super::equality::ComparisonSoundnessPolicy,
    ) -> Option<Type<'db>> {
        super::equality::equality_exclusion_resolved(db, self.mapping.env, self.ty, policy)
    }

    pub(super) fn equality_constraint(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        left: Type<'db>,
        positive: bool,
        operator: super::equality::ComparisonOperator,
        policy: super::equality::ComparisonSoundnessPolicy,
    ) -> Option<Type<'db>> {
        super::equality::equality_constraint_resolved(
            db, inspector, left, self.ty, positive, operator, policy,
        )
    }

    pub(super) fn is_builtin_comparison_literal(&self, db: &'db dyn Db) -> bool {
        super::equality::is_builtin_comparison_literal_resolved(db, self.ty)
    }
    pub(super) fn all_builtin_comparison_literals(self, db: &'db dyn Db) -> bool {
        match self.ty {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| super::equality::is_builtin_literal_type(db, self.mapping.env, *ty)),
            ty => super::equality::is_builtin_literal_type(db, self.mapping.env, ty),
        }
    }

    pub(super) fn builtin_literal_constraint(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        left: Type<'db>,
        operator: super::equality::ComparisonOperator,
        expects_equality: bool,
    ) -> Option<Type<'db>> {
        super::equality::builtin_literal_constraint_resolved(
            db,
            inspector,
            left,
            self.ty,
            operator,
            expects_equality,
        )
    }

    pub(super) fn add_equal_enum_literals(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
        right: super::LiteralValueTypeKind<'db>,
        operator: super::equality::ComparisonOperator,
        builder: &mut super::UnionBuilder<'db>,
    ) {
        super::equality::add_equal_enum_literals_resolved(
            db, inspector, self.ty, right, operator, builder,
        );
    }

    pub(super) fn enum_literal_constraint(
        self,
        db: &'db dyn Db,
        left: Type<'db>,
        operator: super::equality::ComparisonOperator,
        expects_equality: bool,
    ) -> Option<Type<'db>> {
        super::equality::enum_literal_constraint_resolved(
            db,
            self.mapping.env,
            left,
            self.ty,
            operator,
            expects_equality,
        )
    }

    pub(super) fn unsafe_narrowable_builtin_semantics(
        self,
        db: &'db dyn Db,
    ) -> Option<super::equality::KnownComparisonSemantics> {
        super::equality::unsafe_narrowable_builtin_semantics_resolved(db, self.ty)
    }

    pub(super) fn has_reflexive_equality_semantics(
        self,
        evaluator: &super::equality::ComparisonEvaluator<'_, 'db>,
        inspector: &TypeInspector<'_, 'db>,
    ) -> bool {
        super::equality::has_reflexive_equality_semantics_resolved(evaluator, inspector, self.ty)
    }

    pub(super) fn exact_class_for_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let bound = super::SubclassOfType::try_from_instance(db, self.mapping.env, self.ty).ok()?;
        matches!(bound, Type::ClassLiteral(_) | Type::GenericAlias(_)).then_some(bound)
    }

    pub(super) fn paramspec_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self.ty {
            Type::Callable(callable)
                if !callable.has_generic_context(db)
                    && let Some(paramspec) = callable
                        .single_parameters(db)
                        .and_then(Parameters::as_paramspec) =>
            {
                Some(Type::TypeVar(paramspec))
            }
            Type::Callable(callable) => Some(Type::Callable(callable.into_paramspec_value(db))),
            Type::TypeVar(bound)
                if bound.is_paramspec(db) && bound.paramspec_attr(db).is_none() =>
            {
                Some(Type::TypeVar(bound))
            }
            _ => None,
        }
    }

    pub(super) fn data_descriptor(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let env = self.mapping.env;
        let descriptor_ty = self.ty;

        let descriptor_ty = match descriptor_ty {
            Type::TypeForm(typeform) => typeform.type_argument(db).to_meta_type(db, env),
            descriptor_ty => descriptor_ty,
        };

        descriptor_ty
            .is_data_descriptor(db, env)
            .then_some(descriptor_ty)
    }

    pub(super) fn receiver_violates_typevar_domain(
        self,
        db: &'db dyn Db,
        receiver: Type<'db>,
    ) -> bool {
        self.ty.as_typevar().is_some_and(|typevar| {
            super::signatures::Signature::receiver_violates_typevar_domain(
                db,
                self.mapping.env,
                receiver,
                typevar,
            )
        })
    }

    pub(super) fn unpacked_typed_dict(self) -> Option<TypedDictType<'db>> {
        self.ty.as_typed_dict()
    }

    pub(super) fn matches(&self, predicate: fn(Type<'db>) -> bool) -> bool {
        predicate(self.ty)
    }

    pub(super) fn has_same_resolved_type(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        if !other.is_alias_like() {
            return self.ty == other;
        }
        TypeInspector::inspect_root(
            db,
            self.mapping.env,
            TypeInspectionKey::ResolvedIdentityRight,
            other,
            |other, _| self.ty == other,
            |_, other| self.ty == other.ty,
        )
    }

    pub(super) fn has_same_type(&self, other: Type<'db>) -> bool {
        self.ty == other
    }

    pub(super) fn constructor_for_bound(self, db: &'db dyn Db) -> Type<'db> {
        if self.ty.is_object() {
            super::KnownClass::Object.to_class_literal(db, self.mapping.env)
        } else {
            self.ty.to_meta_type(db, self.mapping.env)
        }
    }

    pub(super) fn filter_union(
        self,
        db: &'db dyn Db,
        original: Type<'db>,
        was_union: bool,
        mut predicate: impl FnMut(&Type<'db>) -> bool,
    ) -> Type<'db> {
        match self.ty {
            Type::Union(union) => union.filter(db, predicate),
            ty if was_union => {
                if predicate(&ty) {
                    ty
                } else {
                    Type::Never
                }
            }
            _ => original,
        }
    }

    pub(super) fn enum_values(
        self,
        db: &'db dyn Db,
        visitor: &super::equality::EnumDomainVisitor<'_, 'db>,
    ) -> Option<super::equality::EnumValueSet<'db>> {
        super::equality::EnumValueSet::from_normalized(db, self.mapping.env, self.ty, visitor)
    }

    pub(super) fn collect_enum_partition(
        self,
        db: &'db dyn Db,
        collector: &mut super::equality::EnumPartitionCollector<'_, '_, 'db>,
        original: Type<'db>,
    ) -> Option<()> {
        collector.consume(db, self.ty, original)
    }

    pub(super) fn collect_enum_domains(
        self,
        db: &'db dyn Db,
        collector: &mut super::equality::EnumDomainCollector<'_, '_, 'db>,
    ) -> Option<()> {
        collector.consume(db, self.ty)
    }

    pub(super) fn is_union(&self) -> bool {
        self.ty.is_union()
    }

    pub(super) fn evaluate_comparison(
        self,
        target: &ExpandedType<'_, 'db>,
        evaluator: &super::equality::ComparisonEvaluator<'_, 'db>,
        branch: super::equality::ComparisonBranch,
        operator: super::equality::ComparisonOperator,
    ) -> super::equality::ComparisonResult<'db> {
        evaluator.evaluate_expanded(self.ty, target.ty, branch, operator)
    }

    pub(super) fn infer_comparison(
        self,
        operand: super::infer::BinaryComparisonOperand<'_, 'db, '_>,
    ) -> Result<Type<'db>, super::infer::UnsupportedComparisonError<'db>> {
        operand.infer(self.ty)
    }

    pub(super) fn partial_identity(
        self,
        db: &'db dyn Db,
    ) -> Option<super::known_instance::FunctoolsPartialInstance<'db>> {
        super::infer::upcast_partial_body(db, self.ty)
    }

    pub(super) fn visit(
        self,
        db: &'db dyn Db,
        visitor: &(impl super::visitor::TypeVisitor<'db> + ?Sized),
    ) {
        visitor.visit_type(db, self.ty);
    }

    pub(super) fn recover_alias_cycle(
        self,
        db: &'db dyn Db,
        recovery: &super::type_alias::AliasCycleRecovery<'_, 'db>,
    ) -> Option<Type<'db>> {
        recovery.recover(db, self.ty)
    }

    pub(super) fn is_specialized_generic(&self, db: &'db dyn Db) -> bool {
        self.ty.is_specialized_generic(db)
    }

    pub(super) fn display_string(&self, db: &'db dyn Db) -> String {
        self.ty.display(db, self.mapping.env).to_string()
    }

    pub(super) fn variance_of(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        self.ty.variance_of(db, self.mapping.env, typevar)
    }

    pub(super) fn display_type_alias_declaration(
        self,
        db: &'db dyn Db,
        alias: super::TypeAliasType<'db>,
    ) -> String {
        super::display::render_alias_declaration(
            db,
            self.mapping.env,
            Type::TypeAlias(alias),
            alias.definition(db),
            alias.name(db),
            alias.generic_context(db),
            self.ty,
        )
    }

    pub(super) fn display_alias_declaration(
        self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> Option<String> {
        let (definition, name) = recursive.alias(db)?;
        Some(super::display::render_alias_declaration(
            db,
            self.mapping.env,
            Type::Recursive(recursive),
            definition,
            name,
            recursive.parameters(db),
            self.ty,
        ))
    }

    pub(super) fn search_aliases(
        self,
        db: &'db dyn Db,
        search: &super::visitor::AliasSearch<'_, 'db>,
    ) -> bool {
        search.search(db, self.ty)
    }

    pub(super) fn identity_projection(
        self,
        db: &'db dyn Db,
        upcast: &IdentityUpcast<'_, 'db>,
    ) -> IdentityUpcastResult<'db> {
        upcast.check(db, self.ty)
    }

    pub(super) fn references_typevar_default(self, search: TypeVarDefaultSearch<'db, '_>) -> bool {
        search.visit(self.ty)
    }

    pub(super) fn infer_binary(self, operand: BinaryOperand<'_, 'db, '_>) -> Option<Type<'db>> {
        operand.infer(self.ty)
    }

    pub(super) fn validate_classinfo(
        self,
        db: &'db dyn Db,
        checker: &ClassInfoValidator<'_, 'db>,
        mode: ClassInfoCheck,
    ) -> bool {
        checker.validate_with_mode(db, self.ty, mode)
    }

    pub(super) fn infer_as_formal(
        self,
        builder: &mut SpecializationBuilder<'db, '_>,
        actual: Type<'db>,
        polarity: super::TypeVarVariance,
        visitor: &InferSpecializationVisitor<'_, 'db>,
    ) -> Result<(), SpecializationError<'db>> {
        builder.infer_map_impl(self.ty, actual, polarity, visitor)
    }

    pub(super) fn infer_as_actual(
        self,
        builder: &mut SpecializationBuilder<'db, '_>,
        formal: Type<'db>,
        polarity: super::TypeVarVariance,
        visitor: &InferSpecializationVisitor<'_, 'db>,
    ) -> Result<(), SpecializationError<'db>> {
        builder.infer_map_impl(formal, self.ty, polarity, visitor)
    }

    pub(super) fn classinfo_constraint(
        self,
        db: &'db dyn Db,
        constraint: &ClassInfoConstraint<'_, 'db>,
    ) -> Option<Type<'db>> {
        constraint.generate(db, self.ty)
    }

    pub(super) fn map_alias_value(
        self,
        db: &'db dyn Db,
        alias: super::TypeAliasType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
    ) -> Type<'db> {
        let mapped = self
            .ty
            .apply_type_mapping_impl(db, mapping, context, self.mapping);
        if mapped == Type::TypeAlias(alias) || mapped == self.ty {
            Type::TypeAlias(alias)
        } else if let TypeMapping::Materialize(kind) = mapping
            && alias.is_recursive(db)
        {
            Type::TypeAlias(alias.with_materialization_kind(db, Some(*kind)))
        } else {
            mapped
        }
    }

    pub(super) fn collect_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding: Option<Definition<'db>>,
        typevars: &mut crate::FxOrderSet<super::BoundTypeVarInstance<'db>>,
        visitor: &super::FindLegacyTypeVarsVisitor<'_, 'db>,
    ) {
        self.ty
            .find_legacy_typevars_impl(db, self.mapping.env, binding, typevars, visitor);
    }

    pub(super) fn alternatives(
        self,
        expander: &super::type_expansion::TypeExpander<'_, 'db>,
    ) -> Option<Vec<Type<'db>>> {
        expander.expand(self.ty)
    }

    pub(super) fn upcast_to_callable(
        self,
        db: &'db dyn Db,
        policy: UpcastPolicy,
        context: &CallableUpcastContext<'_, 'db>,
        receiver: Option<Type<'db>>,
    ) -> Option<CallableTypes<'db>> {
        context.check(db, self.ty, policy, receiver)
    }

    pub(super) fn subscript_as_value(
        self,
        db: &'db dyn Db,
        checker: &SubscriptChecker<'_, 'db>,
        slice: Type<'db>,
    ) -> Result<Type<'db>, SubscriptError<'db>> {
        checker.check(db, self.ty, slice)
    }

    pub(super) fn subscript_as_index(
        self,
        db: &'db dyn Db,
        checker: &SubscriptChecker<'_, 'db>,
        value: Type<'db>,
    ) -> Result<Type<'db>, SubscriptError<'db>> {
        checker.check(db, value, self.ty)
    }
}

impl<'db> ComparisonBody<'_, 'db> {
    pub(super) fn has_same_type(&self, ty: Type<'db>) -> bool {
        self.ty == ty
    }

    pub(super) fn relate_as_source<'c>(
        self,
        db: &'db dyn Db,
        comparison: &GuardedRelation<'_, 'c, 'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        comparison.compare_expanded_pair(db, self.ty, target)
    }

    pub(super) fn relate_as_target<'c>(
        self,
        db: &'db dyn Db,
        comparison: &GuardedRelation<'_, 'c, 'db>,
        source: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        comparison.compare_expanded_pair(db, source, self.ty)
    }

    pub(super) fn disjoint_as_source<'c>(
        self,
        db: &'db dyn Db,
        checker: &DisjointnessChecker<'_, 'c, 'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        checker.check_type_pair(db, self.ty, target)
    }

    pub(super) fn disjoint_as_target<'c>(
        self,
        db: &'db dyn Db,
        checker: &DisjointnessChecker<'_, 'c, 'db>,
        source: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        checker.check_type_pair(db, source, self.ty)
    }
}

impl<'a, 'env, 'db, Mode> TypeExpansionContext<'a, 'env, 'db, Mode> {
    fn new(mapping: &'a ApplyTypeMappingVisitor<'env, 'db>) -> Self {
        Self {
            mapping,
            mode: PhantomData,
        }
    }
}

impl<'db, Mode> TypeExpansionContext<'_, '_, 'db, Mode> {
    pub(super) fn environment(&self) -> &TypeOperations<'db> {
        self.mapping.env
    }

    pub(super) fn alias_body(
        &self,
        db: &'db dyn Db,
        alias: super::TypeAliasType<'db>,
    ) -> ExpandedType<'_, 'db> {
        let access = BodyAccess {
            env: self.mapping.env,
        };
        ExpandedType {
            ty: alias.resolve_body(db, &access),
            mapping: self.mapping,
        }
    }

    pub(super) fn needs_intersection_expansion(
        &self,
        db: &'db dyn Db,
        intersection: super::IntersectionType<'db>,
    ) -> bool {
        intersection.needs_alias_expansion(db, self.mapping)
    }

    pub(super) fn comparison_intersection_body(
        &self,
        db: &'db dyn Db,
        intersection: super::IntersectionType<'db>,
    ) -> ComparisonBody<'_, 'db> {
        ComparisonBody {
            ty: intersection.expand_aliases(
                db,
                &SetExpansionContext {
                    mapping: self.mapping,
                },
            ),
            _mapping: self.mapping,
        }
    }

    pub(super) fn comparison_alias_body(
        &self,
        db: &'db dyn Db,
        alias: super::TypeAliasType<'db>,
    ) -> ComparisonBody<'_, 'db> {
        ComparisonBody {
            ty: alias.resolve_body(
                db,
                &BodyAccess {
                    env: self.mapping.env,
                },
            ),
            _mapping: self.mapping,
        }
    }

    pub(super) fn comparison_recursive_body(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> UnfoldResult<'db, ComparisonBody<'_, 'db>> {
        recursive.unfold(db, self.mapping).map(|ty| ComparisonBody {
            ty,
            _mapping: self.mapping,
        })
    }

    /// Guard signature substitution while the caller keeps its operation guard active.
    pub(super) fn expand_signature<R>(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        expand: impl FnOnce(&TypeOperations<'db>) -> R,
    ) -> Option<R> {
        let context = self.mapping.recursion_context();
        context.expand_signature(db, self.mapping.env, callable, || expand(self.mapping.env))
    }

    /// Inspect a deferred signature under this operation's recursion policy.
    pub(super) fn inspect_signatures<R>(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        inspect: impl FnOnce(super::callable::ExpandedSignatures<'_, 'db>) -> R,
    ) -> Option<R> {
        callable.inspect_signatures_in_operation(db, self, inspect)
    }
}

impl<'db> TypeExpansionContext<'_, '_, 'db> {
    /// Inspect a recursive body's outer alternatives without revisiting shared union edges.
    pub(super) fn expand_recursive(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> ExpandedRecursive<'_, 'db> {
        ExpandedRecursive {
            result: recursive
                .unfold(db, self.mapping)
                .map(|body| self.normalized_type(db, body)),
            mapping: self.mapping,
        }
    }

    /// Normalize outer aliases and unions under the caller's active recursion guard.
    pub(super) fn normalize(&self, db: &'db dyn Db, ty: Type<'db>) -> ExpandedType<'_, 'db> {
        ExpandedType {
            ty: self.normalized_type(db, ty),
            mapping: self.mapping,
        }
    }

    fn normalized_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        TypeInspector::with_mapping_visitor(
            self.mapping,
            TypeInspectionKey::Normalize,
            |inspector| inspector.inspect(db, ty, |ty, _| ty, |_, ty| ty.ty),
        )
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
            union.expand_aliases(db, &SetExpansionContext { mapping })
        }
        _ => ty,
    }
}

/// Permission to normalize a set operation inside its consuming operation.
/// Inspection normalizes union alternatives; relations normalize direct intersection elements.
pub(super) struct SetExpansionContext<'a, 'env, 'db> {
    mapping: &'a ApplyTypeMappingVisitor<'env, 'db>,
}

impl<'env, 'db> SetExpansionContext<'_, 'env, 'db> {
    pub(super) fn mapping(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        self.mapping
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
    ) -> ExpandedRecursive<'_, 'db> {
        ExpandedRecursive {
            result: recursive.unfold(db, self.mapping),
            mapping: self.mapping,
        }
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
        self.visit(db, mapping.env, item, || {
            compute(&TypeExpansionContext::new(mapping))
        })
    }

    pub(super) fn visit_structure(
        &self,
        db: &'db dyn Db,
        item: T,
        mapping: &ApplyTypeMappingVisitor<'_, 'db>,
        compute: impl FnOnce(&StructuralExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(db, mapping.env, item, || {
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
    ) -> Result<R, (T, T)> {
        self.try_visit(db, mapping.env, item, reuse_cached, || {
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
        self.visit_active(db, mapping.env, item, |cycle| match cycle {
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
        self.visit(mapping.env, item, on_cycle, || {
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
        self.visit(mapping.env, item, on_cycle, || {
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
        self.visit_type(db, mapping.env, ty, || {
            compute(&TypeExpansionContext::new(mapping))
        })
    }
}

/// Fixed inputs for walks whose recursion key contains one or two types.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum TypeVisitorKey<'db> {
    LegacyTypeVars(Option<Definition<'db>>),
    TypeVarDefault(super::typevar::TypeVarIdentity<'db>),
    SpecializationFlow,
    EnumDomain,
    BinaryComparison,
    BinaryExpression,
    Subscript(ruff_python_ast::ExprContext),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum TypeVisitorOperator {
    Binary(ruff_python_ast::Operator),
    Comparison(ruff_python_ast::CmpOp),
}

impl From<ruff_python_ast::Operator> for TypeVisitorOperator {
    fn from(operator: ruff_python_ast::Operator) -> Self {
        Self::Binary(operator)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TypeVisitorContext<'db> {
    program: Program<'db>,
    operation: TypeVisitorKey<'db>,
    operator: Option<TypeVisitorOperator>,
}

type VisitorGuard<'db> =
    CycleDetector<'db, (), (Type<'db>, TypeVisitorContext<'db>, Type<'db>), (), 3>;

/// The shared guard is independent of a walk's accumulator and result type.
#[derive(Default)]
pub(super) struct TypeVisitorRecursion<'db> {
    guard: OnceCell<Rc<VisitorGuard<'db>>>,
}

pub(super) trait TypeVisitorItem<'db>: Copy {
    fn operands(&self) -> (Type<'db>, Option<TypeVisitorOperator>, Type<'db>);
}

impl<'db> TypeVisitorItem<'db> for Type<'db> {
    fn operands(&self) -> (Type<'db>, Option<TypeVisitorOperator>, Type<'db>) {
        (*self, None, *self)
    }
}

impl<'db> TypeVisitorItem<'db> for (Type<'db>, Type<'db>) {
    fn operands(&self) -> (Type<'db>, Option<TypeVisitorOperator>, Type<'db>) {
        (self.0, None, self.1)
    }
}

impl<'db, O: Copy + Into<TypeVisitorOperator>> TypeVisitorItem<'db> for (Type<'db>, O, Type<'db>) {
    fn operands(&self) -> (Type<'db>, Option<TypeVisitorOperator>, Type<'db>) {
        (self.0, Some(self.1.into()), self.2)
    }
}

/// A visitor borrows shared guards and memoizes results within its own accumulator.
/// Every cached result retains the recursion decisions made by nested operations.
pub(super) struct TypeVisitorState<'env, 'db, T, R> {
    cycles: Rc<VisitorGuard<'db>>,
    memo: OperationMemo<'db, VisitorGuardKey<'db>, R>,
    marker: PhantomData<T>,
    mapping: ApplyTypeMappingVisitor<'env, 'db>,
    key: TypeVisitorKey<'db>,
    fallback: R,
}

impl<'env, 'db, T, R> TypeVisitorState<'env, 'db, T, R> {
    pub(super) fn mapping(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        &self.mapping
    }

    pub(super) fn new(
        env: &'env TypeOperations<'db>,
        key: TypeVisitorKey<'db>,
        fallback: R,
    ) -> Self {
        Self {
            cycles: Rc::new(CycleDetector::new(())),
            memo: OperationMemo::default(),
            marker: PhantomData,
            mapping: ApplyTypeMappingVisitor::new(env),
            key,
            fallback,
        }
    }
}

impl<'db, T, R: Clone> TypeVisitorState<'_, 'db, T, R>
where
    T: TypeVisitorItem<'db>,
{
    fn with_active<R2>(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(Option<CycleKind>) -> R2,
    ) -> R2 {
        let (left, operator, right) = item.operands();
        let key = (
            left,
            TypeVisitorContext {
                program: self.mapping.env.program(db),
                operation: self.key,
                operator,
            },
            right,
        );
        let env = self.mapping.env;
        let shared = env
            .context()
            .state()
            .type_visitors
            .guard
            .get_or_init(|| Rc::new(CycleDetector::new(())));
        let queries = &env.context().state().type_queries;
        if queries.depth.get() != 0 {
            queries.dependencies.borrow_mut().extend([
                QueryDependency::Visitor(VisitorGuardRef(self.cycles.clone()), key),
                QueryDependency::Visitor(VisitorGuardRef(shared.clone()), key),
            ]);
        }
        self.cycles.visit_active(db, env, key, |local_cycle| {
            if local_cycle.is_some() {
                return compute(local_cycle);
            }
            shared.visit_active(db, env, key, |cycle| {
                compute(cycle.map(|_| CycleKind::Reentrant))
            })
        })
    }

    pub(super) fn visit(&self, db: &'db dyn Db, item: T, compute: impl FnOnce() -> R) -> R {
        let (left, operator, right) = item.operands();
        let key = (
            left,
            TypeVisitorContext {
                program: self.mapping.env.program(db),
                operation: self.key,
                operator,
            },
            right,
        );
        self.memo.get_or_compute(db, self.mapping.env, key, || {
            self.with_active(db, item, |cycle| {
                if cycle.is_some() {
                    self.fallback.clone()
                } else {
                    compute()
                }
            })
        })
    }

    pub(super) fn visit_with_expansion(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(&TypeExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(db, item, || {
            compute(&TypeExpansionContext::new(&self.mapping))
        })
    }

    pub(super) fn visit_structure(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(&StructuralExpansionContext<'_, '_, 'db>) -> R,
    ) -> R {
        self.visit(db, item, || {
            compute(&StructuralExpansionContext::new(&self.mapping))
        })
    }
}

impl<'db, T> TypeVisitorState<'_, 'db, T, ()>
where
    T: TypeVisitorItem<'db>,
{
    pub(super) fn visit_active_with_expansion<R>(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce(Result<&TypeExpansionContext<'_, '_, 'db>, CycleKind>) -> R,
    ) -> R {
        self.with_active(db, item, |cycle| match cycle {
            Some(kind) => compute(Err(kind)),
            None => compute(Ok(&TypeExpansionContext::new(&self.mapping))),
        })
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
pub(super) trait TypeWalk<'db, State = (), Mode: TypeTraversalMode = SemanticTraversal>:
    Sized
{
    type Output;

    /// Identify the walk and the fixed inputs that distinguish independent inspections.
    fn recursion_key(&self) -> TypeWalkKey<'db>;

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &TypeWalkVisit<'_, '_, 'db, Self, Mode>,
        ty: Type<'db>,
        state: State,
    ) -> Self::Output;

    /// Return the result of revisiting an active type, including a growing specialization.
    fn on_cycle(
        &self,
        _traversal: &TypeWalkVisit<'_, '_, 'db, Self, Mode>,
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
        traversal: &TypeWalkVisit<'_, '_, 'db, Self, Mode>,
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
        _traversal: &TypeWalkVisit<'_, '_, 'db, Self, Mode>,
        _ty: Type<'db>,
        state: State,
        visit_body: impl FnOnce(State) -> Self::Output,
    ) -> Self::Output {
        visit_body(state)
    }
}

/// A walk callback can only be entered by the traversal currently inspecting its type.
pub(super) struct TypeWalkVisit<'visit, 'env, 'db, O, Mode = SemanticTraversal> {
    traversal: &'visit TypeTraversal<'env, 'db, O, Mode>,
}

impl<'env, 'db, O, Mode> Deref for TypeWalkVisit<'_, 'env, 'db, O, Mode> {
    type Target = TypeTraversal<'env, 'db, O, Mode>;

    fn deref(&self) -> &Self::Target {
        self.traversal
    }
}

pub(super) type StructuralTypeVisit<'visit, 'env, 'db, O> =
    TypeWalkVisit<'visit, 'env, 'db, O, StructuralTraversal>;

/// A walk's identity excludes mutable accumulators but retains fixed semantic inputs.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) enum TypeWalkKey<'db> {
    CallableMemberFlow,
    BuildIntersection,
    BuildUnion,
    ChargeProjectionTerms,
    CheckDescriptorWrite(Type<'db>, Type<'db>),
    ClassPatternSubject(Option<ClassLiteral<'db>>, Type<'db>),
    CollectMappingUnionTypes,
    CollectMembers,
    #[cfg(test)]
    CountAlternatives,
    DefinitePattern(ty_python_core::predicate::PatternPredicateKind<'db>),
    DescriptorWrite(Type<'db>, Type<'db>, bool),
    EnumDomain(super::EnumLiteralType<'db>),
    ExpandUnion(bool, bool, bool),
    FinalAttributeAssignment(Name),
    Inspect(TypeInspectionKey<'db>),
    IsDisjunctive,
    LaxInputType,
    ResolveAttributeWrite(Name),
    SequencePatternFallthrough(ty_python_core::predicate::SequencePatternPredicateKind<'db>),
    TupleShape,
    TypedDictPatternDomain,
    UnaryExpression(ruff_python_ast::UnaryOp),
    VisitRecursiveBody,
    VisitTypedDictField(Name),
    WalkDeferredBody,
}

struct TypeTraversalTag;

struct LiteralArgument;

impl<'db> TypeQuery<'db> for LiteralArgument {
    fn recursion_key(&self) -> TypeQueryKey<'db> {
        TypeQueryKey::LiteralArgument
    }

    type Output = Option<Type<'db>>;

    fn on_cycle(
        &self,
        _context: &TypeQueryVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        (): (),
    ) -> Self::Output {
        None
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        context: &TypeQueryVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        (): (),
    ) -> Self::Output {
        ty.is_literal_or_union_of_literals(db, context.environment())
            .then_some(ty)
    }
}

impl<'db> Type<'db> {
    /// Resolve an alias used as a `Literal` argument, retaining only literal value types.
    pub(super) fn literal_argument_type(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Option<Type<'db>> {
        LiteralArgument.evaluate(db, env, self, ())
    }
}

struct WalkDeferredBody;

impl<'db, V: super::visitor::TypeVisitor<'db> + ?Sized> TypeWalk<'db, &V, StructuralTraversal>
    for WalkDeferredBody
{
    fn recursion_key(&self) -> TypeWalkKey<'db> {
        TypeWalkKey::WalkDeferredBody
    }

    type Output = ();

    fn on_cycle(
        &self,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        visitor: &V,
    ) {
        visitor.notify_skipped_lazy_type_attributes();
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        visitor: &V,
    ) {
        if matches!(ty, Type::Recursive(_)) {
            visitor.notify_skipped_lazy_type_attributes();
        } else {
            visitor.visit_type(db, ty);
        }
    }

    fn visit_recursive_body(
        &self,
        db: &'db dyn Db,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        body: Type<'db>,
        visitor: &V,
    ) {
        visitor.visit_type(db, body);
    }
}

impl<'db> super::TypeAliasType<'db> {
    /// Inspect the declaration without applying arguments or defaults. The body cannot escape
    /// the callback, and nested reads share the operation's declaration guard.
    pub(super) fn inspect_definition<R>(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        inspect: impl FnOnce(ExpandedType<'_, 'db>) -> R,
    ) -> Option<R> {
        env.context().state().alias_definitions.visit(
            env,
            &self.definition(db),
            || None,
            || {
                let mapping = env.mapping();
                let access = BodyAccess { env };
                Some(inspect(ExpandedType {
                    ty: self.resolve_definition(db, &access),
                    mapping: &mapping,
                }))
            },
        )
    }

    /// Walk an alias within the visitor's operation, retaining the guard through the walk.
    pub(super) fn walk_body<V: super::visitor::TypeVisitor<'db> + ?Sized>(
        self,
        db: &'db dyn Db,
        visitor: &V,
    ) {
        StructuralTypeTraversal::with_root(
            visitor.program_environment(),
            WalkDeferredBody,
            |traversal| {
                traversal.visit_with(db, Type::TypeAlias(self), visitor);
            },
        );
    }
}

/// Owns the recursion and materialization state for one type operation.
///
/// A recursive body can expose a union that contains the same recursive type again. The guard
/// therefore covers both unfolding and the operation's subsequent traversal of that body.
/// This is a walk: callbacks can mutate builders or collect occurrences. Use [`TypeQueryContext`]
/// for operations that compute a reusable result from a type and immutable input.
pub(super) struct TypeTraversal<'env, 'db, O, Mode = SemanticTraversal> {
    operation: O,
    env: &'env TypeOperations<'db>,
    mapping: TraversalMapping<'env, 'db>,
    active: OnceCell<Rc<QueryGuard<'db>>>,
    shared: OnceCell<Rc<QueryGuard<'db>>>,
    mode: PhantomData<Mode>,
}

impl<'env, 'db, O, Mode: TypeTraversalMode> TypeTraversal<'env, 'db, O, Mode> {
    fn new(env: &'env TypeOperations<'db>, operation: O) -> Self {
        Self {
            operation,
            env,
            mapping: TraversalMapping::Owned(OnceCell::new()),
            active: OnceCell::new(),
            shared: OnceCell::new(),
            mode: PhantomData,
        }
    }

    /// Run an operation with one borrowed traversal for its entire recursion.
    pub(crate) fn with_root<R>(
        env: &'env TypeOperations<'db>,
        operation: O,
        visit: impl FnOnce(&Self) -> R,
    ) -> R {
        visit(&Self::new(env, operation))
    }

    /// Keep deferred signature expansion active while the consumer visits its contents.
    fn inspect_signatures<R>(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        inspect: impl FnOnce(super::callable::ExpandedSignatures<'_, 'db>) -> R,
    ) -> Option<R> {
        callable.inspect_signatures(db, self.environment(), inspect)
    }

    pub(crate) fn environment(&self) -> &TypeOperations<'db> {
        self.env
    }

    fn active(&self) -> &Rc<QueryGuard<'db>> {
        self.active.get_or_init(|| Rc::new(CycleDetector::new(())))
    }

    fn mapping(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        match &self.mapping {
            TraversalMapping::Owned(mapping) => {
                mapping.get_or_init(|| ApplyTypeMappingVisitor::new(self.env))
            }
            TraversalMapping::Borrowed(mapping) => mapping,
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
            mapping: TraversalMapping::Borrowed(visitor),
            active: OnceCell::new(),
            shared: OnceCell::new(),
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
        let key = || TypeQueryKey::Walk(self.operation.recursion_key());
        self.with_shared_guard(db, ty, key, |cycle| {
            let visit = TypeWalkVisit { traversal: self };
            if let Some(kind) = cycle {
                return self.operation.on_cycle(&visit, ty, kind, state);
            }
            match ty {
                Type::Recursive(recursive) => {
                    self.operation
                        .visit_alias_like(db, &visit, ty, state, |state| {
                            match recursive.unfold(db, self.mapping()) {
                                UnfoldResult::Unfolded(body) => {
                                    self.operation.visit_recursive_body(db, &visit, body, state)
                                }
                                UnfoldResult::Unchanged(_) => {
                                    self.operation.visit(db, &visit, ty, state)
                                }
                            }
                        })
                }
                Type::TypeAlias(alias) => {
                    self.operation
                        .visit_alias_like(db, &visit, ty, state, |state| {
                            let access = BodyAccess { env: self.env };
                            self.visit_with(db, alias.resolve_body(db, &access), state)
                        })
                }
                Type::Union(_) if Mode::NORMALIZE_UNIONS => {
                    let normalized = normalize_outer_union(db, ty, self.mapping());
                    if normalized == ty {
                        self.operation.visit(db, &visit, ty, state)
                    } else {
                        self.visit_with(db, normalized, state)
                    }
                }
                _ => self.operation.visit(db, &visit, ty, state),
            }
        })
    }

    fn with_shared_guard<R>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        key: impl FnOnce() -> TypeQueryKey<'db>,
        visit: impl FnOnce(Option<CycleKind>) -> R,
    ) -> R {
        if !needs_traversal_guard(ty) {
            return visit(None);
        }
        let shared = self.shared.get_or_init(|| {
            let identity = TypeQueryIdentity {
                program: self.env.program(db),
                key: key(),
            };
            self.env.context().state().type_queries.guard(&identity)
        });
        let queries = &self.env.context().state().type_queries;
        if queries.depth.get() != 0 {
            queries.dependencies.borrow_mut().extend([
                QueryDependency::Type(QueryGuardRef(self.active().clone()), ty),
                QueryDependency::Type(QueryGuardRef(shared.clone()), ty),
            ]);
        }
        self.with_active_type(db, ty, |local_cycle| {
            if local_cycle.is_some() {
                return visit(local_cycle);
            }
            shared.visit_active(db, self.env, ty, |shared_cycle| {
                visit(shared_cycle.map(|_| CycleKind::Reentrant))
            })
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
            self.active().visit_active(db, self.env, ty, visit)
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
pub(super) trait TypeQuery<'db, Input = ()>: Sized
where
    Input: Clone + Eq + Hash,
{
    type Output: Clone;

    /// Evaluate one root while memoizing its recursive subqueries. The final root result is
    /// returned directly because this context is discarded as soon as the query finishes.
    fn evaluate(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        ty: Type<'db>,
        input: Input,
    ) -> Self::Output {
        TypeQueryContext::new(env, self).visit_uncached(db, ty, input)
    }

    /// Identify this query and the immutable inputs shared by its recursive calls.
    fn recursion_key(&self) -> TypeQueryKey<'db>;

    fn visit(
        &self,
        db: &'db dyn Db,
        context: &TypeQueryVisit<'_, '_, 'db, Self, Input>,
        ty: Type<'db>,
        input: Input,
    ) -> Self::Output;

    fn on_cycle(
        &self,
        context: &TypeQueryVisit<'_, '_, 'db, Self, Input>,
        ty: Type<'db>,
        kind: CycleKind,
        input: Input,
    ) -> Self::Output;

    /// Keep guards shared with other operations active while inspecting an alias body.
    fn visit_alias_like(
        &self,
        _db: &'db dyn Db,
        _context: &TypeQueryVisit<'_, '_, 'db, Self, Input>,
        _ty: Type<'db>,
        input: Input,
        visit_body: impl FnOnce(Input) -> Self::Output,
    ) -> Self::Output {
        visit_body(input)
    }
}

/// A query callback can only be entered by the context currently inspecting its type.
pub(super) struct TypeQueryVisit<'visit, 'env, 'db, Q, Input = ()>
where
    Q: TypeQuery<'db, Input>,
    Input: Clone + Eq + Hash,
{
    context: &'visit TypeQueryContext<'env, 'db, Q, Input>,
}

impl<'env, 'db, Q, Input> Deref for TypeQueryVisit<'_, 'env, 'db, Q, Input>
where
    Q: TypeQuery<'db, Input>,
    Input: Clone + Eq + Hash,
{
    type Target = TypeQueryContext<'env, 'db, Q, Input>;

    fn deref(&self) -> &Self::Target {
        self.context
    }
}

/// The operation and immutable inputs whose active recursion must survive nested queries.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) enum TypeQueryKey<'db> {
    Walk(TypeWalkKey<'db>),
    IsDefinitelyNonDataDescriptor,
    IsDataDescriptor(bool),
    LookupMroMember(Name, super::MemberLookupPolicy),
    LookupInstanceMember(Name),
    DescriptorGet(Option<Type<'db>>, Type<'db>),
    LookupMember(super::MemberLookupKey<'db>),
    FindDefinition,
    ProjectInstance,
    TypeExpression(
        ScopeId<'db>,
        Option<Definition<'db>>,
        super::infer::InferenceFlags,
    ),
    CollectBindings(Option<Type<'db>>),
    ProjectMetaType,
    IsSingleton,
    InspectGenerator(super::GeneratorTypeMode),
    GeneratorAnnotationSendType,
    IncompatibleYieldFromSendType(Type<'db>),
    TypeFormArgument,
    LiteralArgument,
    TryBool(bool),
    CanSuppressExceptions(bool),
    NarrowWithPresentKey(Name),
    ContainsTypedDict(Option<Name>),
    RuntimeClass,
    FunctionDescriptorGet(Option<Type<'db>>, Option<Type<'db>>),
    MethodDescriptor(super::known_instance::MethodWrapperKind),
    ShadowedDescriptorWriteType,
    TryIterate(bool),
    HasExactRuntimeClass,
    EnumPatternDomain(super::EnumLiteralType<'db>),
    UnpackTypedDict,
    ResolveClassBase(Option<ClassLiteral<'db>>),
    FindTypeCategory,
    StringLiteralCandidates,
    EnumLiteralSubjectNames,
    AttributeDeletion(Name),
    PropagateCallableKind(super::callable::CallableTypeKind),
    MissingAttribute(Name),
    ContainsTypeFormValue(ScopeId<'db>, ExpressionNodeKey),
    InstanceTupleCovers(super::tuple::TupleSpec<'db>),
    InstanceTruthiness(ClassLiteral<'db>),
    ClassBinding(Type<'db>, bool),
    TypedDictKeyExpectedType,
    FindFinalAttribute(Name),
    #[cfg(test)]
    SumGraph,
    #[cfg(test)]
    HasSharedGuard,
    #[cfg(test)]
    SignatureAvailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct TypeQueryIdentity<'db> {
    program: Program<'db>,
    key: TypeQueryKey<'db>,
}

type QueryGuard<'db> = CycleDetector<'db, TypeTraversalTag, Type<'db>, (), 3>;
#[derive(Clone, Eq, PartialEq, Hash)]
enum QueryDependency<'db> {
    Type(QueryGuardRef<'db>, Type<'db>),
    Visitor(VisitorGuardRef<'db>, VisitorGuardKey<'db>),
    Comparison(super::relation::ComparisonKey<'db>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RecursionDecision<'db> {
    Cycle(Option<CycleKind>),
    Comparison(Option<(Type<'db>, Type<'db>)>),
}

type VisitorGuardKey<'db> = (Type<'db>, TypeVisitorContext<'db>, Type<'db>);

#[derive(Clone)]
struct VisitorGuardRef<'db>(Rc<VisitorGuard<'db>>);

impl PartialEq for VisitorGuardRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for VisitorGuardRef<'_> {}
impl Hash for VisitorGuardRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

#[derive(Clone)]
struct QueryGuardRef<'db>(Rc<QueryGuard<'db>>);

impl PartialEq for QueryGuardRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for QueryGuardRef<'_> {}

impl Hash for QueryGuardRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

/// Active query guards belong to the analysis owner. A helper cannot reset them by starting
/// another query context. The dependency log also spans query contexts, so a cached answer
/// retains the guards consulted by nested queries, including their cache hits.
#[derive(Default)]
pub(super) struct TypeQueryRecursion<'db> {
    guards: RefCell<FxHashMap<TypeQueryIdentity<'db>, Rc<QueryGuard<'db>>>>,
    dependencies: RefCell<Vec<QueryDependency<'db>>>,
    depth: Cell<usize>,
}

impl<'db> TypeQueryRecursion<'db> {
    fn guard(&self, key: &TypeQueryIdentity<'db>) -> Rc<QueryGuard<'db>> {
        self.guards
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| Rc::new(CycleDetector::new(())))
            .clone()
    }

    pub(super) fn observe_comparison(&self, key: super::relation::ComparisonKey<'db>) {
        if self.depth.get() != 0 {
            self.dependencies
                .borrow_mut()
                .push(QueryDependency::Comparison(key));
        }
    }

    fn decision(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        dependency: &QueryDependency<'db>,
    ) -> RecursionDecision<'db> {
        match dependency {
            QueryDependency::Type(guard, ty) => {
                RecursionDecision::Cycle(guard.0.cycle_kind(db, env, ty))
            }
            QueryDependency::Visitor(guard, key) => {
                RecursionDecision::Cycle(guard.0.cycle_kind(db, env, key))
            }
            QueryDependency::Comparison(key) => RecursionDecision::Comparison(
                env.context()
                    .state()
                    .comparisons
                    .get()
                    .and_then(|guard| guard.decision(db, env, *key)),
            ),
        }
    }
}

/// Shares completed queries as well as recursion and materialization state.
///
/// A cycle result can depend on the enclosing types. Each cached answer therefore records the
/// guards consulted by its computation. Reuse requires those guards to give the same answers
/// in the current context, including guards reached through another cached query.
/// Changes to guards shared with other operations invalidate the cache as well.
pub(super) struct TypeQueryContext<'env, 'db, Q, Input = ()>
where
    Q: TypeQuery<'db, Input>,
    Input: Clone + Eq + Hash,
{
    traversal: TypeTraversal<'env, 'db, Q>,
    memo: OperationMemo<'db, (Type<'db>, Input), Q::Output>,
}

impl<'env, 'db, Q, Input> TypeQueryContext<'env, 'db, Q, Input>
where
    Q: TypeQuery<'db, Input>,
    Input: Clone + Eq + Hash,
{
    fn new(env: &'env TypeOperations<'db>, query: Q) -> Self {
        Self {
            traversal: TypeTraversal::new(env, query),
            memo: OperationMemo::default(),
        }
    }

    /// Run a query with one cache and one set of guards for all recursive calls.
    #[cfg(test)]
    pub(super) fn with_root<R>(
        env: &'env TypeOperations<'db>,
        query: Q,
        visit: impl FnOnce(&Self) -> R,
    ) -> R {
        visit(&Self::new(env, query))
    }

    pub(crate) fn environment(&self) -> &TypeOperations<'db> {
        self.traversal.environment()
    }

    pub(super) fn inspect_signatures<R>(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        inspect: impl FnOnce(super::callable::ExpandedSignatures<'_, 'db>) -> R,
    ) -> Option<R> {
        callable.inspect_signatures(db, self.environment(), inspect)
    }

    fn mapping_visitor(&self) -> &ApplyTypeMappingVisitor<'env, 'db> {
        self.traversal.mapping()
    }

    pub(crate) fn visit_with(&self, db: &'db dyn Db, ty: Type<'db>, input: Input) -> Q::Output {
        self.memo
            .get_or_compute(db, self.environment(), (ty, input.clone()), || {
                self.visit_uncached(db, ty, input)
            })
    }

    fn visit_uncached(&self, db: &'db dyn Db, ty: Type<'db>, input: Input) -> Q::Output {
        let traversal = &self.traversal;
        let key = || traversal.operation.recursion_key();
        traversal.with_shared_guard(db, ty, key, |cycle| {
            let visit = TypeQueryVisit { context: self };
            let query = &self.traversal.operation;
            if let Some(kind) = cycle {
                return query.on_cycle(&visit, ty, kind, input);
            }
            match ty {
                Type::TypeAlias(alias) => query.visit_alias_like(db, &visit, ty, input, |input| {
                    let access = BodyAccess {
                        env: self.environment(),
                    };
                    self.visit_with(db, alias.resolve_body(db, &access), input)
                }),
                Type::Recursive(recursive) => {
                    query.visit_alias_like(db, &visit, ty, input, |input| {
                        match recursive.unfold(db, self.mapping_visitor()) {
                            UnfoldResult::Unfolded(body) => self.visit_with(db, body, input),
                            UnfoldResult::Unchanged(_) => query.visit(db, &visit, ty, input),
                        }
                    })
                }
                Type::Union(_) => {
                    let expanded = normalize_outer_union(db, ty, self.mapping_visitor());
                    if expanded == ty {
                        query.visit(db, &visit, ty, input)
                    } else {
                        self.visit_with(db, expanded, input)
                    }
                }
                _ => query.visit(db, &visit, ty, input),
            }
        })
    }
}

impl<'db, Q: TypeQuery<'db>> TypeQueryContext<'_, 'db, Q> {
    pub(super) fn visit(&self, db: &'db dyn Db, ty: Type<'db>) -> Q::Output {
        self.visit_with(db, ty, ())
    }
}

/// A completed result is reusable only while every recursion decision it observed is unchanged.
/// The dependency log spans query and visitor contexts, including cache hits in nested operations.
pub(super) struct OperationMemo<'db, K, R> {
    cache: RefCell<CycleDetectorCache<K, Rc<TypeQueryResult<'db, R>>>>,
}

impl<K, R> Default for OperationMemo<'_, K, R> {
    fn default() -> Self {
        Self {
            cache: RefCell::new(CycleDetectorCache::new()),
        }
    }
}

impl<'db, K: Eq + Hash, R: Clone> OperationMemo<'db, K, R> {
    pub(super) fn get_or_compute(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        key: K,
        compute: impl FnOnce() -> R,
    ) -> R {
        let shared = &env.context().state().type_queries;
        shared.depth.set(shared.depth.get() + 1);
        let _frame = TypeQueryFrame {
            depth: &shared.depth,
            dependencies: &shared.dependencies,
        };
        let cached = self.cache.borrow().get(&key).cloned();
        let operation_revision = env.context().state().operation.revision();
        if let Some(cached) = cached
            && cached.shared_dependencies_match(env)
            && (cached.operation_revision.get() == operation_revision
                || cached.dependencies.iter().all(|(dependency, expected)| {
                    TypeQueryRecursion::decision(db, env, dependency) == *expected
                }))
            && cached.shared_dependencies_match(env)
        {
            cached.operation_revision.set(operation_revision);
            let guards = env.context().guard_states();
            for &(index, _) in &cached.shared_dependencies {
                if let Some(guard) = guards[index] {
                    guard.observe();
                }
            }
            shared.dependencies.borrow_mut().extend(
                cached
                    .dependencies
                    .iter()
                    .map(|(dependency, _)| dependency.clone()),
            );
            return cached.output.clone();
        }
        let before = env
            .context()
            .guard_states()
            .map(|guard| guard.map_or((0, 0), |guard| (guard.revision, guard.observations())));
        let start = shared.dependencies.borrow().len();
        let output = compute();
        // Evaluate dependencies after releasing this operation's guards. An answer computed in
        // A -> B -> A still records B's reliance on the enclosing A when B finishes.
        #[expect(
            clippy::mutable_key_type,
            reason = "Guard equality and hashing use the stable allocation address"
        )]
        let dependencies: FxHashSet<_> = shared.dependencies.borrow_mut().drain(start..).collect();
        shared
            .dependencies
            .borrow_mut()
            .extend(dependencies.iter().cloned());
        let dependencies = dependencies
            .into_iter()
            .map(|dependency| {
                let decision = TypeQueryRecursion::decision(db, env, &dependency);
                (dependency, decision)
            })
            .collect();
        let mut shared_dependencies = Vec::new();
        for (index, guard) in env.context().guard_states().into_iter().enumerate() {
            let Some(guard) = guard else {
                continue;
            };
            let (revision, observations) = before[index];
            if guard.observations() == observations {
                continue;
            }
            // An incomplete nested computation can change an enclosing cycle's state. Its
            // result cannot be reused without repeating that effect on the enclosing operation.
            if guard.revision != revision {
                return output;
            }
            shared_dependencies.push((index, revision));
        }
        self.cache.borrow_mut().insert(
            key,
            Rc::new(TypeQueryResult {
                output: output.clone(),
                dependencies,
                shared_dependencies: shared_dependencies.into_boxed_slice(),
                operation_revision: Cell::new(operation_revision),
            }),
        );
        output
    }
}

struct TypeQueryResult<'db, R> {
    output: R,
    dependencies: Box<[(QueryDependency<'db>, RecursionDecision<'db>)]>,
    shared_dependencies: Box<[(usize, u64)]>,
    operation_revision: Cell<u64>,
}

impl<R> TypeQueryResult<'_, R> {
    fn shared_dependencies_match(&self, env: &TypeOperations<'_>) -> bool {
        let guards = env.context().guard_states();
        self.shared_dependencies
            .iter()
            .all(|&(index, expected)| guards[index].map_or(0, |guard| guard.revision) == expected)
    }
}

/// The dependency log belongs to active queries; completed roots retain only their cache entries.
struct TypeQueryFrame<'a, 'db> {
    depth: &'a Cell<usize>,
    dependencies: &'a RefCell<Vec<QueryDependency<'db>>>,
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

/// Identifies the consumer of an expanded outer type. Distinct operands use distinct keys,
/// while recursive calls to the same consumer retain the owner's active guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum TypeInspectionKey<'db> {
    ConstrainedTypevarSubset,
    EnumComplementPositive,
    EnumComplementNegative,
    IntersectionAtom,
    IntersectionGeneric,
    ProjectIntersectionTypevar,
    MergeAmbiguousReturn(usize),
    AddEqualEnumLiterals,
    ApplyTypeContext(ScopeId<'db>, ExpressionNodeKey),
    BindSelfWithReceiver,
    Build,
    BuiltinLiteralConstraint,
    CallableParamspecAndReturn,
    CanInferGenericTypedDictConstructor,
    CheckClassMethodTypevarVariance,
    CheckParamspecReturnPair,
    ClassPatternContexts,
    ClassPatternPositionalResult,
    ClassifyRedundantCondition,
    CollectImplementationRootClasses,
    ComparisonLeft,
    ComparisonLength,
    ComparisonRight,
    ConstructorForTypevarBound,
    ConstructorReturnsInstance,
    ContainmentBehavior,
    DescriptorDecoratedProtocolMember,
    DescriptorSetterDomain,
    DescriptorWriteDomain,
    DivergenceAfterAliases,
    EnumLiteralConstraint,
    EqualityExclusionConstraint,
    EvaluateExprCompareOp,
    EvaluateExprIn,
    EvaluateExprNotIn,
    EvaluateTypeComparison,
    ExactTypevarUpperBound,
    ExposedFunctionBinding,
    ExtractUnpackedTypedDictKeysFromKwargsAnnotation,
    FilterTupleSubscript,
    FilterUnion,
    FunctionBinding,
    HasGenericInstanceAttribute,
    HasOuterType,
    HasReflexiveEqualitySemantics,
    IdentityTypeVarLeft,
    IdentityTypeVarRight,
    InferClassDefinition(Definition<'db>),
    InferDictExpression(ScopeId<'db>, ExpressionNodeKey),
    InferFunctionDefinition(Definition<'db>),
    InferLambdaExpression(ScopeId<'db>, ExpressionNodeKey),
    InferParameterizedSpecialFormTypeExpression(ScopeId<'db>, ExpressionNodeKey),
    InferStarredTypeExpression(ScopeId<'db>, ExpressionNodeKey),
    InferSubclassOfTypeExpression(ScopeId<'db>, ExpressionNodeKey),
    InferTypeFormContextualExpression(ScopeId<'db>, ExpressionNodeKey),
    InspectSignaturesRoot,
    IsBuiltinLiteralType,
    IsLiteralOrUnionOfLiterals,
    IsSelfLikeConstructorReturnTypevar,
    IsStringKeyedMapping,
    KnownConstructorPreservesValueType,
    MappingPatternValueTypeForArm,
    MatchPatternSubjectArms,
    MayPreferDeclaredType,
    NarrowIdentityResolved,
    NarrowNominalAttribute,
    NarrowNominalAttributeByTruthiness,
    NarrowStringMembership,
    NarrowTargets,
    NarrowTypeByLenComparison,
    NominalClass,
    Normalize,
    NormalizeConstraintBound,
    NormalizedConstructorReturn,
    ParamspecParameterSource,
    PossibleClassAttributeDescriptor,
    PreserveOriginalSubjectType,
    ResolvedIdentityLeft,
    ResolvedIdentityRight,
    SingletonIntersection,
    SpecializeGenericClassFromCallableContext,
    SpecializeGenericClassFromContext,
    TruthinessTestOfNoneUnionFromType(ScopeId<'db>, ExpressionNodeKey),
    TypeFormSubclass,
    UnpackedKeywordIsGradual,
    UnpackedTypedDict,
    UnsafeNarrowableBuiltinSemantics,
    ValidateSubscriptAssignmentImpl(ScopeId<'db>, ExpressionNodeKey),
    ValueHasExactKnownClass,
}

struct InspectType<'db>(TypeInspectionKey<'db>);

/// A normalized type together with the outer structure needed by union transformations.
pub(super) struct NormalizedType<'scope, 'db> {
    pub(super) ty: ExpandedType<'scope, 'db>,
    /// A union can normalize to a single type. That result is still an alternative to filter or
    /// use as an inference context, unlike an alias whose body was never a union.
    pub(super) was_union: bool,
}

/// A scoped inspection that keeps recursion guards active while normalizing outer alternatives.
pub(crate) struct TypeInspector<'env, 'db> {
    traversal: StructuralTypeTraversal<'env, 'db, InspectType<'db>>,
}

struct Inspection<'a, 'env, 'db, C, F> {
    inspector: &'a TypeInspector<'env, 'db>,
    on_cycle: C,
    inspect: F,
    was_union: bool,
}

impl<'a, 'env, 'db, R, C, F> TypeWalk<'db, Inspection<'a, 'env, 'db, C, F>, StructuralTraversal>
    for InspectType<'db>
where
    C: FnOnce(Type<'db>, CycleKind) -> R,
    F: FnOnce(&TypeInspector<'_, 'db>, NormalizedType<'_, 'db>) -> R,
{
    fn recursion_key(&self) -> TypeWalkKey<'db> {
        TypeWalkKey::Inspect(self.0)
    }

    type Output = R;

    fn on_cycle(
        &self,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        kind: CycleKind,
        state: Inspection<'a, 'env, 'db, C, F>,
    ) -> R {
        (state.on_cycle)(ty, kind)
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
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
                ty: ExpandedType {
                    ty,
                    mapping: traversal.mapping(),
                },
                was_union: state.was_union,
            },
        )
    }
}

impl<'env, 'db> TypeInspector<'env, 'db> {
    pub(super) fn inspect_signatures<R>(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        inspect: impl FnOnce(super::callable::ExpandedSignatures<'_, 'db>) -> R,
    ) -> Option<R> {
        self.traversal.inspect_signatures(db, callable, inspect)
    }

    /// Transform signatures with this inspection's mapping and recursion state.
    pub(super) fn map_signatures(
        &self,
        db: &'db dyn Db,
        signatures: &super::signatures::CallableSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> super::signatures::CallableSignature<'db> {
        signatures.apply_type_mapping_impl(db, mapping, tcx, self.traversal.mapping())
    }

    pub(super) fn scope<R>(
        env: &'env TypeOperations<'db>,
        key: TypeInspectionKey<'db>,
        inspect: impl FnOnce(&Self) -> R,
    ) -> R {
        inspect(&Self {
            traversal: StructuralTypeTraversal::new(env, InspectType(key)),
        })
    }

    fn with_mapping_visitor<R>(
        mapping: &'env ApplyTypeMappingVisitor<'env, 'db>,
        key: TypeInspectionKey<'db>,
        inspect: impl FnOnce(&Self) -> R,
    ) -> R {
        inspect(&Self {
            traversal: StructuralTypeTraversal::from_mapping_visitor(mapping, InspectType(key)),
        })
    }

    pub(super) fn environment(&self) -> &TypeOperations<'db> {
        self.traversal.environment()
    }

    /// Start a complete inspection. Recursive helpers borrow the inspector passed to `inspect`.
    pub(super) fn inspect_root<R>(
        db: &'db dyn Db,
        env: &'env TypeOperations<'db>,
        key: TypeInspectionKey<'db>,
        ty: Type<'db>,
        on_cycle: impl FnOnce(Type<'db>, CycleKind) -> R,
        inspect: impl FnOnce(&TypeInspector<'_, 'db>, ExpandedType<'_, 'db>) -> R,
    ) -> R {
        Self::scope(env, key, |inspector| {
            inspector.inspect(db, ty, on_cycle, inspect)
        })
    }

    pub(super) fn inspect<R>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        on_cycle: impl FnOnce(Type<'db>, CycleKind) -> R,
        inspect: impl FnOnce(&TypeInspector<'_, 'db>, ExpandedType<'_, 'db>) -> R,
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
        inspect: impl FnOnce(&TypeInspector<'_, 'db>, NormalizedType<'_, 'db>) -> R,
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
///
/// Salsa generates public constructors for interned structs. Keeping this identity opaque
/// prevents callers from constructing recursive variables or bodies outside this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecursiveCycle(RecursiveCycleKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RecursiveCycleKind {
    Alias(salsa::Id),
    TypedDictValues(salsa::Id),
}

impl get_size2::GetSize for RecursiveCycle {}

impl RecursiveCycle {
    fn id(self) -> salsa::Id {
        match self.0 {
            RecursiveCycleKind::Alias(id) | RecursiveCycleKind::TypedDictValues(id) => id,
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
            cycle_initial=|_, id, _, ()| AliasCycleSummary::from_cycle(id),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn cycle_summary_impl<'db>(
            db: &'db dyn Db,
            recursive: RecursiveType<'db>,
            (): (),
        ) -> AliasCycleSummary<'db> {
            let env = TypeOperations::new_root(
                db,
                ProgramEnvironment::from_definition(recursive.definition(db)),
            );
            let body = match recursive.body(db) {
                RecursiveBody::Type(body) => body,
                RecursiveBody::TypedDictValues(class) => {
                    TypedDictType::new(class.identity_specialization(db))
                        .unfold_value_type(db, &env)
                }
            };
            let mut summary = AliasCycleSummary::from_type(db, &env, body);
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
        let cycle = RecursiveCycle(RecursiveCycleKind::Alias(cycle));
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
            RecursiveCycle(RecursiveCycleKind::TypedDictValues(definition.as_id())),
            RecursiveBody::TypedDictValues(origin),
            arguments,
            None,
        ))
    }

    /// Close recursive occurrences after inferring an alias's constructor expression.
    pub(super) fn recover(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
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
            &env.in_environment(ProgramEnvironment::from_definition(definition)),
            result,
        )
    }

    /// Bind references to this alias's query cycle in a closed inference result.
    /// Each bound occurrence retains its arguments and refers to this constructor's cycle.
    fn bind(self, db: &'db dyn Db, env: &TypeOperations<'db>, original: Type<'db>) -> Type<'db> {
        let body = RecursiveReferenceMapper::new(RecursiveSubstitution::Bind(self.cycle(db)))
            .map_type(db, original);
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db, env) {
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
        env: &TypeOperations<'db>,
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
    /// Materializing the body can inspect an argument or simplify a union, which can unfold
    /// another application of this constructor. Reuse the caller's mapping visitor so those
    /// operations retain the active transformations and comparison guards.
    fn unfold(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> UnfoldResult<'db> {
        // Materialize the original body so each recursive reference is transformed according
        // to its variance, rather than inheriting the enclosing application's materialization.
        let unfolded = self
            .with_materialization(db, None)
            .unfolded_body(db, visitor);
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

    /// Close and specialize one body while retaining the enclosing operation.
    fn unfolded_body(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let body = match self.body(db) {
            RecursiveBody::Type(body) => body,
            RecursiveBody::TypedDictValues(origin) => {
                let constructor = self.constructor(db);
                if self == constructor {
                    return TypedDictType::new(origin.identity_specialization(db))
                        .unfold_value_type(db, visitor.env);
                }

                // Specialize one shared constructor body. Reading each specialized schema can
                // expose different inference approximations, so growth analysis and unfolding
                // would follow different recursive references.
                let body = constructor.unfolded_body(db, visitor);
                let Some(arguments) = self.arguments(db) else {
                    return body;
                };
                let env = visitor.env.in_environment(self.environment(db));
                return body.apply_type_mapping_impl(
                    db,
                    &TypeMapping::from_specialization(db, arguments, false),
                    TypeContext::default(),
                    &ApplyTypeMappingVisitor::new_without_union_normalization(&env),
                );
            }
        };
        let env = visitor.env.in_environment(self.environment(db));
        let unfolded =
            RecursiveReferenceMapper::new(RecursiveSubstitution::Unfold(self)).map_type(db, body);
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
        env: &TypeOperations<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = env.in_environment(self.environment(db));
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
    fn into_type(self) -> Type<'db> {
        match self {
            Self::Unfolded(ty) => ty,
            Self::Unchanged(recursive) => Type::Recursive(recursive),
        }
    }
}

impl<'db, T> UnfoldResult<'db, T> {
    /// Return the contained value if unfolding made progress.
    #[inline]
    fn into_unfolded(self) -> Option<T> {
        match self {
            Self::Unfolded(value) => Some(value),
            Self::Unchanged(_) => None,
        }
    }

    /// Transform the contained value, preserving the original recursive type if unfolding made no progress.
    #[inline]
    pub(super) fn map<U>(self, operation: impl FnOnce(T) -> U) -> UnfoldResult<'db, U> {
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
        _env: &TypeOperations<'db>,
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

fn pattern_filtering_arms<'db>(
    db: &'db dyn Db,
    original: Type<'db>,
    ty: Type<'db>,
) -> smallvec::SmallVec<[(Type<'db>, Type<'db>); 2]> {
    match ty {
        Type::Union(union) => union
            .elements(db)
            .iter()
            .map(|ty| (original, *ty))
            .collect(),
        ty => smallvec::smallvec![(original, ty)],
    }
}

impl<'db> NormalizedType<'_, 'db> {
    pub(super) fn narrow_targets(
        self,
        db: &'db dyn Db,
        inspector: &TypeInspector<'_, 'db>,
    ) -> Option<std::borrow::Cow<'db, [Type<'db>]>> {
        match self.ty.ty {
            Type::Union(union) => Some(std::borrow::Cow::Borrowed(union.elements(db))),
            ty if self.was_union => Some(std::borrow::Cow::Owned(vec![ty])),
            Type::NewTypeInstance(newtype) => inspector.inspect_normalized(
                db,
                newtype.concrete_base_type(db, inspector.environment()),
                |_, _| None,
                |inspector, normalized| normalized.narrow_targets(db, inspector),
            ),
            _ => None,
        }
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

    use super::{
        CycleKind, TypeInspectionKey, TypeInspector, TypeQuery, TypeQueryContext, TypeQueryKey,
        TypeQueryVisit, TypeTraversal, TypeWalk, TypeWalkKey, TypeWalkVisit,
    };
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
            fn recursion_key(&self) -> TypeWalkKey<'db> {
                TypeWalkKey::CountAlternatives
            }

            type Output = Vec<Type<'db>>;

            fn visit(
                &self,
                db: &'db dyn Db,
                traversal: &TypeWalkVisit<'_, '_, 'db, Self>,
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
                _traversal: &TypeWalkVisit<'_, '_, 'db, Self>,
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
                            expansion.normalize(&db, left).ty,
                            expansion.normalize(&db, right).ty,
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
            let normalized =
                TypeInspector::scope(&env, TypeInspectionKey::Normalize, |inspector| {
                    inspector.inspect_normalized(
                        &db,
                        ty,
                        |_, _| None,
                        |_, normalized| {
                            Some((
                                normalized
                                    .ty
                                    .has_same_type(KnownClass::Int.to_instance(&db, &env)),
                                normalized.was_union,
                            ))
                        },
                    )
                });
            assert_eq!(normalized, Some((true, was_union)));
        }
        Ok(())
    }

    struct SumGraph<'db> {
        edges: FxHashMap<Type<'db>, Vec<Type<'db>>>,
        visits: RefCell<FxHashMap<(Type<'db>, u64), usize>>,
    }

    impl<'db> TypeQuery<'db, u64> for SumGraph<'db> {
        fn recursion_key(&self) -> TypeQueryKey<'db> {
            TypeQueryKey::SumGraph
        }

        type Output = u64;

        fn visit(
            &self,
            db: &'db dyn Db,
            context: &TypeQueryVisit<'_, '_, 'db, Self, u64>,
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
            _context: &TypeQueryVisit<'_, '_, 'db, Self, u64>,
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
            fn recursion_key(&self) -> TypeQueryKey<'db> {
                TypeQueryKey::HasSharedGuard
            }

            type Output = bool;

            fn visit(
                &self,
                _db: &'db dyn Db,
                _context: &TypeQueryVisit<'_, '_, 'db, Self>,
                _ty: Type<'db>,
                (): (),
            ) -> bool {
                !self.0.meta_type().typevars.is_empty()
            }

            fn on_cycle(
                &self,
                _context: &TypeQueryVisit<'_, '_, 'db, Self>,
                _ty: Type<'db>,
                _kind: CycleKind,
                (): (),
            ) -> bool {
                false
            }
        }

        let db = setup_db();
        let env = db.program_environment();
        let shared = env.context();
        let typevar = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let key = (env.program(&db), typevar.identity(&db));
        TypeQueryContext::with_root(&env, HasSharedGuard(shared), |query| {
            let ty = Type::int_literal(0);
            assert!(!query.visit(&db, ty));
            shared
                .meta_type()
                .typevars
                .visit(&env, &key, || (), || assert!(query.visit(&db, ty)));
            assert!(!query.visit(&db, ty));
        });
    }

    #[test]
    fn operation_cache_tracks_only_observed_shared_guards() {
        let db = setup_db();
        let env = db.program_environment();
        let shared = env.context();
        let [first, second] = ["T", "U"].map(|name| {
            let typevar = BoundTypeVarInstance::synthetic(
                &db,
                &env,
                Name::new(name),
                TypeVarVariance::Invariant,
            );
            (env.program(&db), typevar.identity(&db))
        });
        let memo = super::OperationMemo::default();
        let visits = Cell::new(0);
        let read = || {
            memo.get_or_compute(&db, &env, (), || {
                visits.set(visits.get() + 1);
                shared.meta_type().typevars.contains(&first)
            })
        };

        assert!(!read());
        shared
            .state()
            .normalization
            .visit(&env, &(), || (), || assert!(!read()));
        assert_eq!(visits.get(), 1);

        shared
            .meta_type()
            .typevars
            .visit(&env, &first, || (), || assert!(read()));
        shared
            .meta_type()
            .typevars
            .visit(&env, &second, || (), || assert!(!read()));
        assert_eq!(visits.get(), 3);
    }

    #[test]
    fn operation_cache_propagates_shared_dependencies_through_cache_hits() {
        let db = setup_db();
        let env = db.program_environment();
        let inner = super::OperationMemo::default();
        let outer = super::OperationMemo::default();
        let read = || {
            inner.get_or_compute(&db, &env, (), || {
                env.context().state().normalization.is_empty()
            })
        };

        assert!(read());
        assert!(outer.get_or_compute(&db, &env, (), read));
        env.context().state().normalization.visit(
            &env,
            &(),
            || (),
            || assert!(!outer.get_or_compute(&db, &env, (), read)),
        );
    }

    #[test]
    fn operation_cache_preserves_the_global_cache_boundary() {
        let db = setup_db();
        let env = db.program_environment();
        let memo = super::OperationMemo::default();
        let read =
            || memo.get_or_compute(&db, &env, (), || *env.cached_or_compute(|| &true, || false));

        assert!(read());
        env.context()
            .state()
            .normalization
            .visit(&env, &(), || (), || assert!(!read()));
        assert!(read());
    }

    #[test]
    fn operation_cache_repeats_incomplete_computations() {
        let db = setup_db();
        let env = db.program_environment();
        let typevar = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let guard = &env.context().state().materialized_bounds;
        let memo = super::OperationMemo::default();
        let visits = Cell::new(0);
        let read = || {
            memo.get_or_compute(&db, &env, (), || {
                visits.set(visits.get() + 1);
                guard.visit(&env, &typevar, || true)
            })
        };

        assert_eq!(
            guard.visit(&env, &typevar, || {
                assert_eq!(read(), None);
                assert_eq!(read(), None);
            }),
            None
        );
        assert_eq!(visits.get(), 2);
        assert_eq!(read(), Some(true));
    }

    #[test]
    fn cached_queries_observe_signature_guards() -> anyhow::Result<()> {
        struct SignatureAvailable;

        impl<'db> TypeQuery<'db> for SignatureAvailable {
            fn recursion_key(&self) -> TypeQueryKey<'db> {
                TypeQueryKey::SignatureAvailable
            }

            type Output = bool;

            fn visit(
                &self,
                db: &'db dyn Db,
                context: &TypeQueryVisit<'_, '_, 'db, Self>,
                ty: Type<'db>,
                (): (),
            ) -> bool {
                ty.as_callable().is_some_and(|callable| {
                    context.inspect_signatures(db, callable, |_| ()).is_some()
                })
            }

            fn on_cycle(
                &self,
                _context: &TypeQueryVisit<'_, '_, 'db, Self>,
                _ty: Type<'db>,
                _kind: CycleKind,
                (): (),
            ) -> bool {
                false
            }
        }

        let mut db = setup_db();
        db.write_dedented("/src/callback.py", "callback = lambda: 0")?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/callback.py")?,
            env.program(&db),
        );
        let callable = global_symbol(&db, file, "callback")
            .place
            .expect_type()
            .as_callable()
            .ok_or_else(|| anyhow::anyhow!("expected a lambda"))?;
        let ty = Type::Callable(callable);
        TypeQueryContext::with_root(&env, SignatureAvailable, |query| {
            assert!(query.visit(&db, ty));
            // The query owns its recursion context. Its previous answer is invalid while
            // another operation is expanding this signature, and becomes valid again afterward.
            assert_eq!(
                query
                    .environment()
                    .context()
                    .expand_signature(&db, &env, callable, || {
                        assert!(!query.visit(&db, ty));
                    }),
                Some(())
            );
            assert!(query.visit(&db, ty));
        });
        Ok(())
    }
}

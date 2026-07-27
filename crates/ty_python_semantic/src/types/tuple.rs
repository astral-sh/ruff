//! Types describing fixed- and variable-length tuples.
//!
//! At runtime, a Python tuple is a fixed-length immutable list of values. There is no restriction
//! on the types of the elements of a tuple value. In the type system, we want to model both
//! "heterogeneous" tuples that have elements of a fixed sequence of specific types, and
//! "homogeneous" tuples that have an unknown number of elements of the same single type. And in
//! fact, we want to model tuples that are a combination of the two ("mixed" tuples), with a
//! heterogeneous prefix and/or suffix, and a homogeneous portion of unknown length in between
//! those.
//!
//! The description of which elements can appear in a `tuple` is called a [`TupleSpec`]. Other
//! things besides `tuple` instances can be described by a tuple spec — for instance, the targets
//! of an unpacking assignment. A `tuple` specialization that includes `Never` as one of its
//! fixed-length elements cannot be instantiated. We reduce the entire `tuple` type down to
//! `Never`. The same is not true of tuple specs in general. (That means that it is [`TupleType`]
//! that adds that "collapse `Never`" behavior, whereas [`TupleSpec`] allows you to add any element
//! types, including `Never`.)

use std::cmp::Ordering;
use std::hash::Hash;
use std::num::{NonZeroI32, NonZeroUsize};

use itertools::{Either, EitherOrBoth, Itertools};
use smallvec::SmallVec;

use crate::subscript::{
    Nth, OutOfBoundsError, PyIndex, PySlice, StepSizeZeroError, py_slice_with_step,
};
use crate::types::class::{ClassType, KnownClass};
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::relation::{DisjointnessChecker, TypeRelationChecker, TypeVarEvaluation};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ErrorContext, FindLegacyTypeVarsVisitor,
    IntersectionType, Type, TypeContext, TypeMapping, UnionBuilder, UnionType,
};
use crate::{Db, FxOrderSet, Program};
use ty_python_core::Truthiness;
use ty_python_core::definition::Definition;

pub(crate) mod promotion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TupleLength {
    Fixed(usize),
    Variable(usize, usize),
}

impl TupleLength {
    pub(crate) const fn unknown() -> TupleLength {
        TupleLength::Variable(0, 0)
    }

    pub(crate) const fn is_variable(self) -> bool {
        matches!(self, TupleLength::Variable(_, _))
    }

    /// Returns the minimum and maximum length of this tuple. (The maximum length will be `None`
    /// for a tuple with a variable-length portion.)
    pub(crate) fn size_hint(self) -> (usize, Option<usize>) {
        match self {
            TupleLength::Fixed(len) => (len, Some(len)),
            TupleLength::Variable(prefix, suffix) => (prefix + suffix, None),
        }
    }

    /// Returns the minimum length of this tuple.
    pub(crate) fn minimum(self) -> usize {
        match self {
            TupleLength::Fixed(len) => len,
            TupleLength::Variable(prefix, suffix) => prefix + suffix,
        }
    }

    /// Returns the maximum length of this tuple, if any.
    pub(crate) fn maximum(self) -> Option<usize> {
        match self {
            TupleLength::Fixed(len) => Some(len),
            TupleLength::Variable(_, _) => None,
        }
    }

    /// Given two [`TupleLength`]s, return the more precise instance,
    /// if it makes sense to consider one more precise than the other.
    pub(crate) fn most_precise(self, other: Self) -> Option<Self> {
        match (self, other) {
            // A fixed-length tuple is equally as precise as another fixed-length tuple if they
            // have the same length. For two differently sized fixed-length tuples, however,
            // neither tuple length is more precise than the other: the two tuple lengths are
            // entirely disjoint.
            (TupleLength::Fixed(left), TupleLength::Fixed(right)) => {
                (left == right).then_some(self)
            }

            // A fixed-length tuple is more precise than a variable-length one.
            (fixed @ TupleLength::Fixed(_), TupleLength::Variable(..))
            | (TupleLength::Variable(..), fixed @ TupleLength::Fixed(_)) => Some(fixed),

            // For two variable-length tuples, the tuple with the larger number
            // of required items is more precise.
            (TupleLength::Variable(..), TupleLength::Variable(..)) => {
                Some(match self.minimum().cmp(&other.minimum()) {
                    Ordering::Less => other,
                    Ordering::Equal | Ordering::Greater => self,
                })
            }
        }
    }

    pub(crate) fn display_minimum(self) -> String {
        let minimum_length = self.minimum();
        match self {
            TupleLength::Fixed(_) => minimum_length.to_string(),
            TupleLength::Variable(_, _) => format!("at least {minimum_length}"),
        }
    }

    pub(crate) fn display_maximum(self) -> String {
        match self.maximum() {
            Some(maximum) => maximum.to_string(),
            None => "unlimited".to_string(),
        }
    }

    pub(crate) fn into_fixed_length(self) -> Option<usize> {
        match self {
            TupleLength::Fixed(len) => Some(len),
            TupleLength::Variable(_, _) => None,
        }
    }
}

#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct TupleType<'db> {
    #[returns(ref)]
    pub(crate) tuple: TupleSpec<'db>,
}

pub(super) fn walk_tuple_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    tuple: TupleType<'db>,
    visitor: &V,
) {
    match tuple.tuple(db) {
        Tuple::Fixed(tuple) => {
            for element in tuple.iter_all_elements() {
                visitor.visit_type(db, element);
            }
        }
        Tuple::Variable(tuple) => {
            for element in tuple.iter_prefix_elements() {
                visitor.visit_type(db, element);
            }
            match tuple.variable() {
                VariableSegment::Homogeneous(element) => visitor.visit_type(db, element),
                VariableSegment::TypeVarTuple(typevartuple) => {
                    visitor.visit_type(db, Type::TypeVar(typevartuple));
                }
            }
            for element in tuple.iter_suffix_elements() {
                visitor.visit_type(db, element);
            }
        }
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TupleType<'_> {}

#[salsa::tracked]
impl<'db> TupleType<'db> {
    pub(crate) fn new(db: &'db dyn Db, spec: &TupleSpec<'db>) -> Option<Self> {
        // If a fixed-length (i.e., mandatory) element of the tuple is `Never`, then it's not
        // possible to instantiate the tuple as a whole.
        if spec.fixed_elements().any(Type::is_never) {
            return None;
        }

        // If the variable-length portion is Never, it can only be instantiated with zero elements.
        // That means this isn't a variable-length tuple after all!
        if let TupleSpec::Variable(tuple) = spec
            && matches!(tuple.variable(), VariableSegment::Homogeneous(Type::Never))
        {
            let tuple = TupleSpec::Fixed(FixedLengthTuple::from_elements(
                tuple
                    .iter_prefix_elements()
                    .chain(tuple.iter_suffix_elements()),
            ));
            return Some(TupleType::new_internal::<_, TupleSpec<'db>>(db, tuple));
        }

        Some(TupleType::new_internal(db, spec))
    }

    pub(crate) fn empty(db: &'db dyn Db) -> Self {
        TupleType::new_internal(db, TupleSpec::from(FixedLengthTuple::empty()))
    }

    pub(crate) fn heterogeneous(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        TupleType::new(db, &TupleSpec::heterogeneous(types))
    }

    pub(crate) fn mixed(
        db: &'db dyn Db,
        prefix: impl IntoIterator<Item = Type<'db>>,
        variable: Type<'db>,
        suffix: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        Self::mixed_with_segment(db, prefix, VariableSegment::Homogeneous(variable), suffix)
    }

    pub(crate) fn mixed_with_segment(
        db: &'db dyn Db,
        prefix: impl IntoIterator<Item = Type<'db>>,
        variable: VariableSegment<'db>,
        suffix: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        TupleType::new(db, &VariableLengthTuple::mixed(prefix, variable, suffix))
    }

    pub(crate) fn homogeneous(db: &'db dyn Db, element: Type<'db>) -> Self {
        match element {
            Type::Never => TupleType::empty(db),
            _ => TupleType::new_internal(db, TupleSpec::homogeneous(element)),
        }
    }

    /// Packs a `TypeVarTuple` into the tuple value used for generic specialization relations.
    pub(crate) fn unpacked_typevartuple(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Self {
        debug_assert!(typevar.is_typevartuple(db));
        TupleType::new_internal(
            db,
            VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(typevar), []),
        )
    }

    // N.B. If this method is not Salsa-tracked, we take 10 minutes to check
    // `static-frame` as part of the ecosystem analysis. This is because it's called
    // from `NominalInstanceType::class()`, which is a very hot method.
    #[salsa::tracked(returns(copy), cycle_initial=to_class_type_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn to_class_type(self, db: &'db dyn Db) -> ClassType<'db> {
        let tuple_class = KnownClass::Tuple
            .try_to_class_literal(db)
            .expect("Typeshed should always have a `tuple` class in `builtins.pyi`");

        tuple_class.apply_specialization(db, |generic_context| {
            if generic_context.variables(db).len() == 1 {
                let element_type = self.tuple(db).tuple_class_type(db);
                generic_context.specialize_tuple(db, element_type, self)
            } else {
                generic_context.default_specialization(db, Some(KnownClass::Tuple))
            }
        })
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new_internal(
            db,
            self.tuple(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Option<Self> {
        TupleType::new(
            db,
            &self
                .tuple(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }

    pub(crate) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.tuple(db)
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
    }

    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.tuple(db).is_single_valued(db)
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_tuple_type_pair(
        &self,
        db: &'db dyn Db,
        source: TupleType<'db>,
        target: TupleType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_tuple_spec_pair(db, source.tuple(db), target.tuple(db))
    }

    fn check_tuple_spec_pair(
        &self,
        db: &'db dyn Db,
        source: &TupleSpec<'db>,
        target: &TupleSpec<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match source {
            Tuple::Fixed(source) => self.check_fixed_length_tuple_vs_tuple_spec(db, source, target),
            Tuple::Variable(source) => self.check_variable_length_vs_tuple_spec(db, source, target),
        }
    }

    fn check_fixed_length_tuple_vs_tuple_spec(
        &self,
        db: &'db dyn Db,
        source_tuple: &FixedLengthTuple<Type<'db>>,
        target_tuple: &TupleSpec<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match target_tuple {
            Tuple::Fixed(target) => {
                let equal_length = source_tuple.0.len() == target.0.len();

                if let Some(context) = self.report_context()
                    && !equal_length
                    && self.is_eager_assignability()
                {
                    context.push(ErrorContext::TupleLengthMismatch {
                        source_len: source_tuple.0.len(),
                        target_len: target_tuple.len(),
                    });
                }

                let mut n = 1;
                ConstraintSet::from_bool(self.constraints, equal_length).and(
                    db,
                    self.constraints,
                    || {
                        (source_tuple.0.iter().zip(&target.0)).when_all(
                            db,
                            self.constraints,
                            |(&source, &target)| {
                                let constraint_set = self.check_type_pair(db, source, target);
                                if let Some(context) = self.report_context()
                                    && constraint_set.is_never_satisfied(db)
                                {
                                    context.push(ErrorContext::TupleElementNotCompatible {
                                        source,
                                        target,
                                        element_index: n,
                                        element_count: source_tuple.0.len(),
                                    });
                                }

                                n += 1;

                                constraint_set
                            },
                        )
                    },
                )
            }

            Tuple::Variable(target) => {
                // This tuple must have enough elements to match up with the other tuple's prefix
                // and suffix, and each of those elements must pairwise satisfy the relation.
                let mut result = self.always();
                let mut source_iter = source_tuple.0.iter();
                for &target_ty in target.prefix_elements() {
                    let Some(&source_ty) = source_iter.next() else {
                        return self.never();
                    };
                    let element_constraints = self.check_type_pair(db, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_trivially_never_satisfied()
                    {
                        return result;
                    }
                }
                for target_ty in target.iter_suffix_elements().rev() {
                    let Some(&source_ty) = source_iter.next_back() else {
                        return self.never();
                    };
                    let element_constraints = self.check_type_pair(db, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_trivially_never_satisfied()
                    {
                        return result;
                    }
                }

                match target.variable() {
                    VariableSegment::TypeVarTuple(typevartuple) => {
                        let packed = Type::heterogeneous_tuple(db, source_iter.copied());
                        result.and(db, self.constraints, || {
                            self.check_type_pair(db, packed, Type::TypeVar(typevartuple))
                        })
                    }
                    VariableSegment::Homogeneous(target_ty) => {
                        // In addition, any remaining elements in this tuple must satisfy the
                        // variable-length portion of the other tuple.
                        result.and(db, self.constraints, || {
                            source_iter.when_all(db, self.constraints, |&source_ty| {
                                self.check_type_pair(db, source_ty, target_ty)
                            })
                        })
                    }
                }
            }
        }
    }

    fn check_variable_length_vs_tuple_spec(
        &self,
        db: &'db dyn Db,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &TupleSpec<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match target {
            Tuple::Fixed(target) => {
                // The `...` length specifier of a variable-length tuple type is interpreted
                // differently depending on the type of the variable-length elements.
                //
                // It typically represents the _union_ of all possible lengths. That means that a
                // variable-length tuple type is not a subtype of _any_ fixed-length tuple type.
                //
                // However, as a special case, if the variable-length portion of the tuple is `Any`
                // (or any other dynamic type), then the `...` is the _gradual choice_ of all
                // possible lengths. This means that `tuple[Any, ...]` can match any tuple of any
                // length.
                let VariableSegment::Homogeneous(source_variable) = source.variable() else {
                    // Unlike a dynamic homogeneous segment, a symbolic type variable tuple ranges
                    // over all specializations rather than making a gradual choice of length.
                    return self.never();
                };
                if !self.is_eager_assignability() || !source_variable.is_dynamic() {
                    return self.never();
                }

                // In addition, the other tuple must have enough elements to match up with this
                // tuple's prefix and suffix, and each of those elements must pairwise satisfy the
                // relation.
                let mut result = self.always();
                let mut target_iter = target.iter_all_elements();
                for source_ty in source.prenormalized_prefix_elements(db, None) {
                    let Some(target_ty) = target_iter.next() else {
                        return self.never();
                    };
                    let element_constraints = self.check_type_pair(db, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_trivially_never_satisfied()
                    {
                        return result;
                    }
                }
                let suffix: Vec<_> = source.prenormalized_suffix_elements(db, None).collect();
                for &source_ty in suffix.iter().rev() {
                    let Some(target_ty) = target_iter.next_back() else {
                        return self.never();
                    };
                    let element_constraints = self.check_type_pair(db, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_trivially_never_satisfied()
                    {
                        return result;
                    }
                }

                result
            }

            Tuple::Variable(target) => {
                if let (
                    VariableSegment::TypeVarTuple(source_typevartuple),
                    VariableSegment::TypeVarTuple(target_typevartuple),
                ) = (source.variable(), target.variable())
                    && source_typevartuple.is_same_typevar_as(db, target_typevartuple)
                {
                    if source.prefix_len() != target.prefix_len()
                        || source.suffix_len() != target.suffix_len()
                    {
                        return self.never();
                    }

                    return source
                        .prefix_elements()
                        .iter()
                        .zip(target.prefix_elements())
                        .chain(
                            source
                                .suffix_elements()
                                .iter()
                                .zip(target.suffix_elements()),
                        )
                        .when_all(db, self.constraints, |(&source_ty, &target_ty)| {
                            self.check_type_pair(db, source_ty, target_ty)
                        });
                }

                if self.typevar_evaluation == TypeVarEvaluation::Lazy
                    && let VariableSegment::TypeVarTuple(typevartuple) = target.variable()
                {
                    let source_prefix = source.prefix_elements();
                    let source_suffix = source.suffix_elements();
                    let target_prefix = target.prefix_elements();
                    let target_suffix = target.suffix_elements();
                    if source_prefix.len() < target_prefix.len()
                        || source_suffix.len() < target_suffix.len()
                    {
                        return self.never();
                    }

                    let source_suffix_start = source_suffix.len() - target_suffix.len();
                    let boundary_constraints = source_prefix
                        .iter()
                        .zip(target_prefix)
                        .chain(
                            source_suffix[source_suffix_start..]
                                .iter()
                                .zip(target_suffix),
                        )
                        .when_all(db, self.constraints, |(&source_ty, &target_ty)| {
                            self.check_type_pair(db, source_ty, target_ty)
                        });

                    let packed = Type::tuple(TupleType::new(
                        db,
                        &VariableLengthTuple::mixed(
                            source_prefix[target_prefix.len()..].iter().copied(),
                            source.variable(),
                            source_suffix[..source_suffix_start].iter().copied(),
                        ),
                    ));
                    return boundary_constraints.and(db, self.constraints, || {
                        self.check_type_pair(db, packed, Type::TypeVar(typevartuple))
                    });
                }

                if matches!(target.variable(), VariableSegment::TypeVarTuple(_)) {
                    return self.never();
                }

                // When prenormalizing below, we assume that a dynamic variable-length portion of
                // one tuple materializes to the variable-length portion of the other tuple.
                let source_variable = source.variable().element_type(db);
                let target_variable = target.variable().element_type(db);
                let source_prenormalize_variable = match source.variable() {
                    VariableSegment::Homogeneous(Type::Dynamic(_)) => Some(target_variable),
                    _ => None,
                };
                let target_prenormalize_variable = match target.variable() {
                    VariableSegment::Homogeneous(Type::Dynamic(_)) => Some(source_variable),
                    _ => None,
                };

                // The overlapping parts of the prefixes and suffixes must satisfy the relation.
                // Any remaining parts must satisfy the relation with the other tuple's
                // variable-length part.
                let mut result = self.always();
                let pairwise = source
                    .prenormalized_prefix_elements(db, source_prenormalize_variable)
                    .zip_longest(
                        target.prenormalized_prefix_elements(db, target_prenormalize_variable),
                    );
                for pair in pairwise {
                    let pair_constraints = match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => {
                            self.check_type_pair(db, self_ty, other_ty)
                        }
                        EitherOrBoth::Left(self_ty) => {
                            self.check_type_pair(db, self_ty, target_variable)
                        }
                        EitherOrBoth::Right(other_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !self.is_eager_assignability() || !source_variable.is_dynamic() {
                                return self.never();
                            }
                            self.check_type_pair(db, source_variable, other_ty)
                        }
                    };
                    if result
                        .intersect(db, self.constraints, pair_constraints)
                        .is_trivially_never_satisfied()
                    {
                        return result;
                    }
                }

                let source_suffix: Vec<_> = source
                    .prenormalized_suffix_elements(db, source_prenormalize_variable)
                    .collect();
                let target_suffix: Vec<_> = target
                    .prenormalized_suffix_elements(db, target_prenormalize_variable)
                    .collect();
                let pairwise = source_suffix
                    .iter()
                    .rev()
                    .zip_longest(target_suffix.iter().rev());
                for pair in pairwise {
                    let pair_constraints = match pair {
                        EitherOrBoth::Both(&source_ty, &target_ty) => {
                            self.check_type_pair(db, source_ty, target_ty)
                        }
                        EitherOrBoth::Left(&source_ty) => {
                            self.check_type_pair(db, source_ty, target_variable)
                        }
                        EitherOrBoth::Right(&target_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !self.is_eager_assignability() || !source_variable.is_dynamic() {
                                return self.never();
                            }
                            self.check_type_pair(db, source_variable, target_ty)
                        }
                    };
                    if result
                        .intersect(db, self.constraints, pair_constraints)
                        .is_trivially_never_satisfied()
                    {
                        return result;
                    }
                }

                // And lastly, the variable-length portions must satisfy the relation.
                result.and(db, self.constraints, || {
                    self.check_type_pair(db, source_variable, target_variable)
                })
            }
        }
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    pub(super) fn check_tuple_type_pair(
        &self,
        db: &'db dyn Db,
        left: TupleType<'db>,
        right: TupleType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_tuple_spec_pair(db, left.tuple(db), right.tuple(db))
    }

    pub(super) fn check_tuple_spec_pair(
        &self,
        db: &'db dyn Db,
        left: &TupleSpec<'db>,
        right: &TupleSpec<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // Two tuples with an incompatible number of required elements must always be disjoint.
        let (self_min, self_max) = left.len().size_hint();
        let (other_min, other_max) = right.len().size_hint();
        if self_max.is_some_and(|max| max < other_min) {
            return self.always();
        }
        if other_max.is_some_and(|max| max < self_min) {
            return self.always();
        }

        // If any of the required elements are pairwise disjoint, the tuples are disjoint as well.
        let any_disjoint = |a: &[Type<'db>], b: &[Type<'db>], rev: bool| {
            if rev {
                std::iter::zip(a.iter().rev(), b.iter().rev()).when_any(
                    db,
                    self.constraints,
                    |(&left_elem, &right_elem)| self.check_type_pair(db, left_elem, right_elem),
                )
            } else {
                std::iter::zip(a, b).when_any(db, self.constraints, |(&left_elem, &right_elem)| {
                    self.check_type_pair(db, left_elem, right_elem)
                })
            }
        };

        match (left, right) {
            (Tuple::Fixed(left), Tuple::Fixed(right)) => {
                any_disjoint(left.all_elements(), right.all_elements(), false)
            }

            // Note that we don't compare the variable-length portions; two pure homogeneous tuples
            // `tuple[A, ...]` and `tuple[B, ...]` can never be disjoint even if A and B are
            // disjoint, because `tuple[()]` would be assignable to both.
            (Tuple::Variable(left), Tuple::Variable(right)) => {
                any_disjoint(left.prefix_elements(), right.prefix_elements(), false).or(
                    db,
                    self.constraints,
                    || any_disjoint(left.suffix_elements(), right.suffix_elements(), true),
                )
            }

            (Tuple::Fixed(fixed), Tuple::Variable(variable))
            | (Tuple::Variable(variable), Tuple::Fixed(fixed)) => {
                any_disjoint(fixed.all_elements(), variable.prefix_elements(), false).or(
                    db,
                    self.constraints,
                    || any_disjoint(fixed.all_elements(), variable.suffix_elements(), true),
                )
            }
        }
    }
}

fn to_class_type_cycle_initial<'db>(
    db: &'db dyn Db,
    id: salsa::Id,
    self_: TupleType<'db>,
) -> ClassType<'db> {
    let tuple_class = KnownClass::Tuple
        .try_to_class_literal(db)
        .expect("Typeshed should always have a `tuple` class in `builtins.pyi`");

    tuple_class.apply_specialization(db, |generic_context| {
        if generic_context.variables(db).len() == 1 {
            generic_context.specialize_tuple(db, Type::divergent(id), self_)
        } else {
            generic_context.default_specialization(db, Some(KnownClass::Tuple))
        }
    })
}

/// A tuple spec describes the contents of a tuple type, which might be fixed- or variable-length.
///
/// Tuple specs are used for more than just `tuple` instances, so they allow `Never` to appear as a
/// fixed-length element type. [`TupleType`] adds that additional invariant (since a tuple that
/// must contain an element that can't be instantiated, can't be instantiated itself).
pub(crate) type TupleSpec<'db> = Tuple<Type<'db>, VariableSegment<'db>>;

/// The variable-length portion of a [`TupleSpec`].
///
/// For example, `tuple[str, *tuple[int, ...], bytes]` has a homogeneous `int` segment, while
/// `tuple[str, *Ts, bytes]` has a `TypeVarTuple` segment for `Ts`. The fixed `str` prefix and
/// `bytes` suffix are stored separately by [`VariableLengthTuple`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum VariableSegment<'db> {
    /// A segment whose elements all have the same type, such as `int` in `tuple[int, ...]`.
    Homogeneous(Type<'db>),
    /// An unpacked type variable tuple, such as `Ts` in `tuple[*Ts]`.
    TypeVarTuple(BoundTypeVarInstance<'db>),
}

impl<'db> VariableSegment<'db> {
    pub(crate) const fn homogeneous_type(self) -> Option<Type<'db>> {
        match self {
            Self::Homogeneous(element) => Some(element),
            Self::TypeVarTuple(_) => None,
        }
    }

    pub(crate) const fn typevartuple(self) -> Option<BoundTypeVarInstance<'db>> {
        match self {
            Self::Homogeneous(_) => None,
            Self::TypeVarTuple(typevartuple) => Some(typevartuple),
        }
    }

    pub(crate) fn element_type(self, _db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Homogeneous(element) => element,
            Self::TypeVarTuple(_) => Type::object(),
        }
    }

    /// Returns the type used for the builtin tuple class's single generic parameter.
    ///
    /// Preserve the `TypeVarTuple` here so that variance inference and generic-context traversal
    /// can still observe it. Runtime element operations must use [`Self::element_type`] instead.
    fn tuple_class_type(self) -> Type<'db> {
        match self {
            Self::Homogeneous(element) => element,
            Self::TypeVarTuple(typevartuple) => Type::TypeVar(typevartuple),
        }
    }
}

/// A fixed-length tuple.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct FixedLengthTuple<T>(Box<[T]>);

impl<T> FixedLengthTuple<T> {
    fn empty() -> Self {
        Self(Box::default())
    }

    fn from_elements(elements: impl IntoIterator<Item = T>) -> Self {
        Self(elements.into_iter().collect())
    }

    pub(crate) fn elements_slice(&self) -> &[T] {
        &self.0
    }

    pub(crate) fn owned_elements(self) -> Box<[T]> {
        self.0
    }

    pub(crate) fn all_elements(&self) -> &[T] {
        &self.0
    }

    pub(crate) fn iter_all_elements(&self) -> impl DoubleEndedIterator<Item = T>
    where
        T: Copy,
    {
        self.0.iter().copied()
    }

    /// Returns the length of this tuple.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'db> FixedLengthTuple<Type<'db>> {
    fn resize(
        &self,
        db: &'db dyn Db,
        new_length: TupleLength,
    ) -> Result<TupleSpec<'db>, ResizeTupleError> {
        match new_length {
            TupleLength::Fixed(new_length) => match self.len().cmp(&new_length) {
                Ordering::Less => Err(ResizeTupleError::TooFewValues),
                Ordering::Greater => Err(ResizeTupleError::TooManyValues),
                Ordering::Equal => Ok(Tuple::Fixed(self.clone())),
            },

            TupleLength::Variable(prefix, suffix) => {
                // The number of rhs values that will be consumed by the starred target.
                let Some(variable) = self.len().checked_sub(prefix + suffix) else {
                    return Err(ResizeTupleError::TooFewValues);
                };

                // Extract rhs values into the prefix, then into the starred target, then into the
                // suffix.
                let mut elements = self.iter_all_elements();
                let prefix: Vec<_> = elements.by_ref().take(prefix).collect();
                let variable =
                    UnionType::from_elements_leave_aliases(db, elements.by_ref().take(variable));
                let suffix = elements.by_ref().take(suffix);
                Ok(VariableLengthTuple::mixed(
                    prefix,
                    VariableSegment::Homogeneous(variable),
                    suffix,
                ))
            }
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if nested {
            Some(Self::from_elements(
                self.0
                    .iter()
                    .map(|ty| ty.recursive_type_normalized_impl(db, div, true))
                    .collect::<Option<Box<[_]>>>()?,
            ))
        } else {
            Some(Self::from_elements(
                self.0
                    .iter()
                    .map(|ty| {
                        ty.recursive_type_normalized_impl(db, div, true)
                            .unwrap_or(div)
                    })
                    .collect::<Box<[_]>>(),
            ))
        }
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let tcx_tuple = tcx
            .annotation
            .and_then(|annotation| annotation.known_specialization(db, KnownClass::Tuple))
            .and_then(|specialization| {
                specialization
                    .tuple(db)
                    .expect("the specialization of `KnownClass::Tuple` must have a tuple spec")
                    .resize(db, TupleLength::Fixed(self.0.len()))
                    .ok()
            });

        let tcx_elements = match tcx_tuple.as_ref() {
            None => Either::Right(std::iter::repeat(TypeContext::default())),
            Some(tuple) => Either::Left(
                tuple
                    .iter_element_types(db)
                    .map(|tcx| TypeContext::new(Some(tcx))),
            ),
        };

        Self::from_elements(
            self.0
                .iter()
                .zip(tcx_elements)
                .map(|(ty, tcx)| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
        )
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for ty in &self.0 {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }

    fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        self.0.iter().all(|ty| ty.is_single_valued(db))
    }
}

impl<'db> PyIndex<'db> for &FixedLengthTuple<Type<'db>> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        self.0.py_index(db, index).copied()
    }
}

impl<'db> PySlice<'db> for FixedLengthTuple<Type<'db>> {
    type Item = Type<'db>;

    fn py_slice(
        &self,
        db: &'db dyn Db,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<impl Iterator<Item = Self::Item>, StepSizeZeroError> {
        self.0.py_slice(db, start, stop, step)
    }
}

/// A variable-length tuple.
///
/// The tuple can contain a fixed-length heterogeneous prefix and/or suffix. The variable-length
/// portion is described by `V`; for [`TupleSpec`], it is either homogeneous or an unpacked
/// `TypeVarTuple`.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct VariableLengthTuple<T, V = T> {
    /// Fixed prefix elements followed by fixed suffix elements.
    fixed_elements: smallvec::SmallVec<[T; 0]>,
    /// The number of elements in `fixed_elements` that belong to the prefix.
    prefix_len: usize,
    /// The variable-length portion between the fixed prefix and suffix.
    variable_segment: V,
}

impl<T, V> VariableLengthTuple<T, V> {
    /// Creates a new tuple spec consisting only of the given variable-length segment.
    const fn homogeneous(variable: V) -> Self {
        Self {
            fixed_elements: SmallVec::new_const(),
            prefix_len: 0,
            variable_segment: variable,
        }
    }

    fn mixed(
        prefix: impl IntoIterator<Item = T>,
        variable: V,
        suffix: impl IntoIterator<Item = T>,
    ) -> Tuple<T, V> {
        Tuple::Variable(Self::new(prefix, variable, suffix))
    }

    fn try_new<P, S>(prefix: P, variable: V, suffix: S) -> Option<Self>
    where
        P: IntoIterator<Item = Option<T>>,
        P::IntoIter: ExactSizeIterator,
        S: IntoIterator<Item = Option<T>>,
        S::IntoIter: ExactSizeIterator,
    {
        let prefix = prefix.into_iter();
        let suffix = suffix.into_iter();

        let mut fixed_elements = SmallVec::with_capacity(prefix.len().saturating_add(suffix.len()));

        for element in prefix {
            fixed_elements.push(element?);
        }

        let prefix_len = fixed_elements.len();

        for element in suffix {
            fixed_elements.push(element?);
        }

        fixed_elements.shrink_to_fit();

        Some(Self {
            fixed_elements,
            prefix_len,
            variable_segment: variable,
        })
    }

    fn new(
        prefix: impl IntoIterator<Item = T>,
        variable: V,
        suffix: impl IntoIterator<Item = T>,
    ) -> Self {
        let mut fixed_elements = SmallVec::new_const();
        fixed_elements.extend(prefix);

        let prefix_len = fixed_elements.len();
        fixed_elements.extend(suffix);
        fixed_elements.shrink_to_fit();

        Self {
            fixed_elements,
            prefix_len,
            variable_segment: variable,
        }
    }

    fn new_from_vec(prefix: Vec<T>, variable: V, suffix: Vec<T>) -> Self {
        let mut fixed_elements = SmallVec::from_vec(prefix);

        let prefix_len = fixed_elements.len();
        fixed_elements.extend(suffix);
        fixed_elements.shrink_to_fit();

        Self {
            fixed_elements,
            prefix_len,
            variable_segment: variable,
        }
    }

    pub(crate) fn variable(&self) -> V
    where
        V: Copy,
    {
        self.variable_segment
    }

    pub(crate) fn variable_element_mut(&mut self) -> &mut V {
        &mut self.variable_segment
    }

    pub(crate) fn prefix_elements(&self) -> &[T] {
        &self.fixed_elements[..self.prefix_len]
    }

    pub(crate) fn iter_prefix_elements(&self) -> impl DoubleEndedIterator<Item = T>
    where
        T: Copy,
    {
        self.prefix_elements().iter().copied()
    }

    pub(crate) fn prefix_elements_mut(&mut self) -> &mut [T] {
        &mut self.fixed_elements[..self.prefix_len]
    }

    pub(crate) fn suffix_elements(&self) -> &[T] {
        &self.fixed_elements[self.prefix_len..]
    }

    pub(crate) fn iter_suffix_elements(&self) -> impl DoubleEndedIterator<Item = T>
    where
        T: Copy,
    {
        self.suffix_elements().iter().copied()
    }

    pub(crate) fn suffix_elements_mut(&mut self) -> &mut [T] {
        &mut self.fixed_elements[self.prefix_len..]
    }

    fn fixed_elements(&self) -> impl Iterator<Item = &T> + '_ {
        self.fixed_elements.iter()
    }

    fn into_all_elements_with_kind(self) -> impl Iterator<Item = TupleElement<T, V>> {
        let mut fixed_elements = self.fixed_elements.into_iter();
        let mut remaining_prefix = self.prefix_len;
        let mut variable = Some(self.variable_segment);

        std::iter::from_fn(move || {
            if remaining_prefix > 0 {
                remaining_prefix -= 1;
                return fixed_elements.next().map(TupleElement::Prefix);
            }
            if let Some(variable) = variable.take() {
                return Some(TupleElement::Variable(variable));
            }
            fixed_elements.next().map(TupleElement::Suffix)
        })
    }

    fn prefix_len(&self) -> usize {
        self.prefix_len
    }

    fn suffix_len(&self) -> usize {
        self.fixed_elements.len() - self.prefix_len
    }

    fn len(&self) -> TupleLength {
        TupleLength::Variable(self.prefix_len(), self.suffix_len())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForwardSliceStop {
    /// The default stop at the end of the tuple.
    End,
    /// A non-negative stop index, measured from the front of the tuple.
    Absolute(usize),
    /// A negative stop index that is known to land in the suffix.
    Suffix(usize),
}

/// A fixed prefix or suffix slice in the result of slicing a variable-length tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FixedSlice {
    start: Option<i32>,
    stop: Option<i32>,
    step: NonZeroUsize,
}

/// A fixed-length slice in the result of slicing a variable-length tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FixedPositionSlice {
    origin: FixedPositionOrigin,
    start: usize,
    exclusive_stop: usize,
    step: NonZeroUsize,
}

/// Whether a fixed-position slice is indexed from the front or the back of the tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixedPositionOrigin {
    Front,
    Back,
}

/// How the source tuple's variable segment contributes to the slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VariableSliceKind {
    /// The variable segment does not contribute to the slice.
    Excluded,
    /// The variable segment contributes its runtime element type to a homogeneous approximation.
    ElementType,
    /// The complete variable segment is retained in its original order.
    Preserved,
}

/// The elements folded into the variable part of a sliced variable-length tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VariableSlice {
    kind: VariableSliceKind,
    suffix_start: usize,
    suffix_stop: usize,
}

/// How to approximate a static slice into a variable-length tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VariableTupleSlicePlan {
    /// The slice is always empty, e.g. `t[5:2]` or a forward slice from the suffix
    /// to a non-negative stop in the prefix.
    Empty,
    /// A fixed-length slice whose positions are all known from the front or the back.
    /// Individual positions can still be unions when the variable part might be empty.
    Fixed(FixedPositionSlice),
    /// A slice that can be represented as a single variable-length tuple.
    Mixed {
        fixed_prefix: Option<FixedSlice>,
        variable: VariableSlice,
        fixed_suffix: Option<FixedSlice>,
    },
    /// Fallback for slices whose exact result would require a union of tuple shapes.
    Homogeneous,
}

/// The direction of a static slice into a variable-length tuple.
///
/// Backward slices are planned as forward slices over a reversed tuple, so that there is only one
/// mixed-tuple slice planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TupleSliceDirection {
    Forward,
    Backward,
}

impl FixedSlice {
    fn in_segment(
        segment_len: usize,
        start: Option<usize>,
        stop: Option<usize>,
        step: NonZeroUsize,
    ) -> Option<Self> {
        let effective_start = start.unwrap_or(0);
        let effective_stop = stop.unwrap_or(segment_len);
        (effective_start < effective_stop).then(|| Self::new(start, stop, step))
    }

    fn new(start: Option<usize>, stop: Option<usize>, step: NonZeroUsize) -> Self {
        Self {
            start: start.and_then(|start| i32::try_from(start).ok()),
            stop: stop.and_then(|stop| i32::try_from(stop).ok()),
            step,
        }
    }

    fn step_i32(self) -> NonZeroI32 {
        NonZeroI32::new(
            i32::try_from(self.step.get()).expect("slice steps originate from an i32 slice index"),
        )
        .expect("a non-zero usize step remains non-zero as an i32")
    }
}

impl FixedPositionSlice {
    fn from_front(start: usize, stop: usize, step: NonZeroUsize) -> Self {
        Self {
            origin: FixedPositionOrigin::Front,
            start,
            exclusive_stop: stop,
            step,
        }
    }

    fn from_back(start: usize, stop: usize, step: NonZeroUsize) -> Self {
        Self {
            origin: FixedPositionOrigin::Back,
            start,
            exclusive_stop: stop,
            step,
        }
    }
}

impl ForwardSliceStop {
    fn suffix_stop(self, tuple: &VariableLengthTuple<Type<'_>, VariableSegment<'_>>) -> usize {
        match self {
            ForwardSliceStop::End => tuple.suffix_len(),
            ForwardSliceStop::Absolute(stop) => stop
                .saturating_sub(tuple.prefix_len())
                .min(tuple.suffix_len()),
            ForwardSliceStop::Suffix(stop) => stop,
        }
    }

    fn allows_fixed_suffix(self) -> bool {
        matches!(self, ForwardSliceStop::End | ForwardSliceStop::Suffix(_))
    }
}

impl VariableSlice {
    fn variable_only() -> Self {
        Self {
            kind: VariableSliceKind::Preserved,
            suffix_start: 0,
            suffix_stop: 0,
        }
    }

    fn suffix(start: usize, stop: usize) -> Option<Self> {
        (start < stop).then_some(Self {
            kind: VariableSliceKind::Excluded,
            suffix_start: start,
            suffix_stop: stop,
        })
    }

    fn ty<'db>(
        self,
        db: &'db dyn Db,
        tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
    ) -> Type<'db> {
        UnionType::from_elements_leave_aliases(
            db,
            matches!(
                self.kind,
                VariableSliceKind::ElementType | VariableSliceKind::Preserved
            )
            .then_some(tuple.variable().element_type(db))
            .into_iter()
            .chain(
                tuple
                    .iter_suffix_elements()
                    .skip(self.suffix_start)
                    .take(self.suffix_stop.saturating_sub(self.suffix_start)),
            ),
        )
    }
}

impl VariableTupleSlicePlan {
    fn into_type<'db>(
        self,
        db: &'db dyn Db,
        tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
    ) -> Type<'db> {
        match self {
            VariableTupleSlicePlan::Empty => {
                Type::heterogeneous_tuple(db, std::iter::empty::<Type<'db>>())
            }

            VariableTupleSlicePlan::Fixed(fixed) => {
                Type::heterogeneous_tuple(db, tuple.slice_fixed_position(db, fixed))
            }

            VariableTupleSlicePlan::Mixed {
                fixed_prefix,
                variable,
                fixed_suffix,
            } => {
                let variable_segment = match variable.kind {
                    VariableSliceKind::Preserved => tuple.variable(),
                    VariableSliceKind::Excluded | VariableSliceKind::ElementType => {
                        VariableSegment::Homogeneous(variable.ty(db, tuple))
                    }
                };
                Type::tuple(TupleType::new(
                    db,
                    &VariableLengthTuple::mixed(
                        VariableLengthTuple::optional_fixed_slice(
                            tuple.prefix_elements(),
                            fixed_prefix,
                        ),
                        variable_segment,
                        VariableLengthTuple::optional_fixed_slice(
                            tuple.suffix_elements(),
                            fixed_suffix,
                        ),
                    ),
                ))
            }

            VariableTupleSlicePlan::Homogeneous => tuple.homogeneous_type(db),
        }
    }
}

impl TupleSliceDirection {
    fn from_step(step: NonZeroI32) -> Self {
        if step.get() > 0 {
            Self::Forward
        } else {
            Self::Backward
        }
    }

    fn positive_step(self, step: NonZeroI32) -> NonZeroUsize {
        let step = match self {
            TupleSliceDirection::Forward => {
                usize::try_from(step.get()).expect("a forward slice has a positive step")
            }
            // `i32::MIN.abs()` cannot be represented as `i32`; saturating is enough for the
            // finite fixed segments we model exactly, and leaves the variable segment approximate.
            TupleSliceDirection::Backward => usize::try_from(
                NonZeroI32::new(step.get().saturating_abs())
                    .expect("a non-zero step has a non-zero absolute value")
                    .get(),
            )
            .expect("a positive i32 step is representable as a usize"),
        };

        NonZeroUsize::new(step).expect("a positive i32 step is representable as a non-zero usize")
    }

    fn reverse_index(index: i32) -> i32 {
        let reversed = -i64::from(index) - 1;
        i32::try_from(reversed).expect("reversing an i32 slice index produces another i32")
    }

    fn reverse_bound(bound: Option<i32>) -> Option<i32> {
        bound.map(Self::reverse_index)
    }
}

impl<'db> VariableLengthTuple<Type<'db>, VariableSegment<'db>> {
    fn optional_fixed_slice<'a>(
        elements: &'a [Type<'db>],
        slice: Option<FixedSlice>,
    ) -> impl Iterator<Item = Type<'db>> + 'a {
        match slice {
            Some(slice) => Either::Left(py_slice_with_step(
                elements,
                slice.start,
                slice.stop,
                slice.step_i32(),
            )),
            None => Either::Right(std::iter::empty()),
        }
    }

    fn fixed_prefix_slice(
        &self,
        start: Option<usize>,
        stop: Option<usize>,
        step: NonZeroUsize,
    ) -> Option<FixedSlice> {
        FixedSlice::in_segment(self.prefix_len(), start, stop, step)
    }

    fn fixed_suffix_slice(
        &self,
        start: Option<usize>,
        stop: Option<usize>,
        step: NonZeroUsize,
    ) -> Option<FixedSlice> {
        FixedSlice::in_segment(self.suffix_len(), start, stop, step)
    }

    fn slice_fixed_position<'a>(
        &'a self,
        db: &'db dyn Db,
        slice: FixedPositionSlice,
    ) -> impl Iterator<Item = Type<'db>> + 'a
    where
        'db: 'a,
    {
        let FixedPositionSlice {
            origin,
            start,
            exclusive_stop,
            step,
        } = slice;
        match origin {
            FixedPositionOrigin::Front => {
                Either::Left(self.slice_front_forward(db, start, exclusive_stop, step))
            }
            FixedPositionOrigin::Back => {
                Either::Right(self.slice_back(db, start, exclusive_stop, step))
            }
        }
    }

    fn slice_front_forward<'a>(
        &'a self,
        db: &'db dyn Db,
        start: usize,
        exclusive_stop: usize,
        step: NonZeroUsize,
    ) -> impl Iterator<Item = Type<'db>> + 'a
    where
        'db: 'a,
    {
        (start..exclusive_stop)
            .step_by(step.get())
            .map(move |index| {
                self.type_at_nonnegative_index(db, index).unwrap_or_else(|| {
                    unreachable!(
                        "front-origin fixed slice positions are validated during plan construction"
                    )
                })
            })
    }

    fn slice_back<'a>(
        &'a self,
        db: &'db dyn Db,
        start: usize,
        exclusive_stop: usize,
        step: NonZeroUsize,
    ) -> impl Iterator<Item = Type<'db>> + 'a
    where
        'db: 'a,
    {
        let step = step.get();
        let mut distance = start;

        std::iter::from_fn(move || {
            if distance <= exclusive_stop {
                return None;
            }

            let element = self
                .type_at_negative_distance(db, distance)
                .unwrap_or_else(|| {
                    unreachable!(
                        "back-origin fixed slice positions are validated during plan construction"
                    )
                });

            if distance.saturating_sub(exclusive_stop) <= step {
                distance = exclusive_stop;
            } else {
                distance -= step;
            }

            Some(element)
        })
    }

    fn reversed(&self, db: &'db dyn Db) -> Self {
        // Reversing a `TypeVarTuple` changes its element order, so the result can no longer use
        // the original symbolic segment.
        Self::new(
            self.iter_suffix_elements().rev(),
            VariableSegment::Homogeneous(self.variable().element_type(db)),
            self.iter_prefix_elements().rev(),
        )
    }

    /// Returns a sound static slice result for a mixed tuple.
    ///
    /// We preserve fixed-length results exactly, unioning each output position when it can come
    /// from multiple tuple segments. For cases whose exact result would require a union of tuple
    /// shapes, we fall back to a homogeneous tuple over all possible element types in the original
    /// tuple.
    fn py_slice_type(
        &self,
        db: &'db dyn Db,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<Type<'db>, StepSizeZeroError> {
        let step = step.unwrap_or(1);
        let Some(step) = NonZeroI32::new(step) else {
            return Err(StepSizeZeroError);
        };

        let direction = TupleSliceDirection::from_step(step);
        let step = direction.positive_step(step);

        Ok(match direction {
            TupleSliceDirection::Forward => self
                .forward_slice_plan(start, stop, step)
                .into_type(db, self),
            TupleSliceDirection::Backward => {
                let reversed = self.reversed(db);
                reversed
                    .forward_slice_plan(
                        TupleSliceDirection::reverse_bound(start),
                        TupleSliceDirection::reverse_bound(stop),
                        step,
                    )
                    .into_type(db, &reversed)
            }
        })
    }

    fn forward_slice_plan(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
        step: NonZeroUsize,
    ) -> VariableTupleSlicePlan {
        self.forward_empty_slice_plan(start, stop)
            .or_else(|| self.fixed_front_slice_plan(start, stop, step))
            .or_else(|| self.fixed_back_slice_plan(start, stop, step))
            .or_else(|| self.mixed_forward_slice_plan(start, stop, step))
            .unwrap_or(VariableTupleSlicePlan::Homogeneous)
    }

    fn fixed_front_slice_plan(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
        step: NonZeroUsize,
    ) -> Option<VariableTupleSlicePlan> {
        let minimum_len = self.len().minimum();

        if let Some(stop) = Self::nonnegative_slice_index(stop)
            && let Some(start) = Self::front_start_index(start)
        {
            if start >= stop {
                debug_assert!(
                    start < stop,
                    "empty forward slices are planned before fixed-front slices"
                );
                return None;
            }

            if Self::last_forward_slice_index(start, stop, step) < minimum_len {
                return Some(VariableTupleSlicePlan::Fixed(
                    FixedPositionSlice::from_front(start, stop, step),
                ));
            }
        }

        None
    }

    fn fixed_back_slice_plan(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
        step: NonZeroUsize,
    ) -> Option<VariableTupleSlicePlan> {
        let minimum_len = self.len().minimum();

        let start_distance = Self::distance_from_end(start?)?;
        let stop_distance = match stop {
            None => Some(0),
            Some(stop) => Self::distance_from_end(stop),
        };

        if let Some(stop_distance) = stop_distance {
            if start_distance <= stop_distance {
                debug_assert!(
                    start_distance > stop_distance,
                    "empty forward slices are planned before fixed-back slices"
                );
                return None;
            }

            if start_distance <= minimum_len {
                return Some(VariableTupleSlicePlan::Fixed(
                    FixedPositionSlice::from_back(start_distance, stop_distance, step),
                ));
            }
        }

        None
    }

    fn forward_empty_slice_plan(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
    ) -> Option<VariableTupleSlicePlan> {
        if start.is_some() && start == stop {
            return Some(VariableTupleSlicePlan::Empty);
        }

        if let Some(stop) = Self::nonnegative_slice_index(stop) {
            if let Some(start) = Self::front_start_index(start)
                && start >= stop
            {
                return Some(VariableTupleSlicePlan::Empty);
            }

            if let Some(start_distance) = start.and_then(Self::distance_from_end) {
                let first_start = self.len().minimum().saturating_sub(start_distance);
                if first_start >= stop {
                    return Some(VariableTupleSlicePlan::Empty);
                }
            }
        }

        if let Some(start_distance) = start.and_then(Self::distance_from_end)
            && let Some(stop_distance) = stop.and_then(Self::distance_from_end)
            && start_distance <= stop_distance
        {
            return Some(VariableTupleSlicePlan::Empty);
        }

        None
    }

    fn mixed_forward_slice_plan(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
        step: NonZeroUsize,
    ) -> Option<VariableTupleSlicePlan> {
        let step_value = step.get();

        if step_value == 1 {
            if stop.is_none()
                && let Ok(start) = usize::try_from(start.unwrap_or(0))
            {
                if start <= self.prefix_len() {
                    return Some(VariableTupleSlicePlan::Mixed {
                        fixed_prefix: self.fixed_prefix_slice(Some(start), None, step),
                        variable: VariableSlice::variable_only(),
                        fixed_suffix: self.fixed_suffix_slice(None, None, step),
                    });
                }
                return Some(self.mixed_forward_approximation(start, ForwardSliceStop::End, step));
            }

            if let Some(start) = Self::front_start_index(start)
                && start <= self.prefix_len()
                && let Some(stop) = self.suffix_slice_index(stop)
                && let Ok(stop) = usize::try_from(stop)
            {
                return Some(VariableTupleSlicePlan::Mixed {
                    fixed_prefix: self.fixed_prefix_slice(Some(start), None, step),
                    variable: VariableSlice::variable_only(),
                    fixed_suffix: self.fixed_suffix_slice(None, Some(stop), step),
                });
            }

            if let Some(start) = Self::front_start_index(start) {
                if let Some(stop) = Self::nonnegative_slice_index(stop)
                    && stop > self.prefix_len()
                {
                    return Some(self.mixed_forward_approximation(
                        start,
                        ForwardSliceStop::Absolute(stop),
                        step,
                    ));
                }

                if start > self.prefix_len()
                    && let Some(stop) = self
                        .suffix_slice_index(stop)
                        .and_then(|stop| usize::try_from(stop).ok())
                {
                    return Some(self.mixed_forward_approximation(
                        start,
                        ForwardSliceStop::Suffix(stop),
                        step,
                    ));
                }
            }

            if let Some(start) = self
                .suffix_slice_index(start)
                .and_then(|start| usize::try_from(start).ok())
                && let Some(stop) = Self::nonnegative_slice_index(stop)
                && stop > self.prefix_len()
            {
                let suffix_union_stop = stop
                    .saturating_sub(self.prefix_len())
                    .min(self.suffix_len());
                debug_assert!(
                    start < suffix_union_stop,
                    "empty forward slices are planned before mixed suffix slices"
                );
                let variable = VariableSlice::suffix(start, suffix_union_stop)?;
                return Some(VariableTupleSlicePlan::Mixed {
                    fixed_prefix: None,
                    variable,
                    fixed_suffix: None,
                });
            }

            return None;
        }

        let prefix_start = match start {
            None => Some(0),
            Some(start) if start >= 0 => usize::try_from(start)
                .ok()
                .filter(|start| *start <= self.prefix_len()),
            Some(_) => return None,
        };

        let suffix_stop = match stop {
            None => None,
            Some(stop) if stop >= 0 => {
                let stop = Self::nonnegative_slice_index(Some(stop))?;
                if stop <= self.len().minimum() {
                    return None;
                }
                None
            }
            Some(_) => Some(usize::try_from(self.suffix_slice_index(stop)?).ok()?),
        };

        Some(VariableTupleSlicePlan::Mixed {
            fixed_prefix: prefix_start
                .and_then(|prefix_start| self.fixed_prefix_slice(Some(prefix_start), None, step)),
            variable: VariableSlice {
                kind: VariableSliceKind::ElementType,
                suffix_start: 0,
                suffix_stop: suffix_stop.unwrap_or_else(|| self.suffix_len()),
            },
            fixed_suffix: None,
        })
    }

    fn mixed_forward_approximation(
        &self,
        start: usize,
        stop: ForwardSliceStop,
        step: NonZeroUsize,
    ) -> VariableTupleSlicePlan {
        let suffix_stop = stop.suffix_stop(self);

        if start <= self.prefix_len() {
            return VariableTupleSlicePlan::Mixed {
                fixed_prefix: self.fixed_prefix_slice(Some(start), None, step),
                variable: VariableSlice {
                    kind: VariableSliceKind::ElementType,
                    suffix_start: 0,
                    suffix_stop,
                },
                fixed_suffix: None,
            };
        }

        let suffix_start = start.saturating_sub(self.prefix_len());
        let fixed_suffix_start = suffix_start.min(suffix_stop);
        let variable_suffix_stop = if stop.allows_fixed_suffix() {
            fixed_suffix_start
        } else {
            suffix_stop
        };
        VariableTupleSlicePlan::Mixed {
            fixed_prefix: None,
            variable: VariableSlice {
                kind: VariableSliceKind::ElementType,
                suffix_start: 0,
                suffix_stop: variable_suffix_stop,
            },
            fixed_suffix: if stop.allows_fixed_suffix() {
                self.fixed_suffix_slice(Some(fixed_suffix_start), Some(suffix_stop), step)
            } else {
                None
            },
        }
    }

    fn nonnegative_slice_index(index: Option<i32>) -> Option<usize> {
        index
            .filter(|index| *index >= 0)
            .and_then(|index| usize::try_from(index).ok())
    }

    fn front_start_index(start: Option<i32>) -> Option<usize> {
        match start {
            None => Some(0),
            Some(_) => Self::nonnegative_slice_index(start),
        }
    }

    fn suffix_slice_index(&self, index: Option<i32>) -> Option<i32> {
        let index = index?;
        if index >= 0 {
            return None;
        }
        let distance_from_end = Self::distance_from_end(index)?;
        if distance_from_end <= self.suffix_len() {
            i32::try_from(self.suffix_len() - distance_from_end).ok()
        } else {
            None
        }
    }

    fn distance_from_end(index: i32) -> Option<usize> {
        if index >= 0 {
            return None;
        }
        usize::try_from(index.checked_neg()?).ok()
    }

    fn last_forward_slice_index(start: usize, stop: usize, step: NonZeroUsize) -> usize {
        let step = step.get();
        start + ((stop - start - 1) / step) * step
    }

    fn type_at_nonnegative_index(&self, db: &'db dyn Db, index: usize) -> Option<Type<'db>> {
        (index < self.len().minimum()).then(|| self.type_at_nonnegative_index_unbounded(db, index))
    }

    fn type_at_nonnegative_index_unbounded(&self, db: &'db dyn Db, index: usize) -> Type<'db> {
        if let Some(element) = self.prefix_elements().get(index) {
            *element
        } else {
            let suffix_stop = index - self.prefix_len() + 1;
            self.variable_and_suffix_type(db, Some(suffix_stop))
        }
    }

    fn type_at_negative_distance(&self, db: &'db dyn Db, distance: usize) -> Option<Type<'db>> {
        if distance == 0 || distance > self.len().minimum() {
            return None;
        }

        if distance <= self.suffix_len() {
            return self
                .suffix_elements()
                .get(self.suffix_len() - distance)
                .copied();
        }

        let prefix_and_variable_len = distance - self.suffix_len();
        Some(UnionType::from_elements_leave_aliases(
            db,
            self.iter_prefix_elements()
                .skip(self.prefix_len() - prefix_and_variable_len)
                .chain(std::iter::once(self.variable().element_type(db))),
        ))
    }

    fn iter_all_elements(
        &self,
        db: &'db dyn Db,
    ) -> impl DoubleEndedIterator<Item = Type<'db>> + '_ {
        self.iter_prefix_elements()
            .chain(std::iter::once(self.variable().element_type(db)))
            .chain(self.iter_suffix_elements())
    }

    fn homogeneous_type(&self, db: &'db dyn Db) -> Type<'db> {
        let element = UnionType::from_elements_leave_aliases(db, self.iter_all_elements(db));
        Type::homogeneous_tuple(db, element)
    }

    fn variable_and_suffix_type(&self, db: &'db dyn Db, suffix_stop: Option<usize>) -> Type<'db> {
        UnionType::from_elements_leave_aliases(
            db,
            std::iter::once(self.variable().element_type(db)).chain(
                self.iter_suffix_elements()
                    .take(suffix_stop.unwrap_or_else(|| self.suffix_len())),
            ),
        )
    }

    /// Returns the prefix of the prenormalization of this tuple.
    ///
    /// This is used in our subtyping and equivalence checks below to handle different tuple types
    /// that represent the same set of runtime tuple values. For instance, the following two tuple
    /// types both represent "a tuple of one or more `int`s":
    ///
    /// ```py
    /// tuple[int, *tuple[int, ...]]
    /// tuple[*tuple[int, ...], int]
    /// ```
    ///
    /// Prenormalization rewrites both types into the former form. We arbitrarily prefer the
    /// elements to appear in the prefix if they can, so we move elements from the beginning of the
    /// suffix, which are equivalent to the variable-length portion, to the end of the prefix.
    ///
    /// Complicating matters is that we don't always want to compare with _this_ tuple's
    /// variable-length portion. (When this tuple's variable-length portion is gradual —
    /// `tuple[Any, ...]` — we compare with the assumption that the `Any` materializes to the other
    /// tuple's variable-length portion.)
    fn prenormalized_prefix_elements<'a>(
        &'a self,
        db: &'db dyn Db,
        variable: Option<Type<'db>>,
    ) -> impl Iterator<Item = Type<'db>> + 'a {
        let variable = variable.unwrap_or_else(|| self.variable().element_type(db));
        self.iter_prefix_elements().chain(
            self.iter_suffix_elements()
                .take_while(move |element| element.is_equivalent_to(db, variable)),
        )
    }

    /// Returns the suffix of the prenormalization of this tuple.
    ///
    /// This is used in our subtyping and equivalence checks below to handle different tuple types
    /// that represent the same set of runtime tuple values. For instance, the following two tuple
    /// types both represent "a tuple of one or more `int`s":
    ///
    /// ```py
    /// tuple[int, *tuple[int, ...]]
    /// tuple[*tuple[int, ...], int]
    /// ```
    ///
    /// Prenormalization rewrites both types into the former form. We arbitrarily prefer the
    /// elements to appear in the prefix if they can, so we move elements from the beginning of the
    /// suffix, which are equivalent to the variable-length portion, to the end of the prefix.
    ///
    /// Complicating matters is that we don't always want to compare with _this_ tuple's
    /// variable-length portion. (When this tuple's variable-length portion is gradual —
    /// `tuple[Any, ...]` — we compare with the assumption that the `Any` materializes to the other
    /// tuple's variable-length portion.)
    fn prenormalized_suffix_elements<'a>(
        &'a self,
        db: &'db dyn Db,
        variable: Option<Type<'db>>,
    ) -> impl Iterator<Item = Type<'db>> + 'a {
        let variable = variable.unwrap_or_else(|| self.variable().element_type(db));
        self.iter_suffix_elements()
            .skip_while(move |element| element.is_equivalent_to(db, variable))
    }

    fn resize(
        &self,
        db: &'db dyn Db,
        new_length: TupleLength,
    ) -> Result<TupleSpec<'db>, ResizeTupleError> {
        match new_length {
            TupleLength::Fixed(new_length) => {
                // The number of elements that will get their value from our variable-length
                // portion.
                let Some(variable_count) = new_length.checked_sub(self.len().minimum()) else {
                    return Err(ResizeTupleError::TooManyValues);
                };
                Ok(Tuple::Fixed(FixedLengthTuple::from_elements(
                    (self.iter_prefix_elements())
                        .chain(std::iter::repeat_n(
                            self.variable().element_type(db),
                            variable_count,
                        ))
                        .chain(self.iter_suffix_elements()),
                )))
            }

            TupleLength::Variable(prefix_length, suffix_length) => {
                // "Overflow" are elements of our prefix/suffix that will be folded into the
                // result's variable-length portion. "Underflow" are elements of the result
                // prefix/suffix that will come from our variable-length portion.
                let self_prefix_length = self.prefix_elements().len();
                let prefix_underflow = prefix_length.saturating_sub(self_prefix_length);
                let self_suffix_length = self.suffix_elements().len();
                let suffix_overflow = self_suffix_length.saturating_sub(suffix_length);
                let suffix_underflow = suffix_length.saturating_sub(self_suffix_length);
                // Compute the variable element first, since underflow positions can
                // receive any element that could appear in the variable portion.
                // For example, `tuple[I0, *tuple[I1, ...], I2]` unpacked as
                // `[a, b, *c]` means `b` could be `I1` (variable non-empty) or
                // `I2` (variable empty, suffix shifts left), so it should be `I1 | I2`.
                let variable = UnionType::from_elements_leave_aliases(
                    db,
                    self.iter_prefix_elements()
                        .skip(prefix_length)
                        .chain(std::iter::once(self.variable().element_type(db)))
                        .chain(self.iter_suffix_elements().take(suffix_overflow)),
                );
                let prefix = (self.iter_prefix_elements().take(prefix_length))
                    .chain(std::iter::repeat_n(variable, prefix_underflow));
                let suffix = std::iter::repeat_n(variable, suffix_underflow)
                    .chain(self.iter_suffix_elements().skip(suffix_overflow));
                Ok(VariableLengthTuple::mixed(
                    prefix,
                    VariableSegment::Homogeneous(variable),
                    suffix,
                ))
            }
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if nested {
            let prefix = self
                .prefix_elements()
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, true));

            let variable_segment = match self.variable() {
                VariableSegment::Homogeneous(variable) => VariableSegment::Homogeneous(
                    variable.recursive_type_normalized_impl(db, div, true)?,
                ),
                VariableSegment::TypeVarTuple(typevartuple) => {
                    VariableSegment::TypeVarTuple(typevartuple)
                }
            };

            let suffix = self
                .suffix_elements()
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, true));

            Self::try_new(prefix, variable_segment, suffix)
        } else {
            let prefix = self.prefix_elements().iter().map(|ty| {
                ty.recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div)
            });

            let variable_segment = match self.variable() {
                VariableSegment::Homogeneous(variable) => VariableSegment::Homogeneous(
                    variable
                        .recursive_type_normalized_impl(db, div, true)
                        .unwrap_or(div),
                ),
                VariableSegment::TypeVarTuple(typevartuple) => {
                    VariableSegment::TypeVarTuple(typevartuple)
                }
            };

            let suffix = self.suffix_elements().iter().map(|ty| {
                ty.recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div)
            });

            Some(Self::new(prefix, variable_segment, suffix))
        }
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> TupleSpec<'db> {
        let prefix = self
            .prefix_elements()
            .iter()
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor));
        let suffix = self
            .suffix_elements()
            .iter()
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor));

        match self.variable() {
            VariableSegment::Homogeneous(variable) => Self::mixed(
                prefix,
                VariableSegment::Homogeneous(variable.apply_type_mapping_impl(
                    db,
                    type_mapping,
                    tcx,
                    visitor,
                )),
                suffix,
            ),
            VariableSegment::TypeVarTuple(typevartuple) => {
                let mapped = Type::TypeVar(typevartuple).apply_type_mapping_impl(
                    db,
                    type_mapping,
                    tcx,
                    visitor,
                );
                if mapped == Type::TypeVar(typevartuple) {
                    return Self::mixed(
                        prefix,
                        VariableSegment::TypeVarTuple(typevartuple),
                        suffix,
                    );
                }
                if let Type::TypeVar(mapped_typevartuple) = mapped
                    && mapped_typevartuple.is_typevartuple(db)
                {
                    return Self::mixed(
                        prefix,
                        VariableSegment::TypeVarTuple(mapped_typevartuple),
                        suffix,
                    );
                }
                if let Some(mapped_tuple) = mapped.exact_tuple_instance_spec(db) {
                    let mut builder = TupleSpecBuilder::with_capacity(self.fixed_elements.len());
                    for element in prefix {
                        builder.push(element);
                    }
                    builder = builder.concat(db, &mapped_tuple);
                    for element in suffix {
                        builder.push(element);
                    }
                    return builder.build();
                }

                Self::mixed(prefix, VariableSegment::Homogeneous(mapped), suffix)
            }
        }
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for ty in self.prefix_elements() {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
        match self.variable() {
            VariableSegment::Homogeneous(variable) => {
                variable.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            VariableSegment::TypeVarTuple(typevartuple) => {
                Type::TypeVar(typevartuple).find_legacy_typevars_impl(
                    db,
                    binding_context,
                    typevars,
                    visitor,
                );
            }
        }
        for ty in self.suffix_elements() {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }
}

impl<'db> PyIndex<'db> for &VariableLengthTuple<Type<'db>, VariableSegment<'db>> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match Nth::from_index(index) {
            Nth::FromStart(index) => Ok(self.type_at_nonnegative_index_unbounded(db, index)),

            Nth::FromEnd(index_from_end) => {
                if index_from_end < self.suffix_elements().len() {
                    // index is small enough that it lands in the suffix of the tuple.
                    return Ok(
                        self.suffix_elements()[self.suffix_elements().len() - index_from_end - 1]
                    );
                }

                // index is large enough that it lands past the suffix. The tuple can always be
                // large enough that it lands in the variable-length portion. It might also be
                // small enough to land in the prefix.
                let index_past_suffix = index_from_end - self.suffix_elements().len() + 1;
                Ok(UnionType::from_elements_leave_aliases(
                    db,
                    (self.prefix_elements().iter().rev().copied())
                        .take(index_past_suffix)
                        .rev()
                        .chain(std::iter::once(self.variable().element_type(db))),
                ))
            }
        }
    }
}

/// A tuple that might be fixed- or variable-length.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum Tuple<T, V = T> {
    Fixed(FixedLengthTuple<T>),
    Variable(VariableLengthTuple<T, V>),
}

impl<T, V> Tuple<T, V> {
    /// Returns the inner fixed-length tuple if this is a `Tuple::Fixed` variant.
    pub(crate) fn as_fixed_length(&self) -> Option<&FixedLengthTuple<T>> {
        match self {
            Tuple::Fixed(tuple) => Some(tuple),
            Tuple::Variable(_) => None,
        }
    }

    pub(crate) const fn is_variadic(&self) -> bool {
        matches!(self, Tuple::Variable(_))
    }

    pub(crate) fn heterogeneous(elements: impl IntoIterator<Item = T>) -> Self {
        Self::Fixed(FixedLengthTuple::from_elements(elements))
    }

    /// Returns an iterator of all of the fixed-length element types of this tuple.
    pub(crate) fn fixed_elements(&self) -> impl Iterator<Item = &T> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.all_elements().iter()),
            Tuple::Variable(tuple) => Either::Right(tuple.fixed_elements()),
        }
    }

    pub(crate) fn into_all_elements_with_kind(self) -> impl Iterator<Item = TupleElement<T, V>> {
        match self {
            Tuple::Fixed(tuple) => {
                Either::Left(tuple.owned_elements().into_iter().map(TupleElement::Fixed))
            }
            Tuple::Variable(tuple) => Either::Right(tuple.into_all_elements_with_kind()),
        }
    }

    /// Returns the length of this tuple.
    pub(crate) fn len(&self) -> TupleLength {
        match self {
            Tuple::Fixed(tuple) => TupleLength::Fixed(tuple.len()),
            Tuple::Variable(tuple) => tuple.len(),
        }
    }

    pub(crate) fn truthiness(&self) -> Truthiness {
        match self.len().size_hint() {
            // The tuple type is AlwaysFalse if it contains only the empty tuple
            (_, Some(0)) => Truthiness::AlwaysFalse,
            // The tuple type is AlwaysTrue if its inhabitants must always have length >=1
            (minimum, _) if minimum > 0 => Truthiness::AlwaysTrue,
            // The tuple type is Ambiguous if its inhabitants could be of any length
            _ => Truthiness::Ambiguous,
        }
    }
}

impl<'db> Tuple<Type<'db>, VariableSegment<'db>> {
    pub(crate) const fn homogeneous(element: Type<'db>) -> Self {
        Self::Variable(VariableLengthTuple::homogeneous(
            VariableSegment::Homogeneous(element),
        ))
    }

    pub(crate) fn homogeneous_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Tuple::Fixed(tuple) => {
                UnionType::from_elements_leave_aliases(db, tuple.iter_all_elements())
            }
            Tuple::Variable(tuple) => {
                UnionType::from_elements_leave_aliases(db, tuple.iter_all_elements(db))
            }
        }
    }

    fn tuple_class_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Tuple::Fixed(tuple) => {
                UnionType::from_elements_leave_aliases(db, tuple.iter_all_elements())
            }
            Tuple::Variable(tuple) => UnionType::from_elements_leave_aliases(
                db,
                tuple
                    .iter_prefix_elements()
                    .chain(std::iter::once(tuple.variable().tuple_class_type()))
                    .chain(tuple.iter_suffix_elements()),
            ),
        }
    }

    pub(crate) fn variable_element_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Tuple::Fixed(_) => None,
            Tuple::Variable(tuple) => Some(tuple.variable().element_type(db)),
        }
    }

    pub(crate) fn iter_element_types(
        &self,
        db: &'db dyn Db,
    ) -> impl DoubleEndedIterator<Item = Type<'db>> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.iter_all_elements()),
            Tuple::Variable(tuple) => Either::Right(tuple.iter_all_elements(db)),
        }
    }

    /// Returns the type of a static slice into this tuple.
    ///
    /// Fixed-length tuples produce an exact heterogeneous tuple. Variable-length tuples preserve
    /// exact shape where it is cheap to do so, and otherwise use a sound homogeneous approximation.
    pub(crate) fn py_slice_type(
        &self,
        db: &'db dyn Db,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<Type<'db>, StepSizeZeroError> {
        match self {
            Tuple::Fixed(tuple) => Ok(Type::heterogeneous_tuple(
                db,
                tuple.py_slice(db, start, stop, step)?,
            )),
            Tuple::Variable(tuple) => tuple.py_slice_type(db, start, stop, step),
        }
    }

    /// Resizes this tuple to a different length, if possible. If this tuple cannot satisfy the
    /// desired minimum or maximum length, we return an error. If we return an `Ok` result, the
    /// [`len`][Self::len] of the resulting tuple is guaranteed to be equal to `new_length`.
    pub(crate) fn resize(
        &self,
        db: &'db dyn Db,
        new_length: TupleLength,
    ) -> Result<Self, ResizeTupleError> {
        match self {
            Tuple::Fixed(tuple) => tuple.resize(db, new_length),
            Tuple::Variable(tuple) => tuple.resize(db, new_length),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Tuple::Fixed(tuple) => Some(Tuple::Fixed(
                tuple.recursive_type_normalized_impl(db, div, nested)?,
            )),
            Tuple::Variable(tuple) => Some(Tuple::Variable(
                tuple.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Tuple::Fixed(tuple) => {
                Tuple::Fixed(tuple.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Tuple::Variable(tuple) => tuple.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        }
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self {
            Tuple::Fixed(tuple) => {
                tuple.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            Tuple::Variable(tuple) => {
                tuple.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
    }

    pub(crate) fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_single_valued(db),
            Tuple::Variable(_) => false,
        }
    }

    /// Calls a closure for each pair of elements that could potentially be compared at runtime
    /// between `self` and `other`.
    ///
    /// For two fixed-length tuples, this yields pairs at matching positions.
    /// For variable-length tuples, this yields all pairs of elements that could overlap at runtime,
    /// including prefix/suffix elements matched by position, and variable elements that could
    /// align with any position in the other tuple.
    pub(crate) fn try_for_each_element_pair<F, E>(
        &self,
        db: &'db dyn Db,
        other: &Self,
        mut f: F,
    ) -> Result<(), E>
    where
        F: FnMut(Type<'db>, Type<'db>) -> Result<(), E>,
    {
        match (self, other) {
            // Both fixed-length: just zip elements at matching positions.
            (Tuple::Fixed(left), Tuple::Fixed(right)) => {
                for (l, r) in left.iter_all_elements().zip(right.iter_all_elements()) {
                    f(l, r)?;
                }
            }

            // Both variable-length: all possible element pairings.
            (Tuple::Variable(left), Tuple::Variable(right)) => {
                // 1. Prefix elements at matching positions.
                for (l, r) in left.prefix_elements().iter().zip(right.prefix_elements()) {
                    f(*l, *r)?;
                }

                // 2. Left's extra prefix elements with right's variable.
                for l in left
                    .prefix_elements()
                    .iter()
                    .skip(right.prefix_elements().len())
                {
                    f(*l, right.variable().element_type(db))?;
                }

                // 3. Right's extra prefix elements with left's variable.
                for r in right
                    .prefix_elements()
                    .iter()
                    .skip(left.prefix_elements().len())
                {
                    f(left.variable().element_type(db), *r)?;
                }

                // 4. Variable elements with each other.
                f(
                    left.variable().element_type(db),
                    right.variable().element_type(db),
                )?;

                // 5. Left's extra suffix elements with right's variable.
                for l in left
                    .suffix_elements()
                    .iter()
                    .rev()
                    .skip(right.suffix_elements().len())
                {
                    f(*l, right.variable().element_type(db))?;
                }

                // 6. Right's extra suffix elements with left's variable.
                for r in right
                    .suffix_elements()
                    .iter()
                    .rev()
                    .skip(left.suffix_elements().len())
                {
                    f(left.variable().element_type(db), *r)?;
                }

                // 7. Suffix elements at matching positions (from the end).
                for (l, r) in left
                    .suffix_elements()
                    .iter()
                    .rev()
                    .zip(right.suffix_elements().iter().rev())
                {
                    f(*l, *r)?;
                }
            }

            // Left variable, right fixed.
            (Tuple::Variable(left), Tuple::Fixed(right)) => {
                // Left's prefix with right's corresponding elements.
                for (l, r) in left.prefix_elements().iter().zip(right.all_elements()) {
                    f(*l, *r)?;
                }

                // Left's suffix with right's corresponding elements (from end).
                for (l, r) in left
                    .suffix_elements()
                    .iter()
                    .rev()
                    .zip(right.all_elements().iter().rev())
                {
                    f(*l, *r)?;
                }

                // Left's variable with right's "middle" elements.
                let middle_start = left.prefix_elements().len();
                let middle_end = right.len().saturating_sub(left.suffix_elements().len());
                for r in right
                    .all_elements()
                    .iter()
                    .skip(middle_start)
                    .take(middle_end.saturating_sub(middle_start))
                {
                    f(left.variable().element_type(db), *r)?;
                }
            }

            // Left fixed, right variable.
            (Tuple::Fixed(left), Tuple::Variable(right)) => {
                // Left's elements with right's prefix.
                for (l, r) in left.all_elements().iter().zip(right.prefix_elements()) {
                    f(*l, *r)?;
                }

                // Left's elements (from end) with right's suffix.
                for (l, r) in left
                    .all_elements()
                    .iter()
                    .rev()
                    .zip(right.suffix_elements().iter().rev())
                {
                    f(*l, *r)?;
                }

                // Left's "middle" elements with right's variable.
                let middle_start = right.prefix_elements().len();
                let middle_end = left.len().saturating_sub(right.suffix_elements().len());
                for l in left
                    .all_elements()
                    .iter()
                    .skip(middle_start)
                    .take(middle_end.saturating_sub(middle_start))
                {
                    f(*l, right.variable().element_type(db))?;
                }
            }
        }

        Ok(())
    }

    /// Return the `TupleSpec` for the singleton `sys.version_info`
    pub(crate) fn version_info_spec(db: &'db dyn Db) -> TupleSpec<'db> {
        let python_version = Program::get(db).python_version(db);
        let int_instance_ty = KnownClass::Int.to_instance(db);

        // TODO: just grab this type from typeshed (it's a `sys._ReleaseLevel` type alias there)
        let release_level_ty = {
            let elements: Box<[Type<'db>]> = ["alpha", "beta", "candidate", "final"]
                .iter()
                .map(|level| Type::string_literal(db, *level))
                .collect();

            // For most unions, it's better to go via `UnionType::from_elements` or use `UnionBuilder`;
            // those techniques ensure that union elements are deduplicated and unions are eagerly simplified
            // into other types where necessary. Here, however, we know that there are no duplicates
            // in this union, so it's probably more efficient to use `UnionType::new()` directly.
            Type::Union(UnionType::new(db, elements, RecursivelyDefined::No))
        };

        TupleSpec::heterogeneous([
            Type::int_literal(python_version.major.into()),
            Type::int_literal(python_version.minor.into()),
            int_instance_ty,
            release_level_ty,
            int_instance_ty,
        ])
    }
}

impl<T, V> From<FixedLengthTuple<T>> for Tuple<T, V> {
    fn from(tuple: FixedLengthTuple<T>) -> Self {
        Tuple::Fixed(tuple)
    }
}

impl<T, V> From<VariableLengthTuple<T, V>> for Tuple<T, V> {
    fn from(tuple: VariableLengthTuple<T, V>) -> Self {
        Tuple::Variable(tuple)
    }
}

impl<'db> PyIndex<'db> for &TupleSpec<'db> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match self {
            Tuple::Fixed(tuple) => tuple.py_index(db, index),
            Tuple::Variable(tuple) => tuple.py_index(db, index),
        }
    }
}

pub(crate) enum TupleElement<T, V = T> {
    Fixed(T),
    Prefix(T),
    Variable(V),
    Suffix(T),
}

/// Unpacks tuple values in an unpacking assignment.
///
/// You provide a [`TupleLength`] specifying how many assignment targets there are, and which one
/// (if any) is a starred target. You then call [`unpack_tuple`][TupleUnpacker::unpack_tuple] to
/// unpack the values from a rhs tuple into those targets. If the rhs is a union, call
/// `unpack_tuple` separately for each element of the union. We will automatically wrap the types
/// assigned to the starred target in `list`.
pub(crate) struct TupleUnpacker<'db> {
    db: &'db dyn Db,
    targets: Tuple<UnionBuilder<'db>>,
}

impl<'db> TupleUnpacker<'db> {
    pub(crate) fn new(db: &'db dyn Db, len: TupleLength) -> Self {
        let new_builders = |len: usize| std::iter::repeat_with(|| UnionBuilder::new(db)).take(len);
        let targets = match len {
            TupleLength::Fixed(len) => {
                Tuple::Fixed(FixedLengthTuple::from_elements(new_builders(len)))
            }
            TupleLength::Variable(prefix, suffix) => VariableLengthTuple::mixed(
                new_builders(prefix),
                UnionBuilder::new(db),
                new_builders(suffix),
            ),
        };
        Self { db, targets }
    }

    /// Unpacks a single rhs tuple into the target tuple that we are building. If you want to
    /// unpack a single type into each target, call this method with a homogeneous tuple.
    ///
    /// The lengths of the targets and the rhs have to be compatible, but not necessarily
    /// identical. The lengths only have to be identical if both sides are fixed-length; if either
    /// side is variable-length, we will pull multiple values out of the rhs variable-length
    /// portion, and assign multiple values to the starred target, as needed.
    pub(crate) fn unpack_tuple(&mut self, values: &TupleSpec<'db>) -> Result<(), ResizeTupleError> {
        let values = values.resize(self.db, self.targets.len())?;
        match (&mut self.targets, &values) {
            (Tuple::Fixed(targets), Tuple::Fixed(values)) => {
                targets.unpack_tuple(values);
            }
            (Tuple::Variable(targets), Tuple::Variable(values)) => {
                targets.unpack_tuple(self.db, values);
            }
            _ => panic!("should have ensured that tuples are the same length"),
        }
        Ok(())
    }

    /// Returns the unpacked types for each target. If you called
    /// [`unpack_tuple`][TupleUnpacker::unpack_tuple] multiple times, each target type will be the
    /// union of the type unpacked into that target from each of the rhs tuples. If there is a
    /// starred target, we will each unpacked type in `list`.
    pub(crate) fn into_types(self) -> impl Iterator<Item = Type<'db>> {
        self.targets
            .into_all_elements_with_kind()
            .map(|builder| match builder {
                TupleElement::Variable(builder) => builder.try_build().unwrap_or_else(|| {
                    KnownClass::List.to_specialized_instance(self.db, &[Type::unknown()])
                }),
                TupleElement::Fixed(builder)
                | TupleElement::Prefix(builder)
                | TupleElement::Suffix(builder) => {
                    builder.try_build().unwrap_or_else(Type::unknown)
                }
            })
    }
}

impl<'db> FixedLengthTuple<UnionBuilder<'db>> {
    fn unpack_tuple(&mut self, values: &FixedLengthTuple<Type<'db>>) {
        // We have already verified above that the two tuples have the same length.
        for (target, value) in self.0.iter_mut().zip(values.iter_all_elements()) {
            target.add_in_place(value);
        }
    }
}

impl<'db> VariableLengthTuple<UnionBuilder<'db>> {
    fn unpack_tuple(
        &mut self,
        db: &'db dyn Db,
        values: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
    ) {
        // We have already verified above that the two tuples have the same length.
        for (target, value) in
            (self.prefix_elements_mut().iter_mut()).zip(values.iter_prefix_elements())
        {
            target.add_in_place(value);
        }
        self.variable_element_mut().add_in_place(
            KnownClass::List.to_specialized_instance(db, &[values.variable().element_type(db)]),
        );
        for (target, value) in
            (self.suffix_elements_mut().iter_mut()).zip(values.iter_suffix_elements())
        {
            target.add_in_place(value);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResizeTupleError {
    TooFewValues,
    TooManyValues,
}

/// A builder for creating a new [`TupleSpec`]
#[derive(Clone)]
pub(crate) enum TupleSpecBuilder<'db> {
    Fixed(Vec<Type<'db>>),
    Variable {
        prefix: Vec<Type<'db>>,
        segment: VariableSegment<'db>,
        suffix: Vec<Type<'db>>,
    },
}

impl<'db> TupleSpecBuilder<'db> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        TupleSpecBuilder::Fixed(Vec::with_capacity(capacity))
    }

    pub(crate) fn push(&mut self, element: Type<'db>) {
        match self {
            TupleSpecBuilder::Fixed(elements) => elements.push(element),
            TupleSpecBuilder::Variable { suffix, .. } => suffix.push(element),
        }
    }

    /// Concatenates an unpacked `TypeVarTuple` as the variable-length portion of this tuple.
    pub(crate) fn concat_variadic_typevar(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Self {
        debug_assert!(typevar.is_typevartuple(db));
        let other = VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(typevar), []);
        self.concat(db, &other)
    }

    /// Concatenates another tuple to the end of this tuple, returning a new tuple.
    pub(crate) fn concat(mut self, db: &'db dyn Db, other: &TupleSpec<'db>) -> Self {
        match (&mut self, other) {
            (TupleSpecBuilder::Fixed(left_tuple), TupleSpec::Fixed(right_tuple)) => {
                left_tuple.extend_from_slice(&right_tuple.0);
                self
            }

            (TupleSpecBuilder::Fixed(left_tuple), TupleSpec::Variable(variable_tuple)) => {
                left_tuple.extend_from_slice(variable_tuple.prefix_elements());
                TupleSpecBuilder::Variable {
                    prefix: std::mem::take(left_tuple),
                    segment: variable_tuple.variable(),
                    suffix: variable_tuple.suffix_elements().to_vec(),
                }
            }

            (
                TupleSpecBuilder::Variable {
                    prefix: _,
                    segment: _,
                    suffix,
                },
                TupleSpec::Fixed(right),
            ) => {
                suffix.extend_from_slice(&right.0);
                self
            }

            (
                TupleSpecBuilder::Variable {
                    prefix: left_prefix,
                    segment: left_segment,
                    suffix: left_suffix,
                },
                TupleSpec::Variable(right),
            ) => {
                let variable = UnionType::from_elements_leave_aliases(
                    db,
                    left_suffix
                        .iter()
                        .copied()
                        .chain(std::iter::once(left_segment.element_type(db)))
                        .chain(std::iter::once(right.variable().element_type(db)))
                        .chain(right.iter_prefix_elements()),
                );
                TupleSpecBuilder::Variable {
                    prefix: std::mem::take(left_prefix),
                    segment: VariableSegment::Homogeneous(variable),
                    suffix: right.suffix_elements().to_vec(),
                }
            }
        }
    }

    fn iter_element_types(&self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            TupleSpecBuilder::Fixed(elements) => Either::Left(elements.iter().copied()),
            TupleSpecBuilder::Variable {
                prefix,
                segment,
                suffix,
            } => Either::Right(
                prefix
                    .iter()
                    .copied()
                    .chain(std::iter::once(segment.element_type(db)))
                    .chain(suffix.iter().copied()),
            ),
        }
    }

    /// Return a new tuple-spec builder that reflects the union of this tuple and another tuple.
    ///
    /// For example, if `self` is a tuple-spec builder for `tuple[Literal[42], str]` and `other` is a
    /// tuple-spec for `tuple[Literal[56], str]`, the result will be a tuple-spec builder for
    /// `tuple[Literal[42, 56], str]`.
    ///
    /// To keep things simple, we currently only attempt to preserve the "fixed-length-ness" of
    /// a tuple spec if both `self` and `other` have the exact same length. For example,
    /// if `self` is a tuple-spec builder for `tuple[int, str]` and `other` is a tuple-spec for
    /// `tuple[int, str, bytes]`, the result will be a tuple-spec builder for
    /// `tuple[int | str | bytes, ...]`. We could consider improving this in the future if real-world
    /// use cases arise.
    pub(crate) fn union(mut self, db: &'db dyn Db, other: &TupleSpec<'db>) -> Self {
        match (&mut self, other) {
            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Fixed(new_elements))
                if our_elements.len() == new_elements.len() =>
            {
                for (existing, new) in our_elements.iter_mut().zip(new_elements.all_elements()) {
                    *existing = UnionType::from_elements_leave_aliases(db, [*existing, *new]);
                }
                self
            }

            // We *could* have a branch here where both `self` and `other` are mixed tuples
            // with same-length prefixes and same-length suffixes. We *could* zip the two
            // `prefix` vecs together, unioning each pair of elements to create a new `prefix`
            // vec, and do the same for the `suffix` vecs. This would preserve the tuple specs
            // of the union elements more closely. But it's hard to think of a test where this
            // would actually lead to more precise inference, so it's probably not worth the
            // complexity.
            _ => {
                let unioned = UnionType::from_elements_leave_aliases(
                    db,
                    self.iter_element_types(db)
                        .chain(other.iter_element_types(db)),
                );
                TupleSpecBuilder::Variable {
                    prefix: vec![],
                    segment: VariableSegment::Homogeneous(unioned),
                    suffix: vec![],
                }
            }
        }
    }

    /// Return a new tuple-spec builder that reflects the intersection of this tuple and another
    /// tuple, or `None` if the intersection is impossible (e.g., two fixed-length tuples with
    /// different lengths).
    ///
    /// For example, if `self` is a tuple-spec builder for `tuple[int, str]` and `other` is a
    /// tuple-spec for `tuple[object, object]`, the result will be a tuple-spec builder for
    /// `tuple[int, str]` (since `int & object` simplifies to `int`, and `str & object` to `str`).
    pub(crate) fn intersect(mut self, db: &'db dyn Db, other: &TupleSpec<'db>) -> Option<Self> {
        match (&mut self, other) {
            // Both fixed-length with the same length: element-wise intersection.
            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Fixed(new_elements))
                if our_elements.len() == new_elements.len() =>
            {
                for (existing, new) in our_elements.iter_mut().zip(new_elements.all_elements()) {
                    *existing = IntersectionType::from_elements(db, [*existing, *new]);
                }
                Some(self)
            }

            // Fixed-length tuples with different lengths cannot intersect.
            (TupleSpecBuilder::Fixed(_), TupleSpec::Fixed(_)) => None,

            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Variable(var)) => var
                .resize(db, TupleLength::Fixed(our_elements.len()))
                .ok()
                .and_then(|tuple| self.intersect(db, &tuple)),

            (TupleSpecBuilder::Variable { .. }, TupleSpec::Fixed(fixed)) => self
                .clone()
                .build()
                .resize(db, TupleLength::Fixed(fixed.len()))
                .ok()
                .and_then(|tuple| TupleSpecBuilder::from(&tuple).intersect(db, other)),

            (
                TupleSpecBuilder::Variable {
                    prefix,
                    segment,
                    suffix,
                },
                TupleSpec::Variable(var),
            ) => {
                if prefix.len() == var.prefix_elements().len()
                    && suffix.len() == var.suffix_elements().len()
                {
                    for (existing, new) in prefix.iter_mut().zip(var.prefix_elements()) {
                        *existing = IntersectionType::from_two_elements(db, *existing, *new);
                    }
                    *segment = match (*segment, var.variable()) {
                        (
                            VariableSegment::TypeVarTuple(left),
                            VariableSegment::TypeVarTuple(right),
                        ) if left == right => VariableSegment::TypeVarTuple(left),
                        (left, right) => {
                            VariableSegment::Homogeneous(IntersectionType::from_two_elements(
                                db,
                                left.element_type(db),
                                right.element_type(db),
                            ))
                        }
                    };
                    for (existing, new) in suffix.iter_mut().zip(var.suffix_elements()) {
                        *existing = IntersectionType::from_two_elements(db, *existing, *new);
                    }
                    return Some(self);
                }

                let self_built = self.clone().build();
                let self_len = self_built.len();
                var.resize(db, self_len)
                    .ok()
                    .and_then(|resized| self.intersect(db, &resized))
                    .or_else(|| {
                        self_built.resize(db, var.len()).ok().and_then(|resized| {
                            TupleSpecBuilder::from(&resized).intersect(db, other)
                        })
                    })
            }
        }
    }

    pub(super) fn build(self) -> TupleSpec<'db> {
        match self {
            TupleSpecBuilder::Fixed(elements) => {
                TupleSpec::Fixed(FixedLengthTuple(elements.into_boxed_slice()))
            }
            TupleSpecBuilder::Variable {
                prefix,
                segment,
                suffix,
            } => TupleSpec::Variable(VariableLengthTuple::new_from_vec(prefix, segment, suffix)),
        }
    }
}

impl<'db> From<&TupleSpec<'db>> for TupleSpecBuilder<'db> {
    fn from(tuple: &TupleSpec<'db>) -> Self {
        match tuple {
            TupleSpec::Fixed(fixed) => TupleSpecBuilder::Fixed(fixed.0.to_vec()),
            TupleSpec::Variable(variable) => TupleSpecBuilder::Variable {
                prefix: variable.prefix_elements().to_vec(),
                segment: variable.variable(),
                suffix: variable.suffix_elements().to_vec(),
            },
        }
    }
}

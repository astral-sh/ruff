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
use ruff_python_ast::PythonVersion;
use smallvec::{SmallVec, smallvec_inline};

use crate::subscript::{
    Nth, OutOfBoundsError, PyIndex, PySlice, StepSizeZeroError, py_slice_with_step,
};
use crate::types::class::{ClassType, KnownClass};
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::relation::{DisjointnessChecker, TypeRelationChecker};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ErrorContext, FindLegacyTypeVarsVisitor,
    IntersectionType, Type, TypeContext, TypeMapping, UnionBuilder, UnionType,
};
use crate::{Db, FxOrderSet};
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
    program: crate::Program<'db>,
    tuple: TupleType<'db>,
    visitor: &V,
) {
    for element in tuple.tuple(db).iter_all_elements() {
        visitor.visit_type(db, program, element);
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
        if let TupleSpec::Variable(tuple) = spec {
            if tuple.variable().is_never() {
                let tuple = TupleSpec::Fixed(FixedLengthTuple::from_elements(
                    tuple
                        .iter_prefix_elements()
                        .chain(tuple.iter_suffix_elements()),
                ));
                return Some(TupleType::new_internal::<_, TupleSpec<'db>>(db, tuple));
            }
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
        TupleType::new(db, &VariableLengthTuple::mixed(prefix, variable, suffix))
    }

    pub(crate) fn homogeneous(db: &'db dyn Db, element: Type<'db>) -> Self {
        match element {
            Type::Never => TupleType::empty(db),
            _ => TupleType::new_internal(db, TupleSpec::homogeneous(element)),
        }
    }

    // N.B. If this method is not Salsa-tracked, we take 10 minutes to check
    // `static-frame` as part of the ecosystem analysis. This is because it's called
    // from `NominalInstanceType::class()`, which is a very hot method.
    #[salsa::tracked(cycle_initial=to_class_type_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn to_class_type(
        self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
    ) -> ClassType<'db> {
        let tuple_class = KnownClass::Tuple
            .try_to_class_literal(db, program)
            .expect("Typeshed should always have a `tuple` class in `builtins.pyi`");

        tuple_class.apply_specialization(db, |generic_context| {
            if generic_context.variables(db).len() == 1 {
                let element_type = self.tuple(db).homogeneous_element_type(db, program);
                generic_context.specialize_tuple(db, element_type, self)
            } else {
                generic_context.default_specialization(db, program, Some(KnownClass::Tuple))
            }
        })
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new_internal(
            db,
            self.tuple(db)
                .recursive_type_normalized_impl(db, program, div, nested)?,
        ))
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Option<Self> {
        TupleType::new(
            db,
            &self
                .tuple(db)
                .apply_type_mapping_impl(db, program, type_mapping, tcx, visitor),
        )
    }

    pub(crate) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.tuple(db)
            .find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
    }

    pub(crate) fn is_single_valued(self, db: &'db dyn Db, program: crate::Program<'db>) -> bool {
        self.tuple(db).is_single_valued(db, program)
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_tuple_type_pair(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        source: TupleType<'db>,
        target: TupleType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_tuple_spec_pair(db, program, source.tuple(db), target.tuple(db))
    }

    fn check_tuple_spec_pair(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        source: &TupleSpec<'db>,
        target: &TupleSpec<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match source {
            Tuple::Fixed(source) => {
                self.check_fixed_length_tuple_vs_tuple_spec(db, program, source, target)
            }
            Tuple::Variable(source) => {
                self.check_variable_length_vs_tuple_spec(db, program, source, target)
            }
        }
    }

    fn check_fixed_length_tuple_vs_tuple_spec(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        source_tuple: &FixedLengthTuple<Type<'db>>,
        target_tuple: &TupleSpec<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match target_tuple {
            Tuple::Fixed(target) => {
                let equal_length = source_tuple.0.len() == target.0.len();

                if !equal_length && self.is_eager_assignability() {
                    self.provide_context(|| ErrorContext::TupleLengthMismatch {
                        source_len: source_tuple.0.len(),
                        target_len: target_tuple.len(),
                    });
                }

                let mut n = 1;
                ConstraintSet::from_bool(self.constraints, equal_length).and(
                    db,
                    program,
                    self.constraints,
                    || {
                        (source_tuple.0.iter().zip(&target.0)).when_all(
                            db,
                            program,
                            self.constraints,
                            |(&source, &target)| {
                                let constraint_set =
                                    self.check_type_pair(db, program, source, target);
                                if constraint_set.is_never_satisfied(db, program) {
                                    self.provide_context(|| {
                                        ErrorContext::TupleElementNotCompatible {
                                            source,
                                            target,
                                            element_index: n,
                                            element_count: source_tuple.0.len(),
                                        }
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
                    let element_constraints =
                        self.check_type_pair(db, program, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_never_satisfied(db, program)
                    {
                        return result;
                    }
                }
                for target_ty in target.iter_suffix_elements().rev() {
                    let Some(&source_ty) = source_iter.next_back() else {
                        return self.never();
                    };
                    let element_constraints =
                        self.check_type_pair(db, program, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_never_satisfied(db, program)
                    {
                        return result;
                    }
                }

                // In addition, any remaining elements in this tuple must satisfy the
                // variable-length portion of the other tuple.
                result.and(db, program, self.constraints, || {
                    source_iter.when_all(db, self.program, self.constraints, |&source_ty| {
                        self.check_type_pair(db, program, source_ty, target.variable())
                    })
                })
            }
        }
    }

    fn check_variable_length_vs_tuple_spec(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        source: &VariableLengthTuple<Type<'db>>,
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
                if !self.is_eager_assignability() || !source.variable().is_dynamic() {
                    return self.never();
                }

                // In addition, the other tuple must have enough elements to match up with this
                // tuple's prefix and suffix, and each of those elements must pairwise satisfy the
                // relation.
                let mut result = self.always();
                let mut target_iter = target.iter_all_elements();
                for source_ty in source.prenormalized_prefix_elements(db, program, None) {
                    let Some(target_ty) = target_iter.next() else {
                        return self.never();
                    };
                    let element_constraints =
                        self.check_type_pair(db, program, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_never_satisfied(db, program)
                    {
                        return result;
                    }
                }
                let suffix: Vec<_> = source
                    .prenormalized_suffix_elements(db, program, None)
                    .collect();
                for &source_ty in suffix.iter().rev() {
                    let Some(target_ty) = target_iter.next_back() else {
                        return self.never();
                    };
                    let element_constraints =
                        self.check_type_pair(db, program, source_ty, target_ty);
                    if result
                        .intersect(db, self.constraints, element_constraints)
                        .is_never_satisfied(db, program)
                    {
                        return result;
                    }
                }

                result
            }

            Tuple::Variable(target) => {
                // When prenormalizing below, we assume that a dynamic variable-length portion of
                // one tuple materializes to the variable-length portion of the other tuple.
                let source_prenormalize_variable = match source.variable() {
                    Type::Dynamic(_) => Some(target.variable()),
                    _ => None,
                };
                let target_prenormalize_variable = match target.variable() {
                    Type::Dynamic(_) => Some(source.variable()),
                    _ => None,
                };

                // The overlapping parts of the prefixes and suffixes must satisfy the relation.
                // Any remaining parts must satisfy the relation with the other tuple's
                // variable-length part.
                let mut result = self.always();
                let pairwise = source
                    .prenormalized_prefix_elements(db, program, source_prenormalize_variable)
                    .zip_longest(target.prenormalized_prefix_elements(
                        db,
                        program,
                        target_prenormalize_variable,
                    ));
                for pair in pairwise {
                    let pair_constraints = match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => {
                            self.check_type_pair(db, program, self_ty, other_ty)
                        }
                        EitherOrBoth::Left(self_ty) => {
                            self.check_type_pair(db, program, self_ty, target.variable())
                        }
                        EitherOrBoth::Right(other_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !self.is_eager_assignability() || !source.variable().is_dynamic() {
                                return self.never();
                            }
                            self.check_type_pair(db, program, source.variable(), other_ty)
                        }
                    };
                    if result
                        .intersect(db, self.constraints, pair_constraints)
                        .is_never_satisfied(db, program)
                    {
                        return result;
                    }
                }

                let source_suffix: Vec<_> = source
                    .prenormalized_suffix_elements(db, program, source_prenormalize_variable)
                    .collect();
                let target_suffix: Vec<_> = target
                    .prenormalized_suffix_elements(db, program, target_prenormalize_variable)
                    .collect();
                let pairwise = source_suffix
                    .iter()
                    .rev()
                    .zip_longest(target_suffix.iter().rev());
                for pair in pairwise {
                    let pair_constraints = match pair {
                        EitherOrBoth::Both(&source_ty, &target_ty) => {
                            self.check_type_pair(db, program, source_ty, target_ty)
                        }
                        EitherOrBoth::Left(&source_ty) => {
                            self.check_type_pair(db, program, source_ty, target.variable())
                        }
                        EitherOrBoth::Right(&target_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !self.is_eager_assignability() || !source.variable().is_dynamic() {
                                return self.never();
                            }
                            self.check_type_pair(db, program, source.variable(), target_ty)
                        }
                    };
                    if result
                        .intersect(db, self.constraints, pair_constraints)
                        .is_never_satisfied(db, program)
                    {
                        return result;
                    }
                }

                // And lastly, the variable-length portions must satisfy the relation.
                result.and(db, program, self.constraints, || {
                    self.check_type_pair(db, program, source.variable(), target.variable())
                })
            }
        }
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    pub(super) fn check_tuple_type_pair(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        left: TupleType<'db>,
        right: TupleType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_tuple_spec_pair(db, program, left.tuple(db), right.tuple(db))
    }

    pub(super) fn check_tuple_spec_pair(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
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
                    program,
                    self.constraints,
                    |(&left_elem, &right_elem)| {
                        self.check_type_pair(db, program, left_elem, right_elem)
                    },
                )
            } else {
                std::iter::zip(a, b).when_any(
                    db,
                    program,
                    self.constraints,
                    |(&left_elem, &right_elem)| {
                        self.check_type_pair(db, program, left_elem, right_elem)
                    },
                )
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
                    program,
                    self.constraints,
                    || any_disjoint(left.suffix_elements(), right.suffix_elements(), true),
                )
            }

            (Tuple::Fixed(fixed), Tuple::Variable(variable))
            | (Tuple::Variable(variable), Tuple::Fixed(fixed)) => {
                any_disjoint(fixed.all_elements(), variable.prefix_elements(), false).or(
                    db,
                    program,
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
    program: crate::Program<'db>,
) -> ClassType<'db> {
    let tuple_class = KnownClass::Tuple
        .try_to_class_literal(db, program)
        .expect("Typeshed should always have a `tuple` class in `builtins.pyi`");

    tuple_class.apply_specialization(db, |generic_context| {
        if generic_context.variables(db).len() == 1 {
            generic_context.specialize_tuple(db, Type::divergent(id), self_)
        } else {
            generic_context.default_specialization(db, program, Some(KnownClass::Tuple))
        }
    })
}

/// A tuple spec describes the contents of a tuple type, which might be fixed- or variable-length.
///
/// Tuple specs are used for more than just `tuple` instances, so they allow `Never` to appear as a
/// fixed-length element type. [`TupleType`] adds that additional invariant (since a tuple that
/// must contain an element that can't be instantiated, can't be instantiated itself).
pub(crate) type TupleSpec<'db> = Tuple<Type<'db>>;

/// A fixed-length tuple.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
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

    pub(crate) fn into_all_elements_with_kind(self) -> impl Iterator<Item = TupleElement<T>> {
        self.0.into_iter().map(TupleElement::Fixed)
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
        program: crate::Program<'db>,
        new_length: TupleLength,
    ) -> Result<Tuple<Type<'db>>, ResizeTupleError> {
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
                let variable = UnionType::from_elements_leave_aliases(
                    db,
                    program,
                    elements.by_ref().take(variable),
                );
                let suffix = elements.by_ref().take(suffix);
                Ok(VariableLengthTuple::mixed(prefix, variable, suffix))
            }
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if nested {
            Some(Self::from_elements(
                self.0
                    .iter()
                    .map(|ty| ty.recursive_type_normalized_impl(db, program, div, true))
                    .collect::<Option<Box<[_]>>>()?,
            ))
        } else {
            Some(Self::from_elements(
                self.0
                    .iter()
                    .map(|ty| {
                        ty.recursive_type_normalized_impl(db, program, div, true)
                            .unwrap_or(div)
                    })
                    .collect::<Box<[_]>>(),
            ))
        }
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let tcx_tuple = tcx
            .annotation
            .and_then(|annotation| annotation.known_specialization(db, program, KnownClass::Tuple))
            .and_then(|specialization| {
                specialization
                    .tuple(db)
                    .expect("the specialization of `KnownClass::Tuple` must have a tuple spec")
                    .resize(db, program, TupleLength::Fixed(self.0.len()))
                    .ok()
            });

        let tcx_elements = match tcx_tuple.as_ref() {
            None => Either::Right(std::iter::repeat(TypeContext::default())),
            Some(tuple) => Either::Left(
                tuple
                    .iter_all_elements()
                    .map(|tcx| TypeContext::new(Some(tcx))),
            ),
        };

        Self::from_elements(
            self.0.iter().zip(tcx_elements).map(|(ty, tcx)| {
                ty.apply_type_mapping_impl(db, program, type_mapping, tcx, visitor)
            }),
        )
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for ty in &self.0 {
            ty.find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
        }
    }

    fn is_single_valued(&self, db: &'db dyn Db, program: crate::Program<'db>) -> bool {
        self.0.iter().all(|ty| ty.is_single_valued(db, program))
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
/// The tuple can contain a fixed-length heterogeneous prefix and/or suffix. All of the elements of
/// the variable-length portion must be the same.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct VariableLengthTuple<T> {
    pub(crate) elements: smallvec::SmallVec<[T; 1]>,
    variable_index: usize,
}

impl<T> VariableLengthTuple<T> {
    /// Creates a new tuple spec containing zero or more elements of a given type, with no prefix
    /// or suffix.
    const fn homogeneous(ty: T) -> Self {
        let elements = smallvec_inline![ty];
        Self {
            elements,
            variable_index: 0,
        }
    }

    fn mixed(
        prefix: impl IntoIterator<Item = T>,
        variable: T,
        suffix: impl IntoIterator<Item = T>,
    ) -> Tuple<T> {
        Tuple::Variable(Self::new(prefix, variable, suffix))
    }

    fn try_new<P, S>(prefix: P, variable: T, suffix: S) -> Option<Self>
    where
        P: IntoIterator<Item = Option<T>>,
        P::IntoIter: ExactSizeIterator,
        S: IntoIterator<Item = Option<T>>,
        S::IntoIter: ExactSizeIterator,
    {
        let prefix = prefix.into_iter();
        let suffix = suffix.into_iter();

        let mut elements =
            SmallVec::with_capacity(prefix.len().saturating_add(suffix.len()).saturating_add(1));

        for element in prefix {
            elements.push(element?);
        }

        let variable_index = elements.len();
        elements.push(variable);

        for element in suffix {
            elements.push(element?);
        }

        elements.shrink_to_fit();

        Some(Self {
            elements,
            variable_index,
        })
    }

    fn new(
        prefix: impl IntoIterator<Item = T>,
        variable: T,
        suffix: impl IntoIterator<Item = T>,
    ) -> Self {
        let mut elements = SmallVec::new_const();
        elements.extend(prefix);

        let variable_index = elements.len();
        elements.push(variable);
        elements.extend(suffix);
        elements.shrink_to_fit();

        Self {
            elements,
            variable_index,
        }
    }

    fn new_from_vec(prefix: Vec<T>, variable: T, suffix: Vec<T>) -> Self {
        let mut elements = SmallVec::from_vec(prefix);

        let variable_index = elements.len();
        elements.push(variable);
        elements.extend(suffix);
        elements.shrink_to_fit();

        Self {
            elements,
            variable_index,
        }
    }

    pub(crate) fn variable(&self) -> T
    where
        T: Copy,
    {
        self.elements[self.variable_index]
    }

    pub(crate) fn variable_element(&self) -> &T {
        &self.elements[self.variable_index]
    }

    pub(crate) fn variable_element_mut(&mut self) -> &mut T {
        &mut self.elements[self.variable_index]
    }

    pub(crate) fn prefix_elements(&self) -> &[T] {
        &self.elements[..self.variable_index]
    }

    pub(crate) fn iter_prefix_elements(&self) -> impl DoubleEndedIterator<Item = T>
    where
        T: Copy,
    {
        self.prefix_elements().iter().copied()
    }

    pub(crate) fn prefix_elements_mut(&mut self) -> &mut [T] {
        &mut self.elements[..self.variable_index]
    }

    pub(crate) fn suffix_elements(&self) -> &[T] {
        &self.elements[self.suffix_offset()..]
    }

    pub(crate) fn iter_suffix_elements(&self) -> impl DoubleEndedIterator<Item = T>
    where
        T: Copy,
    {
        self.suffix_elements().iter().copied()
    }

    pub(crate) fn suffix_elements_mut(&mut self) -> &mut [T] {
        let suffix_offset = self.suffix_offset();
        &mut self.elements[suffix_offset..]
    }

    fn suffix_offset(&self) -> usize {
        self.variable_index + 1
    }

    fn fixed_elements(&self) -> impl Iterator<Item = &T> + '_ {
        self.prefix_elements().iter().chain(self.suffix_elements())
    }

    fn all_elements(&self) -> &[T] {
        &self.elements
    }

    fn into_all_elements_with_kind(self) -> impl Iterator<Item = TupleElement<T>> {
        self.elements
            .into_iter()
            .enumerate()
            .map(move |(i, element)| match i.cmp(&self.variable_index) {
                Ordering::Less => TupleElement::Prefix(element),
                Ordering::Equal => TupleElement::Variable(element),
                Ordering::Greater => TupleElement::Suffix(element),
            })
    }

    fn prefix_len(&self) -> usize {
        self.variable_index
    }

    fn suffix_len(&self) -> usize {
        self.elements.len() - self.suffix_offset()
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

/// The elements folded into the variable part of a sliced variable-length tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VariableSlice {
    include_variable: bool,
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
    fn suffix_stop(self, tuple: &VariableLengthTuple<Type<'_>>) -> usize {
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
            include_variable: true,
            suffix_start: 0,
            suffix_stop: 0,
        }
    }

    fn suffix(start: usize, stop: usize) -> Option<Self> {
        (start < stop).then_some(Self {
            include_variable: false,
            suffix_start: start,
            suffix_stop: stop,
        })
    }

    fn ty<'db>(
        self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        tuple: &VariableLengthTuple<Type<'db>>,
    ) -> Type<'db> {
        UnionType::from_elements_leave_aliases(
            db,
            program,
            self.include_variable
                .then_some(tuple.variable())
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
        program: crate::Program<'db>,
        tuple: &VariableLengthTuple<Type<'db>>,
    ) -> Type<'db> {
        match self {
            VariableTupleSlicePlan::Empty => {
                Type::heterogeneous_tuple(db, std::iter::empty::<Type<'db>>())
            }

            VariableTupleSlicePlan::Fixed(fixed) => {
                Type::heterogeneous_tuple(db, tuple.slice_fixed_position(db, program, fixed))
            }

            VariableTupleSlicePlan::Mixed {
                fixed_prefix,
                variable,
                fixed_suffix,
            } => Type::tuple(TupleType::mixed(
                db,
                VariableLengthTuple::optional_fixed_slice(tuple.prefix_elements(), fixed_prefix),
                variable.ty(db, program, tuple),
                VariableLengthTuple::optional_fixed_slice(tuple.suffix_elements(), fixed_suffix),
            )),

            VariableTupleSlicePlan::Homogeneous => tuple.homogeneous_type(db, program),
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

impl<'db> VariableLengthTuple<Type<'db>> {
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
        program: crate::Program<'db>,
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
                Either::Left(self.slice_front_forward(db, program, start, exclusive_stop, step))
            }
            FixedPositionOrigin::Back => {
                Either::Right(self.slice_back(db, program, start, exclusive_stop, step))
            }
        }
    }

    fn slice_front_forward<'a>(
        &'a self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
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
                self.type_at_nonnegative_index(db, program, index)
                    .unwrap_or_else(|| {
                    unreachable!(
                        "front-origin fixed slice positions are validated during plan construction"
                    )
                    })
            })
    }

    fn slice_back<'a>(
        &'a self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
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
                .type_at_negative_distance(db, program, distance)
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

    fn reversed(&self) -> Self {
        Self::new(
            self.iter_suffix_elements().rev(),
            self.variable(),
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
        program: crate::Program<'db>,
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
                .into_type(db, program, self),
            TupleSliceDirection::Backward => {
                let reversed = self.reversed();
                reversed
                    .forward_slice_plan(
                        TupleSliceDirection::reverse_bound(start),
                        TupleSliceDirection::reverse_bound(stop),
                        step,
                    )
                    .into_type(db, program, &reversed)
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
                include_variable: true,
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
                    include_variable: true,
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
                include_variable: true,
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

    fn type_at_nonnegative_index(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        index: usize,
    ) -> Option<Type<'db>> {
        (index < self.len().minimum())
            .then(|| self.type_at_nonnegative_index_unbounded(db, program, index))
    }

    fn type_at_nonnegative_index_unbounded(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        index: usize,
    ) -> Type<'db> {
        if let Some(element) = self.prefix_elements().get(index) {
            *element
        } else {
            let suffix_stop = index - self.prefix_len() + 1;
            self.variable_and_suffix_type(db, program, Some(suffix_stop))
        }
    }

    fn type_at_negative_distance(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        distance: usize,
    ) -> Option<Type<'db>> {
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
            program,
            self.iter_prefix_elements()
                .skip(self.prefix_len() - prefix_and_variable_len)
                .chain(std::iter::once(self.variable())),
        ))
    }

    fn homogeneous_type(&self, db: &'db dyn Db, program: crate::Program<'db>) -> Type<'db> {
        let element = UnionType::from_elements_leave_aliases(db, program, self.all_elements());
        Type::homogeneous_tuple(db, element)
    }

    fn variable_and_suffix_type(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        suffix_stop: Option<usize>,
    ) -> Type<'db> {
        UnionType::from_elements_leave_aliases(
            db,
            program,
            std::iter::once(self.variable()).chain(
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
        program: crate::Program<'db>,
        variable: Option<Type<'db>>,
    ) -> impl Iterator<Item = Type<'db>> + 'a {
        let variable = variable.unwrap_or(self.variable());
        self.iter_prefix_elements().chain(
            self.iter_suffix_elements()
                .take_while(move |element| element.is_equivalent_to(db, program, variable)),
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
        program: crate::Program<'db>,
        variable: Option<Type<'db>>,
    ) -> impl Iterator<Item = Type<'db>> + 'a {
        let variable = variable.unwrap_or(self.variable());
        self.iter_suffix_elements()
            .skip_while(move |element| element.is_equivalent_to(db, program, variable))
    }

    fn resize(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        new_length: TupleLength,
    ) -> Result<Tuple<Type<'db>>, ResizeTupleError> {
        match new_length {
            TupleLength::Fixed(new_length) => {
                // The number of elements that will get their value from our variable-length
                // portion.
                let Some(variable_count) = new_length.checked_sub(self.len().minimum()) else {
                    return Err(ResizeTupleError::TooManyValues);
                };
                Ok(Tuple::Fixed(FixedLengthTuple::from_elements(
                    (self.iter_prefix_elements())
                        .chain(std::iter::repeat_n(self.variable(), variable_count))
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
                    program,
                    self.iter_prefix_elements()
                        .skip(prefix_length)
                        .chain(std::iter::once(self.variable()))
                        .chain(self.iter_suffix_elements().take(suffix_overflow)),
                );
                let prefix = (self.iter_prefix_elements().take(prefix_length))
                    .chain(std::iter::repeat_n(variable, prefix_underflow));
                let suffix = std::iter::repeat_n(variable, suffix_underflow)
                    .chain(self.iter_suffix_elements().skip(suffix_overflow));
                Ok(VariableLengthTuple::mixed(prefix, variable, suffix))
            }
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if nested {
            let prefix = self
                .prefix_elements()
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, program, div, true));

            let variable = self
                .variable()
                .recursive_type_normalized_impl(db, program, div, true)?;

            let suffix = self
                .suffix_elements()
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, program, div, true));

            Self::try_new(prefix, variable, suffix)
        } else {
            let prefix = self.prefix_elements().iter().map(|ty| {
                ty.recursive_type_normalized_impl(db, program, div, true)
                    .unwrap_or(div)
            });

            let variable = self
                .variable()
                .recursive_type_normalized_impl(db, program, div, true)
                .unwrap_or(div);

            let suffix = self.suffix_elements().iter().map(|ty| {
                ty.recursive_type_normalized_impl(db, program, div, true)
                    .unwrap_or(div)
            });

            Some(Self::new(prefix, variable, suffix))
        }
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> TupleSpec<'db> {
        Self::mixed(
            self.prefix_elements()
                .iter()
                .map(|ty| ty.apply_type_mapping_impl(db, program, type_mapping, tcx, visitor)),
            self.variable()
                .apply_type_mapping_impl(db, program, type_mapping, tcx, visitor),
            self.suffix_elements()
                .iter()
                .map(|ty| ty.apply_type_mapping_impl(db, program, type_mapping, tcx, visitor)),
        )
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for ty in self.prefix_elements() {
            ty.find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
        }
        self.variable()
            .find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
        for ty in self.suffix_elements() {
            ty.find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
        }
    }
}

impl<'db> VariableLengthTuple<Type<'db>> {
    #[expect(clippy::unnecessary_wraps)]
    pub(crate) fn py_index(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        index: i32,
    ) -> Result<Type<'db>, OutOfBoundsError> {
        match Nth::from_index(index) {
            Nth::FromStart(index) => {
                Ok(self.type_at_nonnegative_index_unbounded(db, program, index))
            }

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
                    program,
                    (self.prefix_elements().iter().rev().copied())
                        .take(index_past_suffix)
                        .rev()
                        .chain(std::iter::once(self.variable())),
                ))
            }
        }
    }
}

/// A tuple that might be fixed- or variable-length.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub enum Tuple<T> {
    Fixed(FixedLengthTuple<T>),
    Variable(VariableLengthTuple<T>),
}

impl<T> Tuple<T> {
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

    pub(crate) const fn homogeneous(element: T) -> Self {
        Self::Variable(VariableLengthTuple::homogeneous(element))
    }

    pub(crate) fn heterogeneous(elements: impl IntoIterator<Item = T>) -> Self {
        FixedLengthTuple::from_elements(elements).into()
    }

    /// Returns the variable-length element of this tuple, if it has one.
    pub(crate) fn variable_element(&self) -> Option<&T>
    where
        T: Copy,
    {
        match self {
            Tuple::Fixed(_) => None,
            Tuple::Variable(tuple) => Some(tuple.variable_element()),
        }
    }

    /// Returns an iterator of all of the fixed-length element types of this tuple.
    pub(crate) fn fixed_elements(&self) -> impl Iterator<Item = &T> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.all_elements().iter()),
            Tuple::Variable(tuple) => Either::Right(tuple.fixed_elements()),
        }
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// elements, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn all_elements(&self) -> &[T] {
        match self {
            Tuple::Fixed(tuple) => tuple.all_elements(),
            Tuple::Variable(tuple) => tuple.all_elements(),
        }
    }

    pub(crate) fn iter_all_elements(&self) -> impl DoubleEndedIterator<Item = T> + '_
    where
        T: Copy,
    {
        self.all_elements().iter().copied()
    }

    pub(crate) fn into_all_elements_with_kind(self) -> impl Iterator<Item = TupleElement<T>> {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.into_all_elements_with_kind()),
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

impl<'db> Tuple<Type<'db>> {
    pub(crate) fn homogeneous_element_type(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
    ) -> Type<'db> {
        UnionType::from_elements_leave_aliases(db, program, self.all_elements())
    }

    /// Returns the type of a static slice into this tuple.
    ///
    /// Fixed-length tuples produce an exact heterogeneous tuple. Variable-length tuples preserve
    /// exact shape where it is cheap to do so, and otherwise use a sound homogeneous approximation.
    pub(crate) fn py_slice_type(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<Type<'db>, StepSizeZeroError> {
        match self {
            Tuple::Fixed(tuple) => Ok(Type::heterogeneous_tuple(
                db,
                tuple.py_slice(db, start, stop, step)?,
            )),
            Tuple::Variable(tuple) => tuple.py_slice_type(db, program, start, stop, step),
        }
    }

    /// Resizes this tuple to a different length, if possible. If this tuple cannot satisfy the
    /// desired minimum or maximum length, we return an error. If we return an `Ok` result, the
    /// [`len`][Self::len] of the resulting tuple is guaranteed to be equal to `new_length`.
    pub(crate) fn resize(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        new_length: TupleLength,
    ) -> Result<Self, ResizeTupleError> {
        match self {
            Tuple::Fixed(tuple) => tuple.resize(db, program, new_length),
            Tuple::Variable(tuple) => tuple.resize(db, program, new_length),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Tuple::Fixed(tuple) => Some(Tuple::Fixed(
                tuple.recursive_type_normalized_impl(db, program, div, nested)?,
            )),
            Tuple::Variable(tuple) => Some(Tuple::Variable(
                tuple.recursive_type_normalized_impl(db, program, div, nested)?,
            )),
        }
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Tuple::Fixed(tuple) => {
                Tuple::Fixed(tuple.apply_type_mapping_impl(db, program, type_mapping, tcx, visitor))
            }
            Tuple::Variable(tuple) => {
                tuple.apply_type_mapping_impl(db, program, type_mapping, tcx, visitor)
            }
        }
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self {
            Tuple::Fixed(tuple) => {
                tuple.find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
            }
            Tuple::Variable(tuple) => {
                tuple.find_legacy_typevars_impl(db, program, binding_context, typevars, visitor);
            }
        }
    }

    pub(crate) fn is_single_valued(&self, db: &'db dyn Db, program: crate::Program<'db>) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_single_valued(db, program),
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
    pub(crate) fn try_for_each_element_pair<F, E>(&self, other: &Self, mut f: F) -> Result<(), E>
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
                    f(*l, right.variable())?;
                }

                // 3. Right's extra prefix elements with left's variable.
                for r in right
                    .prefix_elements()
                    .iter()
                    .skip(left.prefix_elements().len())
                {
                    f(left.variable(), *r)?;
                }

                // 4. Variable elements with each other.
                f(left.variable(), right.variable())?;

                // 5. Left's extra suffix elements with right's variable.
                for l in left
                    .suffix_elements()
                    .iter()
                    .rev()
                    .skip(right.suffix_elements().len())
                {
                    f(*l, right.variable())?;
                }

                // 6. Right's extra suffix elements with left's variable.
                for r in right
                    .suffix_elements()
                    .iter()
                    .rev()
                    .skip(left.suffix_elements().len())
                {
                    f(left.variable(), *r)?;
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
                    f(left.variable(), *r)?;
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
                    f(*l, right.variable())?;
                }
            }
        }

        Ok(())
    }

    /// Return the `TupleSpec` for the singleton `sys.version_info`
    pub(crate) fn version_info_spec(
        db: &'db dyn Db,
        program: crate::Program<'db>,
        python_version: PythonVersion,
    ) -> TupleSpec<'db> {
        let int_instance_ty = KnownClass::Int.to_instance(db, program);

        // TODO: just grab this type from typeshed (it's a `sys._ReleaseLevel` type alias there)
        let release_level_ty = {
            let elements: Box<[Type<'db>]> = ["alpha", "beta", "candidate", "final"]
                .iter()
                .map(|level| Type::string_literal(db, level))
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

impl<T> From<FixedLengthTuple<T>> for Tuple<T> {
    fn from(tuple: FixedLengthTuple<T>) -> Self {
        Tuple::Fixed(tuple)
    }
}

impl<T> From<VariableLengthTuple<T>> for Tuple<T> {
    fn from(tuple: VariableLengthTuple<T>) -> Self {
        Tuple::Variable(tuple)
    }
}

impl<'db> Tuple<Type<'db>> {
    pub(crate) fn py_index(
        &self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        index: i32,
    ) -> Result<Type<'db>, OutOfBoundsError> {
        match self {
            Tuple::Fixed(tuple) => tuple.py_index(db, index),
            Tuple::Variable(tuple) => tuple.py_index(db, program, index),
        }
    }
}

pub(crate) enum TupleElement<T> {
    Fixed(T),
    Prefix(T),
    Variable(T),
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
    program: crate::Program<'db>,
    targets: Tuple<UnionBuilder<'db>>,
}

impl<'db> TupleUnpacker<'db> {
    pub(crate) fn new(db: &'db dyn Db, program: crate::Program<'db>, len: TupleLength) -> Self {
        let new_builders =
            |len: usize| std::iter::repeat_with(|| UnionBuilder::new(db, program)).take(len);
        let targets = match len {
            TupleLength::Fixed(len) => {
                Tuple::Fixed(FixedLengthTuple::from_elements(new_builders(len)))
            }
            TupleLength::Variable(prefix, suffix) => VariableLengthTuple::mixed(
                new_builders(prefix),
                UnionBuilder::new(db, program),
                new_builders(suffix),
            ),
        };
        Self {
            db,
            program,
            targets,
        }
    }

    /// Unpacks a single rhs tuple into the target tuple that we are building. If you want to
    /// unpack a single type into each target, call this method with a homogeneous tuple.
    ///
    /// The lengths of the targets and the rhs have to be compatible, but not necessarily
    /// identical. The lengths only have to be identical if both sides are fixed-length; if either
    /// side is variable-length, we will pull multiple values out of the rhs variable-length
    /// portion, and assign multiple values to the starred target, as needed.
    pub(crate) fn unpack_tuple(
        &mut self,
        values: &Tuple<Type<'db>>,
    ) -> Result<(), ResizeTupleError> {
        let values = values.resize(self.db, self.program, self.targets.len())?;
        match (&mut self.targets, &values) {
            (Tuple::Fixed(targets), Tuple::Fixed(values)) => {
                targets.unpack_tuple(values);
            }
            (Tuple::Variable(targets), Tuple::Variable(values)) => {
                targets.unpack_tuple(self.db, self.program, values);
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
            .map(move |builder| match builder {
                TupleElement::Variable(builder) => builder.try_build().unwrap_or_else(|| {
                    KnownClass::List.to_specialized_instance(
                        self.db,
                        self.program,
                        &[Type::unknown()],
                    )
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
        program: crate::Program<'db>,
        values: &VariableLengthTuple<Type<'db>>,
    ) {
        // We have already verified above that the two tuples have the same length.
        for (target, value) in
            (self.prefix_elements_mut().iter_mut()).zip(values.iter_prefix_elements())
        {
            target.add_in_place(value);
        }
        self.variable_element_mut()
            .add_in_place(KnownClass::List.to_specialized_instance(
                db,
                program,
                &[values.variable()],
            ));
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
        variable: Type<'db>,
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

    /// Concatenates another tuple to the end of this tuple, returning a new tuple.
    pub(crate) fn concat(
        mut self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        other: &TupleSpec<'db>,
    ) -> Self {
        match (&mut self, other) {
            (TupleSpecBuilder::Fixed(left_tuple), TupleSpec::Fixed(right_tuple)) => {
                left_tuple.extend_from_slice(&right_tuple.0);
                self
            }

            (TupleSpecBuilder::Fixed(left_tuple), TupleSpec::Variable(variable_tuple)) => {
                left_tuple.extend_from_slice(variable_tuple.prefix_elements());
                TupleSpecBuilder::Variable {
                    prefix: std::mem::take(left_tuple),
                    variable: variable_tuple.variable(),
                    suffix: variable_tuple.suffix_elements().to_vec(),
                }
            }

            (
                TupleSpecBuilder::Variable {
                    prefix: _,
                    variable: _,
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
                    variable: left_variable,
                    suffix: left_suffix,
                },
                TupleSpec::Variable(right),
            ) => {
                let variable = UnionType::from_elements_leave_aliases(
                    db,
                    program,
                    left_suffix
                        .iter()
                        .chain([left_variable, &right.variable()])
                        .chain(right.prefix_elements()),
                );
                TupleSpecBuilder::Variable {
                    prefix: std::mem::take(left_prefix),
                    variable,
                    suffix: right.suffix_elements().to_vec(),
                }
            }
        }
    }

    fn all_elements(&self) -> impl Iterator<Item = &Type<'db>> {
        match self {
            TupleSpecBuilder::Fixed(elements) => Either::Left(elements.iter()),
            TupleSpecBuilder::Variable {
                prefix,
                variable,
                suffix,
            } => Either::Right(prefix.iter().chain(std::iter::once(variable)).chain(suffix)),
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
    pub(crate) fn union(
        mut self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        other: &TupleSpec<'db>,
    ) -> Self {
        match (&mut self, other) {
            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Fixed(new_elements))
                if our_elements.len() == new_elements.len() =>
            {
                for (existing, new) in our_elements.iter_mut().zip(new_elements.all_elements()) {
                    *existing =
                        UnionType::from_elements_leave_aliases(db, program, [*existing, *new]);
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
                    program,
                    self.all_elements().chain(other.all_elements()),
                );
                TupleSpecBuilder::Variable {
                    prefix: vec![],
                    variable: unioned,
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
    pub(crate) fn intersect(
        mut self,
        db: &'db dyn Db,
        program: crate::Program<'db>,
        other: &TupleSpec<'db>,
    ) -> Option<Self> {
        match (&mut self, other) {
            // Both fixed-length with the same length: element-wise intersection.
            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Fixed(new_elements))
                if our_elements.len() == new_elements.len() =>
            {
                for (existing, new) in our_elements.iter_mut().zip(new_elements.all_elements()) {
                    *existing = IntersectionType::from_elements(db, program, [*existing, *new]);
                }
                Some(self)
            }

            // Fixed-length tuples with different lengths cannot intersect.
            (TupleSpecBuilder::Fixed(_), TupleSpec::Fixed(_)) => None,

            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Variable(var)) => var
                .resize(db, program, TupleLength::Fixed(our_elements.len()))
                .ok()
                .and_then(|tuple| self.intersect(db, program, &tuple)),

            (TupleSpecBuilder::Variable { .. }, TupleSpec::Fixed(fixed)) => self
                .clone()
                .build()
                .resize(db, program, TupleLength::Fixed(fixed.len()))
                .ok()
                .and_then(|tuple| TupleSpecBuilder::from(&tuple).intersect(db, program, other)),

            (
                TupleSpecBuilder::Variable {
                    prefix,
                    variable,
                    suffix,
                },
                TupleSpec::Variable(var),
            ) => {
                if prefix.len() == var.prefix_elements().len()
                    && suffix.len() == var.suffix_elements().len()
                {
                    for (existing, new) in prefix.iter_mut().zip(var.prefix_elements()) {
                        *existing =
                            IntersectionType::from_two_elements(db, program, *existing, *new);
                    }
                    *variable =
                        IntersectionType::from_two_elements(db, program, *variable, var.variable());
                    for (existing, new) in suffix.iter_mut().zip(var.suffix_elements()) {
                        *existing =
                            IntersectionType::from_two_elements(db, program, *existing, *new);
                    }
                    return Some(self);
                }

                let self_built = self.clone().build();
                let self_len = self_built.len();
                var.resize(db, program, self_len)
                    .ok()
                    .and_then(|resized| self.intersect(db, program, &resized))
                    .or_else(|| {
                        self_built
                            .resize(db, program, var.len())
                            .ok()
                            .and_then(|resized| {
                                TupleSpecBuilder::from(&resized).intersect(db, program, other)
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
                variable,
                suffix,
            } => TupleSpec::Variable(VariableLengthTuple::new_from_vec(prefix, variable, suffix)),
        }
    }
}

impl<'db> From<&TupleSpec<'db>> for TupleSpecBuilder<'db> {
    fn from(tuple: &TupleSpec<'db>) -> Self {
        match tuple {
            TupleSpec::Fixed(fixed) => TupleSpecBuilder::Fixed(fixed.0.to_vec()),
            TupleSpec::Variable(variable) => TupleSpecBuilder::Variable {
                prefix: variable.prefix_elements().to_vec(),
                variable: variable.variable(),
                suffix: variable.suffix_elements().to_vec(),
            },
        }
    }
}

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

use itertools::{Either, EitherOrBoth, Itertools};

use crate::semantic_index::definition::Definition;
use crate::subscript::{Nth, OutOfBoundsError, PyIndex, PySlice, StepSizeZeroError};
use crate::types::builder::RecursivelyDefined;
use crate::types::class::{ClassType, KnownClass};
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::generics::InferableTypeVars;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, FindLegacyTypeVarsVisitor, HasRelationToVisitor,
    IsDisjointVisitor, IsEquivalentVisitor, NormalizedVisitor, Type, TypeMapping, TypeRelation,
    UnionBuilder, UnionType,
};
use crate::types::{Truthiness, TypeContext};
use crate::{Db, FxOrderSet, Program};

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

/// # Ordering
/// Ordering is based on the tuple's salsa-assigned id and not on its elements.
/// The id may change between runs, or when the tuple was garbage collected and recreated.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct TupleType<'db> {
    #[returns(ref)]
    pub(crate) tuple: TupleSpec<'db>,
}

pub(super) fn walk_tuple_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    tuple: TupleType<'db>,
    visitor: &V,
) {
    for element in tuple.tuple(db).all_elements() {
        visitor.visit_type(db, *element);
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
            if tuple.variable.is_never() {
                let tuple = TupleSpec::Fixed(FixedLengthTuple::from_elements(
                    tuple.prefix.iter().chain(&tuple.suffix).copied(),
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

    #[cfg(test)]
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
    // `static-frame` as part of a mypy_primer run! This is because it's called
    // from `NominalInstanceType::class()`, which is a very hot method.
    #[salsa::tracked(cycle_initial=to_class_type_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn to_class_type(self, db: &'db dyn Db) -> ClassType<'db> {
        let tuple_class = KnownClass::Tuple
            .try_to_class_literal(db)
            .expect("Typeshed should always have a `tuple` class in `builtins.pyi`");

        tuple_class.apply_specialization(db, |generic_context| {
            if generic_context.variables(db).len() == 1 {
                let element_type = self.tuple(db).homogeneous_element_type(db);
                generic_context.specialize_tuple(db, element_type, self)
            } else {
                generic_context.default_specialization(db, Some(KnownClass::Tuple))
            }
        })
    }

    /// Return a normalized version of `self`.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Option<Self> {
        TupleType::new(db, &self.tuple(db).normalized_impl(db, visitor))
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

    pub(crate) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        self.tuple(db).has_relation_to_impl(
            db,
            other.tuple(db),
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        )
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        self.tuple(db)
            .is_equivalent_to_impl(db, other.tuple(db), inferable, visitor)
    }

    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.tuple(db).is_single_valued(db)
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
pub(crate) type TupleSpec<'db> = Tuple<Type<'db>>;

/// A fixed-length tuple.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
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

    pub(crate) fn elements(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator + '_ {
        self.0.iter()
    }

    pub(crate) fn all_elements(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
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
                let mut elements = self.elements().copied();
                let prefix = elements.by_ref().take(prefix).collect();
                let variable = UnionType::from_elements(db, elements.by_ref().take(variable));
                let suffix = elements.by_ref().take(suffix).collect();
                Ok(Tuple::Variable(VariableLengthTuple {
                    prefix,
                    variable,
                    suffix,
                }))
            }
        }
    }

    #[must_use]
    fn normalized_impl(&self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::from_elements(self.0.iter().map(|ty| ty.normalized_impl(db, visitor)))
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
            Some(tuple) => {
                Either::Left(tuple.all_elements().map(|tcx| TypeContext::new(Some(*tcx))))
            }
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

    fn has_relation_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Tuple<Type<'db>>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match other {
            Tuple::Fixed(other) => {
                ConstraintSet::from(self.0.len() == other.0.len()).and(db, || {
                    (self.0.iter().zip(&other.0)).when_all(db, |(self_ty, other_ty)| {
                        self_ty.has_relation_to_impl(
                            db,
                            *other_ty,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                })
            }

            Tuple::Variable(other) => {
                // This tuple must have enough elements to match up with the other tuple's prefix
                // and suffix, and each of those elements must pairwise satisfy the relation.
                let mut result = ConstraintSet::from(true);
                let mut self_iter = self.0.iter();
                for other_ty in &other.prefix {
                    let Some(self_ty) = self_iter.next() else {
                        return ConstraintSet::from(false);
                    };
                    let element_constraints = self_ty.has_relation_to_impl(
                        db,
                        *other_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    );
                    if result
                        .intersect(db, element_constraints)
                        .is_never_satisfied(db)
                    {
                        return result;
                    }
                }
                for other_ty in other.suffix.iter().rev() {
                    let Some(self_ty) = self_iter.next_back() else {
                        return ConstraintSet::from(false);
                    };
                    let element_constraints = self_ty.has_relation_to_impl(
                        db,
                        *other_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    );
                    if result
                        .intersect(db, element_constraints)
                        .is_never_satisfied(db)
                    {
                        return result;
                    }
                }

                // In addition, any remaining elements in this tuple must satisfy the
                // variable-length portion of the other tuple.
                result.and(db, || {
                    self_iter.when_all(db, |self_ty| {
                        self_ty.has_relation_to_impl(
                            db,
                            other.variable,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                })
            }
        }
    }

    fn is_equivalent_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        ConstraintSet::from(self.0.len() == other.0.len()).and(db, || {
            (self.0.iter())
                .zip(&other.0)
                .when_all(db, |(self_ty, other_ty)| {
                    self_ty.is_equivalent_to_impl(db, *other_ty, inferable, visitor)
                })
        })
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
/// The tuple can contain a fixed-length heterogeneous prefix and/or suffix. All of the elements of
/// the variable-length portion must be the same.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub struct VariableLengthTuple<T> {
    pub(crate) prefix: Box<[T]>,
    pub(crate) variable: T,
    pub(crate) suffix: Box<[T]>,
}

impl<T> VariableLengthTuple<T> {
    /// Creates a new tuple spec containing zero or more elements of a given type, with no prefix
    /// or suffix.
    fn homogeneous(ty: T) -> Tuple<T> {
        Self::mixed([], ty, [])
    }

    fn mixed(
        prefix: impl IntoIterator<Item = T>,
        variable: T,
        suffix: impl IntoIterator<Item = T>,
    ) -> Tuple<T> {
        Tuple::Variable(Self {
            prefix: prefix.into_iter().collect(),
            variable,
            suffix: suffix.into_iter().collect(),
        })
    }

    pub(crate) fn prefix_elements(
        &self,
    ) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator + '_ {
        self.prefix.iter()
    }

    pub(crate) fn suffix_elements(
        &self,
    ) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator + '_ {
        self.suffix.iter()
    }

    fn fixed_elements(&self) -> impl Iterator<Item = &T> + '_ {
        self.prefix_elements().chain(self.suffix_elements())
    }

    fn all_elements(&self) -> impl Iterator<Item = &T> + '_ {
        (self.prefix_elements())
            .chain(std::iter::once(&self.variable))
            .chain(self.suffix_elements())
    }

    fn into_all_elements_with_kind(self) -> impl Iterator<Item = TupleElement<T>> {
        (self.prefix.into_iter().map(TupleElement::Prefix))
            .chain(std::iter::once(TupleElement::Variable(self.variable)))
            .chain(self.suffix.into_iter().map(TupleElement::Suffix))
    }

    fn len(&self) -> TupleLength {
        TupleLength::Variable(self.prefix.len(), self.suffix.len())
    }
}

impl<'db> VariableLengthTuple<Type<'db>> {
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
        let variable = variable.unwrap_or(self.variable);
        self.prefix_elements()
            .chain(
                self.suffix_elements()
                    .take_while(move |element| element.is_equivalent_to(db, variable)),
            )
            .copied()
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
        let variable = variable.unwrap_or(self.variable);
        self.suffix_elements()
            .skip_while(move |element| element.is_equivalent_to(db, variable))
            .copied()
    }

    fn resize(
        &self,
        db: &'db dyn Db,
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
                    (self.prefix_elements().copied())
                        .chain(std::iter::repeat_n(self.variable, variable_count))
                        .chain(self.suffix_elements().copied()),
                )))
            }

            TupleLength::Variable(prefix_length, suffix_length) => {
                // "Overflow" are elements of our prefix/suffix that will be folded into the
                // result's variable-length portion. "Underflow" are elements of the result
                // prefix/suffix that will come from our variable-length portion.
                let self_prefix_length = self.prefix.len();
                let prefix_underflow = prefix_length.saturating_sub(self_prefix_length);
                let self_suffix_length = self.suffix.len();
                let suffix_overflow = self_suffix_length.saturating_sub(suffix_length);
                let suffix_underflow = suffix_length.saturating_sub(self_suffix_length);
                let prefix = (self.prefix_elements().copied().take(prefix_length))
                    .chain(std::iter::repeat_n(self.variable, prefix_underflow));
                let variable = UnionType::from_elements(
                    db,
                    (self.prefix_elements().copied().skip(prefix_length))
                        .chain(std::iter::once(self.variable))
                        .chain(self.suffix_elements().copied().take(suffix_overflow)),
                );
                let suffix = std::iter::repeat_n(self.variable, suffix_underflow)
                    .chain(self.suffix_elements().copied().skip(suffix_overflow));
                Ok(VariableLengthTuple::mixed(prefix, variable, suffix))
            }
        }
    }

    #[must_use]
    fn normalized_impl(&self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> TupleSpec<'db> {
        let prefix = self
            .prenormalized_prefix_elements(db, None)
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect::<Box<_>>();
        let suffix = self
            .prenormalized_suffix_elements(db, None)
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect::<Box<_>>();
        let variable = self.variable.normalized_impl(db, visitor);
        TupleSpec::Variable(Self {
            prefix,
            variable,
            suffix,
        })
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let prefix = if nested {
            self.prefix
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, true))
                .collect::<Option<Box<_>>>()?
        } else {
            self.prefix
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, div, true)
                        .unwrap_or(div)
                })
                .collect::<Box<_>>()
        };
        let suffix = if nested {
            self.suffix
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, true))
                .collect::<Option<Box<_>>>()?
        } else {
            self.suffix
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, div, true)
                        .unwrap_or(div)
                })
                .collect::<Box<_>>()
        };
        let variable = if nested {
            self.variable
                .recursive_type_normalized_impl(db, div, true)?
        } else {
            self.variable
                .recursive_type_normalized_impl(db, div, true)
                .unwrap_or(div)
        };
        Some(Self {
            prefix,
            variable,
            suffix,
        })
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> TupleSpec<'db> {
        Self::mixed(
            self.prefix
                .iter()
                .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
            self.variable
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            self.suffix
                .iter()
                .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
        )
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for ty in &self.prefix {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
        self.variable
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        for ty in &self.suffix {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }

    fn has_relation_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Tuple<Type<'db>>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match other {
            Tuple::Fixed(other) => {
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
                if !relation.is_assignability() || !self.variable.is_dynamic() {
                    return ConstraintSet::from(false);
                }

                // In addition, the other tuple must have enough elements to match up with this
                // tuple's prefix and suffix, and each of those elements must pairwise satisfy the
                // relation.
                let mut result = ConstraintSet::from(true);
                let mut other_iter = other.elements().copied();
                for self_ty in self.prenormalized_prefix_elements(db, None) {
                    let Some(other_ty) = other_iter.next() else {
                        return ConstraintSet::from(false);
                    };
                    let element_constraints = self_ty.has_relation_to_impl(
                        db,
                        other_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    );
                    if result
                        .intersect(db, element_constraints)
                        .is_never_satisfied(db)
                    {
                        return result;
                    }
                }
                let suffix: Vec<_> = self.prenormalized_suffix_elements(db, None).collect();
                for self_ty in suffix.iter().rev() {
                    let Some(other_ty) = other_iter.next_back() else {
                        return ConstraintSet::from(false);
                    };
                    let element_constraints = self_ty.has_relation_to_impl(
                        db,
                        other_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    );
                    if result
                        .intersect(db, element_constraints)
                        .is_never_satisfied(db)
                    {
                        return result;
                    }
                }

                result
            }

            Tuple::Variable(other) => {
                // When prenormalizing below, we assume that a dynamic variable-length portion of
                // one tuple materializes to the variable-length portion of the other tuple.
                let self_prenormalize_variable = match self.variable {
                    Type::Dynamic(_) => Some(other.variable),
                    _ => None,
                };
                let other_prenormalize_variable = match other.variable {
                    Type::Dynamic(_) => Some(self.variable),
                    _ => None,
                };

                // The overlapping parts of the prefixes and suffixes must satisfy the relation.
                // Any remaining parts must satisfy the relation with the other tuple's
                // variable-length part.
                let mut result = ConstraintSet::from(true);
                let pairwise = (self.prenormalized_prefix_elements(db, self_prenormalize_variable))
                    .zip_longest(
                        other.prenormalized_prefix_elements(db, other_prenormalize_variable),
                    );
                for pair in pairwise {
                    let pair_constraints = match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => self_ty.has_relation_to_impl(
                            db,
                            other_ty,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        ),
                        EitherOrBoth::Left(self_ty) => self_ty.has_relation_to_impl(
                            db,
                            other.variable,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        ),
                        EitherOrBoth::Right(other_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !relation.is_assignability() || !self.variable.is_dynamic() {
                                return ConstraintSet::from(false);
                            }
                            self.variable.has_relation_to_impl(
                                db,
                                other_ty,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        }
                    };
                    if result
                        .intersect(db, pair_constraints)
                        .is_never_satisfied(db)
                    {
                        return result;
                    }
                }

                let self_suffix: Vec<_> = self
                    .prenormalized_suffix_elements(db, self_prenormalize_variable)
                    .collect();
                let other_suffix: Vec<_> = other
                    .prenormalized_suffix_elements(db, other_prenormalize_variable)
                    .collect();
                let pairwise = (self_suffix.iter().rev()).zip_longest(other_suffix.iter().rev());
                for pair in pairwise {
                    let pair_constraints = match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => self_ty.has_relation_to_impl(
                            db,
                            *other_ty,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        ),
                        EitherOrBoth::Left(self_ty) => self_ty.has_relation_to_impl(
                            db,
                            other.variable,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        ),
                        EitherOrBoth::Right(other_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !relation.is_assignability() || !self.variable.is_dynamic() {
                                return ConstraintSet::from(false);
                            }
                            self.variable.has_relation_to_impl(
                                db,
                                *other_ty,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        }
                    };
                    if result
                        .intersect(db, pair_constraints)
                        .is_never_satisfied(db)
                    {
                        return result;
                    }
                }

                // And lastly, the variable-length portions must satisfy the relation.
                result.and(db, || {
                    self.variable.has_relation_to_impl(
                        db,
                        other.variable,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }
        }
    }

    fn is_equivalent_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        self.variable
            .is_equivalent_to_impl(db, other.variable, inferable, visitor)
            .and(db, || {
                (self.prenormalized_prefix_elements(db, None))
                    .zip_longest(other.prenormalized_prefix_elements(db, None))
                    .when_all(db, |pair| match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => {
                            self_ty.is_equivalent_to_impl(db, other_ty, inferable, visitor)
                        }
                        EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                            ConstraintSet::from(false)
                        }
                    })
            })
            .and(db, || {
                (self.prenormalized_suffix_elements(db, None))
                    .zip_longest(other.prenormalized_suffix_elements(db, None))
                    .when_all(db, |pair| match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => {
                            self_ty.is_equivalent_to_impl(db, other_ty, inferable, visitor)
                        }
                        EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                            ConstraintSet::from(false)
                        }
                    })
            })
    }
}

impl<'db> PyIndex<'db> for &VariableLengthTuple<Type<'db>> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match Nth::from_index(index) {
            Nth::FromStart(index) => {
                if let Some(element) = self.prefix.get(index) {
                    // index is small enough that it lands in the prefix of the tuple.
                    return Ok(*element);
                }

                // index is large enough that it lands past the prefix. The tuple can always be
                // large enough that it lands in the variable-length portion. It might also be
                // small enough to land in the suffix.
                let index_past_prefix = index - self.prefix.len() + 1;
                Ok(UnionType::from_elements(
                    db,
                    std::iter::once(self.variable)
                        .chain(self.suffix_elements().copied().take(index_past_prefix)),
                ))
            }

            Nth::FromEnd(index_from_end) => {
                if index_from_end < self.suffix.len() {
                    // index is small enough that it lands in the suffix of the tuple.
                    return Ok(self.suffix[self.suffix.len() - index_from_end - 1]);
                }

                // index is large enough that it lands past the suffix. The tuple can always be
                // large enough that it lands in the variable-length portion. It might also be
                // small enough to land in the prefix.
                let index_past_suffix = index_from_end - self.suffix.len() + 1;
                Ok(UnionType::from_elements(
                    db,
                    (self.prefix_elements().rev().copied())
                        .take(index_past_suffix)
                        .rev()
                        .chain(std::iter::once(self.variable)),
                ))
            }
        }
    }
}

/// A tuple that might be fixed- or variable-length.
///
/// Our tuple representation can hold instances of any Rust type. For tuples containing Python
/// types, use [`TupleSpec`], which defines some additional type-specific methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum Tuple<T> {
    Fixed(FixedLengthTuple<T>),
    Variable(VariableLengthTuple<T>),
}

impl<T> Tuple<T> {
    pub(crate) fn homogeneous(element: T) -> Self {
        VariableLengthTuple::homogeneous(element)
    }

    pub(crate) fn heterogeneous(elements: impl IntoIterator<Item = T>) -> Self {
        FixedLengthTuple::from_elements(elements).into()
    }

    /// Returns the variable-length element of this tuple, if it has one.
    pub(crate) fn variable_element(&self) -> Option<&T> {
        match self {
            Tuple::Fixed(_) => None,
            Tuple::Variable(tuple) => Some(&tuple.variable),
        }
    }

    /// Returns an iterator of all of the fixed-length element types of this tuple.
    pub(crate) fn fixed_elements(&self) -> impl Iterator<Item = &T> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.elements()),
            Tuple::Variable(tuple) => Either::Right(tuple.fixed_elements()),
        }
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// elements, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn all_elements(&self) -> impl Iterator<Item = &T> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.all_elements()),
            Tuple::Variable(tuple) => Either::Right(tuple.all_elements()),
        }
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
    pub(crate) fn homogeneous_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(db, self.all_elements())
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

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.normalized_impl(db, visitor)),
            Tuple::Variable(tuple) => tuple.normalized_impl(db, visitor),
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

    fn has_relation_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match self {
            Tuple::Fixed(self_tuple) => self_tuple.has_relation_to_impl(
                db,
                other,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),
            Tuple::Variable(self_tuple) => self_tuple.has_relation_to_impl(
                db,
                other,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),
        }
    }

    fn is_equivalent_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match (self, other) {
            (Tuple::Fixed(self_tuple), Tuple::Fixed(other_tuple)) => {
                self_tuple.is_equivalent_to_impl(db, other_tuple, inferable, visitor)
            }
            (Tuple::Variable(self_tuple), Tuple::Variable(other_tuple)) => {
                self_tuple.is_equivalent_to_impl(db, other_tuple, inferable, visitor)
            }
            (Tuple::Fixed(_), Tuple::Variable(_)) | (Tuple::Variable(_), Tuple::Fixed(_)) => {
                ConstraintSet::from(false)
            }
        }
    }

    pub(super) fn is_disjoint_from_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // Two tuples with an incompatible number of required elements must always be disjoint.
        let (self_min, self_max) = self.len().size_hint();
        let (other_min, other_max) = other.len().size_hint();
        if self_max.is_some_and(|max| max < other_min) {
            return ConstraintSet::from(true);
        }
        if other_max.is_some_and(|max| max < self_min) {
            return ConstraintSet::from(true);
        }

        // If any of the required elements are pairwise disjoint, the tuples are disjoint as well.
        #[allow(clippy::items_after_statements)]
        fn any_disjoint<'s, 'db>(
            db: &'db dyn Db,
            a: impl IntoIterator<Item = &'s Type<'db>>,
            b: impl IntoIterator<Item = &'s Type<'db>>,
            inferable: InferableTypeVars<'_, 'db>,
            disjointness_visitor: &IsDisjointVisitor<'db>,
            relation_visitor: &HasRelationToVisitor<'db>,
        ) -> ConstraintSet<'db>
        where
            'db: 's,
        {
            (a.into_iter().zip(b)).when_any(db, |(self_element, other_element)| {
                self_element.is_disjoint_from_impl(
                    db,
                    *other_element,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            })
        }

        match (self, other) {
            (Tuple::Fixed(self_tuple), Tuple::Fixed(other_tuple)) => any_disjoint(
                db,
                self_tuple.elements(),
                other_tuple.elements(),
                inferable,
                disjointness_visitor,
                relation_visitor,
            ),

            // Note that we don't compare the variable-length portions; two pure homogeneous tuples
            // `tuple[A, ...]` and `tuple[B, ...]` can never be disjoint even if A and B are
            // disjoint, because `tuple[()]` would be assignable to both.
            (Tuple::Variable(self_tuple), Tuple::Variable(other_tuple)) => any_disjoint(
                db,
                self_tuple.prefix_elements(),
                other_tuple.prefix_elements(),
                inferable,
                disjointness_visitor,
                relation_visitor,
            )
            .or(db, || {
                any_disjoint(
                    db,
                    self_tuple.suffix_elements().rev(),
                    other_tuple.suffix_elements().rev(),
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }),

            (Tuple::Fixed(fixed), Tuple::Variable(variable))
            | (Tuple::Variable(variable), Tuple::Fixed(fixed)) => any_disjoint(
                db,
                fixed.elements(),
                variable.prefix_elements(),
                inferable,
                disjointness_visitor,
                relation_visitor,
            )
            .or(db, || {
                any_disjoint(
                    db,
                    fixed.elements().rev(),
                    variable.suffix_elements().rev(),
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }),
        }
    }

    pub(crate) fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_single_valued(db),
            Tuple::Variable(_) => false,
        }
    }

    /// Return the `TupleSpec` for the singleton `sys.version_info`
    pub(crate) fn version_info_spec(db: &'db dyn Db) -> TupleSpec<'db> {
        let python_version = Program::get(db).python_version(db);
        let int_instance_ty = KnownClass::Int.to_instance(db);

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
            Type::IntLiteral(python_version.major.into()),
            Type::IntLiteral(python_version.minor.into()),
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

impl<'db> PyIndex<'db> for &Tuple<Type<'db>> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match self {
            Tuple::Fixed(tuple) => tuple.py_index(db, index),
            Tuple::Variable(tuple) => tuple.py_index(db, index),
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
    pub(crate) fn unpack_tuple(
        &mut self,
        values: &Tuple<Type<'db>>,
    ) -> Result<(), ResizeTupleError> {
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
                    KnownClass::List.to_specialized_instance(self.db, [Type::unknown()])
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
        for (target, value) in self.0.iter_mut().zip(values.elements().copied()) {
            target.add_in_place(value);
        }
    }
}

impl<'db> VariableLengthTuple<UnionBuilder<'db>> {
    fn unpack_tuple(&mut self, db: &'db dyn Db, values: &VariableLengthTuple<Type<'db>>) {
        // We have already verified above that the two tuples have the same length.
        for (target, value) in (self.prefix.iter_mut()).zip(values.prefix_elements().copied()) {
            target.add_in_place(value);
        }
        self.variable
            .add_in_place(KnownClass::List.to_specialized_instance(db, [values.variable]));
        for (target, value) in (self.suffix.iter_mut()).zip(values.suffix_elements().copied()) {
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
    pub(crate) fn concat(mut self, db: &'db dyn Db, other: &TupleSpec<'db>) -> Self {
        match (&mut self, other) {
            (TupleSpecBuilder::Fixed(left_tuple), TupleSpec::Fixed(right_tuple)) => {
                left_tuple.extend_from_slice(&right_tuple.0);
                self
            }

            (
                TupleSpecBuilder::Fixed(left_tuple),
                TupleSpec::Variable(VariableLengthTuple {
                    prefix,
                    variable,
                    suffix,
                }),
            ) => {
                left_tuple.extend_from_slice(prefix);
                TupleSpecBuilder::Variable {
                    prefix: std::mem::take(left_tuple),
                    variable: *variable,
                    suffix: suffix.to_vec(),
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
                TupleSpec::Variable(VariableLengthTuple {
                    prefix: right_prefix,
                    variable: right_variable,
                    suffix: right_suffix,
                }),
            ) => {
                let variable = UnionType::from_elements(
                    db,
                    left_suffix
                        .iter()
                        .chain([left_variable, right_variable])
                        .chain(right_prefix),
                );
                TupleSpecBuilder::Variable {
                    prefix: std::mem::take(left_prefix),
                    variable,
                    suffix: right_suffix.to_vec(),
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
    pub(crate) fn union(mut self, db: &'db dyn Db, other: &TupleSpec<'db>) -> Self {
        match (&mut self, other) {
            (TupleSpecBuilder::Fixed(our_elements), TupleSpec::Fixed(new_elements))
                if our_elements.len() == new_elements.len() =>
            {
                for (existing, new) in our_elements.iter_mut().zip(new_elements.elements()) {
                    *existing = UnionType::from_elements(db, [*existing, *new]);
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
                let unioned =
                    UnionType::from_elements(db, self.all_elements().chain(other.all_elements()));
                TupleSpecBuilder::Variable {
                    prefix: vec![],
                    variable: unioned,
                    suffix: vec![],
                }
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
            } => TupleSpec::Variable(VariableLengthTuple {
                prefix: prefix.into_boxed_slice(),
                variable,
                suffix: suffix.into_boxed_slice(),
            }),
        }
    }
}

impl<'db> From<&TupleSpec<'db>> for TupleSpecBuilder<'db> {
    fn from(tuple: &TupleSpec<'db>) -> Self {
        match tuple {
            TupleSpec::Fixed(fixed) => TupleSpecBuilder::Fixed(fixed.0.to_vec()),
            TupleSpec::Variable(variable) => TupleSpecBuilder::Variable {
                prefix: variable.prefix.to_vec(),
                variable: variable.variable,
                suffix: variable.suffix.to_vec(),
            },
        }
    }
}

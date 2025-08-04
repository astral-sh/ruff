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

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::hash::Hash;

use itertools::{Either, EitherOrBoth, Itertools};

use crate::types::class::{ClassType, KnownClass};
use crate::types::{SubclassOfType, Truthiness};
use crate::types::{
    Type, TypeMapping, TypeRelation, TypeTransformer, TypeVarInstance, TypeVarVariance,
    UnionBuilder, UnionType, cyclic::PairVisitor,
};
use crate::util::subscript::{Nth, OutOfBoundsError, PyIndex, PySlice, StepSizeZeroError};
use crate::{Db, FxOrderSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TupleLength {
    Fixed(usize),
    Variable(usize, usize),
}

impl TupleLength {
    pub(crate) const fn unknown() -> TupleLength {
        TupleLength::Variable(0, 0)
    }

    pub(crate) fn is_variable(self) -> bool {
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
#[salsa::interned(debug, constructor=new_internal)]
#[derive(PartialOrd, Ord)]
pub struct TupleType<'db> {
    #[returns(ref)]
    pub(crate) tuple: TupleSpec<'db>,
}

pub(super) fn walk_tuple_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    tuple: TupleType<'db>,
    visitor: &mut V,
) {
    for element in tuple.tuple(db).all_elements() {
        visitor.visit_type(db, *element);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TupleType<'_> {}

impl<'db> Type<'db> {
    pub(crate) fn tuple(tuple: Option<TupleType<'db>>) -> Self {
        let Some(tuple) = tuple else {
            return Type::Never;
        };
        Self::Tuple(tuple)
    }

    pub(crate) fn homogeneous_tuple(db: &'db dyn Db, element: Type<'db>) -> Self {
        Type::tuple(TupleType::homogeneous(db, element))
    }

    pub(crate) fn heterogeneous_tuple<I, T>(db: &'db dyn Db, elements: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        Type::tuple(TupleType::from_elements(
            db,
            elements.into_iter().map(Into::into),
        ))
    }

    pub(crate) fn empty_tuple(db: &'db dyn Db) -> Self {
        Type::Tuple(TupleType::empty(db))
    }
}

impl<'db> TupleType<'db> {
    pub(crate) fn new<T>(db: &'db dyn Db, tuple_key: T) -> Option<Self>
    where
        T: Borrow<TupleSpec<'db>> + Hash + salsa::plumbing::interned::Lookup<TupleSpec<'db>>,
        TupleSpec<'db>: salsa::plumbing::interned::HashEqLike<T>,
    {
        // If a fixed-length (i.e., mandatory) element of the tuple is `Never`, then it's not
        // possible to instantiate the tuple as a whole.
        let tuple = tuple_key.borrow();
        if tuple.fixed_elements().any(Type::is_never) {
            return None;
        }

        // If the variable-length portion is Never, it can only be instantiated with zero elements.
        // That means this isn't a variable-length tuple after all!
        if let TupleSpec::Variable(tuple) = tuple {
            if tuple.variable.is_never() {
                let tuple = TupleSpec::Fixed(FixedLengthTuple::from_elements(
                    tuple.prefix.iter().chain(&tuple.suffix).copied(),
                ));
                return Some(TupleType::new_internal::<_, TupleSpec<'db>>(db, tuple));
            }
        }

        Some(TupleType::new_internal(db, tuple_key))
    }

    pub(crate) fn empty(db: &'db dyn Db) -> Self {
        TupleType::new(db, TupleSpec::from(FixedLengthTuple::empty()))
            .expect("TupleType::new() should always return `Some` for an empty `TupleSpec`")
    }

    pub(crate) fn from_elements(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        TupleType::new(db, TupleSpec::from_elements(types))
    }

    #[cfg(test)]
    pub(crate) fn mixed(
        db: &'db dyn Db,
        prefix: impl IntoIterator<Item = Type<'db>>,
        variable: Type<'db>,
        suffix: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        TupleType::new(db, VariableLengthTuple::mixed(prefix, variable, suffix))
    }

    pub(crate) fn homogeneous(db: &'db dyn Db, element: Type<'db>) -> Option<Self> {
        TupleType::new(db, TupleSpec::homogeneous(element))
    }

    pub(crate) fn to_class_type(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        KnownClass::Tuple
            .try_to_class_literal(db)
            .and_then(|class_literal| match class_literal.generic_context(db) {
                None => Some(ClassType::NonGeneric(class_literal)),
                Some(generic_context) if generic_context.variables(db).len() != 1 => None,
                Some(generic_context) => Some(
                    class_literal
                        .apply_specialization(db, |_| generic_context.specialize_tuple(db, self)),
                ),
            })
    }

    pub(crate) fn to_subclass_of(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.to_class_type(db)
            .map(|class| SubclassOfType::from(db, class))
    }

    /// Return a normalized version of `self`.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> Option<Self> {
        TupleType::new(db, self.tuple(db).normalized_impl(db, visitor))
    }

    pub(crate) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Option<Self> {
        TupleType::new(db, self.tuple(db).materialize(db, variance))
    }

    pub(crate) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Option<Self> {
        TupleType::new(db, self.tuple(db).apply_type_mapping(db, type_mapping))
    }

    pub(crate) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        self.tuple(db).find_legacy_typevars(db, typevars);
    }

    pub(crate) fn has_relation_to(
        self,
        db: &'db dyn Db,
        other: Self,
        relation: TypeRelation,
    ) -> bool {
        self.tuple(db)
            .has_relation_to(db, other.tuple(db), relation)
    }

    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.tuple(db).is_equivalent_to(db, other.tuple(db))
    }

    pub(crate) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        visitor: &mut PairVisitor<'db>,
    ) -> bool {
        self.tuple(db)
            .is_disjoint_from_impl(db, other.tuple(db), visitor)
    }

    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.tuple(db).is_single_valued(db)
    }

    pub(crate) fn truthiness(self, db: &'db dyn Db) -> Truthiness {
        self.tuple(db).truthiness()
    }
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FixedLengthTuple<T>(Vec<T>);

impl<T> FixedLengthTuple<T> {
    fn empty() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    fn from_elements(elements: impl IntoIterator<Item = T>) -> Self {
        Self(elements.into_iter().collect())
    }

    pub(crate) fn elements_slice(&self) -> &[T] {
        &self.0
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

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn push(&mut self, element: T) {
        self.0.push(element);
    }
}

impl<'db> FixedLengthTuple<Type<'db>> {
    fn concat(&self, other: &Tuple<Type<'db>>) -> Tuple<Type<'db>> {
        match other {
            TupleSpec::Fixed(other) => TupleSpec::Fixed(FixedLengthTuple::from_elements(
                self.elements().chain(other.elements()).copied(),
            )),

            TupleSpec::Variable(other) => VariableLengthTuple::mixed(
                self.elements().chain(other.prefix_elements()).copied(),
                other.variable,
                other.suffix_elements().copied(),
            ),
        }
    }

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
    fn normalized_impl(&self, db: &'db dyn Db, visitor: &mut TypeTransformer<'db>) -> Self {
        Self::from_elements(self.0.iter().map(|ty| ty.normalized_impl(db, visitor)))
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::from_elements(self.0.iter().map(|ty| ty.materialize(db, variance)))
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self::from_elements(
            self.0
                .iter()
                .map(|ty| ty.apply_type_mapping(db, type_mapping)),
        )
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        for ty in &self.0 {
            ty.find_legacy_typevars(db, typevars);
        }
    }

    fn has_relation_to(
        &self,
        db: &'db dyn Db,
        other: &Tuple<Type<'db>>,
        relation: TypeRelation,
    ) -> bool {
        match other {
            Tuple::Fixed(other) => {
                self.0.len() == other.0.len()
                    && (self.0.iter())
                        .zip(&other.0)
                        .all(|(self_ty, other_ty)| self_ty.has_relation_to(db, *other_ty, relation))
            }

            Tuple::Variable(other) => {
                // This tuple must have enough elements to match up with the other tuple's prefix
                // and suffix, and each of those elements must pairwise satisfy the relation.
                let mut self_iter = self.0.iter();
                for other_ty in &other.prefix {
                    let Some(self_ty) = self_iter.next() else {
                        return false;
                    };
                    if !self_ty.has_relation_to(db, *other_ty, relation) {
                        return false;
                    }
                }
                for other_ty in other.suffix.iter().rev() {
                    let Some(self_ty) = self_iter.next_back() else {
                        return false;
                    };
                    if !self_ty.has_relation_to(db, *other_ty, relation) {
                        return false;
                    }
                }

                // In addition, any remaining elements in this tuple must satisfy the
                // variable-length portion of the other tuple.
                self_iter.all(|self_ty| self_ty.has_relation_to(db, other.variable, relation))
            }
        }
    }

    fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && (self.0.iter())
                .zip(&other.0)
                .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
    }

    fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        self.0.iter().all(|ty| ty.is_single_valued(db))
    }
}

#[allow(unsafe_code)]
unsafe impl<T> salsa::Update for FixedLengthTuple<T>
where
    T: salsa::Update,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        unsafe {
            let old_value = &mut *old_pointer;
            Vec::maybe_update(&raw mut old_value.0, new_value.0)
        }
    }
}

impl<'db> PyIndex<'db> for &FixedLengthTuple<Type<'db>> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        self.0.as_slice().py_index(db, index).copied()
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VariableLengthTuple<T> {
    pub(crate) prefix: Vec<T>,
    pub(crate) variable: T,
    pub(crate) suffix: Vec<T>,
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

    fn prefix_elements(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator + '_ {
        self.prefix.iter()
    }

    fn suffix_elements(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator + '_ {
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

    fn push(&mut self, element: T) {
        self.suffix.push(element);
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

    fn concat(&self, db: &'db dyn Db, other: &Tuple<Type<'db>>) -> Tuple<Type<'db>> {
        match other {
            TupleSpec::Fixed(other) => VariableLengthTuple::mixed(
                self.prefix_elements().copied(),
                self.variable,
                self.suffix_elements().chain(other.elements()).copied(),
            ),

            Tuple::Variable(other) => {
                let variable = UnionType::from_elements(
                    db,
                    (self.suffix_elements().copied())
                        .chain([self.variable, other.variable])
                        .chain(other.prefix_elements().copied()),
                );
                VariableLengthTuple::mixed(
                    self.prefix_elements().copied(),
                    variable,
                    other.suffix_elements().copied(),
                )
            }
        }
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
    fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> TupleSpec<'db> {
        let prefix = self
            .prenormalized_prefix_elements(db, None)
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect::<Vec<_>>();
        let suffix = self
            .prenormalized_suffix_elements(db, None)
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect::<Vec<_>>();
        let variable = self.variable.normalized_impl(db, visitor);
        Self::mixed(prefix, variable, suffix)
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> TupleSpec<'db> {
        Self::mixed(
            self.prefix.iter().map(|ty| ty.materialize(db, variance)),
            self.variable.materialize(db, variance),
            self.suffix.iter().map(|ty| ty.materialize(db, variance)),
        )
    }

    fn apply_type_mapping<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> TupleSpec<'db> {
        Self::mixed(
            self.prefix
                .iter()
                .map(|ty| ty.apply_type_mapping(db, type_mapping)),
            self.variable.apply_type_mapping(db, type_mapping),
            self.suffix
                .iter()
                .map(|ty| ty.apply_type_mapping(db, type_mapping)),
        )
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        for ty in &self.prefix {
            ty.find_legacy_typevars(db, typevars);
        }
        self.variable.find_legacy_typevars(db, typevars);
        for ty in &self.suffix {
            ty.find_legacy_typevars(db, typevars);
        }
    }

    fn has_relation_to(
        &self,
        db: &'db dyn Db,
        other: &Tuple<Type<'db>>,
        relation: TypeRelation,
    ) -> bool {
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
                if relation == TypeRelation::Subtyping || !matches!(self.variable, Type::Dynamic(_))
                {
                    return false;
                }

                // In addition, the other tuple must have enough elements to match up with this
                // tuple's prefix and suffix, and each of those elements must pairwise satisfy the
                // relation.
                let mut other_iter = other.elements().copied();
                for self_ty in self.prenormalized_prefix_elements(db, None) {
                    let Some(other_ty) = other_iter.next() else {
                        return false;
                    };
                    if !self_ty.has_relation_to(db, other_ty, relation) {
                        return false;
                    }
                }
                let suffix: Vec<_> = self.prenormalized_suffix_elements(db, None).collect();
                for self_ty in suffix.iter().rev() {
                    let Some(other_ty) = other_iter.next_back() else {
                        return false;
                    };
                    if !self_ty.has_relation_to(db, other_ty, relation) {
                        return false;
                    }
                }

                true
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
                if !self
                    .prenormalized_prefix_elements(db, self_prenormalize_variable)
                    .zip_longest(
                        other.prenormalized_prefix_elements(db, other_prenormalize_variable),
                    )
                    .all(|pair| match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => {
                            self_ty.has_relation_to(db, other_ty, relation)
                        }
                        EitherOrBoth::Left(self_ty) => {
                            self_ty.has_relation_to(db, other.variable, relation)
                        }
                        EitherOrBoth::Right(_) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide.
                            false
                        }
                    })
                {
                    return false;
                }

                let self_suffix: Vec<_> = self
                    .prenormalized_suffix_elements(db, self_prenormalize_variable)
                    .collect();
                let other_suffix: Vec<_> = other
                    .prenormalized_suffix_elements(db, other_prenormalize_variable)
                    .collect();
                if !(self_suffix.iter().rev())
                    .zip_longest(other_suffix.iter().rev())
                    .all(|pair| match pair {
                        EitherOrBoth::Both(self_ty, other_ty) => {
                            self_ty.has_relation_to(db, *other_ty, relation)
                        }
                        EitherOrBoth::Left(self_ty) => {
                            self_ty.has_relation_to(db, other.variable, relation)
                        }
                        EitherOrBoth::Right(_) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide.
                            false
                        }
                    })
                {
                    return false;
                }

                // And lastly, the variable-length portions must satisfy the relation.
                self.variable.has_relation_to(db, other.variable, relation)
            }
        }
    }

    fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.variable.is_equivalent_to(db, other.variable)
            && (self.prenormalized_prefix_elements(db, None))
                .zip_longest(other.prenormalized_prefix_elements(db, None))
                .all(|pair| match pair {
                    EitherOrBoth::Both(self_ty, other_ty) => self_ty.is_equivalent_to(db, other_ty),
                    EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => false,
                })
            && (self.prenormalized_suffix_elements(db, None))
                .zip_longest(other.prenormalized_suffix_elements(db, None))
                .all(|pair| match pair {
                    EitherOrBoth::Both(self_ty, other_ty) => self_ty.is_equivalent_to(db, other_ty),
                    EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => false,
                })
    }
}

#[allow(unsafe_code)]
unsafe impl<T> salsa::Update for VariableLengthTuple<T>
where
    T: salsa::Update,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_value = unsafe { &mut *old_pointer };
        let mut changed = false;
        changed |= unsafe { Vec::maybe_update(&raw mut old_value.prefix, new_value.prefix) };
        changed |= unsafe { T::maybe_update(&raw mut old_value.variable, new_value.variable) };
        changed |= unsafe { Vec::maybe_update(&raw mut old_value.suffix, new_value.suffix) };
        changed
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Tuple<T> {
    Fixed(FixedLengthTuple<T>),
    Variable(VariableLengthTuple<T>),
}

impl<T> Tuple<T> {
    pub(crate) fn homogeneous(element: T) -> Self {
        VariableLengthTuple::homogeneous(element)
    }

    pub(crate) fn from_elements(elements: impl IntoIterator<Item = T>) -> Self {
        FixedLengthTuple::from_elements(elements).into()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Tuple::Fixed(FixedLengthTuple::with_capacity(capacity))
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

    pub(crate) const fn is_variadic(&self) -> bool {
        matches!(self, Tuple::Variable(_))
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

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_empty(),
            Tuple::Variable(_) => false,
        }
    }

    pub(crate) fn push(&mut self, element: T) {
        match self {
            Tuple::Fixed(tuple) => tuple.push(element),
            Tuple::Variable(tuple) => tuple.push(element),
        }
    }
}

impl<'db> Tuple<Type<'db>> {
    pub(crate) fn homogeneous_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(db, self.all_elements())
    }

    /// Concatenates another tuple to the end of this tuple, returning a new tuple.
    pub(crate) fn concat(&self, db: &'db dyn Db, other: &Self) -> Self {
        match self {
            Tuple::Fixed(tuple) => tuple.concat(other),
            Tuple::Variable(tuple) => tuple.concat(db, other),
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

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.normalized_impl(db, visitor)),
            Tuple::Variable(tuple) => tuple.normalized_impl(db, visitor),
        }
    }

    pub(crate) fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.materialize(db, variance)),
            Tuple::Variable(tuple) => tuple.materialize(db, variance),
        }
    }

    pub(crate) fn apply_type_mapping<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.apply_type_mapping(db, type_mapping)),
            Tuple::Variable(tuple) => tuple.apply_type_mapping(db, type_mapping),
        }
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        match self {
            Tuple::Fixed(tuple) => tuple.find_legacy_typevars(db, typevars),
            Tuple::Variable(tuple) => tuple.find_legacy_typevars(db, typevars),
        }
    }

    fn has_relation_to(&self, db: &'db dyn Db, other: &Self, relation: TypeRelation) -> bool {
        match self {
            Tuple::Fixed(self_tuple) => self_tuple.has_relation_to(db, other, relation),
            Tuple::Variable(self_tuple) => self_tuple.has_relation_to(db, other, relation),
        }
    }

    fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self, other) {
            (Tuple::Fixed(self_tuple), Tuple::Fixed(other_tuple)) => {
                self_tuple.is_equivalent_to(db, other_tuple)
            }
            (Tuple::Variable(self_tuple), Tuple::Variable(other_tuple)) => {
                self_tuple.is_equivalent_to(db, other_tuple)
            }
            (Tuple::Fixed(_), Tuple::Variable(_)) | (Tuple::Variable(_), Tuple::Fixed(_)) => false,
        }
    }

    fn is_disjoint_from_impl(
        &'db self,
        db: &'db dyn Db,
        other: &'db Self,
        visitor: &mut PairVisitor<'db>,
    ) -> bool {
        // Two tuples with an incompatible number of required elements must always be disjoint.
        let (self_min, self_max) = self.len().size_hint();
        let (other_min, other_max) = other.len().size_hint();
        if self_max.is_some_and(|max| max < other_min) {
            return true;
        }
        if other_max.is_some_and(|max| max < self_min) {
            return true;
        }

        // If any of the required elements are pairwise disjoint, the tuples are disjoint as well.
        #[allow(clippy::items_after_statements)]
        fn any_disjoint<'db>(
            db: &'db dyn Db,
            a: impl IntoIterator<Item = &'db Type<'db>>,
            b: impl IntoIterator<Item = &'db Type<'db>>,
            visitor: &mut PairVisitor<'db>,
        ) -> bool {
            a.into_iter().zip(b).any(|(self_element, other_element)| {
                self_element.is_disjoint_from_impl(db, *other_element, visitor)
            })
        }

        match (self, other) {
            (Tuple::Fixed(self_tuple), Tuple::Fixed(other_tuple)) => {
                if any_disjoint(db, self_tuple.elements(), other_tuple.elements(), visitor) {
                    return true;
                }
            }

            (Tuple::Variable(self_tuple), Tuple::Variable(other_tuple)) => {
                if any_disjoint(
                    db,
                    self_tuple.prefix_elements(),
                    other_tuple.prefix_elements(),
                    visitor,
                ) {
                    return true;
                }
                if any_disjoint(
                    db,
                    self_tuple.suffix_elements().rev(),
                    other_tuple.suffix_elements().rev(),
                    visitor,
                ) {
                    return true;
                }
            }

            (Tuple::Fixed(fixed), Tuple::Variable(variable))
            | (Tuple::Variable(variable), Tuple::Fixed(fixed)) => {
                if any_disjoint(db, fixed.elements(), variable.prefix_elements(), visitor) {
                    return true;
                }
                if any_disjoint(
                    db,
                    fixed.elements().rev(),
                    variable.suffix_elements().rev(),
                    visitor,
                ) {
                    return true;
                }
            }
        }

        // Two pure homogeneous tuples `tuple[A, ...]` and `tuple[B, ...]` can never be
        // disjoint even if A and B are disjoint, because `tuple[()]` would be assignable to
        // both.
        false
    }

    fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_single_valued(db),
            Tuple::Variable(_) => false,
        }
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

#[allow(unsafe_code)]
unsafe impl<T> salsa::Update for Tuple<T>
where
    T: salsa::Update,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_value = unsafe { &mut *old_pointer };
        match (old_value, new_value) {
            (Tuple::Fixed(old), Tuple::Fixed(new)) => unsafe {
                FixedLengthTuple::maybe_update(old, new)
            },
            (Tuple::Variable(old), Tuple::Variable(new)) => unsafe {
                VariableLengthTuple::maybe_update(old, new)
            },
            (old_value, new_value) => {
                *old_value = new_value;
                true
            }
        }
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

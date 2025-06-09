//! Defines types for describing fixed- and variable-length tuples.
//!
//! At runtime, a Python tuple is a fixed-length immutable list of values. There is no restriction
//! on the types of the elements of a tuple value. In the type system, we want to model both
//! "heterogeneous" tuples that have elements of a fixed sequence of specific types, and
//! "homogenous" tuples that have an unknown number of elements of the same single type. And in
//! fact, we want to model tuples that are a combination of the two, with a heterogeneous prefix
//! and/or suffix, and a homogeneous portion of unknown length in between those.

use itertools::Either;
use smallvec::{SmallVec, smallvec};

use crate::types::class::KnownClass;
use crate::types::{Type, TypeMapping, TypeRelation, TypeVarInstance, UnionType};
use crate::util::subscript::{OutOfBoundsError, PyIndex, PySlice, StepSizeZeroError};
use crate::{Db, FxOrderSet};

/// # Ordering
/// Ordering is based on the tuple's salsa-assigned id and not on its elements.
/// The id may change between runs, or when the tuple was garbage collected and recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct TupleType<'db> {
    #[returns(ref)]
    pub(crate) tuple: FixedLengthTuple<'db>,
}

impl<'db> Type<'db> {
    pub(crate) fn tuple(db: &'db dyn Db, tuple: FixedLengthTuple<'db>) -> Self {
        Self::Tuple(TupleType::new(db, tuple))
    }
}

impl<'db> TupleType<'db> {
    pub(crate) fn homogeneous_supertype(self, db: &'db dyn Db) -> Type<'db> {
        KnownClass::Tuple.to_specialized_instance(
            db,
            [UnionType::from_elements(db, self.tuple(db).elements())],
        )
    }

    pub(crate) fn empty(db: &'db dyn Db) -> Type<'db> {
        Type::tuple(db, FixedLengthTuple::empty())
    }

    pub(crate) fn from_elements(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = impl Into<Type<'db>>>,
    ) -> Type<'db> {
        let tuple = FixedLengthTuple::from_elements(types);
        if tuple.elements().any(|ty| ty.is_never()) {
            return Type::Never;
        }
        Type::tuple(db, tuple)
    }

    /// Return a normalized version of `self`.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Type<'db> {
        Type::tuple(db, self.tuple(db).normalized(db))
    }

    pub(crate) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Type<'db> {
        Type::tuple(db, self.tuple(db).apply_type_mapping(db, type_mapping))
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

    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.tuple(db).is_gradual_equivalent_to(db, other.tuple(db))
    }

    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Self) -> bool {
        self.tuple(db).is_disjoint_from(db, other.tuple(db))
    }

    pub(crate) fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.tuple(db).is_fully_static(db)
    }

    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.tuple(db).is_single_valued(db)
    }

    pub fn len(self, db: &'db dyn Db) -> usize {
        self.tuple(db).len()
    }
}

/// A fixed-length tuple.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct FixedLengthTuple<'db>(SmallVec<[Type<'db>; 4]>);

impl<'db> FixedLengthTuple<'db> {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(SmallVec::with_capacity(capacity))
    }

    pub(crate) fn singleton(element: Type<'db>) -> Self {
        Self(smallvec![element])
    }

    pub(crate) fn from_elements(elements: impl IntoIterator<Item = impl Into<Type<'db>>>) -> Self {
        Self(elements.into_iter().map(Into::into).collect())
    }

    pub(crate) fn as_slice(&self) -> &[Type<'db>] {
        &self.0
    }

    /// Returns an iterator of all of the element types of this tuple.
    pub(crate) fn elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.0.iter().copied()
    }

    /// Returns the length of this tuple.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Concatenates another tuple to the end of this tuple, returning a new tuple.
    pub(crate) fn concat(&self, other: &Self) -> Self {
        let mut elements = SmallVec::with_capacity(self.0.len() + other.0.len());
        elements.extend_from_slice(&self.0);
        elements.extend_from_slice(&other.0);
        Self(elements)
    }

    /// Adds a sequence of fixed elements to the end of this tuple.
    pub(crate) fn extend(&mut self, elements: impl Iterator<Item = Type<'db>>) {
        self.0.extend(elements);
    }

    pub(crate) fn push(&mut self, element: Type<'db>) {
        self.0.push(element);
    }

    pub(crate) fn extend_from_slice(&mut self, elements: &[Type<'db>]) {
        self.0.extend_from_slice(elements);
    }

    /// Adds a homogeneous, variable-sized element to the end of this tuple.
    pub(crate) fn extend_homogeneous(self, variable: Type<'db>) -> Tuple<'db> {
        VariableLengthTuple {
            prefix: self.0.into_vec(),
            variable,
            suffix: vec![],
        }
        .into()
    }

    #[must_use]
    fn normalized(&self, db: &'db dyn Db) -> Self {
        Self(self.0.iter().map(|ty| ty.normalized(db)).collect())
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self(
            self.0
                .iter()
                .map(|ty| ty.apply_type_mapping(db, type_mapping))
                .collect(),
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

    fn has_relation_to(&self, db: &'db dyn Db, other: &Self, relation: TypeRelation) -> bool {
        self.0.len() == other.0.len()
            && (self.0.iter())
                .zip(&other.0)
                .all(|(self_ty, other_ty)| self_ty.has_relation_to(db, *other_ty, relation))
    }

    fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && (self.0.iter())
                .zip(&other.0)
                .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
    }

    fn is_gradual_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && (self.0.iter())
                .zip(&other.0)
                .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
    }

    fn is_disjoint_from(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.len() != other.0.len()
            || (self.0.iter())
                .zip(&other.0)
                .any(|(self_ty, other_ty)| self_ty.is_disjoint_from(db, *other_ty))
    }

    fn is_fully_static(&self, db: &'db dyn Db) -> bool {
        self.0.iter().all(|ty| ty.is_fully_static(db))
    }

    fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        self.0.iter().all(|ty| ty.is_single_valued(db))
    }
}

impl<'db> PyIndex for &FixedLengthTuple<'db> {
    type Item = Type<'db>;

    fn py_index(self, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        self.0.as_slice().py_index(index).copied()
    }
}

impl<'db> PySlice for FixedLengthTuple<'db> {
    type Item = Type<'db>;

    fn py_slice(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<impl Iterator<Item = &Self::Item>, StepSizeZeroError> {
        self.0.py_slice(start, stop, step)
    }
}

/// A variable-length tuple.
///
/// The tuple can contain a fixed-length heterogeneous prefix and/or suffix. All of the elements of
/// the variable-length portion must be of the same type.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VariableLengthTuple<'db> {
    prefix: Vec<Type<'db>>,
    variable: Type<'db>,
    suffix: Vec<Type<'db>>,
}

impl<'db> VariableLengthTuple<'db> {
    /// Creates a new tuple containing zero or more elements of a given type, with no prefix or
    /// suffix.
    pub(crate) fn homogeneous(ty: Type<'db>) -> Self {
        Self {
            prefix: vec![],
            variable: ty,
            suffix: vec![],
        }
    }

    /// Creates a new tuple containing zero or more elements of a given type, along with an
    /// optional prefix and/or suffix.
    pub(crate) fn new(
        prefix: impl IntoIterator<Item = Type<'db>>,
        variable: Type<'db>,
        suffix: impl IntoIterator<Item = Type<'db>>,
    ) -> Self {
        Self {
            prefix: prefix.into_iter().collect(),
            variable,
            suffix: suffix.into_iter().collect(),
        }
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// tuples, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        (self.prefix.iter().copied())
            .chain(std::iter::once(self.variable))
            .chain(self.suffix.iter().copied())
    }

    /// Returns the minimum length of this tuple.
    pub(crate) fn minimum_length(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    /// Adds a sequence of fixed elements to the end of this tuple.
    pub(crate) fn extend(&mut self, types: impl Iterator<Item = Type<'db>>) {
        self.suffix.extend(types);
    }

    /// Adds a homogeneous, variable-sized element to the end of this tuple. Returns an error if
    /// the tuple already contains a homogeneous element of a different type, or if it contains a
    /// fixed-length suffix after an existing homogeneous element.
    pub(crate) fn extend_homogeneous(
        &mut self,
        variable: Type<'db>,
    ) -> Result<(), TupleError<'db>> {
        if self.variable != variable {
            return Err(TupleError::IncompatibleVariableLengthElements {
                existing: self.variable,
                new: variable,
            });
        }
        if !self.suffix.is_empty() {
            return Err(TupleError::SuffixAfterVariableLengthElement);
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn normalized(&self, db: &'db dyn Db) -> Self {
        Self {
            prefix: self.prefix.iter().map(|ty| ty.normalized(db)).collect(),
            variable: self.variable.normalized(db),
            suffix: self.suffix.iter().map(|ty| ty.normalized(db)).collect(),
        }
    }

    pub(crate) fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.prefix.len() == other.prefix.len()
            && self.suffix.len() == other.suffix.len()
            && (self.prefix.iter())
                .zip(&other.prefix)
                .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
            && self.variable.is_equivalent_to(db, other.variable)
            && (self.suffix.iter())
                .zip(&other.suffix)
                .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
    }

    pub(crate) fn is_gradual_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.prefix.len() == other.prefix.len()
            && self.suffix.len() == other.suffix.len()
            && (self.prefix.iter())
                .zip(&other.prefix)
                .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
            && self.variable.is_gradual_equivalent_to(db, other.variable)
            && (self.suffix.iter())
                .zip(&other.suffix)
                .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
    }
}

/// A tuple that might be fixed- or variable-length.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Tuple<'db> {
    Fixed(FixedLengthTuple<'db>),
    Variable(VariableLengthTuple<'db>),
}

impl<'db> Tuple<'db> {
    pub(crate) fn empty() -> Self {
        FixedLengthTuple::empty().into()
    }

    pub(crate) fn fixed_length(elements: impl IntoIterator<Item = Type<'db>>) -> Self {
        FixedLengthTuple::from_elements(elements).into()
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// tuples, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.elements()),
            Tuple::Variable(tuple) => Either::Right(tuple.elements()),
        }
    }

    /// Returns the minimum and maximum length of this tuple. (The maximum length will be `None`
    /// for a tuple with a variable-length portion.)
    pub(crate) fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Tuple::Fixed(tuple) => {
                let len = tuple.len();
                (len, Some(len))
            }
            Tuple::Variable(tuple) => (tuple.minimum_length(), None),
        }
    }

    /// Adds a sequence of fixed elements to the end of this tuple.
    pub(crate) fn extend(&mut self, types: impl Iterator<Item = Type<'db>>) {
        match self {
            Tuple::Fixed(tuple) => tuple.extend(types),
            Tuple::Variable(tuple) => tuple.extend(types),
        }
    }

    /// Adds a homogeneous, variable-sized element to the end of this tuple. Returns an error if
    /// the tuple already contains a homogeneous element of a different type, or if it contains a
    /// fixed-length suffix after an existing homogeneous element.
    pub(crate) fn extend_homogeneous(
        &mut self,
        variable: Type<'db>,
    ) -> Result<(), TupleError<'db>> {
        match self {
            Tuple::Fixed(tuple) => {
                *self = std::mem::take(tuple).extend_homogeneous(variable);
                Ok(())
            }
            Tuple::Variable(tuple) => tuple.extend_homogeneous(variable),
        }
    }

    #[must_use]
    pub(crate) fn normalized(&self, db: &'db dyn Db) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.normalized(db)),
            Tuple::Variable(tuple) => Tuple::Variable(tuple.normalized(db)),
        }
    }

    pub(crate) fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
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

    pub(crate) fn is_gradual_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self, other) {
            (Tuple::Fixed(self_tuple), Tuple::Fixed(other_tuple)) => {
                self_tuple.is_gradual_equivalent_to(db, other_tuple)
            }
            (Tuple::Variable(self_tuple), Tuple::Variable(other_tuple)) => {
                self_tuple.is_gradual_equivalent_to(db, other_tuple)
            }
            (Tuple::Fixed(_), Tuple::Variable(_)) | (Tuple::Variable(_), Tuple::Fixed(_)) => false,
        }
    }
}

impl<'db> From<FixedLengthTuple<'db>> for Tuple<'db> {
    fn from(tuple: FixedLengthTuple<'db>) -> Self {
        Tuple::Fixed(tuple)
    }
}

impl<'db> From<VariableLengthTuple<'db>> for Tuple<'db> {
    fn from(tuple: VariableLengthTuple<'db>) -> Self {
        Tuple::Variable(tuple)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TupleError<'db> {
    IncompatibleVariableLengthElements { existing: Type<'db>, new: Type<'db> },
    SuffixAfterVariableLengthElement,
}

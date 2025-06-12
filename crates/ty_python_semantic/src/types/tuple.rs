//! Defines types for describing fixed- and variable-length tuples.
//!
//! At runtime, a Python tuple is a fixed-length immutable list of values. There is no restriction
//! on the types of the elements of a tuple value. In the type system, we want to model both
//! "heterogeneous" tuples that have elements of a fixed sequence of specific types, and
//! "homogenous" tuples that have an unknown number of elements of the same single type. And in
//! fact, we want to model tuples that are a combination of the two, with a heterogeneous prefix
//! and/or suffix, and a homogeneous portion of unknown length in between those.

use itertools::Either;
use smallvec::SmallVec;

use crate::types::class::KnownClass;
use crate::types::{Type, TypeMapping, TypeRelation, TypeVarInstance, TypeVarVariance, UnionType};
use crate::util::subscript::{Nth, OutOfBoundsError, PyIndex, PySlice, StepSizeZeroError};
use crate::{Db, FxOrderSet};

/// # Ordering
/// Ordering is based on the tuple's salsa-assigned id and not on its elements.
/// The id may change between runs, or when the tuple was garbage collected and recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct TupleType<'db> {
    #[returns(ref)]
    pub(crate) tuple: Tuple<'db>,
}

impl<'db> Type<'db> {
    pub(crate) fn tuple(db: &'db dyn Db, tuple: impl Into<Tuple<'db>>) -> Self {
        // If a fixed-length (i.e., mandatory) element of the tuple is `Never`, then it's not
        // possible to instantiate the tuple as a whole. (This is not true of the variable-length
        // portion of the tuple, since it can contain no elements.)
        let tuple = tuple.into();
        if tuple.fixed_elements().any(|ty| ty.is_never()) {
            return Type::Never;
        }
        Self::Tuple(TupleType::new(db, tuple))
    }
}

impl<'db> TupleType<'db> {
    pub(crate) fn homogeneous_supertype(self, db: &'db dyn Db) -> Type<'db> {
        self.to_class_type(db)
            .to_instance(db)
            .unwrap_or_else(Type::unknown)
    }

    pub(crate) fn empty(db: &'db dyn Db) -> Type<'db> {
        Type::tuple(db, FixedLengthTuple::empty())
    }

    pub(crate) fn from_elements(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = impl Into<Type<'db>>>,
    ) -> Type<'db> {
        Type::tuple(db, FixedLengthTuple::from_elements(types))
    }

    pub(crate) fn homogeneous(db: &'db dyn Db, element: Type<'db>) -> Type<'db> {
        Type::tuple(db, VariableLengthTuple::homogeneous(element))
    }

    pub(crate) fn to_class_type(self, db: &'db dyn Db) -> Type<'db> {
        KnownClass::Tuple
            .to_specialized_class_type(
                db,
                [UnionType::from_elements(db, self.tuple(db).all_elements())],
            )
            .map(Type::from)
            .unwrap_or_else(Type::unknown)
    }

    /// Return a normalized version of `self`.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Type<'db> {
        Type::tuple(db, self.tuple(db).normalized(db))
    }

    pub(crate) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Type<'db> {
        Type::tuple(db, self.tuple(db).materialize(db, variance))
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

    pub(crate) fn from_elements(elements: impl IntoIterator<Item = impl Into<Type<'db>>>) -> Self {
        Self(elements.into_iter().map(Into::into).collect())
    }

    pub(crate) fn as_slice(&self) -> &[Type<'db>] {
        &self.0
    }

    pub(crate) fn fixed_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.0.iter().copied()
    }

    pub(crate) fn all_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.0.iter().copied()
    }

    /// Returns the length of this tuple.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn concat(&self, other: &Tuple<'db>) -> Tuple<'db> {
        match other {
            Tuple::Fixed(other) => {
                let mut elements = SmallVec::with_capacity(self.0.len() + other.0.len());
                elements.extend_from_slice(&self.0);
                elements.extend_from_slice(&other.0);
                Tuple::Fixed(FixedLengthTuple(elements))
            }

            Tuple::Variable(other) => {
                let mut prefix = Vec::with_capacity(self.0.len() + other.prefix.len());
                prefix.extend_from_slice(&self.0);
                prefix.extend_from_slice(&other.prefix);
                Tuple::Variable(VariableLengthTuple {
                    prefix,
                    variable: other.variable,
                    suffix: other.suffix.clone(),
                })
            }
        }
    }

    pub(crate) fn push(&mut self, element: Type<'db>) {
        self.0.push(element);
    }

    pub(crate) fn extend_from_slice(&mut self, elements: &[Type<'db>]) {
        self.0.extend_from_slice(elements);
    }

    #[must_use]
    fn normalized(&self, db: &'db dyn Db) -> Self {
        Self(self.0.iter().map(|ty| ty.normalized(db)).collect())
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self(
            self.0
                .iter()
                .map(|ty| ty.materialize(db, variance))
                .collect(),
        )
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

    fn has_relation_to(&self, db: &'db dyn Db, other: &Tuple<'db>, relation: TypeRelation) -> bool {
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
pub struct VariableLengthTuple<'db> {
    pub(crate) prefix: Vec<Type<'db>>,
    pub(crate) variable: Type<'db>,
    pub(crate) suffix: Vec<Type<'db>>,
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

    fn fixed_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        (self.prefix.iter().copied()).chain(self.suffix.iter().copied())
    }

    fn all_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        (self.prefix.iter().copied())
            .chain(std::iter::once(self.variable))
            .chain(self.suffix.iter().copied())
    }

    /// Returns the minimum length of this tuple.
    pub(crate) fn minimum_length(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    fn concat(&self, db: &'db dyn Db, other: &Tuple<'db>) -> Tuple<'db> {
        match other {
            Tuple::Fixed(other) => {
                let mut suffix = Vec::with_capacity(self.suffix.len() + other.0.len());
                suffix.extend_from_slice(&self.suffix);
                suffix.extend_from_slice(&other.0);
                Tuple::Variable(VariableLengthTuple {
                    prefix: self.prefix.clone(),
                    variable: self.variable,
                    suffix,
                })
            }

            Tuple::Variable(other) => {
                let variable = UnionType::from_elements(
                    db,
                    (self.suffix.iter().copied())
                        .chain([self.variable, other.variable])
                        .chain(other.prefix.iter().copied()),
                );
                Tuple::Variable(VariableLengthTuple {
                    prefix: self.prefix.clone(),
                    variable,
                    suffix: other.suffix.clone(),
                })
            }
        }
    }

    fn push(&mut self, element: Type<'db>) {
        self.suffix.push(element);
    }

    #[must_use]
    fn normalized(&self, db: &'db dyn Db) -> Self {
        Self {
            prefix: self.prefix.iter().map(|ty| ty.normalized(db)).collect(),
            variable: self.variable.normalized(db),
            suffix: self.suffix.iter().map(|ty| ty.normalized(db)).collect(),
        }
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self {
            prefix: self
                .prefix
                .iter()
                .map(|ty| ty.materialize(db, variance))
                .collect(),
            variable: self.variable.materialize(db, variance),
            suffix: self
                .suffix
                .iter()
                .map(|ty| ty.materialize(db, variance))
                .collect(),
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self {
            prefix: self
                .prefix
                .iter()
                .map(|ty| ty.apply_type_mapping(db, type_mapping))
                .collect(),
            variable: self.variable.apply_type_mapping(db, type_mapping),
            suffix: self
                .suffix
                .iter()
                .map(|ty| ty.apply_type_mapping(db, type_mapping))
                .collect(),
        }
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

    fn has_relation_to(&self, db: &'db dyn Db, other: &Tuple<'db>, relation: TypeRelation) -> bool {
        match other {
            Tuple::Fixed(other) => {
                // The other tuple must have enough elements to match up with this tuple's prefix
                // and suffix, and each of those elements must pairwise satisfy the relation.
                let mut other_iter = other.0.iter();
                for self_ty in &self.prefix {
                    let Some(other_ty) = other_iter.next() else {
                        return false;
                    };
                    if !self_ty.has_relation_to(db, *other_ty, relation) {
                        return false;
                    }
                }
                for self_ty in self.suffix.iter().rev() {
                    let Some(other_ty) = other_iter.next_back() else {
                        return false;
                    };
                    if !self_ty.has_relation_to(db, *other_ty, relation) {
                        return false;
                    }
                }

                // In addition, any remaining elements in the other tuple must satisfy the
                // variable-length portion of this tuple.
                other_iter.all(|other_ty| self.variable.has_relation_to(db, *other_ty, relation))
            }

            Tuple::Variable(other) => {
                // The overlapping parts of the prefixes and suffixes must satisfy the relation.
                let mut self_prefix = self.prefix.iter();
                let mut other_prefix = other.prefix.iter();
                let prefixes_match = (&mut self_prefix)
                    .zip(&mut other_prefix)
                    .all(|(self_ty, other_ty)| self_ty.has_relation_to(db, *other_ty, relation));
                if !prefixes_match {
                    return false;
                }

                let mut self_suffix = self.suffix.iter().rev();
                let mut other_suffix = other.suffix.iter().rev();
                let suffixes_match = (&mut self_suffix)
                    .zip(&mut other_suffix)
                    .all(|(self_ty, other_ty)| self_ty.has_relation_to(db, *other_ty, relation));
                if !suffixes_match {
                    return false;
                }

                // Any remaining parts of either prefix or suffix must satisfy the relation with
                // the other tuple's variable-length portion.
                let prefix_matches_variable = self_prefix
                    .all(|self_ty| self_ty.has_relation_to(db, other.variable, relation));
                if !prefix_matches_variable {
                    return false;
                }
                let prefix_matches_variable = other_prefix
                    .all(|other_ty| self.variable.has_relation_to(db, *other_ty, relation));
                if !prefix_matches_variable {
                    return false;
                }

                let suffix_matches_variable = self_suffix
                    .all(|self_ty| self_ty.has_relation_to(db, other.variable, relation));
                if !suffix_matches_variable {
                    return false;
                }
                let suffix_matches_variable = other_suffix
                    .all(|other_ty| self.variable.has_relation_to(db, *other_ty, relation));
                if !suffix_matches_variable {
                    return false;
                }

                // And lastly, the variable-length portions must satisfy the relation.

                self.variable.has_relation_to(db, other.variable, relation)
            }
        }
    }

    fn is_fully_static(&self, db: &'db dyn Db) -> bool {
        self.variable.is_fully_static(db)
            && self.prefix.iter().all(|ty| ty.is_fully_static(db))
            && self.suffix.iter().all(|ty| ty.is_fully_static(db))
    }
}

impl<'db> PyIndex for &VariableLengthTuple<'db> {
    type Item = Type<'db>;

    fn py_index(self, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match Nth::from_index(index) {
            Nth::FromStart(nth) => Ok(self.prefix.get(nth).copied().unwrap_or(self.variable)),
            Nth::FromEnd(nth_rev) => Ok((self.suffix.len().checked_sub(nth_rev + 1))
                .map(|idx| self.suffix[idx])
                .unwrap_or(self.variable)),
        }
    }
}

/// A tuple that might be fixed- or variable-length.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Tuple<'db> {
    Fixed(FixedLengthTuple<'db>),
    Variable(VariableLengthTuple<'db>),
}

impl<'db> Tuple<'db> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Tuple::Fixed(FixedLengthTuple::with_capacity(capacity))
    }

    /// Returns an iterator of all of the fixed-length element types of this tuple.
    pub(crate) fn fixed_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.fixed_elements()),
            Tuple::Variable(tuple) => Either::Right(tuple.fixed_elements()),
        }
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// elements, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn all_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            Tuple::Fixed(tuple) => Either::Left(tuple.all_elements()),
            Tuple::Variable(tuple) => Either::Right(tuple.all_elements()),
        }
    }

    pub(crate) fn display_minimum_length(&self) -> String {
        match self {
            Tuple::Fixed(tuple) => tuple.len().to_string(),
            Tuple::Variable(tuple) => format!("at least {}", tuple.minimum_length()),
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

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_empty(),
            Tuple::Variable(_) => false,
        }
    }

    /// Concatenates another tuple to the end of this tuple, returning a new tuple.
    pub(crate) fn concat(&self, db: &'db dyn Db, other: &Self) -> Self {
        match self {
            Tuple::Fixed(tuple) => tuple.concat(other),
            Tuple::Variable(tuple) => tuple.concat(db, other),
        }
    }

    pub(crate) fn push(&mut self, element: Type<'db>) {
        match self {
            Tuple::Fixed(tuple) => tuple.push(element),
            Tuple::Variable(tuple) => tuple.push(element),
        }
    }

    fn normalized(&self, db: &'db dyn Db) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.normalized(db)),
            Tuple::Variable(tuple) => Tuple::Variable(tuple.normalized(db)),
        }
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.materialize(db, variance)),
            Tuple::Variable(tuple) => Tuple::Variable(tuple.materialize(db, variance)),
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        match self {
            Tuple::Fixed(tuple) => Tuple::Fixed(tuple.apply_type_mapping(db, type_mapping)),
            Tuple::Variable(tuple) => Tuple::Variable(tuple.apply_type_mapping(db, type_mapping)),
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
        self.has_relation_to(db, other, TypeRelation::Subtyping)
            && other.has_relation_to(db, self, TypeRelation::Subtyping)
    }

    fn is_gradual_equivalent_to(&self, db: &'db dyn Db, other: &Tuple<'db>) -> bool {
        self.has_relation_to(db, other, TypeRelation::Assignability)
            && other.has_relation_to(db, self, TypeRelation::Assignability)
    }

    fn is_disjoint_from(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self, other) {
            (Tuple::Fixed(self_tuple), Tuple::Fixed(other_tuple)) => {
                self_tuple.is_disjoint_from(db, other_tuple)
            }
            // TODO: Consider checking for disjointness between the tuples' prefixes and suffixes.
            (Tuple::Variable(_), Tuple::Variable(_)) => false,
            // TODO: Consider checking for disjointness between the fixed-length tuple and the
            // variable-length tuple's prefix/suffix.
            (Tuple::Fixed(_), Tuple::Variable(_)) | (Tuple::Variable(_), Tuple::Fixed(_)) => false,
        }
    }

    fn is_fully_static(&self, db: &'db dyn Db) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_fully_static(db),
            Tuple::Variable(tuple) => tuple.is_fully_static(db),
        }
    }

    fn is_single_valued(&self, db: &'db dyn Db) -> bool {
        match self {
            Tuple::Fixed(tuple) => tuple.is_single_valued(db),
            Tuple::Variable(_) => false,
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

impl<'db> PyIndex for &Tuple<'db> {
    type Item = Type<'db>;

    fn py_index(self, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match self {
            Tuple::Fixed(tuple) => tuple.py_index(index),
            Tuple::Variable(tuple) => tuple.py_index(index),
        }
    }
}

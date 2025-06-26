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

use itertools::{Either, EitherOrBoth, Itertools};

use crate::types::class::{ClassType, KnownClass};
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
    pub(crate) tuple: TupleSpec<'db>,
}

impl<'db> Type<'db> {
    pub(crate) fn tuple(db: &'db dyn Db, tuple: TupleType<'db>) -> Self {
        // If a fixed-length (i.e., mandatory) element of the tuple is `Never`, then it's not
        // possible to instantiate the tuple as a whole.
        if tuple.tuple(db).fixed_elements().any(|ty| ty.is_never()) {
            return Type::Never;
        }
        Self::Tuple(tuple)
    }
}

impl<'db> TupleType<'db> {
    pub(crate) fn empty(db: &'db dyn Db) -> Type<'db> {
        Type::tuple(
            db,
            TupleType::new(db, TupleSpec::from(FixedLengthTupleSpec::empty())),
        )
    }

    pub(crate) fn from_elements(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Type<'db> {
        Type::tuple(
            db,
            TupleType::new(
                db,
                TupleSpec::from(FixedLengthTupleSpec::from_elements(types)),
            ),
        )
    }

    #[cfg(test)]
    pub(crate) fn mixed(
        db: &'db dyn Db,
        prefix: impl IntoIterator<Item = Type<'db>>,
        variable: Type<'db>,
        suffix: impl IntoIterator<Item = Type<'db>>,
    ) -> Type<'db> {
        Type::tuple(
            db,
            TupleType::new(db, VariableLengthTupleSpec::mixed(prefix, variable, suffix)),
        )
    }

    pub(crate) fn homogeneous(db: &'db dyn Db, element: Type<'db>) -> Type<'db> {
        Type::tuple(db, TupleType::new(db, TupleSpec::homogeneous(element)))
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

    /// Return a normalized version of `self`.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        TupleType::new(db, self.tuple(db).normalized(db))
    }

    pub(crate) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        TupleType::new(db, self.tuple(db).materialize(db, variance))
    }

    pub(crate) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
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

    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Self) -> bool {
        self.tuple(db).is_disjoint_from(db, other.tuple(db))
    }

    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.tuple(db).is_single_valued(db)
    }
}

/// A fixed-length tuple spec.
///
/// Tuple specs are used for more than just `tuple` instances, so they allow `Never` to appear as a
/// fixed-length element type. [`TupleType`] adds that additional invariant (since a tuple that
/// must contain an element that can't be instantiated, can't be instantiated itself).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, salsa::Update)]
pub struct FixedLengthTupleSpec<'db>(Vec<Type<'db>>);

impl<'db> FixedLengthTupleSpec<'db> {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub(crate) fn from_elements(elements: impl IntoIterator<Item = Type<'db>>) -> Self {
        Self(elements.into_iter().collect())
    }

    pub(crate) fn elements_slice(&self) -> &[Type<'db>] {
        &self.0
    }

    pub(crate) fn elements(
        &self,
    ) -> impl DoubleEndedIterator<Item = Type<'db>> + ExactSizeIterator + '_ {
        self.0.iter().copied()
    }

    /// Returns the length of this tuple.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn concat(&self, other: &TupleSpec<'db>) -> TupleSpec<'db> {
        match other {
            TupleSpec::Fixed(other) => TupleSpec::Fixed(FixedLengthTupleSpec::from_elements(
                self.elements().chain(other.elements()),
            )),

            TupleSpec::Variable(other) => VariableLengthTupleSpec::mixed(
                self.elements().chain(other.prefix_elements()),
                other.variable,
                other.suffix_elements(),
            ),
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
        Self::from_elements(self.0.iter().map(|ty| ty.normalized(db)))
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
        other: &TupleSpec<'db>,
        relation: TypeRelation,
    ) -> bool {
        match other {
            TupleSpec::Fixed(other) => {
                self.0.len() == other.0.len()
                    && (self.0.iter())
                        .zip(&other.0)
                        .all(|(self_ty, other_ty)| self_ty.has_relation_to(db, *other_ty, relation))
            }

            TupleSpec::Variable(other) => {
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

impl<'db> PyIndex<'db> for &FixedLengthTupleSpec<'db> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        self.0.as_slice().py_index(db, index).copied()
    }
}

impl<'db> PySlice<'db> for FixedLengthTupleSpec<'db> {
    type Item = Type<'db>;

    fn py_slice(
        &'db self,
        db: &'db dyn Db,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<impl Iterator<Item = &'db Self::Item>, StepSizeZeroError> {
        self.0.py_slice(db, start, stop, step)
    }
}

/// A variable-length tuple spec.
///
/// The tuple spec can contain a fixed-length heterogeneous prefix and/or suffix. All of the
/// elements of the variable-length portion must be of the same type.
///
/// Tuple specs are used for more than just `tuple` instances, so they allow `Never` to appear as a
/// fixed-length element type. [`TupleType`] adds that additional invariant (since a tuple that
/// must contain an element that can't be instantiated, can't be instantiated itself).
#[derive(Clone, Debug, Eq, Hash, PartialEq, salsa::Update)]
pub struct VariableLengthTupleSpec<'db> {
    pub(crate) prefix: Vec<Type<'db>>,
    pub(crate) variable: Type<'db>,
    pub(crate) suffix: Vec<Type<'db>>,
}

impl<'db> VariableLengthTupleSpec<'db> {
    /// Creates a new tuple spec containing zero or more elements of a given type, with no prefix
    /// or suffix.
    fn homogeneous(ty: Type<'db>) -> TupleSpec<'db> {
        Self::mixed([], ty, [])
    }

    fn mixed(
        prefix: impl IntoIterator<Item = Type<'db>>,
        variable: Type<'db>,
        suffix: impl IntoIterator<Item = Type<'db>>,
    ) -> TupleSpec<'db> {
        // If the variable-length portion is Never, it can only be instantiated with zero elements.
        // That means this isn't a variable-length tuple after all!
        if variable.is_never() {
            return TupleSpec::Fixed(FixedLengthTupleSpec::from_elements(
                prefix.into_iter().chain(suffix),
            ));
        }

        TupleSpec::Variable(Self {
            prefix: prefix.into_iter().collect(),
            variable,
            suffix: suffix.into_iter().collect(),
        })
    }

    fn prefix_elements(
        &self,
    ) -> impl DoubleEndedIterator<Item = Type<'db>> + ExactSizeIterator + '_ {
        self.prefix.iter().copied()
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
        let variable = variable.unwrap_or(self.variable);
        self.prefix_elements().chain(
            self.suffix_elements()
                .take_while(move |element| element.is_equivalent_to(db, variable)),
        )
    }

    fn suffix_elements(
        &self,
    ) -> impl DoubleEndedIterator<Item = Type<'db>> + ExactSizeIterator + '_ {
        self.suffix.iter().copied()
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
    }

    fn fixed_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.prefix_elements().chain(self.suffix_elements())
    }

    fn all_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.prefix_elements()
            .chain(std::iter::once(self.variable))
            .chain(self.suffix_elements())
    }

    /// Returns the minimum length of this tuple.
    pub(crate) fn minimum_length(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    fn concat(&self, db: &'db dyn Db, other: &TupleSpec<'db>) -> TupleSpec<'db> {
        match other {
            TupleSpec::Fixed(other) => VariableLengthTupleSpec::mixed(
                self.prefix_elements(),
                self.variable,
                self.suffix_elements().chain(other.elements()),
            ),

            TupleSpec::Variable(other) => {
                let variable = UnionType::from_elements(
                    db,
                    self.suffix_elements()
                        .chain([self.variable, other.variable])
                        .chain(other.prefix_elements()),
                );
                VariableLengthTupleSpec::mixed(
                    self.prefix_elements(),
                    variable,
                    other.suffix_elements(),
                )
            }
        }
    }

    fn push(&mut self, element: Type<'db>) {
        self.suffix.push(element);
    }

    #[must_use]
    fn normalized(&self, db: &'db dyn Db) -> TupleSpec<'db> {
        Self::mixed(
            self.prenormalized_prefix_elements(db, None)
                .map(|ty| ty.normalized(db)),
            self.variable.normalized(db),
            self.prenormalized_suffix_elements(db, None)
                .map(|ty| ty.normalized(db)),
        )
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
        other: &TupleSpec<'db>,
        relation: TypeRelation,
    ) -> bool {
        match other {
            TupleSpec::Fixed(other) => {
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
                let mut other_iter = other.elements();
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

            TupleSpec::Variable(other) => {
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

impl<'db> PyIndex<'db> for &VariableLengthTupleSpec<'db> {
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
                        .chain(self.suffix_elements().take(index_past_prefix)),
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
                    (self.prefix_elements().rev())
                        .take(index_past_suffix)
                        .rev()
                        .chain(std::iter::once(self.variable)),
                ))
            }
        }
    }
}

/// A tuple spec that might be fixed- or variable-length.
///
/// Tuple specs are used for more than just `tuple` instances, so they allow `Never` to appear as a
/// fixed-length element type. [`TupleType`] adds that additional invariant (since a tuple that
/// must contain an element that can't be instantiated, can't be instantiated itself).
#[derive(Clone, Debug, Eq, Hash, PartialEq, salsa::Update)]
pub enum TupleSpec<'db> {
    Fixed(FixedLengthTupleSpec<'db>),
    Variable(VariableLengthTupleSpec<'db>),
}

impl<'db> TupleSpec<'db> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        TupleSpec::Fixed(FixedLengthTupleSpec::with_capacity(capacity))
    }

    pub(crate) fn homogeneous(element: Type<'db>) -> Self {
        VariableLengthTupleSpec::homogeneous(element)
    }

    /// Returns an iterator of all of the fixed-length element types of this tuple.
    pub(crate) fn fixed_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            TupleSpec::Fixed(tuple) => Either::Left(tuple.elements()),
            TupleSpec::Variable(tuple) => Either::Right(tuple.fixed_elements()),
        }
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// elements, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn all_elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            TupleSpec::Fixed(tuple) => Either::Left(tuple.elements()),
            TupleSpec::Variable(tuple) => Either::Right(tuple.all_elements()),
        }
    }

    pub(crate) fn display_minimum_length(&self) -> String {
        match self {
            TupleSpec::Fixed(tuple) => tuple.len().to_string(),
            TupleSpec::Variable(tuple) => format!("at least {}", tuple.minimum_length()),
        }
    }

    pub(crate) const fn is_variadic(&self) -> bool {
        matches!(self, TupleSpec::Variable(_))
    }

    /// Returns the minimum and maximum length of this tuple. (The maximum length will be `None`
    /// for a tuple with a variable-length portion.)
    pub(crate) fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            TupleSpec::Fixed(tuple) => {
                let len = tuple.len();
                (len, Some(len))
            }
            TupleSpec::Variable(tuple) => (tuple.minimum_length(), None),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            TupleSpec::Fixed(tuple) => tuple.is_empty(),
            TupleSpec::Variable(_) => false,
        }
    }

    /// Concatenates another tuple to the end of this tuple, returning a new tuple.
    pub(crate) fn concat(&self, db: &'db dyn Db, other: &Self) -> Self {
        match self {
            TupleSpec::Fixed(tuple) => tuple.concat(other),
            TupleSpec::Variable(tuple) => tuple.concat(db, other),
        }
    }

    pub(crate) fn push(&mut self, element: Type<'db>) {
        match self {
            TupleSpec::Fixed(tuple) => tuple.push(element),
            TupleSpec::Variable(tuple) => tuple.push(element),
        }
    }

    fn normalized(&self, db: &'db dyn Db) -> Self {
        match self {
            TupleSpec::Fixed(tuple) => TupleSpec::Fixed(tuple.normalized(db)),
            TupleSpec::Variable(tuple) => tuple.normalized(db),
        }
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self {
            TupleSpec::Fixed(tuple) => TupleSpec::Fixed(tuple.materialize(db, variance)),
            TupleSpec::Variable(tuple) => tuple.materialize(db, variance),
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        match self {
            TupleSpec::Fixed(tuple) => TupleSpec::Fixed(tuple.apply_type_mapping(db, type_mapping)),
            TupleSpec::Variable(tuple) => tuple.apply_type_mapping(db, type_mapping),
        }
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        match self {
            TupleSpec::Fixed(tuple) => tuple.find_legacy_typevars(db, typevars),
            TupleSpec::Variable(tuple) => tuple.find_legacy_typevars(db, typevars),
        }
    }

    fn has_relation_to(&self, db: &'db dyn Db, other: &Self, relation: TypeRelation) -> bool {
        match self {
            TupleSpec::Fixed(self_tuple) => self_tuple.has_relation_to(db, other, relation),
            TupleSpec::Variable(self_tuple) => self_tuple.has_relation_to(db, other, relation),
        }
    }

    fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self, other) {
            (TupleSpec::Fixed(self_tuple), TupleSpec::Fixed(other_tuple)) => {
                self_tuple.is_equivalent_to(db, other_tuple)
            }
            (TupleSpec::Variable(self_tuple), TupleSpec::Variable(other_tuple)) => {
                self_tuple.is_equivalent_to(db, other_tuple)
            }
            (TupleSpec::Fixed(_), TupleSpec::Variable(_))
            | (TupleSpec::Variable(_), TupleSpec::Fixed(_)) => false,
        }
    }

    fn is_disjoint_from(&self, db: &'db dyn Db, other: &Self) -> bool {
        // Two tuples with an incompatible number of required elements must always be disjoint.
        let (self_min, self_max) = self.size_hint();
        let (other_min, other_max) = other.size_hint();
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
            a: impl IntoIterator<Item = Type<'db>>,
            b: impl IntoIterator<Item = Type<'db>>,
        ) -> bool {
            a.into_iter().zip(b).any(|(self_element, other_element)| {
                self_element.is_disjoint_from(db, other_element)
            })
        }

        match (self, other) {
            (TupleSpec::Fixed(self_tuple), TupleSpec::Fixed(other_tuple)) => {
                if any_disjoint(db, self_tuple.elements(), other_tuple.elements()) {
                    return true;
                }
            }

            (TupleSpec::Variable(self_tuple), TupleSpec::Variable(other_tuple)) => {
                if any_disjoint(
                    db,
                    self_tuple.prefix_elements(),
                    other_tuple.prefix_elements(),
                ) {
                    return true;
                }
                if any_disjoint(
                    db,
                    self_tuple.suffix_elements().rev(),
                    other_tuple.suffix_elements().rev(),
                ) {
                    return true;
                }
            }

            (TupleSpec::Fixed(fixed), TupleSpec::Variable(variable))
            | (TupleSpec::Variable(variable), TupleSpec::Fixed(fixed)) => {
                if any_disjoint(db, fixed.elements(), variable.prefix_elements()) {
                    return true;
                }
                if any_disjoint(db, fixed.elements().rev(), variable.suffix_elements().rev()) {
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
            TupleSpec::Fixed(tuple) => tuple.is_single_valued(db),
            TupleSpec::Variable(_) => false,
        }
    }
}

impl<'db> From<FixedLengthTupleSpec<'db>> for TupleSpec<'db> {
    fn from(tuple: FixedLengthTupleSpec<'db>) -> Self {
        TupleSpec::Fixed(tuple)
    }
}

impl<'db> From<VariableLengthTupleSpec<'db>> for TupleSpec<'db> {
    fn from(tuple: VariableLengthTupleSpec<'db>) -> Self {
        TupleSpec::Variable(tuple)
    }
}

impl<'db> PyIndex<'db> for &TupleSpec<'db> {
    type Item = Type<'db>;

    fn py_index(self, db: &'db dyn Db, index: i32) -> Result<Self::Item, OutOfBoundsError> {
        match self {
            TupleSpec::Fixed(tuple) => tuple.py_index(db, index),
            TupleSpec::Variable(tuple) => tuple.py_index(db, index),
        }
    }
}

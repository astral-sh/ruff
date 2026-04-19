use itertools::Either;

use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, PublicTypePolicy, TypeOrigin,
};
use crate::types::class::KnownClass;
use crate::types::{Type, TypeQualifiers};
use crate::types::{TypeVarBoundOrConstraints, visitor};
use crate::{Db, FxOrderSet};

pub(crate) mod builder;

pub(crate) use builder::{IntersectionBuilder, UnionBuilder};

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct UnionType<'db> {
    /// The union type includes values in any of these types.
    #[returns(deref)]
    pub elements: Box<[Type<'db>]>,
    /// Whether the value pointed to by this type is recursively defined.
    /// If `Yes`, union literal widening is performed early.
    pub(crate) recursively_defined: RecursivelyDefined,
}

pub(crate) fn walk_union<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    union: UnionType<'db>,
    visitor: &V,
) {
    for element in union.elements(db) {
        visitor.visit_type(db, *element);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for UnionType<'_> {}

#[salsa::tracked]
impl<'db> UnionType<'db> {
    /// Create a union from a list of elements
    /// (which may be eagerly simplified into a different variant of [`Type`] altogether).
    ///
    /// For performance reasons, consider using [`UnionType::from_two_elements`] if
    /// the union is constructed from exactly two elements.
    pub fn from_elements<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut iter_elements = elements.into_iter();

        if let Some(first) = iter_elements.next() {
            if let Some(second) = iter_elements.next() {
                let builder = UnionBuilder::new(db).add(first.into()).add(second.into());
                iter_elements
                    .fold(builder, |builder, element| builder.add(element.into()))
                    .build()
            } else {
                first.into()
            }
        } else {
            Type::Never
        }
    }

    /// Create a union type `A | B` from two elements `A` and `B`.
    #[salsa::tracked(
        cycle_initial=|_, id, _, _| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, result: Type<'db>, _, _| {
            result.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub fn from_two_elements(db: &'db dyn Db, a: Type<'db>, b: Type<'db>) -> Type<'db> {
        UnionBuilder::new(db).add(a).add(b).build()
    }

    /// Create a union from a list of elements without unpacking type aliases.
    pub(crate) fn from_elements_leave_aliases<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(
                UnionBuilder::new(db).unpack_aliases(false),
                |builder, element| builder.add(element.into()),
            )
            .build()
    }

    pub(crate) fn from_elements_cycle_recovery<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(
                UnionBuilder::new(db).cycle_recovery(true),
                |builder, element| builder.add(element.into()),
            )
            .build()
    }

    /// A fallible version of [`UnionType::from_elements`].
    ///
    /// If all items in `elements` are `Some()`, the result of unioning all elements is returned.
    /// As soon as a `None` element in the iterable is encountered,
    /// the function short-circuits and returns `None`.
    pub(crate) fn try_from_elements<I, T>(db: &'db dyn Db, elements: I) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = Option<T>>,
        T: Into<Type<'db>>,
    {
        let mut builder = UnionBuilder::new(db);
        for element in elements {
            builder = builder.add(element?.into());
        }
        Some(builder.build())
    }

    /// Apply a transformation function to all elements of the union,
    /// and create a new union from the resulting set of types.
    pub(crate) fn map(
        self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.elements(db)
            .iter()
            .map(transform_fn)
            .fold(UnionBuilder::new(db), |builder, element| {
                builder.add(element)
            })
            .recursively_defined(self.recursively_defined(db))
            .build()
    }

    /// A version of [`UnionType::map`] that does not unpack type aliases.
    pub(crate) fn map_leave_aliases(
        self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.elements(db)
            .iter()
            .map(transform_fn)
            .fold(
                UnionBuilder::new(db).unpack_aliases(false),
                UnionBuilder::add,
            )
            .recursively_defined(self.recursively_defined(db))
            .build()
    }

    /// A fallible version of [`UnionType::map`].
    ///
    /// For each element in `self`, `transform_fn` is called on that element.
    /// If `transform_fn` returns `Some()` for all elements in `self`,
    /// the result of unioning all transformed elements is returned.
    /// As soon as `transform_fn` returns `None` for an element, however,
    /// the function short-circuits and returns `None`.
    pub(crate) fn try_map(
        self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db);
        for element in self.elements(db).iter().map(transform_fn) {
            builder = builder.add(element?);
        }
        builder = builder.recursively_defined(self.recursively_defined(db));
        Some(builder.build())
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.try_map(db, |element| element.to_instance(db))
    }

    pub(crate) fn filter(self, db: &'db dyn Db, f: impl FnMut(&Type<'db>) -> bool) -> Type<'db> {
        let current = self.elements(db);
        let new: Box<[Type<'db>]> = current.iter().copied().filter(f).collect();
        match &*new {
            [] => Type::Never,
            [single] => *single,
            _ if new.len() == current.len() => Type::Union(self),
            _ => Type::Union(UnionType::new(db, new, self.recursively_defined(db))),
        }
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = UnionBuilder::new(db);

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.elements(db) {
            let ty_member = transform_fn(ty);
            match ty_member {
                Place::Undefined => {
                    possibly_unbound = true;
                }
                Place::Defined(DefinedPlace {
                    ty: ty_member,
                    origin: member_origin,
                    definedness: member_boundness,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Undefined
        } else {
            Place::Defined(DefinedPlace {
                ty: builder
                    .recursively_defined(self.recursively_defined(db))
                    .build(),
                origin,
                definedness: if possibly_unbound {
                    Definedness::PossiblyUndefined
                } else {
                    Definedness::AlwaysDefined
                },
                public_type_policy: PublicTypePolicy::Raw,
            })
        }
    }

    pub(crate) fn map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = UnionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.elements(db) {
            let PlaceAndQualifiers {
                place: ty_member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match ty_member {
                Place::Undefined => {
                    possibly_unbound = true;
                }
                Place::Defined(DefinedPlace {
                    ty: ty_member,
                    origin: member_origin,
                    definedness: member_boundness,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }
        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(DefinedPlace {
                    ty: builder
                        .recursively_defined(self.recursively_defined(db))
                        .build(),
                    origin,
                    definedness: if possibly_unbound {
                        Definedness::PossiblyUndefined
                    } else {
                        Definedness::AlwaysDefined
                    },
                    public_type_policy: PublicTypePolicy::Raw,
                })
            },
            qualifiers,
        }
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db)
            .unpack_aliases(false)
            .cycle_recovery(true)
            .recursively_defined(self.recursively_defined(db));
        let mut empty = true;
        for ty in self.elements(db) {
            if nested {
                // list[T | Divergent] => list[Divergent]
                let ty = ty.recursive_type_normalized_impl(db, div, nested)?;
                if ty.same_divergent_marker(div) {
                    return Some(ty);
                }
                builder = builder.add(ty);
                empty = false;
            } else {
                // `Divergent` in a union type does not mean true divergence, so we skip it if not nested.
                // e.g. T | Divergent == T | (T | (T | (T | ...))) == T
                if (*ty).same_divergent_marker(div) {
                    builder = builder.recursively_defined(RecursivelyDefined::Yes);
                    continue;
                }
                builder = builder.add(
                    ty.recursive_type_normalized_impl(db, div, nested)
                        .unwrap_or(div),
                );
                empty = false;
            }
        }
        if empty {
            builder = builder.add(div);
        }
        Some(builder.build())
    }

    /// Identify some specific unions of known classes, currently the ones that `float` and
    /// `complex` expand into in type position.
    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownUnion> {
        let mut has_int = false;
        let mut has_float = false;
        let mut has_complex = false;
        for element in self.elements(db) {
            match element.as_nominal_instance()?.known_class(db)? {
                KnownClass::Int => has_int = true,
                KnownClass::Float => has_float = true,
                KnownClass::Complex => has_complex = true,
                _ => return None,
            }
        }
        match (has_int, has_float, has_complex) {
            (true, true, false) => Some(KnownUnion::Float),
            (true, true, true) => Some(KnownUnion::Complex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KnownUnion {
    Float,   // `int | float`
    Complex, // `int | float | complex`
}

impl KnownUnion {
    pub(crate) fn to_type(self, db: &dyn Db) -> Type<'_> {
        match self {
            KnownUnion::Float => UnionType::from_two_elements(
                db,
                KnownClass::Int.to_instance(db),
                KnownClass::Float.to_instance(db),
            ),
            KnownUnion::Complex => UnionType::from_elements(
                db,
                [
                    KnownClass::Int.to_instance(db),
                    KnownClass::Float.to_instance(db),
                    KnownClass::Complex.to_instance(db),
                ],
            ),
        }
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct IntersectionType<'db> {
    /// The intersection type includes only values in all of these types.
    #[returns(ref)]
    pub(crate) positive: FxOrderSet<Type<'db>>,

    /// The intersection type does not include any value in any of these types.
    ///
    /// Negation types aren't expressible in annotations, and are most likely to arise from type
    /// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
    /// directly in intersections rather than as a separate type.
    #[returns(ref)]
    pub(crate) negative: NegativeIntersectionElements<'db>,
}

/// To avoid unnecessary allocations for the common case of 1 negative elements,
/// we use this enum to represent the negative elements of an intersection type.
///
/// It should otherwise have identical behavior to `FxOrderSet<Type<'db>>`.
///
/// Note that we do not try to maintain the invariant that length-0 collections
/// are always represented using `Self::Empty`, and that length-1 collections
/// are always represented using `Self::Single`: `Self::Multiple` is permitted
/// to have 0-1 elements in its wrapped data, and this could happen if you called
/// `Self::swap_remove` or `Self::swap_remove_index` on an instance that is
/// already the `Self::Multiple` variant. Maintaining the invariant that
/// 0-length or 1-length collections are always represented using `Self::Empty`
/// and `Self::Single` would add overhead to methods like `Self::swap_remove`,
/// and would have little value. At the point when you're calling that method, a
/// heap allocation has already taken place.
#[derive(Debug, Clone, get_size2::GetSize, salsa::Update, Default)]
pub enum NegativeIntersectionElements<'db> {
    #[default]
    Empty,
    Single(Type<'db>),
    Multiple(FxOrderSet<Type<'db>>),
}

impl<'db> NegativeIntersectionElements<'db> {
    pub(crate) fn iter(&self) -> NegativeIntersectionElementsIterator<'_, 'db> {
        match self {
            Self::Empty => NegativeIntersectionElementsIterator::EmptyOrOne(None),
            Self::Single(ty) => NegativeIntersectionElementsIterator::EmptyOrOne(Some(ty)),
            Self::Multiple(set) => NegativeIntersectionElementsIterator::Multiple(set.iter()),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Single(_) => 1,
            Self::Multiple(set) => set.len(),
        }
    }

    pub(crate) fn contains(&self, ty: &Type<'db>) -> bool {
        match self {
            Self::Empty => false,
            Self::Single(existing) => existing == ty,
            Self::Multiple(set) => set.contains(ty),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        // See struct-level comment: we don't try to maintain the invariant that empty
        // collections are representend as `Self::Empty`
        self.len() == 0
    }

    /// Insert the type into the collection.
    ///
    /// Returns `true` if the elements was newly added.
    /// Returns `false` if the element was already present in the collection.
    pub(crate) fn insert(&mut self, ty: Type<'db>) -> bool {
        match self {
            Self::Empty => {
                *self = Self::Single(ty);
                true
            }
            Self::Single(existing) => {
                if ty != *existing {
                    *self = Self::Multiple(FxOrderSet::from_iter([*existing, ty]));
                    true
                } else {
                    false
                }
            }
            Self::Multiple(set) => set.insert(ty),
        }
    }

    /// Shrink the capacity of the collection as much as possible.
    pub(crate) fn shrink_to_fit(&mut self) {
        match self {
            Self::Empty | Self::Single(_) => {}
            Self::Multiple(set) => set.shrink_to_fit(),
        }
    }

    /// Remove `ty` from the collection.
    ///
    /// Returns `true` if `ty` was previously in the collection and has now been removed.
    /// Returns `false` if `ty` was never present in the collection.
    ///
    /// If `ty` was previously present in the collection,
    /// the last element in the collection is popped off the end of the collection
    /// and placed at the index where `ty` was previously, allowing this method to complete
    /// in O(1) time (average).
    pub(crate) fn swap_remove(&mut self, ty: &Type<'db>) -> bool {
        match self {
            Self::Empty => false,
            Self::Single(existing) => {
                if existing == ty {
                    *self = Self::Empty;
                    true
                } else {
                    false
                }
            }
            // See struct-level comment: we don't try to maintain the invariant that collections
            // with size 0 or 1 are represented as `Empty` or `Single`.
            Self::Multiple(set) => set.swap_remove(ty),
        }
    }

    /// Remove the element at `index` from the collection.
    ///
    /// The element is removed by swapping it with the last element
    /// of the collection and popping it off, allowing this method to complete
    /// in O(1) time (average).
    pub(crate) fn swap_remove_index(&mut self, index: usize) -> Option<Type<'db>> {
        match self {
            Self::Empty => None,
            Self::Single(existing) => {
                if index == 0 {
                    let ty = *existing;
                    *self = Self::Empty;
                    Some(ty)
                } else {
                    None
                }
            }
            // See struct-level comment: we don't try to maintain the invariant that collections
            // with size 0 or 1 are represented as `Empty` or `Single`.
            Self::Multiple(set) => set.swap_remove_index(index),
        }
    }

    /// Apply a transformation to all elements in this collection,
    /// and return a new collection of the transformed elements.
    fn map(&self, map_fn: impl Fn(&Type<'db>) -> Type<'db>) -> Self {
        match self {
            NegativeIntersectionElements::Empty => NegativeIntersectionElements::Empty,
            NegativeIntersectionElements::Single(ty) => {
                NegativeIntersectionElements::Single(map_fn(ty))
            }
            NegativeIntersectionElements::Multiple(set) => {
                NegativeIntersectionElements::Multiple(set.iter().map(map_fn).collect())
            }
        }
    }

    /// Apply a fallible transformation to all elements in this collection,
    /// and return a new collection of the transformed elements.
    ///
    /// Returns `None` if `map_fn` fails for any element in the collection.
    fn try_map(&self, map_fn: impl Fn(&Type<'db>) -> Option<Type<'db>>) -> Option<Self> {
        match self {
            NegativeIntersectionElements::Empty => Some(NegativeIntersectionElements::Empty),
            NegativeIntersectionElements::Single(ty) => {
                map_fn(ty).map(NegativeIntersectionElements::Single)
            }
            NegativeIntersectionElements::Multiple(set) => {
                Some(NegativeIntersectionElements::Multiple(
                    set.iter().map(map_fn).collect::<Option<_>>()?,
                ))
            }
        }
    }
}

impl<'a, 'db> IntoIterator for &'a NegativeIntersectionElements<'db> {
    type Item = &'a Type<'db>;
    type IntoIter = NegativeIntersectionElementsIterator<'a, 'db>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl PartialEq for NegativeIntersectionElements<'_> {
    fn eq(&self, other: &Self) -> bool {
        // Same implementation as `OrderSet::eq`
        self.len() == other.len() && self.iter().eq(other)
    }
}

impl Eq for NegativeIntersectionElements<'_> {}

impl std::hash::Hash for NegativeIntersectionElements<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Same implementation as `OrderSet::hash`
        self.len().hash(state);
        for value in self {
            value.hash(state);
        }
    }
}

#[derive(Debug)]
pub enum NegativeIntersectionElementsIterator<'a, 'db> {
    EmptyOrOne(Option<&'a Type<'db>>),
    Multiple(ordermap::set::Iter<'a, Type<'db>>),
}

impl<'a, 'db> Iterator for NegativeIntersectionElementsIterator<'a, 'db> {
    type Item = &'a Type<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            NegativeIntersectionElementsIterator::EmptyOrOne(opt) => opt.take(),
            NegativeIntersectionElementsIterator::Multiple(iter) => iter.next(),
        }
    }
}

impl std::iter::FusedIterator for NegativeIntersectionElementsIterator<'_, '_> {}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for IntersectionType<'_> {}

pub(crate) fn walk_intersection_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    intersection: IntersectionType<'db>,
    visitor: &V,
) {
    for element in intersection.positive(db) {
        visitor.visit_type(db, *element);
    }
    for element in intersection.negative(db) {
        visitor.visit_type(db, *element);
    }
}

#[salsa::tracked]
impl<'db> IntersectionType<'db> {
    /// Create an intersection type `E1 & E2 & ... & En` from a list of (positive) elements.
    ///
    /// For performance reasons, consider using [`IntersectionType::from_two_elements`] if
    /// the intersection is constructed from exactly two elements.
    pub(crate) fn from_elements<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut elements_iter = elements.into_iter();

        if let Some(first) = elements_iter.next() {
            if let Some(second) = elements_iter.next() {
                let builder =
                    IntersectionBuilder::new(db).positive_elements([first.into(), second.into()]);
                elements_iter
                    .fold(builder, |builder, element| {
                        builder.add_positive(element.into())
                    })
                    .build()
            } else {
                first.into()
            }
        } else {
            Type::object()
        }
    }

    /// Create an intersection type `A & B` from two elements `A` and `B`.
    #[salsa::tracked(
        cycle_initial=|_, id, _, _| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, result: Type<'db>, _, _| {
            result.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn from_two_elements(db: &'db dyn Db, a: Type<'db>, b: Type<'db>) -> Type<'db> {
        IntersectionBuilder::new(db)
            .positive_elements([a, b])
            .build()
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let positive = if nested {
            self.positive(db)
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, nested))
                .collect::<Option<FxOrderSet<Type<'db>>>>()?
        } else {
            self.positive(db)
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, div, nested)
                        .unwrap_or(div)
                })
                .collect()
        };

        let negative = if nested {
            self.negative(db)
                .try_map(|ty| ty.recursive_type_normalized_impl(db, div, nested))?
        } else {
            self.negative(db).map(|ty| {
                ty.recursive_type_normalized_impl(db, div, nested)
                    .unwrap_or(div)
            })
        };

        Some(IntersectionType::new(db, positive, negative))
    }

    /// Returns an iterator over the positive elements of the intersection. If
    /// there are no positive elements, returns a single `object` type.
    pub(crate) fn positive_elements_or_object(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = Type<'db>> {
        if self.positive(db).is_empty() {
            Either::Left(std::iter::once(Type::object()))
        } else {
            Either::Right(self.positive(db).iter().copied())
        }
    }

    /// Map a type transformation over all positive elements of the intersection. Leave the
    /// negative elements unchanged.
    pub(crate) fn map_positive(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let mut builder = IntersectionBuilder::new(db);
        for ty in self.positive(db) {
            builder = builder.add_positive(transform_fn(ty));
        }
        for ty in self.negative(db) {
            builder = builder.add_negative(*ty);
        }
        builder.build()
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = IntersectionBuilder::new(db);

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.positive_elements_or_object(db) {
            let ty_member = transform_fn(&ty);
            match ty_member {
                Place::Undefined => {}
                Place::Defined(DefinedPlace {
                    ty: ty_member,
                    origin: member_origin,
                    definedness: member_boundness,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    all_unbound = false;
                    if member_boundness == Definedness::AlwaysDefined {
                        any_definitely_bound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Undefined
        } else {
            Place::Defined(DefinedPlace {
                ty: builder.build(),
                origin,
                definedness: if any_definitely_bound {
                    Definedness::AlwaysDefined
                } else {
                    Definedness::PossiblyUndefined
                },
                public_type_policy: PublicTypePolicy::Raw,
            })
        }
    }

    pub(crate) fn map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = IntersectionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.positive_elements_or_object(db) {
            let PlaceAndQualifiers {
                place: member,
                qualifiers: new_qualifiers,
            } = transform_fn(&ty);
            qualifiers |= new_qualifiers;
            match member {
                Place::Undefined => {}
                Place::Defined(DefinedPlace {
                    ty: ty_member,
                    origin: member_origin,
                    definedness: member_boundness,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    all_unbound = false;
                    if member_boundness == Definedness::AlwaysDefined {
                        any_definitely_bound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(DefinedPlace {
                    ty: builder.build(),
                    origin,
                    definedness: if any_definitely_bound {
                        Definedness::AlwaysDefined
                    } else {
                        Definedness::PossiblyUndefined
                    },
                    public_type_policy: PublicTypePolicy::Raw,
                })
            },
            qualifiers,
        }
    }

    /// Return a version of this intersection type where any type variables in the positive elements
    /// have been replaced by their bounds or constraints, and where any newtypes in the positive elements
    /// have been replaced by their concrete base types.
    pub(crate) fn with_expanded_typevars_and_newtypes(self, db: &'db dyn Db) -> Type<'db> {
        expand_intersection_typevars_and_newtypes(db, self.positive(db), self.negative(db))
    }

    pub fn iter_positive(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.positive(db).iter().copied()
    }

    pub fn iter_negative(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.negative(db).iter().copied()
    }

    pub(crate) fn has_one_element(self, db: &'db dyn Db) -> bool {
        (self.positive(db).len() + self.negative(db).len()) == 1
    }

    pub(crate) fn is_simple_negation(self, db: &'db dyn Db) -> bool {
        self.positive(db).is_empty() && self.negative(db).len() == 1
    }
}

fn expand_intersection_typevars_and_newtypes<'db>(
    db: &'db dyn Db,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db);
    for &element in positive {
        match element {
            Type::TypeVar(tvar) => {
                match tvar.typevar(db).bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        builder = builder.add_positive(bound);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        builder = builder.add_positive(constraints.as_type(db));
                    }
                    // Type variables without bounds or constraints implicitly have `object`
                    // as their upper bound, and adding `object` to an intersection is always a no-op
                    None => {}
                }
            }
            Type::NewTypeInstance(newtype) => {
                builder = builder.add_positive(newtype.concrete_base_type(db));
            }
            _ => builder = builder.add_positive(element),
        }
    }

    for &element in negative {
        builder = builder.add_negative(element);
    }

    builder.build()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum RecursivelyDefined {
    Yes,
    No,
}

impl RecursivelyDefined {
    pub(crate) const fn is_yes(self) -> bool {
        matches!(self, RecursivelyDefined::Yes)
    }

    const fn or(self, other: RecursivelyDefined) -> RecursivelyDefined {
        match (self, other) {
            (RecursivelyDefined::Yes, _) | (_, RecursivelyDefined::Yes) => RecursivelyDefined::Yes,
            _ => RecursivelyDefined::No,
        }
    }
}

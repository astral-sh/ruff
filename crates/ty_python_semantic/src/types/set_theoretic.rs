use itertools::Either;
use ruff_db::small_order_set::SmallOrderSet;

use std::convert::Infallible;

use crate::Db;
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy, TypeOrigin,
};
use crate::types::class::KnownClass;
use crate::types::enums::EnumComplement;
use crate::types::{Type, TypeQualifiers};
use crate::types::{TypeVarBoundOrConstraints, visitor};

pub(crate) mod builder;

pub(crate) use builder::{IntersectionBuilder, UnionBuilder};

pub(crate) type IntersectionElementSet<'db> = SmallOrderSet<[Type<'db>; 2]>;

#[cfg(not(debug_assertions))]
static_assertions::const_assert_eq!(
    std::mem::size_of::<IntersectionElementSet<'static>>(),
    std::mem::size_of::<crate::FxOrderSet<Type<'static>>>()
);

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

    /// Returns `true` if any direct element of this union is a type alias.
    pub(crate) fn has_aliases(self, db: &'db dyn Db) -> bool {
        self.elements(db)
            .iter()
            .any(|element| matches!(element, Type::TypeAlias(_)))
    }

    /// Recursively expands aliases that expose top-level union elements.
    ///
    /// Aliases nested inside non-union elements remain part of those elements.
    pub(crate) fn expand_aliases(self, db: &'db dyn Db) -> Type<'db> {
        // Rebuild the union so that `UnionBuilder` simplifies any redundancies exposed.
        Self::from_elements(db, self.elements(db).iter().copied())
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
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let Ok(mapped) =
            self.try_map_impl(db, |element| Ok::<_, Infallible>(transform_fn(element)));
        mapped
    }

    /// A version of [`UnionType::map`] that does not unpack type aliases.
    pub(crate) fn map_leave_aliases(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let elements = self.elements(db);
        let mut iter = elements.iter().enumerate();
        while let Some((i, ty)) = iter.next() {
            let new_ty = transform_fn(ty);
            if &new_ty != ty {
                let mut builder = UnionBuilder::new(db).unpack_aliases(false);
                for prev in &elements[..i] {
                    builder = builder.add(*prev);
                }
                builder = builder.add(new_ty);
                for (_, element) in iter {
                    builder = builder.add(transform_fn(element));
                }
                return builder
                    .recursively_defined(self.recursively_defined(db))
                    .build();
            }
        }

        Type::Union(self)
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
        mut transform_fn: impl FnMut(&Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        self.try_map_impl(db, |element| transform_fn(element).ok_or(()))
            .ok()
    }

    fn try_map_impl<E>(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Result<Type<'db>, E>,
    ) -> Result<Type<'db>, E> {
        let elements = self.elements(db);
        let mut iter = elements.iter().enumerate();
        while let Some((i, ty)) = iter.next() {
            let new_ty = transform_fn(ty)?;
            if &new_ty != ty || matches!(new_ty, Type::TypeAlias(_)) {
                let mut builder = elements[..i]
                    .iter()
                    .copied()
                    .fold(UnionBuilder::new(db), UnionBuilder::add);
                builder = builder.add(new_ty);
                for (_, element) in iter {
                    builder = builder.add(transform_fn(element)?);
                }
                return Ok(builder
                    .recursively_defined(self.recursively_defined(db))
                    .build());
            }
        }

        Ok(Type::Union(self))
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
        let mut provenance = Provenance::Unknown;
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
                    provenance: member_provenance,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }
                    provenance = provenance.or(member_provenance);

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
                provenance,
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
        let mut provenance = Provenance::Unknown;
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
                    provenance: member_provenance,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }
                    provenance = provenance.or(member_provenance);

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
                    provenance,
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
    pub(crate) positive: IntersectionElementSet<'db>,

    /// The intersection type does not include any value in any of these types.
    ///
    /// Negation types aren't expressible in annotations, and are most likely to arise from type
    /// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
    /// directly in intersections rather than as a separate type.
    #[returns(ref)]
    pub(crate) negative: NegativeIntersectionElements<'db>,
}

pub type NegativeIntersectionElements<'db> = SmallOrderSet<[Type<'db>; 2]>;

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
    /// Return the compact enum-complement view of this intersection, if it has one.
    pub(crate) fn enum_complement(self, db: &'db dyn Db) -> Option<EnumComplement<'db>> {
        EnumComplement::from_intersection_parts(db, self.positive(db), self.negative(db))
    }

    /// Return the exact finite alternatives represented by this intersection, if available.
    pub fn finite_alternatives(self, db: &'db dyn Db) -> Option<Vec<Type<'db>>> {
        self.enum_complement(db)
            .map(|complement| complement.remaining_literal_types(db))
    }

    /// Return the exact finite alternative union represented by this intersection, if available.
    pub(crate) fn finite_alternative_union(self, db: &'db dyn Db) -> Option<Type<'db>> {
        Some(self.enum_complement(db)?.remaining_literal_union(db))
    }

    /// Return the finite alternatives only if they remain concise enough for display.
    pub(crate) fn finite_alternatives_for_display(
        self,
        db: &'db dyn Db,
        max_literals: usize,
    ) -> Option<Vec<Type<'db>>> {
        self.enum_complement(db)?
            .remaining_literal_types_for_display(db, max_literals)
    }

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
                .collect::<Option<IntersectionElementSet<'db>>>()?
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
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, nested))
                .collect::<Option<NegativeIntersectionElements<'db>>>()?
        } else {
            self.negative(db)
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, div, nested)
                        .unwrap_or(div)
                })
                .collect()
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
        let mut provenance = Provenance::Unknown;
        for ty in self.positive_elements_or_object(db) {
            let ty_member = transform_fn(&ty);
            match ty_member {
                Place::Undefined => {}
                Place::Defined(DefinedPlace {
                    ty: ty_member,
                    origin: member_origin,
                    definedness: member_boundness,
                    provenance: member_provenance,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    all_unbound = false;
                    if member_boundness == Definedness::AlwaysDefined {
                        any_definitely_bound = true;
                    }
                    provenance = provenance.or(member_provenance);

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
                provenance,
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
        let mut provenance = Provenance::Unknown;
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
                    provenance: member_provenance,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    all_unbound = false;
                    if member_boundness == Definedness::AlwaysDefined {
                        any_definitely_bound = true;
                    }
                    provenance = provenance.or(member_provenance);

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
                    provenance,
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
    positive: &IntersectionElementSet<'db>,
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

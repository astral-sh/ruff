use crate::SemanticContext;
use itertools::Either;

use std::convert::Infallible;

use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy, TypeOrigin,
};
use crate::types::class::KnownClass;
use crate::types::enums::EnumComplement;
use crate::types::{InstanceProjection, Type, TypePair, TypeQualifiers};
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
    #[returns(copy)]
    pub(crate) recursively_defined: RecursivelyDefined,
}

pub(crate) fn walk_union<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    union: UnionType<'db>,
    visitor: &V,
) {
    let db = ctx.db();
    for element in union.elements(db) {
        visitor.visit_type(ctx, *element);
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
    pub fn from_elements<I, T>(ctx: &SemanticContext<'db>, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut iter_elements = elements.into_iter();

        if let Some(first) = iter_elements.next() {
            if let Some(second) = iter_elements.next() {
                let builder = UnionBuilder::new(ctx).add(first.into()).add(second.into());
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
    pub fn from_two_elements(ctx: &SemanticContext<'db>, a: Type<'db>, b: Type<'db>) -> Type<'db> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, id, _| Type::divergent(id),
            cycle_fn=|db, cycle, previous: &Type<'db>, result: Type<'db>, types: TypePair<'db>| {
                result.cycle_normalized(&SemanticContext::from_version(db, types.python_version(db)), *previous, cycle)
            },
            heap_size=ruff_memory_usage::heap_size
        )]
        fn union_from_two_elements<'db>(db: &'db dyn Db, types: TypePair<'db>) -> Type<'db> {
            let ctx = SemanticContext::from_version(db, types.python_version(db));
            UnionBuilder::new(&ctx)
                .add(types.first(db))
                .add(types.second(db))
                .build()
        }

        let db = ctx.db();
        union_from_two_elements(db, TypePair::new(db, ctx.python_version(), a, b))
    }

    /// Create a union from a list of elements without unpacking type aliases.
    pub(crate) fn from_elements_leave_aliases<I, T>(
        ctx: &SemanticContext<'db>,
        elements: I,
    ) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(
                UnionBuilder::new(ctx).unpack_aliases(false),
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
    pub(crate) fn expand_aliases(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        // Rebuild the union so that `UnionBuilder` simplifies any redundancies exposed.
        Self::from_elements(ctx, self.elements(ctx.db()).iter().copied())
    }

    pub(crate) fn from_elements_cycle_recovery<I, T>(
        ctx: &SemanticContext<'db>,
        elements: I,
    ) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(
                UnionBuilder::new(ctx).cycle_recovery(true),
                |builder, element| builder.add(element.into()),
            )
            .build()
    }

    /// A fallible version of [`UnionType::from_elements`].
    ///
    /// If all items in `elements` are `Some()`, the result of unioning all elements is returned.
    /// As soon as a `None` element in the iterable is encountered,
    /// the function short-circuits and returns `None`.
    pub(crate) fn try_from_elements<I, T>(
        ctx: &SemanticContext<'db>,
        elements: I,
    ) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = Option<T>>,
        T: Into<Type<'db>>,
    {
        let mut builder = UnionBuilder::new(ctx);
        for element in elements {
            builder = builder.add(element?.into());
        }
        Some(builder.build())
    }

    /// Apply a transformation function to all elements of the union,
    /// and create a new union from the resulting set of types.
    pub(crate) fn map(
        self,
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let Ok(mapped) =
            self.try_map_impl(ctx, |element| Ok::<_, Infallible>(transform_fn(element)));
        mapped
    }

    /// A version of [`UnionType::map`] that does not unpack type aliases.
    pub(crate) fn map_leave_aliases(
        self,
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let db = ctx.db();
        let elements = self.elements(db);
        let mut iter = elements.iter().enumerate();
        while let Some((i, ty)) = iter.next() {
            let new_ty = transform_fn(ty);
            if &new_ty != ty {
                let mut builder = UnionBuilder::new(ctx).unpack_aliases(false);
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
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        self.try_map_impl(ctx, |element| transform_fn(element).ok_or(()))
            .ok()
    }

    fn try_map_impl<E>(
        self,
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Result<Type<'db>, E>,
    ) -> Result<Type<'db>, E> {
        let db = ctx.db();
        let elements = self.elements(db);
        let mut iter = elements.iter().enumerate();
        while let Some((i, ty)) = iter.next() {
            let new_ty = transform_fn(ty)?;
            if &new_ty != ty || matches!(new_ty, Type::TypeAlias(_)) {
                let mut builder = elements[..i]
                    .iter()
                    .copied()
                    .fold(UnionBuilder::new(ctx), UnionBuilder::add);
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

    pub(crate) fn to_instance(
        self,
        ctx: &SemanticContext<'db>,
    ) -> Option<InstanceProjection<Type<'db>>> {
        let mut is_exact = true;
        let instance = self.try_map(ctx, |element| {
            let projection = element.to_instance(ctx)?;
            is_exact &= projection.is_exact();
            Some(projection.into_inner())
        })?;
        Some(InstanceProjection::new(instance, is_exact))
    }

    /// Returns a shared fully static supertype for a union of literal-value types.
    ///
    /// The returned type is broader than the literal types themselves. For example, the
    /// supertype for `Literal["a"] | Literal["b"]` is `LiteralString`.
    pub(crate) fn common_literal_supertype(self, ctx: &SemanticContext<'db>) -> Option<Type<'db>> {
        let db = ctx.db();
        // Do not use `Type::literal_fallback_instance` here: it also falls back from function
        // literals to `FunctionType`. Since `FunctionType.__call__` is gradual, it can be
        // assignable to a callable that the function literal's precise signature is not.
        // Literal values have fully static supertypes, so a successful relation check for the
        // supertype proves the relation for every literal in the union.
        let supertype = |element: &Type<'db>| match element {
            Type::LiteralValue(literal) if literal.is_string() => Some(Type::literal_string()),
            Type::LiteralValue(literal) => Some(literal.fallback_instance(ctx)),
            _ => None,
        };

        let mut elements = self.elements(db).iter();
        let shared_supertype = supertype(elements.next()?)?;
        elements.try_fold(shared_supertype, |shared_supertype, element| {
            let next = supertype(element)?;
            (next == shared_supertype).then_some(shared_supertype)
        })
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
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let db = ctx.db();
        let mut builder = UnionBuilder::new(ctx);

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
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let db = ctx.db();
        let mut builder = UnionBuilder::new(ctx);
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
        ctx: &SemanticContext<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Type<'db>> {
        let db = ctx.db();
        let mut builder = UnionBuilder::new(ctx)
            .unpack_aliases(false)
            .cycle_recovery(true)
            .recursively_defined(self.recursively_defined(db));
        let mut empty = true;
        for ty in self.elements(db) {
            if nested {
                // list[T | Divergent] => list[Divergent]
                let ty = ty.recursive_type_normalized_impl(ctx, div, nested)?;
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
                    ty.recursive_type_normalized_impl(ctx, div, nested)
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
    pub(crate) fn to_type<'db>(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        match self {
            KnownUnion::Float => UnionType::from_two_elements(
                ctx,
                KnownClass::Int.to_instance(ctx),
                KnownClass::Float.to_instance(ctx),
            ),
            KnownUnion::Complex => UnionType::from_elements(
                ctx,
                [
                    KnownClass::Int.to_instance(ctx),
                    KnownClass::Float.to_instance(ctx),
                    KnownClass::Complex.to_instance(ctx),
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
#[derive(Debug, Clone, get_size2::GetSize, Default, salsa::SalsaValue)]
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
        // collections are represented as `Self::Empty`
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

const MAX_INTERSECTION_DNF_TERMS: usize = 4;

pub(crate) fn walk_intersection_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    intersection: IntersectionType<'db>,
    visitor: &V,
) {
    let db = ctx.db();
    for element in intersection.positive(db) {
        visitor.visit_type(ctx, *element);
    }
    for element in intersection.negative(db) {
        visitor.visit_type(ctx, *element);
    }
}

#[salsa::tracked]
impl<'db> IntersectionType<'db> {
    /// Return the compact enum-complement view of this intersection, if it has one.
    pub(crate) fn enum_complement(self, ctx: &SemanticContext<'db>) -> Option<EnumComplement<'db>> {
        EnumComplement::from_intersection_parts(
            ctx,
            self.positive(ctx.db()),
            self.negative(ctx.db()),
        )
    }

    /// Return the exact finite alternatives represented by this intersection, if available.
    pub fn finite_alternatives(self, ctx: &SemanticContext<'db>) -> Option<Vec<Type<'db>>> {
        self.enum_complement(ctx)
            .map(|complement| complement.remaining_literal_types(ctx))
    }

    /// Return the exact finite alternative union represented by this intersection, if available.
    pub(crate) fn finite_alternative_union(self, ctx: &SemanticContext<'db>) -> Option<Type<'db>> {
        Some(self.enum_complement(ctx)?.remaining_literal_union(ctx))
    }

    /// Return the finite alternatives only if they remain concise enough for display.
    pub(crate) fn finite_alternatives_for_display(
        self,
        ctx: &SemanticContext<'db>,
        max_literals: usize,
    ) -> Option<Vec<Type<'db>>> {
        self.enum_complement(ctx)?
            .remaining_literal_types_for_display(ctx, max_literals)
    }

    /// Create an intersection type `E1 & E2 & ... & En` from a list of (positive) elements.
    ///
    /// For performance reasons, consider using [`IntersectionType::from_two_elements`] if
    /// the intersection is constructed from exactly two elements.
    pub(crate) fn from_elements<I, T>(ctx: &SemanticContext<'db>, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut elements_iter = elements.into_iter();

        if let Some(first) = elements_iter.next() {
            if let Some(second) = elements_iter.next() {
                let builder =
                    IntersectionBuilder::new(ctx).positive_elements([first.into(), second.into()]);
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

    /// Create an intersection type `E1 & E2 & ... & En` from a list of (positive) elements, while
    /// ensuring that we only expand an intersection of unions within a limited budget.
    ///
    /// Our `Type` representation is in DNF, which means that the size of an intersection of unions
    /// grows as the product of all of the union sizes. [`from_elements`][Self::from_elements] will
    /// blindly calculate that full expansion. This method detects when we exceed a fixed budget of
    /// work, and if so, returns `None`. (Redundant terms do not count toward the budget.)
    ///
    /// Like [`from_elements`][Self::from_elements], a successful result is exact.
    pub(crate) fn bounded_from_elements<I, T>(
        ctx: &SemanticContext<'db>,
        elements: I,
    ) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Clone,
        Type<'db>: From<T>,
    {
        // TODO: Consider folding this logic into IntersectionBuilder itself, and having it check
        // an optional budget as part of its existing `add_positive` methods.

        let elements = elements.into_iter().map(Type::from);
        let union_count = elements.clone().filter(|ty| ty.is_union()).count();
        if union_count <= 1 {
            // If there are no unions, then all we have to do is check for redundant elements. If
            // there is a single union, the product of all union counts should be reasonable, even
            // if it exceeds the budget below. In both cases, just return the precise answer
            // without considering the budget.
            return Some(Self::from_elements(ctx, elements));
        }

        let non_union_elements = elements.clone().filter(|element| !element.is_union());
        let initial = Self::from_elements(ctx, non_union_elements);
        let insert_candidate = |candidates: &mut Vec<Type<'db>>, new_ty: Type<'db>| -> Option<()> {
            if new_ty.is_never()
                || candidates
                    .iter()
                    .any(|old| new_ty.is_redundant_with(ctx, *old))
            {
                return Some(());
            }

            candidates.retain(|old| !old.is_redundant_with(ctx, new_ty));
            if candidates.len() >= MAX_INTERSECTION_DNF_TERMS {
                return None;
            }
            candidates.push(new_ty);
            Some(())
        };

        let mut frontier = Vec::new();
        let mut next = Vec::new();
        insert_candidate(&mut frontier, initial)?;

        for (idx, clause) in elements.filter_map(Type::as_union).enumerate() {
            // Don't check the budget for the first union clause. That ensures that we have a
            // chance for pairs of types to "annihilate" each other without contributing to the
            // result. For instance, this allows us to return the precise result for
            // `(A | B | C | D | E) & (A | B | F | G | H)` (in which each class is final), since
            // most of the pairs are disjoint.
            let skip_budget_check = (idx == 0).then_some(());

            next.clear();
            for candidate in &frontier {
                for alternative in clause.elements(ctx.db()) {
                    let refined = Self::from_two_elements(ctx, *candidate, *alternative);
                    insert_candidate(&mut next, refined).or(skip_budget_check)?;
                }
            }

            if next.is_empty() {
                return Some(Type::Never);
            }

            std::mem::swap(&mut frontier, &mut next);
        }

        Some(UnionType::from_elements(ctx, frontier))
    }

    /// Create an intersection type `A & B` from two elements `A` and `B`.
    pub(crate) fn from_two_elements(
        ctx: &SemanticContext<'db>,
        a: Type<'db>,
        b: Type<'db>,
    ) -> Type<'db> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, id, _| Type::divergent(id),
            cycle_fn=|db, cycle, previous: &Type<'db>, result: Type<'db>, types: TypePair<'db>| {
                result.cycle_normalized(&SemanticContext::from_version(db, types.python_version(db)), *previous, cycle)
            },
            heap_size=ruff_memory_usage::heap_size
        )]
        fn intersection_from_two_elements<'db>(db: &'db dyn Db, types: TypePair<'db>) -> Type<'db> {
            let ctx = SemanticContext::from_version(db, types.python_version(db));
            IntersectionBuilder::new(&ctx)
                .positive_elements([types.first(db), types.second(db)])
                .build()
        }

        let db = ctx.db();
        intersection_from_two_elements(db, TypePair::new(db, ctx.python_version(), a, b))
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        ctx: &SemanticContext<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let db = ctx.db();
        let positive = if nested {
            self.positive(db)
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(ctx, div, nested))
                .collect::<Option<FxOrderSet<Type<'db>>>>()?
        } else {
            self.positive(db)
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(ctx, div, nested)
                        .unwrap_or(div)
                })
                .collect()
        };

        let negative = if nested {
            self.negative(db)
                .try_map(|ty| ty.recursive_type_normalized_impl(ctx, div, nested))?
        } else {
            self.negative(db).map(|ty| {
                ty.recursive_type_normalized_impl(ctx, div, nested)
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
        let positive = self.positive(db);
        if positive.is_empty() {
            Either::Left(std::iter::once(Type::object()))
        } else {
            Either::Right(positive.iter().copied())
        }
    }

    /// Map a type transformation over all positive elements of the intersection. Leave the
    /// negative elements unchanged.
    pub(crate) fn map_positive(
        self,
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let db = ctx.db();
        let mut builder = IntersectionBuilder::new(ctx);
        for ty in self.positive(db) {
            builder = builder.add_positive(transform_fn(ty));
        }
        for ty in self.negative(db) {
            builder = builder.add_negative(*ty);
        }
        builder.build()
    }

    /// Compute the `__class__` type when this intersection contains a positive class-backed
    /// protocol constraint.
    ///
    /// Negative instance constraints are not transferred: an object not satisfying `P` does not
    /// imply that other instances of its class cannot satisfy `P`.
    pub(crate) fn try_dunder_class(self, ctx: &SemanticContext<'db>) -> Option<Type<'db>> {
        let db = ctx.db();
        if !self.iter_positive(db).any(|positive| {
            matches!(
                positive,
                Type::ProtocolInstance(protocol) if protocol.class_origin().is_some()
            )
        }) {
            return None;
        }

        Some(
            self.iter_positive(db)
                .fold(IntersectionBuilder::new(ctx), |builder, positive| {
                    builder.add_positive(positive.dunder_class(ctx))
                })
                .build(),
        )
    }

    pub(crate) fn map_with_boundness(
        self,
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let db = ctx.db();
        let mut builder = IntersectionBuilder::new(ctx);

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
        ctx: &SemanticContext<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let db = ctx.db();
        let mut builder = IntersectionBuilder::new(ctx);
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
    pub(crate) fn with_expanded_typevars_and_newtypes(
        self,
        ctx: &SemanticContext<'db>,
    ) -> Type<'db> {
        let db = ctx.db();
        expand_intersection_typevars_and_newtypes(ctx, self.positive(db), self.negative(db))
    }

    pub fn iter_positive(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.positive(db).iter().copied()
    }

    pub fn iter_negative(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.negative(db).iter().copied()
    }

    /// Project an intersection containing class-object types into the corresponding instance types.
    ///
    /// A projected positive element supplies a sound instance-space over-approximation for the
    /// whole intersection. Other positive elements can constrain class objects in a domain with no
    /// instance-space projection, so omitting them is also a sound over-approximation. Negative
    /// elements cannot be projected: a class object excluded by an exact-class negative can still
    /// have subclasses whose instances inhabit the excluded class's instance type. Without a
    /// projected positive element, we cannot tell whether the intersection contains class objects
    /// at all. The result is exact only when every positive element projects exactly and there are
    /// no negative elements.
    ///
    /// For example, Python narrowing can produce `type[Base] & ~TypeOf[Base]`:
    ///
    /// ```py
    /// class Base: ...
    /// class Child(Base): ...
    ///
    /// def make(cls: type[Base]) -> Base:
    ///     if cls is not Base:
    ///         return cls()  # `cls` can be `Child`, so this can return a `Child` instance.
    ///     return Base()
    /// ```
    ///
    /// Projecting only the positive `type[Base]` is an over-approximation, since we have no
    /// representation of an exact instance type excluding subclasses, and projecting the negative
    /// `~TypeOf[Base]` to `~Base` would incorrectly exclude `Child` instances too.
    pub(crate) fn to_instance(
        self,
        ctx: &SemanticContext<'db>,
    ) -> Option<InstanceProjection<Type<'db>>> {
        let db = ctx.db();
        let mut builder = IntersectionBuilder::new(ctx);
        let mut has_projected_positive = false;
        let mut is_exact = self.negative(db).is_empty();
        for positive in self.iter_positive(db) {
            if let Some(projection) = positive.to_instance(ctx) {
                has_projected_positive = true;
                is_exact &= projection.is_exact();
                builder = builder.add_positive(projection.into_inner());
            } else {
                is_exact = false;
            }
        }
        if !has_projected_positive {
            return None;
        }

        Some(InstanceProjection::new(builder.build(), is_exact))
    }

    pub(crate) fn has_one_element(self, db: &'db dyn Db) -> bool {
        (self.positive(db).len() + self.negative(db).len()) == 1
    }

    pub(crate) fn is_simple_negation(self, db: &'db dyn Db) -> bool {
        self.positive(db).is_empty() && self.negative(db).len() == 1
    }
}

fn expand_intersection_typevars_and_newtypes<'db>(
    ctx: &SemanticContext<'db>,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> Type<'db> {
    let db = ctx.db();
    let mut builder = IntersectionBuilder::new(ctx);
    for &element in positive {
        match element {
            Type::TypeVar(tvar) => {
                match tvar.typevar(db).bound_or_constraints(ctx) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        builder = builder.add_positive(bound);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        builder = builder.add_positive(constraints.as_type(ctx));
                    }
                    // Type variables without bounds or constraints implicitly have `object`
                    // as their upper bound, and adding `object` to an intersection is always a no-op
                    None => {}
                }
            }
            Type::NewTypeInstance(newtype) => {
                builder = builder.add_positive(newtype.concrete_base_type(ctx));
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

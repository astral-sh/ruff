use crate::ProgramEnvironment;
use itertools::Either;
use rustc_hash::FxHashSet;

use std::cell::{Cell, RefCell};
use std::convert::Infallible;

use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy, TypeOrigin,
};
use crate::types::class::KnownClass;
use crate::types::enums::EnumComplement;
use crate::types::recursive::{StructuralTraversal, StructuralTypeTraversal, TypeInspector};
use crate::types::{
    ApplyTypeMappingVisitor, CycleKind, InstanceProjection, ProjectInstance, PromotionKind,
    PromotionMode, Type, TypeContext, TypeMapping, TypePair, TypeQualifiers, TypeQueryContext,
    TypeWalk,
};
use crate::types::{TypeVarBoundOrConstraints, visitor};
use crate::{Db, FxOrderSet};

pub(crate) mod builder;
mod generic_gradual_intersections;

pub(crate) use builder::{IntersectionBuilder, UnionBuilder};

struct ExpandUnion<'a, 'db> {
    seen: RefCell<FxHashSet<Type<'db>>>,
    recursively_defined: &'a Cell<RecursivelyDefined>,
    unpack_aliases: bool,
    cycle_recovery: bool,
    expand_named: bool,
}

#[derive(Clone, Copy, Default)]
struct UnionExpansionState {
    unfold_named: bool,
    inside_alias: bool,
}

impl<'db> TypeWalk<'db, UnionExpansionState, StructuralTraversal> for ExpandUnion<'_, 'db> {
    type Output = Result<Vec<Type<'db>>, ()>;

    fn on_cycle(
        &self,
        _traversal: &StructuralTypeTraversal<'_, 'db, Self>,
        _ty: Type<'db>,
        kind: CycleKind,
        _state: UnionExpansionState,
    ) -> Self::Output {
        match kind {
            CycleKind::Exact => Ok(Vec::new()),
            CycleKind::MatchingIdentity => Err(()),
        }
    }

    fn visit_alias_like(
        &self,
        db: &'db dyn Db,
        _traversal: &StructuralTypeTraversal<'_, 'db, Self>,
        ty: Type<'db>,
        state: UnionExpansionState,
        visit_body: impl FnOnce(UnionExpansionState) -> Self::Output,
    ) -> Self::Output {
        if !self.seen.borrow_mut().insert(ty) {
            return Ok(Vec::new());
        }
        let unfold = match ty {
            Type::TypeAlias(_) => self.unpack_aliases,
            Type::Recursive(recursive) => {
                self.expand_named
                    || state.unfold_named
                    || ((self.unpack_aliases || self.cycle_recovery)
                        && recursive.alias(db).is_none())
            }
            _ => false,
        };
        if !unfold {
            return Ok(vec![ty]);
        }

        let result = visit_body(UnionExpansionState {
            unfold_named: state.unfold_named || matches!(ty, Type::TypeAlias(_)),
            inside_alias: true,
        });
        // Retain the outer constructor when any part of its expansion grows. Retaining only
        // an inner constructor would expose another layer of mutual recursion on each pass.
        if state.inside_alias {
            result
        } else {
            Ok(result.unwrap_or_else(|()| vec![ty]))
        }
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &StructuralTypeTraversal<'_, 'db, Self>,
        ty: Type<'db>,
        state: UnionExpansionState,
    ) -> Self::Output {
        match ty {
            Type::Union(union) => {
                if !self.seen.borrow_mut().insert(ty) {
                    return Ok(Vec::new());
                }
                self.recursively_defined.set(
                    self.recursively_defined
                        .get()
                        .or(union.recursively_defined(db)),
                );
                let mut elements = Vec::new();
                for element in union.elements(db) {
                    elements.extend(traversal.visit_with(
                        db,
                        *element,
                        UnionExpansionState {
                            unfold_named: false,
                            ..state
                        },
                    )?);
                }
                Ok(elements)
            }
            Type::Never => Ok(Vec::new()),
            ty => Ok(vec![ty]),
        }
    }
}

/// Expand only outer aliases and unions, preserving a constructor when specialization grows.
fn expand_union_elements<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    unpack_aliases: bool,
    cycle_recovery: bool,
    unfold_named: bool,
) -> (Vec<Type<'db>>, RecursivelyDefined) {
    let recursively_defined = Cell::new(RecursivelyDefined::No);
    let elements = StructuralTypeTraversal::with_mapping_visitor(
        visitor,
        ExpandUnion {
            seen: RefCell::default(),
            recursively_defined: &recursively_defined,
            unpack_aliases,
            cycle_recovery,
            expand_named: unfold_named,
        },
        |root| root.visit_with(db, ty, UnionExpansionState::default()),
    )
    .unwrap_or_else(|()| vec![ty]);
    (elements, recursively_defined.get())
}

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
    /// Combine alternatives without asking whether one is a subtype of another.
    /// Recursive type construction uses this before a relation has begun inspecting the result.
    pub(super) fn from_elements_structural(
        db: &'db dyn Db,
        elements: impl IntoIterator<Item = Type<'db>>,
        mut recursively_defined: RecursivelyDefined,
    ) -> Type<'db> {
        let mut pending: Vec<_> = elements.into_iter().collect();
        pending.reverse();
        let mut elements = FxOrderSet::default();
        while let Some(element) = pending.pop() {
            match element {
                Type::Union(union) => {
                    recursively_defined = recursively_defined.or(union.recursively_defined(db));
                    pending.extend(union.elements(db).iter().rev().copied());
                }
                Type::Never => {}
                ty => {
                    elements.insert(ty);
                }
            }
        }
        match elements.len() {
            0 => Type::Never,
            1 => elements[0],
            _ => Type::Union(Self::new(
                db,
                elements.into_iter().collect::<Box<[_]>>(),
                recursively_defined,
            )),
        }
    }

    /// Create a union from a list of elements
    /// (which may be eagerly simplified into a different variant of [`Type`] altogether).
    ///
    /// For performance reasons, consider using [`UnionType::from_two_elements`] if
    /// the union is constructed from exactly two elements.
    pub fn from_elements<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut iter_elements = elements.into_iter();

        if let Some(first) = iter_elements.next() {
            if let Some(second) = iter_elements.next() {
                let mut builder = UnionBuilder::new(db, env);
                builder.add_in_place(first.into());
                builder.add_in_place(second.into());
                for element in iter_elements {
                    builder.add_in_place(element.into());
                }
                builder.build()
            } else {
                first.into()
            }
        } else {
            Type::Never
        }
    }

    /// Create a union type `A | B` from two elements `A` and `B`.
    pub fn from_two_elements(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        a: Type<'db>,
        b: Type<'db>,
    ) -> Type<'db> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, id, _| Type::divergent(id),
            cycle_fn=|db, cycle, previous: &Type<'db>, result: Type<'db>, types: TypePair<'db>| {
                result.cycle_normalized(db, &ProgramEnvironment::from_program(types.program(db)), *previous, cycle)
            },
            heap_size=ruff_memory_usage::heap_size
        )]
        fn union_from_two_elements<'db>(db: &'db dyn Db, types: TypePair<'db>) -> Type<'db> {
            let env = ProgramEnvironment::from_program(types.program(db));
            UnionBuilder::new(db, &env)
                .add(types.first(db))
                .add(types.second(db))
                .build()
        }

        union_from_two_elements(db, TypePair::new(db, env.program(db), a, b))
    }

    /// Create a union from a list of elements without unpacking type aliases.
    pub(crate) fn from_elements_leave_aliases<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut builder = UnionBuilder::new(db, env).unpack_aliases(false);
        for element in elements {
            builder.add_in_place(element.into());
        }
        builder.build()
    }

    /// Returns `true` if any direct element of this union is a type alias.
    fn has_aliases(self, db: &'db dyn Db) -> bool {
        self.elements(db).iter().copied().any(Type::is_alias_like)
    }

    /// Whether this union still needs expansion in the current type operation.
    ///
    /// A completed expansion can retain recursive references. Those references are compared
    /// individually; expanding the whole union again would expose ever larger specializations.
    pub(super) fn needs_alias_expansion(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        self.has_aliases(db)
            && visitor.expanded_unions.borrow().get(&self) != Some(&Type::Union(self))
    }

    /// Recursively expands aliases that expose top-level union elements.
    ///
    /// Aliases nested inside non-union elements remain part of those elements.
    pub(crate) fn expand_aliases(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        // A relation can compare many source types with this union. Reuse its expansion within
        // the same materialization root, whose active guards can affect recursive unfoldings.
        if let Some(expanded) = visitor.expanded_unions.borrow().get(&self) {
            return *expanded;
        }
        let (elements, recursively_defined) =
            expand_union_elements(db, Type::Union(self), visitor, true, false, true);

        // Keep any growing recursive references opaque when reducing the completed union.
        let mut builder = UnionBuilder::new(db, visitor.env)
            .unpack_aliases(false)
            .or_recursively_defined(recursively_defined);
        for element in elements {
            builder.add_in_place(element);
        }
        let expanded = builder.build();
        let mut expansions = visitor.expanded_unions.borrow_mut();
        // The result is already expanded for this operation, including when changing the source
        // of a relation revisits the same target union. Preserve its remaining recursive boundary.
        if let Type::Union(union) = expanded {
            expansions.insert(union, expanded);
        }
        expansions.insert(self, expanded);
        expanded
    }

    pub(crate) fn from_elements_cycle_recovery<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut builder = UnionBuilder::new(db, env).cycle_recovery(true);
        for element in elements {
            builder.add_in_place(element.into());
        }
        builder.build()
    }

    /// Widen tuple unions that acquire new lengths between successive cycle results.
    ///
    /// Normalizing an existing union during cycle recovery does not imply that its tuple lengths
    /// are growing. Compare the results of successive iterations so stable, finite unions keep
    /// their shapes, including unions nested in a collection's type arguments.
    pub(crate) fn widen_growing_tuples(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> Option<Type<'db>> {
        if previous == current {
            return None;
        }
        let previous_types = match &previous {
            Type::Union(union) => union.elements(db),
            ty => std::slice::from_ref(ty),
        };
        let current_types = match &current {
            Type::Union(union) => union.elements(db),
            ty => std::slice::from_ref(ty),
        };
        let previous_lengths: Vec<_> = previous_types
            .iter()
            .filter_map(|ty| ty.exact_tuple_instance_spec(db).map(|tuple| tuple.len()))
            .collect();
        if previous_lengths.is_empty()
            || !current_types.iter().any(|ty| {
                ty.exact_tuple_instance_spec(db)
                    .is_some_and(|tuple| !previous_lengths.contains(&tuple.len()))
            })
        {
            return None;
        }

        // Recovery cannot perform relation queries, including when combining tuple elements.
        // Mark those elements recursive so growing literal unions also widen promptly.
        let mut elements = UnionBuilder::new(db, env)
            .cycle_recovery(true)
            .or_recursively_defined(RecursivelyDefined::Yes);
        let mut result = UnionBuilder::new(db, env)
            .cycle_recovery(true)
            .or_recursively_defined(current.as_union().map_or(RecursivelyDefined::No, |union| {
                union.recursively_defined(db)
            }));
        // During the first cycle iterations, the caller can discard previous alternatives.
        // Retain their tuple elements for widening without restoring unrelated alternatives.
        for ty in current_types {
            if ty.exact_tuple_instance_spec(db).is_none() {
                result.add_in_place(*ty);
            }
        }
        for ty in previous_types.iter().chain(current_types) {
            if let Some(tuple) = ty.exact_tuple_instance_spec(db) {
                for element in tuple.iter_element_types(db) {
                    elements.add_in_place(element);
                }
            }
        }
        let element_type = match elements.build() {
            // `tuple[Never, ...]` normalizes to the empty tuple, which does not contain
            // fixed-length types like `tuple[Never]`. Preserve a static upper bound.
            Type::Never => Type::object(),
            element_type => element_type,
        };
        Some(
            result
                .add(Type::homogeneous_tuple(db, env, element_type))
                .build(),
        )
    }

    /// A fallible version of [`UnionType::from_elements`].
    ///
    /// If all items in `elements` are `Some()`, the result of unioning all elements is returned.
    /// As soon as a `None` element in the iterable is encountered,
    /// the function short-circuits and returns `None`.
    pub(crate) fn try_from_elements<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = Option<T>>,
        T: Into<Type<'db>>,
    {
        let mut builder = UnionBuilder::new(db, env);
        for element in elements {
            builder.add_in_place(element?.into());
        }
        Some(builder.build())
    }

    /// Map the union's elements, preserving open bodies during structural substitutions.
    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if !visitor.normalize_unions {
            Self::from_elements_structural(
                db,
                self.elements(db)
                    .iter()
                    .map(|element| element.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
                self.recursively_defined(db),
            )
        } else {
            self.map_leave_aliases(db, visitor.env, |element| {
                element.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            })
        }
    }

    /// Apply a transformation function to all elements of the union,
    /// and create a new union from the resulting set of types.
    pub(crate) fn map(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let Ok(mapped) = self.try_map_impl(db, env, |element| {
            Ok::<_, Infallible>(transform_fn(element))
        });
        mapped
    }

    /// A version of [`UnionType::map`] that does not unpack type aliases.
    pub(super) fn map_leave_aliases(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let elements = self.elements(db);
        let mut iter = elements.iter().enumerate();
        while let Some((i, ty)) = iter.next() {
            let new_ty = transform_fn(ty);
            if &new_ty != ty {
                let mut builder = UnionBuilder::new(db, env).unpack_aliases(false);
                for prev in &elements[..i] {
                    builder.add_in_place(*prev);
                }
                builder.add_in_place(new_ty);
                for (_, element) in iter {
                    builder.add_in_place(transform_fn(element));
                }
                return builder
                    .or_recursively_defined(self.recursively_defined(db))
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
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        self.try_map_impl(db, env, |element| transform_fn(element).ok_or(()))
            .ok()
    }

    fn try_map_impl<E>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Result<Type<'db>, E>,
    ) -> Result<Type<'db>, E> {
        let elements = self.elements(db);
        let mut iter = elements.iter().enumerate();
        while let Some((i, ty)) = iter.next() {
            let new_ty = transform_fn(ty)?;
            // The builder unpacks `TypeAlias` nodes but preserves structural recursive types.
            if &new_ty != ty || matches!(new_ty, Type::TypeAlias(_)) {
                let mut builder = UnionBuilder::new(db, env);
                for prev in &elements[..i] {
                    builder.add_in_place(*prev);
                }
                builder.add_in_place(new_ty);
                for (_, element) in iter {
                    builder.add_in_place(transform_fn(element)?);
                }
                return Ok(builder
                    .or_recursively_defined(self.recursively_defined(db))
                    .build());
            }
        }

        Ok(Type::Union(self))
    }

    pub(super) fn to_instance(
        self,
        db: &'db dyn Db,
        traversal: &TypeQueryContext<'_, 'db, ProjectInstance>,
    ) -> Option<InstanceProjection<Type<'db>>> {
        let env = traversal.environment();
        let mut is_exact = true;
        let instance = self.try_map(db, env, |element| {
            let projection = traversal.visit(db, *element)?;
            is_exact &= projection.is_exact();
            Some(projection.into_inner())
        })?;
        Some(InstanceProjection::new(instance, is_exact))
    }

    /// Returns a shared fully static supertype for a union of literal-value types.
    ///
    /// The returned type is broader than the literal types themselves. For example, the
    /// supertype for `Literal["a"] | Literal["b"]` is `LiteralString`.
    pub(crate) fn common_literal_supertype(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        // Do not use `Type::literal_fallback_instance` here: it also falls back from function
        // literals to `FunctionType`. Since `FunctionType.__call__` is gradual, it can be
        // assignable to a callable that the function literal's precise signature is not.
        // Literal values have fully static supertypes, so a successful relation check for the
        // supertype proves the relation for every literal in the union.
        let supertype = |element: &Type<'db>| match element {
            Type::LiteralValue(literal) if literal.is_string() => Some(Type::literal_string()),
            Type::LiteralValue(literal) => Some(literal.fallback_instance(db, env)),
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
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = UnionBuilder::new(db, env);

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
                    builder.add_in_place(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Undefined
        } else {
            Place::Defined(DefinedPlace {
                ty: builder
                    .or_recursively_defined(self.recursively_defined(db))
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
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.filter_map_with_boundness_and_qualifiers(db, env, |ty| Some(transform_fn(ty)))
    }

    pub(crate) fn filter_map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Option<PlaceAndQualifiers<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = UnionBuilder::new(db, env);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        let mut origin = TypeOrigin::Declared;
        let mut provenance = Provenance::Unknown;
        for ty in self.elements(db) {
            let Some(PlaceAndQualifiers {
                place: ty_member,
                qualifiers: new_qualifiers,
            }) = transform_fn(ty)
            else {
                continue;
            };
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
                    builder.add_in_place(ty_member);
                }
            }
        }
        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(DefinedPlace {
                    ty: builder
                        .or_recursively_defined(self.recursively_defined(db))
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
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db, env)
            .unpack_aliases(false)
            .cycle_recovery(true)
            .or_recursively_defined(self.recursively_defined(db));
        let mut empty = true;
        for ty in self.elements(db) {
            if nested {
                // list[T | Divergent] => list[Divergent]
                let ty = ty.recursive_type_normalized_impl(db, env, div, nested)?;
                if ty.same_divergent_marker(div) {
                    return Some(ty);
                }
                builder.add_in_place(ty);
                empty = false;
            } else {
                // `Divergent` in a union type does not mean true divergence, so we skip it if not nested.
                // e.g. T | Divergent == T | (T | (T | (T | ...))) == T
                if (*ty).same_divergent_marker(div) {
                    builder = builder.or_recursively_defined(RecursivelyDefined::Yes);
                    continue;
                }
                builder.add_in_place(
                    ty.recursive_type_normalized_impl(db, env, div, nested)
                        .unwrap_or(div),
                );
                empty = false;
            }
        }
        if empty {
            builder.add_in_place(div);
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
    /// Returns the class whose annotation denotes this numeric-tower union.
    pub(crate) const fn annotation_class(self) -> KnownClass {
        match self {
            Self::Float => KnownClass::Float,
            Self::Complex => KnownClass::Complex,
        }
    }

    /// Returns whether this union contains exact instances of `class`.
    pub(crate) const fn contains(self, class: KnownClass) -> bool {
        match self {
            Self::Float => matches!(class, KnownClass::Int | KnownClass::Float),
            Self::Complex => matches!(
                class,
                KnownClass::Int | KnownClass::Float | KnownClass::Complex
            ),
        }
    }

    pub(crate) fn to_type<'db>(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            KnownUnion::Float => UnionType::from_two_elements(
                db,
                env,
                KnownClass::Int.to_instance(db, env),
                KnownClass::Float.to_instance(db, env),
            ),
            KnownUnion::Complex => UnionType::from_elements(
                db,
                env,
                [
                    KnownClass::Int.to_instance(db, env),
                    KnownClass::Float.to_instance(db, env),
                    KnownClass::Complex.to_instance(db, env),
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

    fn len(&self) -> usize {
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
    fn shrink_to_fit(&mut self) {
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
    fn swap_remove(&mut self, ty: &Type<'db>) -> bool {
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
    fn swap_remove_index(&mut self, index: usize) -> Option<Type<'db>> {
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
    pub(super) fn map(&self, map_fn: impl Fn(&Type<'db>) -> Type<'db>) -> Self {
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
    /// Returns whether a direct positive or negative element is a type alias.
    fn has_aliases(self, db: &'db dyn Db) -> bool {
        self.positive(db)
            .iter()
            .chain(self.negative(db))
            .copied()
            .any(Type::is_alias_like)
    }

    pub(super) fn needs_alias_expansion(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        self.has_aliases(db)
            && visitor.expanded_intersections.borrow().get(&self) != Some(&Type::Intersection(self))
    }

    /// Normalize direct aliases without expanding aliases nested inside containers.
    pub(crate) fn expand_aliases(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if let Some(expanded) = visitor.expanded_intersections.borrow().get(&self) {
            return *expanded;
        }
        let expanded = IntersectionBuilder::expand_aliases(db, self, visitor);
        let mut expansions = visitor.expanded_intersections.borrow_mut();
        // A normalized intersection can retain recursive references. Compare those elements
        // individually instead of expanding the whole intersection again.
        if let Type::Intersection(intersection) = expanded {
            expansions.insert(intersection, expanded);
        }
        expansions.insert(self, expanded);
        expanded
    }

    /// Return the compact enum-complement view of this intersection, if it has one.
    pub(crate) fn enum_complement(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<EnumComplement<'db>> {
        EnumComplement::from_intersection_parts(db, env, self.positive(db), self.negative(db))
    }

    /// Return the exact finite alternatives represented by this intersection, if available.
    pub fn finite_alternatives(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Vec<Type<'db>>> {
        self.enum_complement(db, env)
            .map(|complement| complement.remaining_literal_types(db, env))
    }

    /// Return the exact finite alternative union represented by this intersection, if available.
    pub(crate) fn finite_alternative_union(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        Some(
            self.enum_complement(db, env)?
                .remaining_literal_union(db, env),
        )
    }

    /// Return the finite alternatives only if they remain concise enough for display.
    pub(crate) fn finite_alternatives_for_display(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        max_literals: usize,
    ) -> Option<Vec<Type<'db>>> {
        self.enum_complement(db, env)?
            .remaining_literal_types_for_display(db, env, max_literals)
    }

    /// Create an intersection type `E1 & E2 & ... & En` from a list of (positive) elements.
    ///
    /// For performance reasons, consider using [`IntersectionType::from_two_elements`] if
    /// the intersection is constructed from exactly two elements.
    pub(crate) fn from_elements<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        let mut elements_iter = elements.into_iter();

        if let Some(first) = elements_iter.next() {
            if let Some(second) = elements_iter.next() {
                let mut builder = IntersectionBuilder::new(db, env)
                    .positive_elements([first.into(), second.into()]);
                for element in elements_iter {
                    builder.add_positive_in_place(element.into());
                }
                builder.build()
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
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Clone,
        Type<'db>: From<T>,
    {
        IntersectionBuilder::bounded_from_elements(db, env, elements)
    }

    /// Create an intersection type `A & B` from two elements `A` and `B`.
    pub(crate) fn from_two_elements(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        a: Type<'db>,
        b: Type<'db>,
    ) -> Type<'db> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, id, _| Type::divergent(id),
            cycle_fn=|db, cycle, previous: &Type<'db>, result: Type<'db>, types: TypePair<'db>| {
                result.cycle_normalized(db, &ProgramEnvironment::from_program(types.program(db)), *previous, cycle)
            },
            heap_size=ruff_memory_usage::heap_size
        )]
        fn intersection_from_two_elements<'db>(db: &'db dyn Db, types: TypePair<'db>) -> Type<'db> {
            let env = ProgramEnvironment::from_program(types.program(db));
            IntersectionBuilder::new(db, &env)
                .positive_elements([types.first(db), types.second(db)])
                .build()
        }

        intersection_from_two_elements(db, TypePair::new(db, env.program(db), a, b))
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let positive = if nested {
            self.positive(db)
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, env, div, nested))
                .collect::<Option<FxOrderSet<Type<'db>>>>()?
        } else {
            self.positive(db)
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, env, div, nested)
                        .unwrap_or(div)
                })
                .collect()
        };

        let negative = if nested {
            self.negative(db)
                .try_map(|ty| ty.recursive_type_normalized_impl(db, env, div, nested))?
        } else {
            self.negative(db).map(|ty| {
                ty.recursive_type_normalized_impl(db, env, div, nested)
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

    /// Map both signs of the intersection, preserving open bodies during structural substitutions.
    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let positive = self
            .positive(db)
            .iter()
            .map(|positive| positive.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            .collect::<FxOrderSet<_>>();
        let mut negative = NegativeIntersectionElements::default();
        // Regular promotion removes negative contributions from intersections.
        if !matches!(
            type_mapping,
            TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular)
        ) {
            for element in self.negative(db) {
                negative.insert(element.apply_type_mapping_impl(
                    db,
                    &type_mapping.flip(),
                    tcx,
                    visitor,
                ));
            }
        }

        // An unchanged substitution must not expand another layer of a recursive type.
        // Otherwise, repeatedly resolving type-variable bindings cannot reach a fixed point.
        if positive == *self.positive(db) && negative == *self.negative(db) {
            return Type::Intersection(self);
        }

        let mut builder = IntersectionBuilder::new(db, visitor.env)
            .preserve_aliases(true)
            .positive_elements(positive);
        for element in &negative {
            builder.add_negative_in_place(*element);
        }
        builder.build()
    }

    /// Map a type transformation over all positive elements of the intersection. Leave the
    /// negative elements unchanged.
    pub(crate) fn map_positive(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let mut builder = IntersectionBuilder::new(db, env);
        for ty in self.positive(db) {
            builder.add_positive_in_place(transform_fn(ty));
        }
        for ty in self.negative(db) {
            builder.add_negative_in_place(*ty);
        }
        builder.build()
    }

    /// Compute the `__class__` type for class-backed protocols and `TypedDict` instances,
    /// whose runtime classes differ from their internal meta-types.
    ///
    /// Negative instance constraints are not transferred: an object not satisfying `P` does not
    /// imply that other instances of its class cannot satisfy `P`.
    pub(crate) fn try_dunder_class(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        if !self.iter_positive(db).any(|positive| {
            matches!(
                positive,
                Type::ProtocolInstance(protocol) if protocol.class_origin(db).is_some()
            ) || positive.is_typed_dict()
        }) {
            return None;
        }

        let mut builder = IntersectionBuilder::new(db, env);
        for positive in self.iter_positive(db) {
            builder.add_positive_in_place(positive.dunder_class(db, env));
        }
        Some(builder.build())
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = IntersectionBuilder::new(db, env);

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

                    builder.add_positive_in_place(ty_member);
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
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.filter_map_with_boundness_and_qualifiers(db, env, |ty| Some(transform_fn(ty)))
    }

    pub(crate) fn filter_map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> Option<PlaceAndQualifiers<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = IntersectionBuilder::new(db, env);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        let mut origin = TypeOrigin::Declared;
        let mut provenance = Provenance::Unknown;
        for ty in self.positive_elements_or_object(db) {
            let Some(PlaceAndQualifiers {
                place: member,
                qualifiers: new_qualifiers,
            }) = transform_fn(&ty)
            else {
                continue;
            };
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

                    builder.add_positive_in_place(ty_member);
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
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        expand_intersection_typevars_and_newtypes(db, env, self.positive(db), self.negative(db))
    }

    pub fn iter_positive(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.positive(db).iter().copied()
    }

    pub(crate) fn iter_negative(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
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
    pub(super) fn to_instance(
        self,
        db: &'db dyn Db,
        traversal: &TypeQueryContext<'_, 'db, ProjectInstance>,
    ) -> Option<InstanceProjection<Type<'db>>> {
        let env = traversal.environment();
        let mut builder = IntersectionBuilder::new(db, env);
        let mut has_projected_positive = false;
        let mut is_exact = self.negative(db).is_empty();
        for positive in self.iter_positive(db) {
            if let Some(projection) = traversal.visit(db, positive) {
                has_projected_positive = true;
                is_exact &= projection.is_exact();
                builder.add_positive_in_place(projection.into_inner());
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
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db, env);
    for &element in positive {
        let projected = TypeInspector::inspect_root(
            db,
            env,
            element,
            |_, _| None,
            |_, ty| match ty {
                Type::TypeVar(tvar) => Some(match tvar.require_bound_or_constraints(db, env) {
                    TypeVarBoundOrConstraints::UpperBound(bound) => bound,
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        constraints.as_type(db, env)
                    }
                }),
                Type::NewTypeInstance(newtype) => Some(newtype.concrete_base_type(db)),
                _ => None,
            },
        );
        builder.add_positive_in_place(projected.unwrap_or(element));
    }

    for &element in negative {
        builder.add_negative_in_place(element);
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
